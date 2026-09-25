//! `factory development custody|current-work|inhabitation`.
//!
//! The developmental state is the optional first positional argument, as for
//! every other `factory development` verb. When it is omitted the command
//! locates the nearest Factory project placement (`.factory/project.json`) from
//! the working directory, through the same `factory project locate` reading.
//!
//! Refusals are three-part. Under `--json` the refusal document goes to stdout
//! and the process exits 2 (as every other `factory` failure does); otherwise it
//! goes to stderr. Readings whose outcome
//! is `none` or `ambiguous` are answers, not refusals, and exit 0.

use crate::core::run::{RunRef, WorkflowUnitRef};
use crate::current_work::{derive_current_work, CurrentWorkOutcome};
use crate::inhabitation::inhabitation_reading;
use crate::journey::JourneyRef;
use crate::project_development_store::read_developmental_state;
use crate::work_custody::{
    self, list_in, state_refusal, validate_position_ref, AssignRequest, CustodyListingFilter,
    CustodyReceipt, CustodyState, Refusal, UpdateRequest, ASSIGN_USAGE, NOTHING_READ, UPDATE_USAGE,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const LIST_USAGE: &str = "factory development custody list [<state>] [--position <central:position:…>] [--run <run-ref>] [--state <custody-state>] [--json]";
pub const CURRENT_WORK_USAGE: &str =
    "factory development current-work [<state>] --position <central:position:…> [--json]";
pub const INHABITATION_USAGE: &str = "factory development inhabitation [<state>] [--run <run-ref>] [--position <central:position:…>] [--json]";

pub fn help() -> String {
    format!(
        "World inhabitation (Position custody, current work, occupant relations):\n  {ASSIGN_USAGE}\n  {UPDATE_USAGE}\n  {LIST_USAGE}\n  {CURRENT_WORK_USAGE}\n  {INHABITATION_USAGE}\nWithout <state>, the nearest Factory project placement above the working directory is used."
    )
}

/// True for the command heads this module owns (after the `factory` binary).
pub fn is_inhabitation_command(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("development")
        && matches!(
            args.get(1).map(String::as_str),
            Some("custody" | "current-work" | "inhabitation")
        )
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

struct Parsed {
    positional: Vec<String>,
    values: BTreeMap<&'static str, String>,
    reopen: bool,
    usage: String,
}

const DID_NOT_RUN: &str = "The command did not run; nothing was read or persisted.";

fn usage_for(fact: impl Into<String>, usage: &str) -> Refusal {
    Refusal::new(
        "factory.custody.usage",
        fact,
        DID_NOT_RUN,
        format!("run: {usage}"),
    )
}

fn parse(
    args: &[String],
    allowed: &[&'static str],
    allow_reopen: bool,
    usage_line: &str,
) -> Result<Parsed, Refusal> {
    let usage = |fact: String| usage_for(fact, usage_line);
    let mut parsed = Parsed {
        positional: Vec::new(),
        values: BTreeMap::new(),
        reopen: false,
        usage: usage_line.to_owned(),
    };
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--reopen" && allow_reopen {
            parsed.reopen = true;
        } else if let Some(flag) = allowed.iter().find(|flag| **flag == argument.as_str()) {
            let value = args
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| usage(format!("{flag} needs a value")))?;
            if parsed.values.insert(flag, value.clone()).is_some() {
                return Err(usage(format!("{flag} was given more than once")));
            }
            index += 1;
        } else if argument.starts_with("--") {
            return Err(usage(format!("unknown option {argument}")));
        } else {
            parsed.positional.push(argument.clone());
        }
        index += 1;
    }
    if parsed.positional.len() > 1 {
        return Err(usage(format!(
            "expected at most one developmental state path, got {}",
            parsed.positional.join(" ")
        )));
    }
    Ok(parsed)
}

impl Parsed {
    fn get(&self, flag: &str) -> Option<&String> {
        self.values.get(flag)
    }

    fn required(&self, flag: &str) -> Result<String, Refusal> {
        self.get(flag)
            .cloned()
            .ok_or_else(|| usage_for(format!("{flag} is required"), &self.usage))
    }

    fn reference<T: std::str::FromStr>(&self, flag: &str) -> Result<Option<T>, Refusal>
    where
        T::Err: std::fmt::Display,
    {
        self.get(flag)
            .map(|value| {
                value.parse::<T>().map_err(|error| {
                    usage_for(format!("{flag} {value} is not valid: {error}"), &self.usage)
                })
            })
            .transpose()
    }

