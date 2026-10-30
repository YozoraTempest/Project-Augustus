// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    CountryLedgerAnnualRollover, CountryLedgerMonthlyConsolidation, CountryLedgerPendingFlush,
    MonthlyEconomy, MonthlyPipeline, NavalConstructionId,
    VanillaNativeMigrationScoreCandidateInput, calculate_vanilla_native_migration_score,
    select_vanilla_native_migration_candidate,
};
use augustus_eu4::{ContentSet, TextEncoding, VanillaExternalState, VanillaWorld};
use clap::Parser;
use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    version,
    about = "Load and advance the supported EU4 final-edition world systems"
)]
struct Cli {
    /// EU4 installation root; locally present DLC archives are loaded automatically.
    #[arg(long)]
    game_root: PathBuf,
    /// Directory Mod containing descriptor.mod; repeat for each candidate Mod.
    #[arg(long = "mod", value_name = "DIRECTORY")]
    mods: Vec<PathBuf>,
    /// EU4 user directory mounted above Mods and base game content.
    #[arg(long, value_name = "DIRECTORY")]
    user_directory: Option<PathBuf>,
    /// EU4 launcher dlc_load.json; selects supplied Mods and discovers selected .mod directories.
    #[arg(long, value_name = "FILE")]
    launcher_config: Option<PathBuf>,
    /// JSON state bundle supplied by another simulation or an external save reader.
    /// With --eu4-save, fields in this bundle supplement the translated save state.
    #[arg(long, value_name = "FILE")]
    external_state: Option<PathBuf>,
    /// EU4 save whose supported state and random stream are restored before startup. Use --eu4-token-map for EU4bin.
    #[arg(
        long = "eu4-save",
        value_name = "FILE",
        conflicts_with_all = ["player", "ai_only", "seed"]
    )]
    eu4_save: Option<PathBuf>,
    /// Player-provided token map for EU4bin, including optional `lookup <index> <value>` lines.
    #[arg(long, value_name = "FILE", requires = "eu4_save")]
    eu4_token_map: Option<PathBuf>,
    /// Export the translated state from --eu4-save as a VanillaExternalState JSON file.
    #[arg(long, value_name = "FILE", requires = "eu4_save")]
    export_external_state: Option<PathBuf>,
    /// Submit new naval orders with caller-supplied prices and absolute core completion days.
    /// Requests execute once per invocation, after --load and before --days.
    #[arg(
        long,
        value_name = "FILE",
        group = "naval_submission",
        conflicts_with = "compare_administration_save"
    )]
    naval_constructions: Option<PathBuf>,
    /// Submit caller-priced orders as per-port FIFO queues at empty ports.
    /// The file uses the same request array as --naval-constructions.
    #[arg(
        long,
        value_name = "FILE",
        group = "naval_submission",
        conflicts_with = "compare_administration_save"
    )]
    naval_construction_queues: Option<PathBuf>,
    /// Cancel pending naval orders by a JSON array of stable Augustus IDs.
    /// Cancellation runs after --load and before --days, without refunding
    /// costs or reversing ledger expenses.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = [
            "naval_constructions",
            "naval_construction_queues",
            "naval_construction_ledger",
            "compare_administration_save"
        ]
    )]
    cancel_naval_constructions: Option<PathBuf>,
    /// Import saved multi-order naval queues before --load and --days.
    /// Accepts omitted zero progress on waiting orders.
    #[arg(
        long,
        requires = "eu4_save",
        conflicts_with = "compare_administration_save"
    )]
    import_naval_construction_queues: bool,
    /// Show current pending naval orders and FIFO positions after --load and --days.
    #[arg(long, conflicts_with = "compare_administration_save")]
    show_naval_constructions: bool,
    /// Query ordinary galley build durations with loaded defines and explicit modifiers.
    /// Queries run after restore and --days and do not submit orders.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "compare_administration_save"
    )]
    naval_construction_time_queries: Option<PathBuf>,
    /// Execute caller-resolved native migration transfers after restore and
    /// before --days, then include their reports in the JSON output.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "compare_administration_save"
    )]
    native_migration_transfers: Option<PathBuf>,
    /// Record submitted naval costs in the EXPENSEFLEET ledger category; requires known treasury.
    #[arg(long, requires = "naval_submission")]
    naval_construction_ledger: bool,
    /// Evaluate caller-resolved native migration queries using current world power and capital.
    /// Queries run after --load and --days; they do not submit migration commands.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "compare_administration_save"
    )]
    native_migration_queries: Option<PathBuf>,
    /// Evaluate migration queries with costs from the loaded NCountry defines.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "compare_administration_save"
    )]
    native_migration_content_queries: Option<PathBuf>,
    /// Query migration with current native membership/steps and loaded costs.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "compare_administration_save"
    )]
    native_migration_state_queries: Option<PathBuf>,
    /// Evaluate caller-resolved native migration AI scores without mutating the world.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "compare_administration_save"
    )]
    native_migration_score_queries: Option<PathBuf>,
    /// Import native membership and migration steps from the selected EU4 save.
    #[arg(long, requires = "eu4_save", conflicts_with = "native_migration_state")]
    import_native_migration_state: bool,
    /// Bind explicit country native migration states before restoring a snapshot.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "compare_administration_save"
    )]
    native_migration_state: Option<PathBuf>,
    #[arg(long, default_value = "1444.11.11")]
    date: String,
    /// Explicit seed for a new world; EU4 saves carry their own random state.
    #[arg(long, value_parser = parse_eu4_seed, allow_negative_numbers = true)]
    seed: Option<u32>,
    /// Advance calendar days and execute selected monthly and yearly systems.
    #[arg(
        long,
        default_value_t = 0,
        conflicts_with = "compare_administration_save"
    )]
    days: u64,
    /// Omit the full content-loading report from JSON output.
    #[arg(long)]
    omit_load_report: bool,
    /// Use the monthly ledger pipeline; loaded snapshots select their recorded version.
    #[arg(long, conflicts_with = "compare_administration_save")]
    monthly_ledger_pipeline: bool,
    /// Run the explicit annual ledger rollover; keep this option when restoring its snapshots.
    #[arg(long, conflicts_with = "compare_administration_save")]
    annual_ledger_rollover: bool,
    /// Restore an Augustus snapshot after loading matching local content.
    #[arg(long, conflicts_with = "compare_administration_save")]
    load: Option<PathBuf>,
    /// Save an Augustus snapshot without overwriting an existing file.
    #[arg(long)]
    save: Option<PathBuf>,
    #[arg(long)]
    country: Option<String>,
    /// Human-controlled countries; all other loaded countries are AI-controlled.
    #[arg(long, conflicts_with = "ai_only")]
    player: Vec<String>,
    /// Supply an explicit AI-only control roster.
    #[arg(long)]
    ai_only: bool,
    #[arg(long)]
    province: Option<u32>,
    #[arg(long)]
    diplomacy: bool,
    #[arg(long)]
    economy: bool,
    /// Compare administration with an uncompressed, unadvanced EU4txt baseline save.
    #[arg(long)]
    compare_administration_save: Option<PathBuf>,
    #[arg(long)]
    technology: Option<String>,
    #[arg(long)]
    modifier: Option<String>,
    /// Show one unit template by its file stem.
    #[arg(long)]
    unit: Option<String>,
}

