//! `factory development wave act|read`: the planning dial and the wave SDLC
//! (`wave_sdlc.rs`) through Factory's command surface.
//!
//! The developmental state is the optional first positional argument, located
//! from the nearest Factory project placement when omitted, as for the other
//! World inhabitation verbs. An act's document rides stdin (`-`) or a file;
//! its actor is the occupancy stamped into the process, or the person's
//! desktop caller and grant in the document. Refusals are three-part: under
//! `--json` the refusal document goes to stdout and the process exits 2.

use crate::core::run::RunRef;
use crate::wave_sdlc::{self, WaveReading, ACT_USAGE, READ_USAGE};
use crate::work_custody::{CustodyActor, Refusal, NOTHING_READ};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn help() -> String {
    format!(
        "Planning dial and waves (OpenRig's planning dial and wave SDLC):\n  {ACT_USAGE}\n  {READ_USAGE}\n                                    each unit's rung, rounds and lock, and each wave's state, gate and next step, derived now"
    )
}

/// True for `development wave ...`.
pub fn is_wave_command(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("development")
        && args.get(1).map(String::as_str) == Some("wave")
}

/// Process entry: `args` start at `development`.
pub fn main(args: &[String]) -> ExitCode {
    let json = args.iter().any(|argument| argument == "--json");
    let stripped = args
        .iter()
        .skip(1)
        .filter(|argument| argument.as_str() != "--json")
        .cloned()
        .collect::<Vec<_>>();
    match execute(&stripped, json) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&refusal).unwrap_or_else(|_| refusal.to_string())
                );
            } else {
                eprintln!("factory: {refusal}");
            }
            ExitCode::from(2)
        }
    }
}

fn usage(fact: impl Into<String>, line: &str) -> Refusal {
    Refusal::new(
        "factory.wave.usage",
        fact,
        "The command did not run; nothing was read or persisted.",
        format!("run: {line}"),
    )
}

/// Execute with `args` after `development` and `--json` removed.
pub fn execute(args: &[String], json: bool) -> Result<String, Refusal> {
    match args.get(1).map(String::as_str) {
        Some("act") => {
            let rest = &args[2..];
            if let Some(flag) = rest.iter().find(|argument| argument.starts_with("--")) {
                return Err(usage(format!("unknown option {flag}"), ACT_USAGE));
            }
            let (state, source) = match rest {
                [source] => (None, source),
                [state, source] => (Some(state), source),
                _ => {
                    return Err(usage(
                        "act takes an optional state and one request (a file, or - for stdin)",
                        ACT_USAGE,
                    ))
                }
            };
            let text = if source == "-" {
                std::io::read_to_string(std::io::stdin())
            } else {
                std::fs::read_to_string(source)
            }
            .map_err(|error| {
                usage(
                    format!("the request {source} could not be read: {error}"),
                    ACT_USAGE,
                )
            })?;
            let request = wave_sdlc::parse_request(&text)?;
            let occupancy = CustodyActor::from_env();
            let actor = wave_sdlc::resolve_actor(&request, occupancy.clone())?;
            // A Position acts only as Actuation's current occupant of it.
            if let Some(occupancy) = occupancy.as_ref() {
                crate::work_custody::confirm_current_generation(occupancy)?;
            }
            let path = state_path(state.map(String::as_str))?;
            let receipt = wave_sdlc::act(&path, request, actor)?;
            render(&receipt, json, || {
                format!(
                    "{} {} {} (plan revision {})",
                    receipt.act, receipt.result, receipt.run_ref, receipt.plan_revision
                )
            })
        }
        Some("read") => {
            let mut state = None;
            let mut run = None;
            let mut index = 2;
            while index < args.len() {
                let argument = &args[index];
                if argument == "--run" {
                    let value = args
                        .get(index + 1)
                        .filter(|value| !value.starts_with("--"))
                        .ok_or_else(|| usage("--run needs a value", READ_USAGE))?;
                    if run.replace(value.clone()).is_some() {
                        return Err(usage("--run was given more than once", READ_USAGE));
                    }
                    index += 1;
                } else if argument.starts_with("--") {
                    return Err(usage(format!("unknown option {argument}"), READ_USAGE));
                } else if state.replace(argument.clone()).is_some() {
                    return Err(usage(
                        "expected at most one developmental state path",
                        READ_USAGE,
                    ));
                }
                index += 1;
            }
            let run = run.ok_or_else(|| usage("--run is required", READ_USAGE))?;
            let run_ref: RunRef = run
                .parse()
                .map_err(|error| usage(format!("--run {run} is not valid: {error}"), READ_USAGE))?;
            let path = state_path(state.as_deref())?;
            let state = crate::project_development_store::read_developmental_state(&path).map_err(
                |error| {
                    let mut refusal = crate::work_custody::state_refusal(&path, error);
                    refusal.consequence = NOTHING_READ.into();
                    refusal
                },
            )?;
            let reading = wave_sdlc::read(&state, &run_ref)?;
            render(&reading, json, || text(&reading))
        }
        _ => Err(usage("wave needs act or read", &help())),
    }
}

