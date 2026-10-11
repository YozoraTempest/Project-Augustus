// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Synthetic constant-branch workload. This does not measure EU4 or MT speed.

use std::{hint::black_box, mem::size_of, time::Instant};

use augustus_core::{Action, Condition, EntityKind, Plan, Schema, World};
use augustus_eu4::{ScriptCompiler, ScriptLibrary, TextDocument, TextEncoding};
use clap::Parser;

#[derive(Parser)]
#[command(about = "Compare constant folding against the same bound, unoptimised plans")]
struct Cli {
    #[arg(long, default_value_t = 512)]
    blocks: usize,
    #[arg(long, default_value_t = 2000)]
    iterations: usize,
    #[arg(long, default_value_t = 7)]
    samples: usize,
}

fn condition_bytes(condition: &Condition) -> usize {
    match condition {
        Condition::All(items) => {
            items.capacity() * size_of::<Condition>()
                + items.iter().map(condition_bytes).sum::<usize>()
        }
        Condition::Not(item) => size_of::<Condition>() + condition_bytes(item),
        _ => 0,
    }
}

fn plan_bytes(plan: &Plan) -> usize {
    plan.capacity() * size_of::<Action>()
        + plan
            .iter()
            .map(|action| match action {
                Action::If {
                    condition,
                    then,
                    otherwise,
                } => condition_bytes(condition) + plan_bytes(then) + plan_bytes(otherwise),
                Action::RandomBranch { then, otherwise } => {
                    plan_bytes(then) + plan_bytes(otherwise)
                }
                Action::With { plan, .. } | Action::Schedule { plan, .. } => plan_bytes(plan),
                Action::Emit(message) => message.capacity(),
                _ => 0,
            })
            .sum::<usize>()
}

fn execute(
    schema: &Schema,
    plan: &Plan,
    iterations: usize,
) -> Result<(u128, World), Box<dyn std::error::Error>> {
    let mut world = World::new(schema.clone());
    let country = world.add_entity("Benchmark", EntityKind::Country);
    let start = Instant::now();
    for _ in 0..iterations {
        world.execute(country, black_box(plan))?;
    }
    let elapsed = start.elapsed().as_nanos();
    black_box(&world);
    Ok((elapsed, world))
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if cli.blocks == 0
        || cli.blocks > 4096
        || cli.iterations == 0
        || cli.iterations > 100_000
        || !(3..=31).contains(&cli.samples)
    {
        return Err("use 1..4096 blocks, 1..100000 iterations, 3..31 samples".into());
    }
    let schema = Schema::new(["variable/score", "variable/gate"])?;
    let library = ScriptLibrary::new();
    let mut text = String::new();
    for index in 0..cli.blocks {
        text.push_str(
            "if = { limit = { always = yes } change_variable = { which = score value = 1 } }\n",
        );
        if index % 10 == 0 {
            text.push_str("if = { limit = { check_variable = { which = gate value = 0 } } change_variable = { which = score value = 1 } }\n");
        }
    }
    let document = TextDocument::from_bytes(
        "self-authored-benchmark.txt",
        text.as_bytes().to_vec(),
        TextEncoding::Utf8,
    );
    let start = Instant::now();
    let plain =
        ScriptCompiler::new(&schema, &library, false).compile(&document, EntityKind::Country)?;
    let plain_compile_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let folded =
        ScriptCompiler::new(&schema, &library, true).compile(&document, EntityKind::Country)?;
    let folded_compile_ns = start.elapsed().as_nanos();
    execute(&schema, &plain.plan, 20)?;
    execute(&schema, &folded.plan, 20)?;
    let mut plain_ns = Vec::new();
    let mut folded_ns = Vec::new();
    for sample in 0..cli.samples {
        let (baseline, optimised) = if sample % 2 == 0 {
            (
                execute(&schema, &plain.plan, cli.iterations)?,
                execute(&schema, &folded.plan, cli.iterations)?,
            )
        } else {
            let optimised = execute(&schema, &folded.plan, cli.iterations)?;
            (execute(&schema, &plain.plan, cli.iterations)?, optimised)
        };
        if baseline.1 != optimised.1 {
            return Err("optimisation changed the world state".into());
        }
        plain_ns.push(baseline.0);
        folded_ns.push(optimised.0);
    }
    let median = |values: &[u128]| {
        let mut values = values.to_vec();
        values.sort_unstable();
        values[values.len() / 2]
    };
    let baseline_median = median(&plain_ns);
    let optimised_median = median(&folded_ns);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "workload": "self-authored constant branches with live variable checks",
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "architecture": std::env::consts::ARCH, "os": std::env::consts::OS,
            "blocks": cli.blocks, "iterations": cli.iterations, "samples": cli.samples,
            "input_bytes": text.len(), "folded_branches": folded.folded_branches, "states_equal": true,
            "baseline_ns": plain_ns, "optimised_ns": folded_ns,
            "baseline_median_ns": baseline_median, "optimised_median_ns": optimised_median,
            "median_speedup": baseline_median as f64 / optimised_median as f64,
            "baseline_compile_ns": plain_compile_ns, "optimised_compile_ns": folded_compile_ns,
            "baseline_plan_owned_heap_capacity_bytes": plan_bytes(&plain.plan),
            "optimised_plan_owned_heap_capacity_bytes": plan_bytes(&folded.plan),
            "memory_scope": "plan Vec, Box and String capacities; excludes allocator overhead, source maps and world",
            "baseline": "same name binding and call inlining; constant folding disabled"
        }))?
    );
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