fn parse_eu4_seed(value: &str) -> Result<u32, String> {
    let value = value.trim_start_matches([' ', '\t']);
    let (negative, digits) = if let Some(digits) = value.strip_prefix('-') {
        (true, digits)
    } else if let Some(digits) = value.strip_prefix('+') {
        (false, digits)
    } else {
        (false, value)
    };
    let limit = if negative {
        i32::MAX as u64 + 1
    } else {
        i32::MAX as u64
    };
    let mut magnitude = 0_u64;
    let mut has_digits = false;
    for digit in digits.bytes() {
        if !digit.is_ascii_digit() {
            break;
        }
        has_digits = true;
        magnitude = magnitude
            .saturating_mul(10)
            .saturating_add(u64::from(digit - b'0'))
            .min(limit);
    }
    if !has_digits {
        return Ok(0);
    }
    let signed = if negative {
        if magnitude == i32::MAX as u64 + 1 {
            i32::MIN
        } else {
            -(magnitude as i32)
        }
    } else {
        magnitude as i32
    };
    Ok(signed as u32)
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let players = if cli.ai_only || !cli.player.is_empty() {
        Some(
            cli.player
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
        )
    } else {
        None
    };
    let content = ContentSet::from_installation(
        &cli.game_root,
        &cli.mods,
        cli.user_directory.as_deref(),
        cli.launcher_config.as_deref(),
        TextEncoding::Windows1252,
    )?;
    let external_state = cli
        .external_state
        .as_ref()
        .map(
            |path| -> Result<VanillaExternalState, Box<dyn std::error::Error>> {
                Ok(serde_json::from_slice(&std::fs::read(path)?)?)
            },
        )
        .transpose()?;
    let save_state = cli
        .eu4_save
        .as_ref()
        .map(|path| match cli.eu4_token_map.as_ref() {
            Some(token_map) => {
                augustus_eu4::read_vanilla_save_state_and_trade_node_runtime_states_with_token_map(
                    path, token_map,
                )
            }
            None => augustus_eu4::read_vanilla_save_state_and_trade_node_runtime_states(path),
        })
        .transpose()?;
    let naval_constructions = cli
        .eu4_save
        .as_ref()
        .map(|path| match cli.eu4_token_map.as_ref() {
            Some(token_map) => {
                augustus_eu4::read_vanilla_naval_constructions_with_token_map(path, token_map)
            }
            None => augustus_eu4::read_vanilla_naval_constructions(path),
        })
        .transpose()?;
    let native_migration_import = if cli.import_native_migration_state {
        let path = cli.eu4_save.as_ref().expect("required by clap");
        Some(match cli.eu4_token_map.as_ref() {
            Some(token_map) => {
                augustus_eu4::read_vanilla_native_migration_states_with_token_map(path, token_map)?
            }
            None => augustus_eu4::read_vanilla_native_migration_states(path)?,
        })
    } else {
        None
    };
    let load_snapshot = cli.load.as_ref().map(std::fs::read_to_string).transpose()?;
    let loaded_monthly_system_id = load_snapshot
        .as_deref()
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()?
        .and_then(|snapshot| {
            snapshot
                .get("monthly_system_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    let mut loaded = if cli.eu4_save.is_some() {
        let save = save_state
            .as_ref()
            .expect("save state loaded from the selected EU4 save");
        VanillaWorld::load_from_content_with_save_state_and_naval_constructions(
            &content,
            &save.state,
            naval_constructions
                .as_ref()
                .expect("naval constructions read from the selected EU4 save"),
            external_state.as_ref(),
        )?
    } else {
        match (&players, cli.seed, external_state.as_ref()) {
            (Some(players), Some(seed), Some(state)) => {
                VanillaWorld::load_from_content_with_players_and_seed_and_external_state(
                    &content, &cli.date, players, seed, state,
                )?
            }
            (Some(players), None, Some(state)) => {
                VanillaWorld::load_from_content_with_players_and_external_state(
                    &content, &cli.date, players, state,
                )?
            }
            (None, Some(seed), Some(state)) => {
                VanillaWorld::load_from_content_with_seed_and_external_state(
                    &content, &cli.date, seed, state,
                )?
            }
            (None, None, Some(state)) => {
                VanillaWorld::load_from_content_with_external_state(&content, &cli.date, state)?
            }
            (Some(players), Some(seed), None) => {
                VanillaWorld::load_from_content_with_players_and_seed(
                    &content, &cli.date, players, seed,
                )?
            }
            (Some(players), None, None) => {
                VanillaWorld::load_from_content_with_players(&content, &cli.date, players)?
            }
            (None, Some(seed), None) => {
                VanillaWorld::load_from_content_with_seed(&content, &cli.date, seed)?
            }
            (None, None, None) => VanillaWorld::load_from_content(&content, &cli.date)?,
        }
    };
    if cli.import_naval_construction_queues {
        let path = cli.eu4_save.as_ref().expect("required by clap");
        let queues = match cli.eu4_token_map.as_ref() {
            Some(token_map) => {
                augustus_eu4::read_vanilla_naval_constructions_for_queue_import_with_token_map(
                    path, token_map,
                )?
            }
            None => augustus_eu4::read_vanilla_naval_constructions_for_queue_import(path)?,
        };
        loaded.import_naval_construction_queues(&queues)?;
    }
    if let Some(import) = native_migration_import.as_ref() {
        let states = import.states_for_world(&loaded);
        loaded.set_country_native_migration(states)?;
    }
    if let Some(path) = cli.native_migration_state.as_ref() {
        let states = serde_json::from_slice::<
            std::collections::BTreeMap<String, augustus_eu4::VanillaExternalNativeMigrationState>,
        >(&std::fs::read(path)?)?;
        loaded.set_country_native_migration(states)?;
    }
    if let Some(save) = save_state.as_ref() {
        let trade_node_states = save.trade_node_runtime_states_for_world(&loaded);
        loaded.set_vanilla_trade_node_runtime_states(&trade_node_states)?;
    }
    if let (Some(output_path), Some(save)) =
        (cli.export_external_state.as_ref(), save_state.as_ref())
    {
        let mut exported = save.state.state_for_world(&loaded)?;
        if let Some(overlay) = external_state.as_ref() {
            exported.apply_overlay(overlay);
        }
        let bytes = serde_json::to_vec_pretty(&exported)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_path)?;
        file.write_all(&bytes)?;
    }
    let pending_flush = CountryLedgerPendingFlush;
    let economy = MonthlyEconomy;
    let ledger_consolidation = CountryLedgerMonthlyConsolidation::default();
    let legacy_monthly_pipeline = MonthlyPipeline::new(
        "augustus-world-monthly-ledger:v1",
        Some(&pending_flush),
        Some(&economy),
        Some(&ledger_consolidation),
    )?;
    let ledger_tail = MonthlyPipeline::new(
        "augustus-world-monthly-ledger-tail:v1",
        Some(&pending_flush),
        None,
        Some(&ledger_consolidation),
    )?;
    let current_monthly_pipeline = MonthlyPipeline::new(
        "augustus-world-monthly-ledger:v2",
        Some(&economy),
        None,
        Some(&ledger_tail),
    )?;
    let monthly_pipeline = if cli.monthly_ledger_pipeline {
        if loaded_monthly_system_id.as_deref() == Some("augustus-world-monthly-ledger:v1") {
            Some(&legacy_monthly_pipeline)
        } else {
            Some(&current_monthly_pipeline)
        }
    } else {
        None
    };
    let annual_rollover = CountryLedgerAnnualRollover;
    if let Some(pipeline) = monthly_pipeline {
        let (
            taxation,
            monthly_accruals,
            monthly_convergence,
            monthly_stocks,
            monthly_production,
            monthly_cashflows,
        ) = {
            let world = loaded.initial.world();
            (
                world.taxation().cloned(),
                world.monthly_accruals().cloned(),
                world.monthly_convergence().cloned(),
                world.monthly_stocks().cloned(),
                world.monthly_production().cloned(),
                world.monthly_cashflows().cloned(),
            )
        };
        loaded.select_monthly_system(pipeline)?;
        if let Some(state) = taxation {
            loaded.initial.set_taxation(state)?;
        }
        if let Some(state) = monthly_accruals {
            loaded.initial.set_monthly_accruals(state)?;
        }
        if let Some(state) = monthly_convergence {
            loaded.initial.set_monthly_convergence(Some(state))?;
        }
        if let Some(state) = monthly_stocks {
            loaded.initial.set_monthly_stocks(Some(state))?;
        }
        if let Some(state) = monthly_production {
            loaded.initial.set_monthly_production(state)?;
        }
        if let Some(state) = monthly_cashflows {
            loaded.initial.set_monthly_cashflows(Some(state))?;
        }
    }
    if cli.annual_ledger_rollover {
        loaded.select_yearly_system(&annual_rollover)?;
    }
    if cli.load.is_some() {
        loaded.restore(load_snapshot.as_deref().expect("snapshot was loaded"))?;
    }
    let cancelled_naval_constructions = cli
        .cancel_naval_constructions
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let ids = serde_json::from_slice::<Vec<u64>>(&std::fs::read(path)?)?
                .into_iter()
                .map(NavalConstructionId)
                .collect::<Vec<_>>();
            loaded.cancel_naval_constructions(&ids)?;
            Ok(ids)
        })
        .transpose()?;
    let submitted_naval_constructions = cli
        .naval_constructions
        .as_ref()
        .or(cli.naval_construction_queues.as_ref())
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let requests: Vec<augustus_eu4::VanillaNavalConstructionRequest> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            Ok(
                match (
                    cli.naval_construction_queues.is_some(),
                    cli.naval_construction_ledger,
                ) {
                    (true, true) => {
                        loaded.submit_naval_construction_queues_with_ledger(&requests)?
                    }
                    (true, false) => loaded.submit_naval_construction_queues(&requests)?,
                    (false, true) => loaded.submit_naval_constructions_with_ledger(&requests)?,
                    (false, false) => loaded.submit_naval_constructions(&requests)?,
                },
            )
        })
        .transpose()?;
    let native_migration_transfers = cli
        .native_migration_transfers
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let requests: Vec<augustus_eu4::VanillaNativeMigrationTransferRequest> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            requests
                .iter()
                .map(|request| {
                    loaded
                        .execute_native_migration_transfer_from_state(request)
                        .map_err(Into::into)
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
        })
        .transpose()?;
    if monthly_pipeline.is_some() || cli.annual_ledger_rollover {
        let monthly =
            monthly_pipeline.map(|pipeline| pipeline as &dyn augustus_core::MonthlySystem);
        let yearly = cli
            .annual_ledger_rollover
            .then_some(&annual_rollover as &dyn augustus_core::YearlySystem);
        loaded.advance_with_systems_and_yearly(cli.days, None, monthly, yearly)?;
    } else {
        loaded.advance(cli.days)?;
    }
    let native_migration_queries = cli
        .native_migration_queries
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let queries: Vec<augustus_eu4::VanillaNativeMigrationQuery> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            queries
                .iter()
                .map(|query| loaded.query_native_migration(query))
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .transpose()?;
    let native_migration_content_queries = cli
        .native_migration_content_queries
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let queries: Vec<augustus_eu4::VanillaNativeMigrationContentQuery> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            queries
                .iter()
                .map(|query| loaded.query_native_migration_from_content(query))
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .transpose()?;
    let native_migration_state_queries = cli
        .native_migration_state_queries
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let queries: Vec<augustus_eu4::VanillaNativeMigrationStateQuery> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            queries
                .iter()
                .map(|query| loaded.query_native_migration_from_state(query))
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .transpose()?;
    let native_migration_score_queries = cli
        .native_migration_score_queries
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let candidates: Vec<VanillaNativeMigrationScoreCandidateInput> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            let candidate_reports = candidates
                .iter()
                .map(|candidate| {
                    let score =
                        calculate_vanilla_native_migration_score(candidate.score.clone())
                            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
                    Ok::<_, Box<dyn std::error::Error>>(serde_json::json!({
                        "candidate_id": candidate.candidate_id,
                        "score": score,
                    }))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let winner = select_vanilla_native_migration_candidate(&candidates)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            Ok(serde_json::json!({
                "candidates": candidate_reports,
                "winner": winner,
            }))
        })
        .transpose()?;
    let naval_construction_time_queries = cli
        .naval_construction_time_queries
        .as_ref()
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            let queries: Vec<augustus_eu4::VanillaNavalConstructionTimeQuery> =
                serde_json::from_slice(&std::fs::read(path)?)?;
            queries
                .iter()
                .map(|query| loaded.query_naval_construction_time(query))
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
        .transpose()?;
    let naval_construction_orders = cli
        .show_naval_constructions
        .then(|| loaded.naval_construction_orders())
        .transpose()?
        .map(serde_json::to_value)
        .transpose()?;
    if let Some(path) = cli.save {
        let snapshot = loaded.save()?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(snapshot.as_bytes())?;
    }
    let mut output = serde_json::json!({
        "simulation": {"day": loaded.initial.world().day(), "date": loaded.initial.world().date()?,
            "monthly_system_id": loaded.initial.world().monthly_system_id(),
            "yearly_system_id": loaded.initial.world().yearly_system_id(),
            "eu4_random": loaded.initial.world().eu4_random_state().map(|(seed, count, index)| {
                serde_json::json!({"seed": seed, "count": count, "index": index})
            }),
            "pending_tasks": loaded.initial.world().pending_tasks(),
            "pending_naval_constructions": loaded.initial.world().naval_constructions().len(),
            "tax_settlements": loaded.initial.world().taxation().map(|t| t.settlement_count),
            "last_tax_day": loaded.initial.world().taxation().and_then(|t| t.last_settlement.as_ref().map(|s| s.day)),
            "production_settlements": loaded.initial.world().monthly_production().map(|p| p.settlement_count),
            "last_production_day": loaded.initial.world().monthly_production().and_then(|p| p.last_settlement.as_ref().map(|s| s.day)),
            "cashflow_settlements": loaded.initial.world().monthly_cashflows().map(|c| c.settlement_count),
            "last_cashflow_day": loaded.initial.world().monthly_cashflows().and_then(|c| c.last_settlement.as_ref().map(|s| s.day)),
            "armies": loaded.initial.armies().len(),
            "fleets": loaded.initial.fleets().len(),
            "naval_units": loaded.initial.naval_units().len()}});
    if !cli.omit_load_report {
        output["report"] = serde_json::to_value(&loaded.report)?;
        if naval_constructions.is_some() {
            output["naval_construction_import"] =
                serde_json::to_value(loaded.naval_construction_import_report())?;
        }
    }
    if let Some(orders) = naval_construction_orders {
        output["naval_construction_orders"] = orders;
    }
    if let Some(queries) = naval_construction_time_queries {
        output["naval_construction_time_queries"] = serde_json::to_value(queries)?;
    }
    if let Some(queries) = native_migration_state_queries {
        output["native_migration_state_queries"] = serde_json::to_value(queries)?;
    }
    if let Some(report) = native_migration_score_queries {
        output["native_migration_score_queries"] = report;
    }
    if let Some(queries) = native_migration_content_queries {
        output["native_migration_content_queries"] = serde_json::to_value(queries)?;
    }
    if let Some(queries) = native_migration_queries {
        output["native_migration_queries"] = serde_json::to_value(queries)?;
    }
    if let Some(reports) = native_migration_transfers {
        output["native_migration_transfers"] = serde_json::to_value(reports)?;
    }
    if let Some(ids) = submitted_naval_constructions {
        output["submitted_naval_constructions"] = serde_json::to_value(ids)?;
    }
    if let Some(ids) = cancelled_naval_constructions {
        output["cancelled_naval_constructions"] = serde_json::to_value(ids)?;
    }
    if !loaded.initial.world().country_controllers().is_empty() {
        output["country_controllers"] = serde_json::json!(
            loaded
                .initial
                .world()
                .country_controllers()
                .iter()
                .map(|(id, control)| (
                    &loaded
                        .initial
                        .world()
                        .entity(*id)
                        .expect("validated country")
                        .name,
                    control
                ))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
    }
    if let Some(path) = cli.compare_administration_save {
        output["administration_comparison"] =
            serde_json::to_value(augustus_eu4::compare_administration_save(&loaded, path)?)?;
    }
    if let Some(branch) = cli.technology {
        output["technology"] = serde_json::json!({"branch": branch, "levels": loaded.initial.world().technology().tables.get(&branch).ok_or("unknown technology branch")?});
    }
    if let Some(name) = cli.modifier {
        output["modifier"] = serde_json::json!({"id": name, "definition": loaded.initial.world().modifiers().definitions.get(&name).ok_or("unknown modifier")?});
        output["modifier_source"] = serde_json::to_value(
            &loaded.report.setup.definition_sources[&format!("modifier:{name}")],
        )?;
    }
    if let Some(name) = cli.unit {
        let unit_name = name.as_str();
        let unlocked_at = loaded
            .initial
            .world()
            .technology()
            .tables
            .iter()
            .flat_map(|(branch, levels)| {
                levels.iter().enumerate().filter_map(move |(level, entry)| {
                    if entry
                        .enabled_units
                        .iter()
                        .any(|unit| unit == unit_name)
                    {
                        Some(serde_json::json!({"branch": branch, "level": level, "year": entry.year}))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();
        output["unit_template"] = serde_json::json!({
            "id": name,
            "definition": loaded.initial.world().unit_templates().definitions.get(&name).ok_or("unknown unit template")?,
            "unlocked_at": unlocked_at,
        });
        output["unit_template_source"] = serde_json::to_value(
            loaded
                .report
                .setup
                .unit_template_sources
                .get(&name)
                .ok_or("missing unit template source")?,
        )?;
    }
    if cli.economy {
        let economy = loaded.initial.world().economy();
        let province_id = |entity| -> Result<u32, Box<dyn std::error::Error>> {
            Ok(loaded.initial.world().entity(entity)?.name.parse()?)
        };
        let nodes = economy.trade_nodes.iter().map(|(id, node)| {
            Ok(serde_json::json!({
                "id": id, "location": province_id(node.location)?,
                "members": node.members.iter().copied().map(&province_id).collect::<Result<Vec<_>, _>>()?,
                "outgoing": node.outgoing, "inland": node.inland, "end": node.end,
                "current_milli": loaded.initial.world().trade_node_current_value_milli(id).flatten(),
                "runtime_state": loaded.initial.world().trade_node_runtime_state(id).flatten(),
            }))
        }).collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        output["economy"] = serde_json::json!({
            "buildings": economy.buildings, "trade_goods": economy.trade_goods, "trade_nodes": nodes,
        });
    }
    if cli.diplomacy {
        output["diplomacy"] = serde_json::to_value(loaded.diplomacy())?;
        let country_tags = loaded
            .initial
            .countries()
            .iter()
            .map(|country| {
                (
                    loaded
                        .initial
                        .country_entity(&country.id)
                        .expect("registered country"),
                    country.id.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut relation_states = BTreeMap::<&str, BTreeMap<&str, serde_json::Value>>::new();
        for (owner, target, state) in loaded
            .initial
            .world()
            .country_relation_states()
            .filter(|(_, _, state)| !state.is_empty())
        {
            let owner_tag = country_tags
                .get(&owner)
                .expect("registered relation owner")
                .as_str();
            if cli
                .country
                .as_deref()
                .is_some_and(|selected| selected != owner_tag)
            {
                continue;
            }
            let target_tag = country_tags
                .get(&target)
                .expect("registered relation target")
                .as_str();
            relation_states.entry(owner_tag).or_default().insert(
                target_tag,
                serde_json::json!({
                    "attitude": state.attitude.as_deref(),
                    "trust_value": state.trust_value,
                }),
            );
        }
        output["country_relation_states"] = serde_json::to_value(relation_states)?;
        let calendar = loaded
            .initial
            .world()
            .calendar()
            .expect("loaded world calendar");
        let mut truces = Vec::new();
        for (&first, records) in &loaded.initial.world().truces().records {
            for (&second, truce) in records {
                let Some(truce) = truce else {
                    continue;
                };
                let expires_on = truce
                    .expires_on
                    .map(|day| {
                        let date = calendar.date(day)?;
                        Ok::<_, augustus_core::Error>(format!(
                            "{:04}.{:02}.{:02}",
                            date.year, date.month, date.day
                        ))
                    })
                    .transpose()?;
                truces.push(serde_json::json!({
                    "first": country_tags.get(&first).expect("registered truce country"),
                    "second": country_tags.get(&second).expect("registered truce country"),
                    "expires_on": expires_on,
                    "monthly": truce.monthly,
                }));
            }
        }
        output["truces"] = serde_json::to_value(truces)?;
        output["political_offices"] = serde_json::json!(
            loaded
                .initial
                .world()
                .politics()
                .offices
                .iter()
                .map(|(office, holder)| (
                    office,
                    &loaded
                        .initial
                        .world()
                        .entity(*holder)
                        .expect("validated office")
                        .name
                ))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
    }
    if let Some(id) = cli.country {
        output["country"] = serde_json::to_value(loaded.country(&id).ok_or("unknown country")?)?;
        output["country"]["attributes"] = serde_json::to_value(loaded.country_attributes(&id)?)?;
        let country_entity = loaded.initial.country_entity(&id).expect("known country");
        let available_units = loaded
            .initial
            .world()
            .available_unit_templates(country_entity)?;
        let selected_units = available_units
            .keys()
            .filter_map(|category| {
                loaded
                    .initial
                    .world()
                    .selected_unit(country_entity, category)
                    .map(|unit| (category.clone(), unit.to_owned()))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        output["country"]["available_unit_templates"] = serde_json::to_value(available_units)?;
        output["country"]["selected_unit_templates"] = serde_json::to_value(selected_units)?;
        if let Some(ledger) = loaded
            .initial
            .world()
            .country_ledgers()
            .get(&country_entity)
        {
            output["country"]["ledger"] = serde_json::to_value(ledger)?;
        }
        if let Some(accounts) = loaded
            .initial
            .world()
            .monthly_accruals()
            .and_then(|state| state.last_settlement.as_ref())
            .and_then(|last| last.accounts.get(&country_entity))
        {
            output["country"]["last_monthly_accruals"] = serde_json::to_value(accounts)?;
        }
        output["country_sources"] = serde_json::to_value(&loaded.country_sources[&id])?;
    }
    if let Some(id) = cli.province {
        let mut province = serde_json::to_value(loaded.province(id).ok_or("unknown province")?)?;
        province["trade_node_current_milli"] =
            serde_json::json!(loaded.province_trade_node_current_milli(id));
        output["province"] = province;
        output["province_trade_good_base_price_milli"] =
            serde_json::to_value(loaded.province_trade_good_base_price_milli(id)?)?;
        output["province_trade_good_price_milli"] =
            serde_json::to_value(loaded.province_trade_good_price_milli(id)?)?;
        output["province_sources"] = serde_json::to_value(&loaded.province_sources[&id])?;
    }
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod seed_tests {
    use super::parse_eu4_seed;

    #[test]
    fn eu4_seed_parser_matches_signed_32_bit_atol_conversion() {
        let cases = [
            ("2147483647", 2_147_483_647),
            ("2147483648", 2_147_483_647),
            ("4149563735", 2_147_483_647),
            ("-2147483647", 2_147_483_649),
            ("-2147483648", 2_147_483_648),
            ("-2147483649", 2_147_483_648),
            ("-145403561", 4_149_563_735),
            ("123suffix", 123),
            ("no digits", 0),
            (" \t-123suffix", 4_294_967_173),
            ("\n-123", 0),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_eu4_seed(input).unwrap(), expected, "input {input:?}");
        }
    }
}
