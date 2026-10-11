// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_eu4::VanillaWorld;
use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    version,
    about = "Load and advance the supported EU4 final-edition world systems"
)]
struct Cli {
    #[arg(long)]
    game_root: PathBuf,
    #[arg(long, default_value = "1444.11.11")]
    date: String,
    /// Advance calendar days and execute supported monthly tax settlements.
    #[arg(
        long,
        default_value_t = 0,
        conflicts_with = "compare_administration_save"
    )]
    days: u64,
    /// Restore an Augustus snapshot after loading matching local content.
    #[arg(long, conflicts_with = "compare_administration_save")]
    load: Option<PathBuf>,
    /// Save an Augustus snapshot without overwriting an existing file.
    #[arg(long)]
    save: Option<PathBuf>,
    #[arg(long)]
    country: Option<String>,
    #[arg(long)]
    province: Option<u32>,
    #[arg(long)]
    diplomacy: bool,
    #[arg(long)]
    economy: bool,
    /// Compare administrative inputs with an uncompressed, unadvanced vanilla baseline save.
    #[arg(long)]
    compare_administration_save: Option<PathBuf>,
    #[arg(long)]
    technology: Option<String>,
    #[arg(long)]
    modifier: Option<String>,
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let mut loaded = VanillaWorld::load(cli.game_root, &cli.date)?;
    if let Some(path) = cli.load {
        loaded.restore(&std::fs::read_to_string(path)?)?;
    }
    loaded.advance(cli.days)?;
    if let Some(path) = cli.save {
        use std::io::Write;
        let snapshot = loaded.save()?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(snapshot.as_bytes())?;
    }
    let mut output = serde_json::json!({"report": loaded.report,
        "simulation": {"day": loaded.initial.world().day(), "date": loaded.initial.world().date()?,
            "tax_settlements": loaded.initial.world().taxation().map(|t| t.settlement_count),
            "last_tax_day": loaded.initial.world().taxation().and_then(|t| t.last_settlement.as_ref().map(|s| s.day))}});
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
        output["country_sources"] = serde_json::to_value(&loaded.country_sources[&id])?;
    }
    if let Some(id) = cli.province {
        output["province"] = serde_json::to_value(loaded.province(id).ok_or("unknown province")?)?;
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
