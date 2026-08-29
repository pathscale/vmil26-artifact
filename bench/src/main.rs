//! `cargo run --release` writes `results.json` and prints a summary.
//!
//! The summary is derived from the same structure that is serialised, so the
//! terminal output and the JSON cannot disagree. It leads with the crossover
//! ratio and the worst configuration for the native binding, in that order,
//! because those are the two things a reader most needs and the two an author
//! is most tempted to bury.

use dom_boundary_bench::counts::Derivation;
use dom_boundary_bench::report::Report;
use dom_boundary_bench::runner;

fn main() {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "results.json".to_string());

    eprintln!("Building the wasm guest and warming up.");

    let report = runner::run_all();

    let json = serde_json::to_string_pretty(&report).expect("the report should serialise");
    std::fs::write(&output, format!("{json}\n"))
        .unwrap_or_else(|err| panic!("could not write {output}: {err}"));

    print_summary(&report);
    eprintln!("\nWrote {output}");
}

fn rule() -> String {
    "=".repeat(94)
}

fn print_summary(report: &Report) {
    println!("\ndom-boundary-bench");
    println!("{}", rule());
    println!("{}", report.machine.chip);
    println!(
        "{} / {} / {} profile",
        report.machine.os, report.machine.rustc, report.machine.profile
    );
    if let Some(commit) = &report.machine.engine.commit {
        println!(
            "engine {} @ {}{}",
            report.machine.engine.branch.as_deref().unwrap_or("?"),
            &commit[..commit.len().min(12)],
            if report.machine.engine.dirty {
                " (dirty)"
            } else {
                ""
            }
        );
    }

    print_crossover(report);
    print_sweep(report);
    print_upstream(report);
    print_gaps(report);

    println!("\n{}", rule());
    println!("Counts are the result. Timings are secondary and machine-local.");
    println!("Arm B was not run: every arm-B number is statically derived.");
}

fn print_crossover(report: &Report) {
    let crossover = &report.crossover;
    println!("\n{}", rule());
    println!("THE CROSSOVER");
    println!("{}", rule());
    println!("{}", wrap(&crossover.headline, 92, ""));

    println!("\n  Machine-independent, needing no exchange rate:");
    println!("{}", wrap(&crossover.machine_independent, 90, "  "));

    println!("\n  What the two estimates disagreeing means:");
    println!("{}", wrap(&crossover.interpretation, 90, "  "));

    println!("\n  Observed, straight off the vocabulary ladder:");
    for observed in &crossover.observed {
        match observed.crossover_ratio {
            Some(ratio) => println!("    {:<6} names:  R = {:.0}", observed.name_length, ratio),
            None => println!(
                "    {:<6} names:  no crossing on this ladder",
                observed.name_length
            ),
        }
        println!("{}", wrap(&observed.explanation, 88, "      "));
    }

    let rate = &crossover.contrasts;
    println!("\n  Exchange rate from controlled contrasts (this machine only):");
    match rate.ns_per_crossing {
        Some(value) => println!(
            "    one crossing: {value:.0} ns   (from {} pair(s) differing only in call count)",
            rate.crossing_pairs.len()
        ),
        None => println!("    one crossing: not measurable"),
    }
    match rate.ns_per_byte {
        Some(value) => println!(
            "    one byte:     {value:.4} ns  (from {} pair(s) differing only in bytes)",
            rate.byte_pairs.len()
        ),
        None => println!(
            "    one byte:     below the noise floor across {} pair(s)",
            rate.byte_pairs.len()
        ),
    }
    match rate.bytes_per_crossing {
        Some(k) => println!(
            "    k = {k:.0} bytes per crossing{}",
            if rate.k_is_lower_bound {
                " (lower bound)"
            } else {
                ""
            }
        ),
        None => println!("    k = not estimable"),
    }
    println!("{}", wrap(&rate.caveat, 88, "    "));

    for threshold in &crossover.thresholds {
        match threshold.crossover_ratio {
            Some(ratio) => println!(
                "    modelled R* for {:<5} names ({:>3.0} bytes): {:.0}{}",
                threshold.name_length,
                threshold.mean_name_bytes,
                ratio,
                if threshold.is_lower_bound {
                    " (lower bound)"
                } else {
                    ""
                }
            ),
            None => println!(
                "    modelled R* for {:<5} names ({:>3.0} bytes): not estimable",
                threshold.name_length, threshold.mean_name_bytes
            ),
        }
    }

    // The worst case first, and in the same weight as the best. A benchmark
    // whose author only publishes favourable points is worthless.
    println!("\n  WORST configuration for the native binding:");
    match &crossover.worst_for_native {
        Some(worst) => print_observation(worst),
        None => println!("    none recorded"),
    }
    println!("\n  Best configuration for the native binding:");
    match &crossover.best_for_native {
        Some(best) => print_observation(best),
        None => println!("    none recorded"),
    }

    if crossover.native_losses.is_empty() {
        println!(
            "\n  The native binding did not lose outright at any configuration in this sweep."
        );
        println!(
            "{}",
            wrap(
                "Read that as a limit on the sweep, not as a property of the ABI. The two \
                 axes most likely to reverse the result, read-heavy workloads and structural \
                 churn, cannot be run at all: see the gaps below.",
                90,
                "  "
            )
        );
    } else {
        println!(
            "\n  The native binding LOST at {} configuration(s):",
            crossover.native_losses.len()
        );
        for loss in &crossover.native_losses {
            println!("    {loss}");
        }
    }
}