    fn custody_state(&self, flag: &str) -> Result<Option<CustodyState>, Refusal> {
        self.get(flag)
            .map(|value| {
                CustodyState::parse(value).ok_or_else(|| {
                    usage_for(
                        format!("{flag} {value} is not a custody state; use in-progress, blocked, released, completed or handed-off"),
                        &self.usage,
                    )
                })
            })
            .transpose()
    }

    fn required_position(&self, flag: &str) -> Result<String, Refusal> {
        self.required(flag)?;
        Ok(self.position(flag)?.unwrap_or_default())
    }

    fn position(&self, flag: &str) -> Result<Option<String>, Refusal> {
        self.get(flag)
            .map(|position| {
                validate_position_ref(position).map_err(|fact| {
                    Refusal::new(
                        "factory.custody.invalid_position",
                        fact,
                        DID_NOT_RUN,
                        format!(
                            "pass the Position's ref as Central defines it, then run: {}",
                            self.usage
                        ),
                    )
                })?;
                Ok(position.clone())
            })
            .transpose()
    }
}

/// The explicit state path, or the one the nearest project placement names.
fn state_path(parsed: &Parsed) -> Result<PathBuf, Refusal> {
    if let Some(path) = parsed.positional.first() {
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
    let root = cwd
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
                "pass the developmental state path as the first argument, or run inside a project set up with `factory project setup <root> <key>`",
            )
        })?;
    let location = crate::project_setup::locate(root).map_err(|error| {
        Refusal::new(
            "factory.state.not_located",
            format!(
                "the Factory placement at {} did not locate a valid state: {error}",
                root.display()
            ),
            NOTHING_READ,
            format!(
                "inspect it with `factory project locate {}`",
                root.display()
            ),
        )
    })?;
    location["statePath"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| {
            Refusal::new(
                "factory.state.not_located",
                "the Factory placement reading carried no statePath",
                NOTHING_READ,
                format!(
                    "inspect it with `factory project locate {}`",
                    root.display()
                ),
            )
        })
}

