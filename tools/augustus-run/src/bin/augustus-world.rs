// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    CountryLedgerAnnualRollover, CountryLedgerMonthlyConsolidation, CountryLedgerPendingFlush,
    MonthlyEconomy, MonthlyPipeline,
};
use augustus_eu4::{ContentSet, TextEncoding, VanillaExternalState, VanillaWorld};
use clap::Parser;
use std::{fs::OpenOptions, io::Write, path::PathBuf, process::ExitCode};

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
    /// EU4 launcher dlc_load.json; filters the supplied Mod directories by descriptor name.
    #[arg(long, value_name = "FILE")]
    launcher_config: Option<PathBuf>,
    /// JSON state bundle supplied by another simulation or an external save reader.
    /// With --eu4-save, fields in this bundle supplement the translated save state.
    #[arg(long, value_name = "FILE")]
    external_state: Option<PathBuf>,
    /// EU4 text save or ZIP-wrapped text save whose supported state is translated before startup.
    #[arg(
        long = "eu4-save",
        value_name = "FILE",
        conflicts_with_all = ["player", "ai_only", "seed"]
    )]
    eu4_save: Option<PathBuf>,
    /// Export the translated state from --eu4-save as a VanillaExternalState JSON file.
    #[arg(long, value_name = "FILE", requires = "eu4_save")]
    export_external_state: Option<PathBuf>,
    #[arg(long, default_value = "1444.11.11")]
    date: String,
    /// Explicit EU4 random seed; omitted seeds use the runtime clock for local AI event choices.
    #[arg(long)]
    seed: Option<u32>,
    /// Advance calendar days and execute selected monthly and yearly systems.
    #[arg(
        long,
        default_value_t = 0,
        conflicts_with = "compare_administration_save"
    )]
    days: u64,
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
        .map(augustus_eu4::read_vanilla_save_state)
        .transpose()?;
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
        VanillaWorld::load_from_content_with_save_state_and_external_state(
            &content,
            save,
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
    if let (Some(output_path), Some(save)) =
        (cli.export_external_state.as_ref(), save_state.as_ref())
    {
        let mut exported = save.state_for_world(&loaded)?;
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
    if let Some(path) = cli.save {
        let snapshot = loaded.save()?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(snapshot.as_bytes())?;
    }
    let mut output = serde_json::json!({"report": loaded.report,
        "simulation": {"day": loaded.initial.world().day(), "date": loaded.initial.world().date()?,
            "monthly_system_id": loaded.initial.world().monthly_system_id(),
            "yearly_system_id": loaded.initial.world().yearly_system_id(),
            "eu4_random": loaded.initial.world().eu4_random_state().map(|(seed, count, index)| {
                serde_json::json!({"seed": seed, "count": count, "index": index})
            }),
            "tax_settlements": loaded.initial.world().taxation().map(|t| t.settlement_count),
            "last_tax_day": loaded.initial.world().taxation().and_then(|t| t.last_settlement.as_ref().map(|s| s.day)),
            "production_settlements": loaded.initial.world().monthly_production().map(|p| p.settlement_count),
            "last_production_day": loaded.initial.world().monthly_production().and_then(|p| p.last_settlement.as_ref().map(|s| s.day)),
            "cashflow_settlements": loaded.initial.world().monthly_cashflows().map(|c| c.settlement_count),
            "last_cashflow_day": loaded.initial.world().monthly_cashflows().and_then(|c| c.last_settlement.as_ref().map(|s| s.day)),
            "armies": loaded.initial.armies().len(),
            "naval_units": loaded.initial.naval_units().len()}});
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
            }))
        }).collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        output["economy"] = serde_json::json!({
            "buildings": economy.buildings, "trade_goods": economy.trade_goods, "trade_nodes": nodes,
        });
    }
    if cli.diplomacy {
        output["diplomacy"] = serde_json::to_value(loaded.diplomacy())?;
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
        output["province"] = serde_json::to_value(loaded.province(id).ok_or("unknown province")?)?;
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