fn text(reading: &WaveReading) -> String {
    let mut lines = vec![format!(
        "{} {} (plan revision {})",
        reading.schema, reading.run_ref, reading.plan_revision
    )];
    if let Some(integration) = &reading.integration {
        lines.push(format!(
            "Integrates onto {} in {}: {}",
            integration.onto,
            integration.repo.display(),
            integration.basis
        ));
    }
    for unit in &reading.units {
        lines.push(format!(
            "{} {}{} — {}{}",
            unit.workflow_unit_ref,
            unit.dial.rung.as_str(),
            if unit.dial.lock.is_some() {
                " locked"
            } else {
                ""
            },
            unit.dial.reason,
            if unit.dial.needs.is_empty() {
                String::new()
            } else {
                format!(
                    " (needs {})",
                    unit.dial
                        .needs
                        .iter()
                        .map(|round| round.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        ));
    }
    for wave in &reading.waves {
        lines.push(format!(
            "Wave {} {}: {}. Next: {}",
            wave.wave,
            wave.state.as_str(),
            wave.gate,
            wave.next
        ));
    }
    lines.join("\n")
}

fn render<T: serde::Serialize>(
    value: &T,
    json: bool,
    text: impl FnOnce() -> String,
) -> Result<String, Refusal> {
    if json {
        serde_json::to_string_pretty(value).map_err(|error| {
            Refusal::new(
                "factory.internal",
                format!("the reading could not be serialised: {error}"),
                NOTHING_READ,
                "report this Factory defect with the command that produced it",
            )
        })
    } else {
        Ok(text())
    }
}

/// The explicit state path, or the one the nearest project placement names.
fn state_path(explicit: Option<&str>) -> Result<PathBuf, Refusal> {
    if let Some(path) = explicit {
        return Ok(PathBuf::from(path));
    }
    let cwd = std::env::current_dir().map_err(|error| {
        Refusal::new(
            "factory.state.not_located",
            format!("the working directory could not be read: {error}"),
            NOTHING_READ,
            "pass the developmental state path as the first argument",
        )
    })?;
    let root: &Path = cwd
        .ancestors()
        .find(|dir| dir.join(".factory").join("project.json").is_file())
        .ok_or_else(|| {
            Refusal::new(
                "factory.state.not_located",
                format!(
                    "no Factory project placement (.factory/project.json) was found at or above {}",
                    cwd.display()
                ),
                NOTHING_READ,
                "pass the developmental state path as the first argument",
            )
        })?;
    crate::project_setup::locate(root)
        .ok()
        .and_then(|location| location["statePath"].as_str().map(PathBuf::from))
        .ok_or_else(|| {
            Refusal::new(
                "factory.state.not_located",
                format!(
                    "the Factory placement at {} did not locate a state",
                    root.display()
                ),
                NOTHING_READ,
                format!(
                    "inspect it with `factory project locate {}`",
                    root.display()
                ),
            )
        })
}