fn read_state(
    path: &Path,
) -> Result<crate::developmental_read::FactoryDevelopmentalState, Refusal> {
    read_developmental_state(path).map_err(|error| {
        let mut refusal = state_refusal(path, &error);
        refusal.consequence = NOTHING_READ.into();
        refusal
    })
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

fn receipt_text(receipt: &CustodyReceipt) -> String {
    let record = &receipt.custody;
    let mut lines = vec![
        format!("{} ({})", receipt.schema, receipt.result),
        format!(
            "Custody: {} {} for {} (revision {})",
            record.custody_ref, record.state, record.position_ref, record.revision
        ),
        format!("Work: {}", record.work_ref),
    ];
    if let Some(run) = &record.run_ref {
        lines.push(format!(
            "Run: {run}{}",
            record
                .workflow_unit_ref
                .as_ref()
                .map(|unit| format!(" / {unit}"))
                .unwrap_or_default()
        ));
    }
    if let Some(next) = &receipt.successor {
        lines.push(format!(
            "Handed off to: {} as {}",
            next.position_ref, next.custody_ref
        ));
    }
    lines.join("\n")
}

/// Execute with `args` after `development` and `--json` already removed.
pub fn execute(args: &[String], json: bool) -> Result<String, Refusal> {
    match args.first().map(String::as_str) {
        Some("custody") => match args.get(1).map(String::as_str) {
            Some("assign") => {
                let parsed = parse(
                    &args[2..],
                    &[
                        "--position",
                        "--work",
                        "--reason",
                        "--run",
                        "--journey",
                        "--workflow-unit",
                        "--origin-communique",
                        "--state",
                        "--custody",
                    ],
                    false,
                    ASSIGN_USAGE,
                )?;
                let request = AssignRequest {
                    position_ref: parsed.required_position("--position")?,
                    work_ref: parsed.required("--work")?,
                    reason: parsed.required("--reason")?,
                    run_ref: parsed.reference::<RunRef>("--run")?,
                    journey_ref: parsed.reference::<JourneyRef>("--journey")?,
                    workflow_unit_ref: parsed.reference::<WorkflowUnitRef>("--workflow-unit")?,
                    origin_communique_ref: parsed.get("--origin-communique").cloned(),
                    initial_state: parsed.custody_state("--state")?,
                    custody_ref: parsed.get("--custody").cloned(),
                };
                let receipt = work_custody::assign(&state_path(&parsed)?, request)?;
                render(&receipt, json, || receipt_text(&receipt))
            }
            Some("update") => {
                let parsed = parse(
                    &args[2..],
                    &[
                        "--custody",
                        "--state",
                        "--reason",
                        "--to-position",
                        "--expected-revision",
                    ],
                    true,
                    UPDATE_USAGE,
                )?;
                parsed.required("--state")?;
                let actor = work_custody::CustodyActor::from_env().ok_or_else(|| {
                    work_custody::Refusal::unchanged(
                        "factory.custody.actor_required",
                        "a custody change with no occupancy stamp is not an operator",
                        "inhabit the holding Position and retry from that body",
                    )
                })?;
                if actor
                    .generation_ref
                    .as_deref()
                    .map(str::trim)
                    .filter(|generation| !generation.is_empty())
                    .is_none()
                {
                    return Err(work_custody::Refusal::unchanged(
                        "factory.custody.generation_required",
                        format!(
                            "this body names {} but carries no occupant generation",
                            actor.position_ref
                        ),
                        "inhabit so OI_OCCUPANT_GENERATION is set, then retry",
                    ));
                }
                let request = UpdateRequest {
                    custody_ref: parsed.required("--custody")?,
                    state: parsed.custody_state("--state")?,
                    reason: parsed.required("--reason")?,
                    reopen: parsed.reopen,
                    to_position_ref: parsed.position("--to-position")?,
                    expected_revision: parsed
                        .get("--expected-revision")
                        .map(|value| {
                            value.parse::<u64>().map_err(|_| {
                                usage_for(
                                    format!("--expected-revision {value} is not a revision number"),
                                    UPDATE_USAGE,
                                )
                            })
                        })
                        .transpose()?,
                    actor: Some(actor),
                };
                let receipt = work_custody::update(&state_path(&parsed)?, request)?;
                render(&receipt, json, || receipt_text(&receipt))
            }
            Some("list") => {
                let parsed = parse(
                    &args[2..],
                    &["--position", "--run", "--state"],
                    false,
                    LIST_USAGE,
                )?;
                let filter = CustodyListingFilter {
                    position_ref: parsed.position("--position")?,
                    run_ref: parsed
                        .reference::<RunRef>("--run")?
                        .map(|run| run.to_string()),
                    state: parsed.custody_state("--state")?,
                };
                let listing = list_in(&read_state(&state_path(&parsed)?)?, filter);
                render(&listing, json, || {
                    let mut lines =
                        vec![format!("{} ({} record(s))", listing.schema, listing.count)];
                    lines.extend(listing.custody.iter().map(|record| {
                        format!(
                            "{} {} {} work={} r{}",
                            record.custody_ref,
                            record.state,
                            record.position_ref,
                            record.work_ref,
                            record.revision
                        )
                    }));
                    lines.join("\n")
                })
            }
            _ => Err(usage_for(
                "custody needs one of assign, update or list",
                &help(),
            )),
        },
        Some("current-work") => {
            let parsed = parse(&args[1..], &["--position"], false, CURRENT_WORK_USAGE)?;
            let position = parsed.required_position("--position")?;
            let reading = derive_current_work(&read_state(&state_path(&parsed)?)?, &position);
            render(&reading, json, || {
                let current = match (&reading.outcome, &reading.current) {
                    (CurrentWorkOutcome::One, Some(node)) => node.node_ref.clone(),
                    _ => "(none)".into(),
                };
                format!(
                    "{}\nPosition: {}\nOutcome: {}\nCurrent: {current}\nCandidates: {}\nConsidered: {}\nBasis: {}",
                    reading.schema,
                    reading.position_ref,
                    serde_json::to_value(reading.outcome)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    reading.candidates.len(),
                    reading.considered,
                    reading.basis
                )
            })
        }
        Some("inhabitation") => {
            let parsed = parse(
                &args[1..],
                &["--run", "--position"],
                false,
                INHABITATION_USAGE,
            )?;
            let run = parsed.reference::<RunRef>("--run")?;
            let position_filter = parsed.position("--position")?;
            let state = read_state(&state_path(&parsed)?)?;
            let reading = inhabitation_reading(&state, run.as_ref(), position_filter.as_deref())?;
            render(&reading, json, || {
                let mut lines = vec![format!(
                    "{}\nProject: {}\nRuns: {}",
                    reading.schema,
                    reading.project_ref,
                    reading.runs.len()
                )];
                for run in &reading.runs {
                    lines.push(format!(
                        "Run {} ({} Position(s), {} custody, {} occupant attempt(s))",
                        run.run_ref,
                        run.positions.len(),
                        run.custody.len(),
                        run.occupants.len()
                    ));
                }
                if !reading.custody_outside_runs.is_empty() {
                    lines.push(format!(
                        "Custody outside Runs: {}",
                        reading.custody_outside_runs.len()
                    ));
                }
                lines.join("\n")
            })
        }
        _ => Err(usage_for(
            "expected custody, current-work or inhabitation",
            &help(),
        )),
    }
}