fn print_observation(observation: &dom_boundary_bench::crossover::Observation) {
    println!("    {}  [{}]", observation.id, observation.axis);
    println!(
        "      repetition ratio {:.2}  ({} uses / {} distinct names, {:.0} bytes each)",
        observation.repetition_ratio,
        observation.name_uses,
        observation.distinct_names,
        observation.mean_name_bytes
    );
    println!(
        "      spent {} crossings to save {} bytes",
        observation.crossings_spent, observation.bytes_saved
    );
    match observation.time_advantage {
        Some(advantage) => println!(
            "      margin: {:.2}x on bytes, {:.2}x on time",
            observation.bytes_advantage, advantage
        ),
        None => println!("      margin: {:.2}x on bytes", observation.bytes_advantage),
    }
}

fn print_sweep(report: &Report) {
    println!("\n{}", rule());
    println!("THE SWEEP  (arm A = Boa, arm C = native binding; counts are machine-independent)");
    println!("{}", rule());
    println!(
        "{:<26} {:>6} {:>9} {:>9} {:>9} {:>9} {:>7}",
        "configuration", "ratio", "A bytes", "C bytes", "A cross", "C cross", "C:A t"
    );
    println!("{}", "-".repeat(94));

    let mut axis = "";
    for entry in &report.sweep {
        if entry.axis != axis {
            axis = entry.axis;
            println!("  -- {axis} --");
        }
        let arm = |which: &str| entry.report.arms.iter().find(|a| a.arm == which);
        let (Some(a), Some(c)) = (arm("A"), arm("C")) else {
            continue;
        };
        let ratio = match (&a.timing, &c.timing) {
            (Some(ta), Some(tc)) if tc.median_ms > 0.0 => {
                format!("{:.2}x", ta.median_ms / tc.median_ms)
            }
            _ => "-".to_string(),
        };
        println!(
            "{:<26} {:>6.1} {:>9} {:>9} {:>9} {:>9} {:>7}",
            truncate(&entry.id, 26),
            entry.report.repetition_ratio,
            a.counts.bytes_copied,
            c.counts.bytes_copied,
            a.counts.boundary_crossings,
            c.counts.boundary_crossings,
            ratio,
        );
    }

    let identical = report
        .sweep
        .iter()
        .all(|entry| entry.report.trees_identical);
    println!(
        "\n  trees identical across arms at every configuration: {}",
        if identical { "yes" } else { "NO" }
    );
}

fn print_upstream(report: &Report) {
    println!("\n{}", rule());
    println!("UPSTREAM BASELINE  (the cited js-framework-benchmark workload)");
    println!("{}", rule());
    for operation in &report.operations {
        println!(
            "\n{}  (repetition ratio {:.1}, {} distinct vocabulary strings)",
            operation.name, operation.repetition_ratio, operation.vocabulary_strings
        );
        println!(
            "  {:<44} {:>9} {:>9} {:>10}",
            "arm", "dom ops", "crossings", "bytes"
        );
        for arm in &operation.arms {
            println!(
                "  {:<44} {:>9} {:>9} {:>10}{}",
                truncate(&format!("{} {}", arm.arm, arm.description), 44),
                arm.counts.dom_operations,
                arm.counts.boundary_crossings,
                arm.counts.bytes_copied,
                derived_marker(arm.counts.derivation),
            );
        }
        for arm in &operation.arms {
            if let Some(samples) = &arm.timing {
                println!(
                    "    {:<42} {:>8.2} ms  IQR {:.2} ms{}",
                    truncate(arm.description, 42),
                    samples.median_ms,
                    samples.iqr_ms,
                    if samples.drift.significant {
                        format!("  DRIFT {:+.0}%", samples.drift.shift_percent)
                    } else {
                        String::new()
                    }
                );
            }
        }
    }
}

fn print_gaps(report: &Report) {
    println!("\n{}", rule());
    println!("AXES THAT CANNOT BE RUN  (named, not omitted)");
    println!("{}", rule());
    for gap in &report.unmeasured_axes {
        println!("\n  {}", gap.axis.to_uppercase());
        println!("{}", wrap(gap.what_blocks_it, 90, "    "));
        println!("    operations it would need:");
        for operation in &gap.operations_required {
            println!("      - {operation}");
        }
        println!("    expected finding:");
        println!("{}", wrap(gap.expected_finding, 90, "      "));
    }
}

fn derived_marker(derivation: Derivation) -> &'static str {
    match derivation {
        Derivation::Measured => "",
        Derivation::StaticAnalysis => "  [derived]",
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.len() <= width {
        text.to_string()
    } else {
        format!("{}...", &text[..width.saturating_sub(3)])
    }
}

/// Wrap prose to a width, with a fixed indent. The summary carries whole
/// paragraphs of caveat, and a caveat that runs off the edge of a terminal is a
/// caveat nobody reads.
fn wrap(text: &str, width: usize, indent: &str) -> String {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(format!("{indent}{line}"));
            line = String::new();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(format!("{indent}{line}"));
    }
    lines.join("\n")
}
