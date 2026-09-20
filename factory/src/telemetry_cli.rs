//! The `factory telemetry` command family: an ordinary operator and Agent
//! interface over the developmental state Factory already owns.
//!
//! These commands extend the real services — the execution correlations, the
//! native attempt store and the development ledger — rather than importing a
//! demonstration telemetry set. Content search delegates to AIKit's knowledge
//! service (which now includes the NOW-field ripgrep fast path); Factory
//! enriches the hits it can honestly relate to runs, correlations and days,
//! and says plainly when a hit has no Factory ancestry.
//!
//! Exit semantics: usage errors exit 2 through the ordinary CLI error path.
//! Every substantive answer — including "the provider is unavailable" — is a
//! structured, versioned document on stdout, because a monitoring surface that
//! dies loudly is useless to the consumers it serves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::CliError;
use crate::core::identity::Ref;
use crate::developmental_read::{
    FactoryDevelopmentalFileProvider, FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER,
};

pub const FACTORY_TELEMETRY_STATUS_CONTRACT: &str = "factory.telemetry-status/v1";
pub const FACTORY_TELEMETRY_INSPECT_CONTRACT: &str = "factory.telemetry-inspection/v1";
pub const FACTORY_TELEMETRY_SEARCH_CONTRACT: &str = "factory.telemetry-search/v1";
pub const FACTORY_TELEMETRY_STATS_CONTRACT: &str = "factory.telemetry-stats/v1";
pub const FACTORY_TELEMETRY_DOCTOR_CONTRACT: &str = "factory.telemetry-doctor/v1";
pub const FACTORY_TELEMETRY_COMPARE_CONTRACT: &str = "factory.telemetry-compare/v1";

/// Hard bounds: a monitoring surface must not become an unbounded scan.
const SEARCH_DEFAULT_LIMIT: usize = 10;
const SEARCH_MAX_LIMIT: usize = 100;
const DOCTOR_NOW_REF_PROBES: usize = 5;
/// The AIKit knowledge pipeline rebuilds its runtime per operation when an
/// owner horizon is present; on a real ground that costs seconds, not millis.
const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(120);

pub fn execute(args: &[String], json: bool) -> Result<String, CliError> {
    let operation = args.first().ok_or_else(|| {
        CliError::new(
            "missing telemetry operation; expected status|inspect|search|stats|export|doctor",
        )
    })?;
    let rest = &args[1..];
    match operation.as_str() {
        "status" => status(rest, json),
        "inspect" => inspect(rest, json),
        "search" => search(rest, json),
        "stats" => stats(rest, json),
        "watch" => watch(rest, json),
        "compare" => compare(rest, json),
        "export" => export(rest, json),
        "doctor" => doctor(rest, json),
        other => Err(CliError::new(format!(
            "unknown telemetry operation `{other}`; expected status|inspect|search|stats|watch|compare|export|doctor"
        ))),
    }
}

fn require_state_path(args: &[String]) -> Result<PathBuf, CliError> {
    let raw = args
        .first()
        .ok_or_else(|| CliError::new("missing state path"))?;
    let path = PathBuf::from(raw);
    if !path.exists() {
        return Err(CliError::new(format!(
            "state file {} does not exist; pass the developmental provider state explicitly",
            path.display()
        )));
    }
    Ok(path)
}

fn render(
    document: &Value,
    json: bool,
    human: impl FnOnce() -> String,
) -> Result<String, CliError> {
    if json {
        serde_json::to_string_pretty(document).map_err(|e| CliError::new(e.to_string()))
    } else {
        Ok(human())
    }
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

fn status(args: &[String], json: bool) -> Result<String, CliError> {
    let state_path = require_state_path(args)?;
    let provider = FactoryDevelopmentalFileProvider::open(&state_path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let state = provider.state();
    let correlations = &state.execution_correlations;
    let attempts: usize = state
        .attempt_states
        .values()
        .map(|run| run.attempts().len())
        .sum();
    let with_child_now = correlations
        .iter()
        .filter(|c| c.temporal.child_now_ref.is_some())
        .count();
    let with_day_refs = correlations
        .iter()
        .filter(|c| !c.temporal.day_refs.is_empty())
        .count();
    let with_git_basis = correlations
        .iter()
        .filter(|c| c.git_basis.is_some())
        .count();
    let document = json!({
        "contract": FACTORY_TELEMETRY_STATUS_CONTRACT,
        "state": state_path.to_string_lossy(),
        "schema": state.schema,
        "counts": {
            "executionCorrelations": correlations.len(),
            "attempts": attempts,
            "attemptedRuns": state.attempt_states.len(),
            "journeys": state.journeys.len(),
            "commissions": state.commissions.len(),
            "runs": state.build.run_count(),
        },
        "correlationCompleteness": {
            "withChildNowRef": with_child_now,
            "withDayRefs": with_day_refs,
            "withGitBasis": with_git_basis,
            "total": correlations.len(),
        },
        "tools": tool_presence(),
    });
    render(&document, json, || {
        format!(
            "{}\nState: {}\nCorrelations: {} (child-NOW {}, day-refs {}, git-basis {})\nAttempts: {} across {} run(s)\nTools: ctrl {} · aikit {} · rg {}",
            FACTORY_TELEMETRY_STATUS_CONTRACT,
            state_path.display(),
            correlations.len(),
            with_child_now,
            with_day_refs,
            with_git_basis,
            attempts,
            state.attempt_states.len(),
            yes_no(tool_presence()["ctrl"]),
            yes_no(tool_presence()["aikit"]),
            yes_no(tool_presence()["rg"]),
        )
    })
}

fn yes_no(present: bool) -> &'static str {
    if present {
        "present"
    } else {
        "absent"
    }
}

fn tool_presence() -> BTreeMap<&'static str, bool> {
    BTreeMap::from([
        ("ctrl", probe("ctrl", &["--help"])),
        ("aikit", probe("aikit", &["--version"])),
        ("rg", probe("rg", &["--version"])),
    ])
}

fn probe(executable: &str, args: &[&str]) -> bool {
    Command::new(executable)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttemptSummarySource {
    attempt_ref: String,
    task_ref: String,
    workflow_unit_ref: Value,
    #[serde(default)]
    execution_ref: Option<String>,
    reserved_execution_ref: String,
    #[serde(default)]
    readable_return: Option<Value>,
}

fn inspect(args: &[String], json: bool) -> Result<String, CliError> {
    let state_path = require_state_path(args)?;
    let telemetry_ref: Ref = args
        .get(1)
        .ok_or_else(|| CliError::new("missing telemetry-ref"))?
        .parse()
        .map_err(|error| CliError::new(format!("invalid telemetry-ref: {error}")))?;
    let provider = FactoryDevelopmentalFileProvider::open(&state_path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let reading = match provider.execution_telemetry_reading(&telemetry_ref) {
        Ok(reading) => reading,
        Err(error) => {
            // A recorded correlation whose owner records are not yet
            // materialised (no admitted Agency / execution in this state) is
            // genuine missing correlation, not a broken command: say exactly
            // what is missing and return the recorded correlation.
            let degraded = provider
                .state()
                .execution_correlations
                .iter()
                .find(|correlation| correlation.telemetry_ref == telemetry_ref)
                .map(|correlation| serde_json::to_value(correlation).unwrap_or(Value::Null));
            let Some(correlation) = degraded else {
                return Err(CliError::new(error.to_string()));
            };
            let document = json!({
                "contract": FACTORY_TELEMETRY_INSPECT_CONTRACT,
                "state": state_path.to_string_lossy(),
                "status": "correlation-recorded-owners-pending",
                "correlation": correlation,
                "absences": [error.to_string()],
            });
            return render(&document, json, || {
                format!(
                    "{}\ncorrelation recorded; owner records pending:\n  {}\nTelemetry: {}\nChild NOW: {}\nDay refs: {}\nGit basis recorded: {}",
                    FACTORY_TELEMETRY_INSPECT_CONTRACT,
                    error,
                    telemetry_ref,
                    correlation["temporal"]["childNowRef"]["reference"].as_str().unwrap_or("(none)"),
                    correlation["temporal"]["dayRefs"].as_array().map(|v| v.len()).unwrap_or(0),
                    !correlation["gitBasis"].is_null(),
                )
            });
        }
    };
    let mut reading_value =
        serde_json::to_value(&reading).map_err(|e| CliError::new(e.to_string()))?;

    // Join the native attempt store: the attempts of the same run whose
    // execution or workflow unit matches this correlation.
    let run_attempts = provider
        .state()
        .attempt_states
        .iter()
        .find(|(run_ref, _)| **run_ref == reading.run_ref)
        .map(|(_, run_attempts)| run_attempts);
    let related: Vec<Value> = match run_attempts {
        Some(run_attempts) if !run_attempts.attempts().is_empty() => {
            let serialised: BTreeMap<String, AttemptSummarySource> = run_attempts
                .attempts()
                .iter()
                .filter_map(|(attempt_ref, record)| {
                    let raw = serde_json::to_value(record).ok()?;
                    let parsed: AttemptSummarySource = serde_json::from_value(raw).ok()?;
                    (parsed.attempt_ref == *attempt_ref).then(|| (attempt_ref.clone(), parsed))
                })
                .collect();
            serialised
                .into_iter()
                .filter(|(_, attempt)| {
                    attempt.execution_ref.as_deref() == Some(reading.execution_ref.as_str())
                        || attempt.reserved_execution_ref == reading.execution_ref
                        || attempt.workflow_unit_ref
                            == serde_json::to_value(&reading.workflow_unit_ref)
                                .unwrap_or(Value::Null)
                })
                .map(|(attempt_ref, attempt)| {
                    json!({
                        "attemptRef": attempt_ref,
                        "taskRef": attempt.task_ref,
                        "executionRef": attempt.execution_ref,
                        "returned": attempt.readable_return.is_some(),
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    };
    reading_value["relatedAttempts"] = json!(related);
    let document = json!({
        "contract": FACTORY_TELEMETRY_INSPECT_CONTRACT,
        "state": state_path.to_string_lossy(),
        "reading": reading_value,
    });

    render(&document, json, || {
        let temporal = &reading.temporal;
        format!(
            "{}\nTelemetry: {}\nRun: {} · Unit: {} · Execution: {}\nAgency: {}\nChild NOW: {}\nDay refs: {}\nGit basis: {}\nReturned evidence: {}\nRelated attempts: {}",
            FACTORY_TELEMETRY_INSPECT_CONTRACT,
            reading.telemetry_ref,
            reading.run_ref,
            reading.workflow_unit_ref,
            reading.execution_ref,
            reading.condition.agency_ref,
            temporal
                .child_now_ref
                .as_ref()
                .map(|r| r.reference.as_str())
                .unwrap_or("(none recorded)"),
            temporal.day_refs.len(),
            reading
                .git_basis
                .as_ref()
                .map(|basis| format!(
                    "{} @ {}{}",
                    basis.repository,
                    basis.base_head,
                    basis
                        .branch
                        .as_ref()
                        .map(|branch| format!(" ({branch})"))
                        .unwrap_or_default()
                ))
                .unwrap_or_else(|| "(none recorded)".into()),
            reading.return_state.evidence_refs.len(),
            related.len(),
        )
    })
}

// ---------------------------------------------------------------------------
// search — delegated discovery, Factory-enriched relations
// ---------------------------------------------------------------------------

fn search(args: &[String], json: bool) -> Result<String, CliError> {
    let mut rest = args.to_vec();
    let mut regex = false;
    let mut aikit_bin: Option<String> = None;
    let mut limit: Option<usize> = None;
    let mut positional = Vec::new();
    let mut iterator = rest.into_iter();
    while let Some(arg) = iterator.next() {
        match arg.as_str() {
            "--regex" => regex = true,
            "--aikit" => aikit_bin = iterator.next(),
            "--limit" => {
                limit = Some(
                    iterator
                        .next()
                        .and_then(|value| value.parse().ok())
                        .ok_or_else(|| CliError::new("--limit requires a number"))?,
                );
            }
            other => positional.push(other.to_string()),
        }
    }
    rest = positional;
    let state_path = require_state_path(&rest)?;
    // Effective settings from the configuration plane's sidecar are the
    // defaults; explicit flags always win.
    let effective = crate::configuration::read_telemetry_effective_all(&state_path);
    let configured_limit = effective
        .get(crate::configuration::TELEMETRY_SEARCH_LIMIT_SETTING)
        .copied()
        .map(|n| n as usize);
    let limit = limit
        .or(configured_limit)
        .map(|n| n.min(SEARCH_MAX_LIMIT))
        .unwrap_or(SEARCH_DEFAULT_LIMIT);
    let query = rest
        .get(1)
        .cloned()
        .ok_or_else(|| CliError::new("missing search query"))?;

    let provider = FactoryDevelopmentalFileProvider::open(&state_path)
        .map_err(|error| CliError::new(error.to_string()))?;

    let aikit = aikit_bin
        .or_else(|| std::env::var("FACTORY_AIKIT_BIN").ok())
        .unwrap_or_else(|| "aikit".into());
    let mut argv = vec![aikit.clone()];
    // Anchor the delegated search at the ground root when it can be found:
    // NOW-field search's natural scope is the whole field, and a project
    // working directory would additionally trigger the per-repo code index,
    // which is not what a provenance question needs.
    if let Some(central_root) = central_root_for(&state_path) {
        argv.push("--cwd".into());
        argv.push(central_root.to_string_lossy().into_owned());
    }
    argv.extend([
        "--json".into(),
        "knowledge".into(),
        "search".into(),
        query.clone(),
        "--limit".into(),
        limit.to_string(),
    ]);
    if regex {
        // AIKit's knowledge contract searches literal by default; the explicit
        // regex path is a deliberate caller decision mirrored flag-for-flag.
        argv.push("--regex".into());
    }
    let timeout_secs = effective
        .get(crate::configuration::TELEMETRY_SEARCH_TIMEOUT_SETTING)
        .copied()
        .unwrap_or(SUBPROCESS_TIMEOUT.as_secs_f64());
    let output = run_command_with_timeout(&argv, Duration::from_secs_f64(timeout_secs));
    let (hits, absences, provider_status) = match output {
        Ok(stdout) => match parse_knowledge_search(&stdout) {
            Ok(parsed) => parsed,
            Err(error) => (
                Vec::new(),
                vec![format!("aikit returned an unreadable search document: {error}")],
                "unreadable".to_string(),
            ),
        },
        Err(error) => (
            Vec::new(),
            vec![format!(
                "AIKit knowledge search unavailable via `{aikit}`: {error}; remedy: install ai-kit's aikit or pass --aikit <path>"
            )],
            "unavailable".to_string(),
        ),
    };

    let enriched: Vec<Value> = hits
        .iter()
        .take(limit)
        .map(|hit| {
            let relations = relate_hit_to_state(provider.state(), hit);
            json!({
                "resource": hit.resource,
                "label": hit.label,
                "snippet": hit.snippet,
                "provider": hit.provider,
                "factoryRelations": relations,
            })
        })
        .collect();
    let status = if provider_status == "ok" {
        "ok"
    } else {
        "provider-unavailable"
    };
    let document = json!({
        "contract": FACTORY_TELEMETRY_SEARCH_CONTRACT,
        "status": if enriched.is_empty() && provider_status == "ok" { "no-match" } else { status },
        "query": query,
        "literal": !regex,
        "aikit": aikit,
        "hits": enriched,
        "absences": absences,
    });
    render(&document, json, || {
        if provider_status != "ok" {
            return format!(
                "{}\nAIKit search unavailable: {}",
                FACTORY_TELEMETRY_SEARCH_CONTRACT,
                absences.first().map(String::as_str).unwrap_or("unknown")
            );
        }
        let mut text = format!(
            "{}\nQuery: {:?} ({} matches)\n",
            FACTORY_TELEMETRY_SEARCH_CONTRACT,
            query,
            enriched.len()
        );
        for hit in &enriched {
            text.push_str(&format!(
                "- {} — {}\n",
                hit["resource"].as_str().unwrap_or_default(),
                hit["snippet"].as_str().unwrap_or_default()
            ));
            let runs = hit["factoryRelations"]["runs"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !runs.is_empty() {
                text.push_str(&format!(
                    "  factory runs: {}\n",
                    runs.iter()
                        .filter_map(|run| run.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        text
    })
}

#[derive(Debug, Deserialize)]
struct KnowledgeHit {
    resource: String,
    #[serde(default)]
    label: Value,
    #[serde(default)]
    snippet: Value,
    #[serde(default)]
    provider: Value,
}

fn parse_knowledge_search(
    stdout: &str,
) -> Result<(Vec<KnowledgeHit>, Vec<String>, String), String> {
    let document: Value =
        serde_json::from_str(stdout).map_err(|error| format!("not JSON: {error}"))?;
    let data = document
        .get("data")
        .cloned()
        .unwrap_or_else(|| document.clone());
    let hits = data.get("hits").cloned().unwrap_or_default();
    let hits: Vec<KnowledgeHit> = serde_json::from_value(hits).map_err(|e| e.to_string())?;
    let absences = data
        .get("absences")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok((hits, absences, "ok".into()))
}

/// The honest enrichment: attach only the relations this state actually
/// records. A hit nothing correlates to — Direct work, unrecorded context —
/// carries an explicit empty relation set, never a fabricated ancestry.
fn relate_hit_to_state(
    state: &crate::developmental_read::FactoryDevelopmentalState,
    hit: &KnowledgeHit,
) -> Value {
    let mut runs: Vec<String> = Vec::new();
    let mut correlations: Vec<String> = Vec::new();
    let resource = hit.resource.as_str();

    for correlation in &state.execution_correlations {
        let temporal = &correlation.temporal;
        // A NOW clearing hit relates through a recorded child/parent NOW ref
        // with the same clearing key.
        let now_related = [
            temporal.child_now_ref.as_ref(),
            temporal.parent_now_ref.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|reference| refs_share_record(&reference.reference, resource));
        // A day hit relates through the correlation's recorded day refs.
        let day_related = temporal
            .day_refs
            .iter()
            .chain(temporal.source_day_ref.iter())
            .any(|reference| refs_share_record(&reference.reference, resource));
        let source_related = temporal
            .source_changes
            .iter()
            .any(|reference| reference.reference == resource);
        if now_related || day_related || source_related {
            runs.push(correlation.run_ref.to_string());
            correlations.push(correlation.correlation_ref.to_string());
        }
    }
    runs.sort();
    runs.dedup();
    correlations.sort();
    correlations.dedup();

    let empty = runs.is_empty() && correlations.is_empty();
    json!({
        "runs": runs,
        "correlations": correlations,
        "disclosure": if empty {
            "no factory relation recorded for this hit; it is Direct or unrecorded work, not Factory ancestry"
        } else {
            "relations recorded in this developmental state"
        },
    })
}

/// Two refs share a record when the hit's path carries the owner ref's
/// terminal identity (the clearing key of a now.json, the date of a day
/// file). Deterministic string identity only; no timing inference.
fn refs_share_record(owner_reference: &str, hit_resource: &str) -> bool {
    let Some((reference_kind, reference_key)) = owner_reference.rsplit_once(':') else {
        return false;
    };
    if reference_key.is_empty() {
        return false;
    }
    hit_resource.contains(reference_key) && reference_kind.len() > 2
}

fn run_command(argv: &[String]) -> Result<String, String> {
    run_command_with_timeout(argv, SUBPROCESS_TIMEOUT)
}

fn run_command_with_timeout(argv: &[String], timeout: Duration) -> Result<String, String> {
    use std::io::Read;
    let Some((program, args)) = argv.split_first() else {
        return Err("empty command".into());
    };
    let mut child = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    // Drain the pipes on threads while waiting: a child that emits more than
    // the pipe buffer must never block on a parent that waits for exit first.
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = pipe.read_to_string(&mut buffer);
        }
        buffer
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = pipe.read_to_string(&mut buffer);
        }
        buffer
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(format!("timed out after {timeout:?}"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(format!("wait failed: {error}")),
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    if status.success() {
        return Ok(stdout);
    }
    Err(format!(
        "exited with {status}{}",
        if stderr.trim().is_empty() {
            String::new()
        } else {
            format!(": {}", stderr.trim())
        }
    ))
}

// ---------------------------------------------------------------------------
// stats — deterministic aggregates with explicit denominators
// ---------------------------------------------------------------------------

fn stats(args: &[String], json: bool) -> Result<String, CliError> {
    stats_inner(args, json)
}

fn stats_inner(args: &[String], json: bool) -> Result<String, CliError> {
    let state_path = require_state_path(args)?;
    let provider = FactoryDevelopmentalFileProvider::open(&state_path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let state = provider.state();

    let mut dispositions: BTreeMap<String, usize> = BTreeMap::new();
    let mut observations = 0usize;
    let mut verifications = 0usize;
    let mut returned = 0usize;
    for run_attempts in state.attempt_states.values() {
        for record in run_attempts.attempts().values() {
            // A readable grouping key: the situated Agency on its harness,
            // not the whole serialised disposition.
            let key = format!(
                "{} @ {}",
                record.disposition.participant.agency_ref, record.disposition.body.harness_ref
            );
            *dispositions.entry(key).or_default() += 1;
            observations += record.observations.len();
            verifications += record.verifications.len();
            if record.readable_return.is_some() {
                returned += 1;
            }
        }
    }
    let attempt_total: usize = dispositions.values().sum();
    let correlation_total = state.execution_correlations.len();

    // Saved analysis templates (§6): deterministic aggregates over the real
    // records, each able to name its included occasions. A template that
    // cannot be answered from these records says so instead of inventing a
    // denominator.
    let mut template_name = String::new();
    let mut drill_down = false;
    let mut iterator = args.iter().cloned();
    let mut positional: Vec<String> = Vec::new();
    while let Some(arg) = iterator.next() {
        match arg.as_str() {
            "--template" => template_name = iterator.next().unwrap_or_default(),
            "--drill-down" => drill_down = true,
            other => positional.push(other.to_string()),
        }
    }
    if positional.is_empty() && !template_name.is_empty() {
        // `stats --template X` without a state path is a usage error handled
        // by require_state_path; keep the positional list as the argument tail.
    }
    if !template_name.is_empty() {
        return stats_template(state, &state_path, &template_name, drill_down, json);
    }
    let document = json!({
        "contract": FACTORY_TELEMETRY_STATS_CONTRACT,
        "state": state_path.to_string_lossy(),
        "windows": "the whole provider state; no time filtering is implemented yet",
        "attemptsByDisposition": dispositions,
        "attempts": {
            "total": attempt_total,
            "withReadableReturn": returned,
            "observations": observations,
            "verifications": verifications,
        },
        "correlations": {
            "total": correlation_total,
            "withChildNowRef": state.execution_correlations.iter().filter(|c| c.temporal.child_now_ref.is_some()).count(),
            "withDayRefs": state.execution_correlations.iter().filter(|c| !c.temporal.day_refs.is_empty()).count(),
            "withGitBasis": state.execution_correlations.iter().filter(|c| c.git_basis.is_some()).count(),
            "denominator": correlation_total,
        },
    });
    render(&document, json, || {
        let mut text = format!(
            "{}\nAttempts: {attempt_total} (returned {returned}, observations {observations}, verifications {verifications})\nBy disposition:",
            FACTORY_TELEMETRY_STATS_CONTRACT
        );
        for (disposition, count) in &dispositions {
            text.push_str(&format!("\n  {disposition}: {count}"));
        }
        text.push_str(&format!(
            "\nCorrelations: {correlation_total} (child-NOW {}, day-refs {}, git-basis {})",
            document["correlations"]["withChildNowRef"],
            document["correlations"]["withDayRefs"],
            document["correlations"]["withGitBasis"],
        ));
        text
    })
}

// ---------------------------------------------------------------------------
// saved analysis templates — deterministic, drillable aggregates
// ---------------------------------------------------------------------------

fn stats_template(
    state: &crate::developmental_read::FactoryDevelopmentalState,
    state_path: &Path,
    template: &str,
    drill_down: bool,
    json: bool,
) -> Result<String, CliError> {
    let mut included = 0usize;
    let document = match template {
        // §6: attempts by situated Agency and harness. Failure is read from
        // the record itself: failure evidence or a missing return is not
        // invented into a rate.
        "attempts-by-agency" => {
            let mut groups: BTreeMap<String, Value> = BTreeMap::new();
            let mut total = 0usize;
            for run_attempts in state.attempt_states.values() {
                for (attempt_ref, record) in run_attempts.attempts() {
                    total += 1;
                    let key = format!(
                        "{} @ {}",
                        record.disposition.participant.agency_ref,
                        record.disposition.body.harness_ref
                    );
                    let group = groups.entry(key.clone()).or_insert_with(|| {
                        json!({
                            "attempts": 0,
                            "verified": 0,
                            "returned": 0,
                            "withFailureEvidence": 0,
                            "occasions": [],
                        })
                    });
                    group["attempts"] = json!(group["attempts"].as_u64().unwrap_or(0) + 1);
                    if !record.verifications.is_empty() {
                        group["verified"] = json!(group["verified"].as_u64().unwrap_or(0) + 1);
                    }
                    if record.readable_return.is_some() {
                        group["returned"] = json!(group["returned"].as_u64().unwrap_or(0) + 1);
                    }
                    if !record.failure_evidence_refs.is_empty() {
                        group["withFailureEvidence"] =
                            json!(group["withFailureEvidence"].as_u64().unwrap_or(0) + 1);
                    }
                    if drill_down {
                        group["occasions"]
                            .as_array_mut()
                            .expect("occasions array")
                            .push(json!(attempt_ref));
                    }
                    included += 1;
                }
            }
            json!({
                "template": template,
                "denominator": {"attempts": total, "window": "whole provider state"},
                "groups": groups,
            })
        }
        // §6: missing evidence — correlations whose temporal provenance or
        // Git basis was never recorded.
        "correlation-completeness" => {
            let mut incomplete: Vec<Value> = Vec::new();
            let total = state.execution_correlations.len();
            for correlation in &state.execution_correlations {
                let mut gaps: Vec<&str> = Vec::new();
                if correlation.temporal.child_now_ref.is_none() {
                    gaps.push("childNowRef");
                }
                if correlation.temporal.day_refs.is_empty() {
                    gaps.push("dayRefs");
                }
                if correlation.git_basis.is_none() {
                    gaps.push("gitBasis");
                }
                if !gaps.is_empty() {
                    if drill_down {
                        incomplete.push(json!({
                            "correlationRef": correlation.correlation_ref.to_string(),
                            "gaps": gaps,
                        }));
                    } else {
                        incomplete.push(json!(correlation.correlation_ref.to_string()));
                    }
                    included += 1;
                }
            }
            json!({
                "template": template,
                "denominator": {"correlations": total, "incomplete": incomplete.len()},
                "incomplete": incomplete,
            })
        }
        // §6: return-to-verification delay. Verification receipts carry the
        // owner revision they verified against, not a clock, so a duration
        // cannot be computed from these records today. The template counts
        // and names that gap instead of inventing a number.
        "return-to-verification" => {
            let mut attempts = 0usize;
            let mut with_verification = 0usize;
            let mut named: Vec<Value> = Vec::new();
            for (run_ref, run_attempts) in &state.attempt_states {
                for (attempt_ref, record) in run_attempts.attempts() {
                    attempts += 1;
                    match record.verifications.last() {
                        Some(verification) => {
                            with_verification += 1;
                            if drill_down {
                                named.push(json!({
                                    "attemptRef": attempt_ref,
                                    "runRef": run_ref.to_string(),
                                    "verificationRef": verification.verification_ref,
                                    "verifiedOwnerRevision": verification.source_revision,
                                }));
                            }
                        }
                        None => {
                            if drill_down {
                                named.push(json!({
                                    "attemptRef": attempt_ref,
                                    "runRef": run_ref.to_string(),
                                    "verificationRef": null,
                                }));
                            }
                        }
                    }
                    included += 1;
                }
            }
            json!({
                "template": template,
                "denominator": {
                    "attempts": attempts,
                    "withVerification": with_verification,
                },
                "verificationTargets": if drill_down { json!(named) } else { json!([]) },
                "disclosure": "verification receipts record the owner revision they verified, not a time; a true return-to-verification duration needs time-stamped verification facts, which is a named producer gap — not a zero",
            })
        }
        other => {
            return Err(CliError::new(format!(
                "unknown stats template `{other}`; available: attempts-by-agency, correlation-completeness, return-to-verification"
            )))
        }
    };
    let mut full = document;
    full["contract"] = json!(FACTORY_TELEMETRY_STATS_CONTRACT);
    full["state"] = json!(state_path.to_string_lossy());
    full["drillDown"] = json!(drill_down);
    full["includedOccasions"] = json!(included);
    render(&full, json, || {
        format!(
            "{}\ntemplate {} — {} occasion(s) in scope; drill-down: {}",
            FACTORY_TELEMETRY_STATS_CONTRACT, template, included, drill_down
        )
    })
}

// ---------------------------------------------------------------------------
// export — bounded JSONL interchange for analysis
// ---------------------------------------------------------------------------

fn export(args: &[String], json: bool) -> Result<String, CliError> {
    let state_path = require_state_path(args)?;
    let provider = FactoryDevelopmentalFileProvider::open(&state_path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let state = provider.state();
    let mut lines = Vec::new();
    for correlation in &state.execution_correlations {
        lines.push(json!({
            "kind": "execution-correlation",
            "schema": FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER,
            "record": correlation,
        }));
    }
    for (run_ref, run_attempts) in &state.attempt_states {
        for (attempt_ref, record) in run_attempts.attempts() {
            lines.push(json!({
                "kind": "attempt",
                "runRef": run_ref.to_string(),
                "attemptRef": attempt_ref,
                "record": record,
            }));
        }
    }
    let body = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    if json {
        return Ok(body);
    }
    Ok(body)
}

// ---------------------------------------------------------------------------
// doctor — the producer-to-consumer path, verified end to end
// ---------------------------------------------------------------------------

fn doctor(args: &[String], json: bool) -> Result<String, CliError> {
    let state_path = require_state_path(args)?;
    let mut checks: Vec<Value> = Vec::new();
    let mut check = |name: &str, ok: bool, detail: String, remedy: &str| {
        checks.push(json!({
            "check": name,
            "status": if ok { "pass" } else { "fail" },
            "detail": detail,
            "remedy": remedy,
        }));
    };

    let provider = match FactoryDevelopmentalFileProvider::open(&state_path) {
        Ok(provider) => {
            check(
                "state",
                true,
                format!(
                    "{} opened at revision {}",
                    FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER,
                    provider.state().build.revision().get()
                ),
                "",
            );
            provider
        }
        Err(error) => {
            check(
                "state",
                false,
                format!("developmental state could not be opened: {error}"),
                "pass a valid factory developmental provider state file",
            );
            return finish_doctor(checks, json);
        }
    };

    for (tool, args) in [
        ("ctrl", vec!["--help"]),
        ("aikit", vec!["--version"]),
        ("rg", vec!["--version"]),
    ] {
        let present = probe(tool, &args);
        check(
            &format!("tool-{tool}"),
            present,
            if present {
                format!("{tool} is installed and runnable")
            } else {
                format!("{tool} is not runnable from this environment")
            },
            match tool {
                "ctrl" => "install Central's ctrl binary or adjust PATH",
                "aikit" => "install ai-kit's aikit binary or adjust PATH",
                _ => "install ripgrep (rg) or adjust PATH",
            },
        );
    }

    // The Central join: day readable through the owner surface.
    let central_root = central_root_for(&state_path);
    let now_refs: Vec<String> = provider
        .state()
        .execution_correlations
        .iter()
        .filter_map(|correlation| {
            correlation
                .temporal
                .child_now_ref
                .as_ref()
                .map(|reference| reference.reference.clone())
        })
        .take(DOCTOR_NOW_REF_PROBES)
        .collect();
    if let Some(root) = central_root.clone() {
        match run_command(&ctrl_argv(&root, "central.day.read", "{}")) {
            Ok(stdout) => match serde_json::from_str::<Value>(&stdout) {
                Ok(document) if document["ok"] == true => {
                    let day = document["data"]["reading"]["temporal"]["day_ref"]
                        .as_str()
                        .unwrap_or("unknown");
                    check(
                        "central-day",
                        true,
                        format!("Central answered central.day.read; today is {day}"),
                        "",
                    );
                }
                other => check(
                    "central-day",
                    false,
                    format!("central.day.read returned an unexpected document: {other:?}"),
                    "verify Central's native action registry with `ctrl actions`",
                ),
            },
            Err(error) => check(
                "central-day",
                false,
                format!("central.day.read failed: {error}"),
                "confirm the ctrl binary can read this ground",
            ),
        }
        for now_ref in &now_refs {
            let input = json!({"now_ref": now_ref}).to_string();
            match run_command(&ctrl_argv(&root, "central.now.read", &input)) {
                Ok(stdout) => {
                    let ok = serde_json::from_str::<Value>(&stdout)
                        .map(|document| document["ok"] == true)
                        .unwrap_or(false);
                    check(
                        "now-ref",
                        ok,
                        format!(
                            "{now_ref}: {}",
                            if ok {
                                "resolves through Central"
                            } else {
                                "refused or missing"
                            }
                        ),
                        if ok {
                            ""
                        } else {
                            "re-allocate the child NOW or correct the recorded ref"
                        },
                    );
                }
                Err(error) => check(
                    "now-ref",
                    false,
                    format!("{now_ref}: {error}"),
                    "confirm Central is reachable",
                ),
            }
        }
    } else {
        check(
            "central-ground",
            false,
            "no Central ground (Control/ + Work/) found above the state file".to_string(),
            "run inside the personal ground or pass an explicit root",
        );
    }

    // The search join: does AIKit's knowledge service see the NOW field?
    if probe("aikit", &["--version"]) {
        match run_command(&[
            "aikit".into(),
            "--json".into(),
            "knowledge".into(),
            "status".into(),
        ]) {
            Ok(stdout) => {
                let document = serde_json::from_str::<Value>(&stdout).unwrap_or(Value::Null);
                let now_field = document["data"]["sources"]
                    .as_array()
                    .map(|sources| {
                        sources
                            .iter()
                            .find(|source| {
                                source["provider"]["provider"] == "provider/source-pool/now-field"
                                    || source["provider"] == "provider/source-pool/now-field"
                            })
                            .map(|source| source["available"] == true)
                    })
                    .unwrap_or(None);
                check(
                    "aikit-now-field",
                    now_field.unwrap_or(false),
                    match now_field {
                        Some(true) => "AIKit's knowledge service reports the NOW-field provider available".into(),
                        Some(false) => "AIKit reports the NOW-field provider present but unavailable (ripgrep missing?)".into(),
                        None => "AIKit's knowledge status does not list the NOW-field provider; the installed aikit predates NOW-field search".into(),
                    },
                    "update the installed aikit to a cut carrying NOW-field search",
                );
            }
            Err(error) => check(
                "aikit-now-field",
                false,
                format!("aikit knowledge status failed: {error}"),
                "confirm the aikit binary runs",
            ),
        }
    }

    finish_doctor(checks, json)
}

fn finish_doctor(checks: Vec<Value>, json: bool) -> Result<String, CliError> {
    let failed = checks
        .iter()
        .filter(|check| check["status"] == "fail")
        .count();
    let document = json!({
        "contract": FACTORY_TELEMETRY_DOCTOR_CONTRACT,
        "healthy": failed == 0,
        "failed": failed,
        "checks": checks,
    });
    render(&document, json, || {
        let mut text = format!(
            "{}\n{}",
            FACTORY_TELEMETRY_DOCTOR_CONTRACT,
            if failed == 0 {
                "healthy"
            } else {
                "unhealthy — fix the failing checks:"
            }
        );
        for check in &checks {
            if check["status"] == "fail" {
                text.push_str(&format!(
                    "\n- {} — {} — remedy: {}",
                    check["check"].as_str().unwrap_or_default(),
                    check["detail"].as_str().unwrap_or_default(),
                    check["remedy"].as_str().unwrap_or_default()
                ));
            }
        }
        text
    })
}

fn ctrl_argv(root: &Path, action: &str, input: &str) -> Vec<String> {
    vec![
        "ctrl".into(),
        "--json".into(),
        "--root".into(),
        root.to_string_lossy().into_owned(),
        "action".into(),
        "run".into(),
        action.into(),
        input.into(),
    ]
}

/// Walk upward from the state file for the personal ground marker.
fn central_root_for(from: &Path) -> Option<PathBuf> {
    // Relative state paths must resolve against the process working
    // directory before an ancestor walk can find the ground.
    let absolute = if from.is_absolute() {
        from.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(from)
    };
    let mut cursor = Some(absolute);
    while let Some(current) = cursor {
        if current.join("Control").is_dir() && current.join("Work").is_dir() {
            return Some(current);
        }
        cursor = current.parent().map(Path::to_path_buf);
    }
    None
}

// ---------------------------------------------------------------------------
// watch — a resumable bounded stream over the state's own change history
// ---------------------------------------------------------------------------

/// The state is a single revisioned document with one writer (the locked
/// owner mutations). A watcher therefore polls the revision and emits the
/// correlations of every revision past its cursor as JSONL — bounded by
/// `--max-events`, `--duration` or Ctrl-C — and always ends by naming the
/// cursor a resume should carry.
fn watch(args: &[String], _json: bool) -> Result<String, CliError> {
    let mut interval_secs = 2.0f64;
    let mut max_events = 100usize;
    let mut duration_secs = 30.0f64;
    let mut resume_revision: Option<u64> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut iterator = args.iter().cloned();
    while let Some(arg) = iterator.next() {
        match arg.as_str() {
            "--interval" => {
                interval_secs = iterator
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| CliError::new("--interval requires seconds"))?;
            }
            "--max-events" => {
                max_events = iterator
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| CliError::new("--max-events requires a number"))?;
            }
            "--duration" => {
                duration_secs = iterator
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| CliError::new("--duration requires seconds"))?;
            }
            "--resume" => {
                let raw = iterator
                    .next()
                    .ok_or_else(|| CliError::new("--resume requires a cursor document"))?;
                let cursor: Value = serde_json::from_str(&raw).map_err(|error| {
                    CliError::new(format!("--resume is not a cursor document: {error}"))
                })?;
                resume_revision = Some(cursor["stateRevision"].as_u64().ok_or_else(|| {
                    CliError::new("resume cursor must carry the stateRevision key")
                })?);
            }
            other => positional.push(other.to_string()),
        }
    }
    let state_path = require_state_path(&positional)?;
    let effective = crate::configuration::read_telemetry_effective_all(&state_path);
    if interval_secs == 2.0 {
        if let Some(configured) = effective
            .get(crate::configuration::TELEMETRY_WATCH_INTERVAL_SETTING)
            .copied()
        {
            interval_secs = configured;
        }
    }
    let mut observed_revision = resume_revision.unwrap_or(0);

    let deadline = std::time::Instant::now() + Duration::from_secs_f64(duration_secs);
    let mut emitted = 0usize;
    let mut lines: Vec<String> = Vec::new();
    while std::time::Instant::now() < deadline && emitted < max_events {
        // A torn write or a concurrent replacement is one skipped poll, not
        // a dead stream.
        if let Ok(provider) = FactoryDevelopmentalFileProvider::open(&state_path) {
            let state = provider.state();
            let revision = state.build.revision().get();
            if revision > observed_revision {
                for correlation in &state.execution_correlations {
                    if emitted >= max_events {
                        break;
                    }
                    lines.push(
                        json!({
                            "type": "execution-correlation",
                            "stateRevision": revision,
                            "correlationRef": correlation.correlation_ref.to_string(),
                            "telemetryRef": correlation.telemetry_ref.to_string(),
                            "runRef": correlation.run_ref.to_string(),
                            "childNowRef": correlation.temporal.child_now_ref.as_ref().map(|r| r.reference.clone()),
                        })
                        .to_string(),
                    );
                    emitted += 1;
                }
                observed_revision = revision;
            }
        }
        if emitted >= max_events || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_secs_f64(interval_secs));
    }
    lines.push(
        json!({
            "type": "cursor",
            "cursor": {"stateRevision": observed_revision},
            "emitted": emitted,
            "resumeWith": "--resume",
        })
        .to_string(),
    );
    Ok(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// compare — original basis A against changed basis B, with movement named
// ---------------------------------------------------------------------------

fn compare(args: &[String], json: bool) -> Result<String, CliError> {
    if args.len() < 2 {
        return Err(CliError::new(
            "compare needs two states: factory telemetry compare <original-state> <changed-state>",
        ));
    }
    let path_a = require_state_path(&args[0..1])?;
    let path_b = require_state_path(&args[1..2])?;
    let a = FactoryDevelopmentalFileProvider::open(&path_a)
        .map_err(|e| CliError::new(e.to_string()))?;
    let b = FactoryDevelopmentalFileProvider::open(&path_b)
        .map_err(|e| CliError::new(e.to_string()))?;

    let runs_a: Vec<String> = a
        .state()
        .build
        .run_refs()
        .iter()
        .map(|r| r.to_string())
        .collect();
    let runs_b: Vec<String> = b
        .state()
        .build
        .run_refs()
        .iter()
        .map(|r| r.to_string())
        .collect();
    let shared_runs = runs_a
        .iter()
        .filter(|run| runs_b.contains(run))
        .cloned()
        .collect::<Vec<_>>();

    let refs_a: Vec<String> = a
        .state()
        .execution_correlations
        .iter()
        .map(|c| c.correlation_ref.to_string())
        .collect();
    let refs_b: Vec<String> = b
        .state()
        .execution_correlations
        .iter()
        .map(|c| c.correlation_ref.to_string())
        .collect();
    let added: Vec<String> = refs_b
        .iter()
        .filter(|r| !refs_a.contains(r))
        .cloned()
        .collect();
    let removed: Vec<String> = refs_a
        .iter()
        .filter(|r| !refs_b.contains(r))
        .cloned()
        .collect();

    let moved = b.state().build.revision().get() != a.state().build.revision().get();
    let mut basis_changes: Vec<Value> = Vec::new();
    for correlation in &b.state().execution_correlations {
        let Some(new_basis) = &correlation.git_basis else {
            continue;
        };
        if let Some(old_correlation) = a
            .state()
            .execution_correlations
            .iter()
            .find(|c| c.correlation_ref == correlation.correlation_ref)
        {
            if let Some(old_basis) = &old_correlation.git_basis {
                if new_basis.base_head != old_basis.base_head {
                    basis_changes.push(json!({
                        "correlationRef": correlation.correlation_ref.to_string(),
                        "from": old_basis.base_head,
                        "to": new_basis.base_head,
                    }));
                }
            }
        }
    }

    let completeness = |provider: &FactoryDevelopmentalFileProvider| -> Value {
        let correlations = &provider.state().execution_correlations;
        json!({
            "total": correlations.len(),
            "withChildNowRef": correlations.iter().filter(|c| c.temporal.child_now_ref.is_some()).count(),
            "withDayRefs": correlations.iter().filter(|c| !c.temporal.day_refs.is_empty()).count(),
            "withGitBasis": correlations.iter().filter(|c| c.git_basis.is_some()).count(),
        })
    };

    let verdict = if shared_runs.is_empty() {
        "unrelated-bases"
    } else if !added.is_empty() || !removed.is_empty() || moved {
        "moved"
    } else {
        "identical"
    };

    let document = json!({
        "contract": FACTORY_TELEMETRY_COMPARE_CONTRACT,
        "original": path_a.to_string_lossy(),
        "changed": path_b.to_string_lossy(),
        "verdict": verdict,
        "movement": {
            "stateRevision": {"a": a.state().build.revision().get(), "b": b.state().build.revision().get()},
            "correlationsAdded": added,
            "correlationsRemoved": removed,
            "gitBasisChanges": basis_changes,
        },
        "temporalCompleteness": {"a": completeness(&a), "b": completeness(&b)},
        "disclosure": "A is the original basis, B the changed basis; movement is detected by revision and correlation delta, never by timing",
    });
    render(&document, json, || {
        format!(
            "{}\nverdict: {} (A rev {}, B rev {})\ncorrelations +{} -{}, git-basis changes {}",
            FACTORY_TELEMETRY_COMPARE_CONTRACT,
            verdict,
            document["movement"]["stateRevision"]["a"],
            document["movement"]["stateRevision"]["b"],
            added.len(),
            removed.len(),
            basis_changes.len()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::create_developmental_conformance_state;
    use std::sync::Mutex;

    /// The doctor probes real executables; serialise the environment-touching
    /// tests so parallel cargo test threads cannot race a probe.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(super) fn conformance_state() -> (
        tempfile::TempDir,
        crate::conformance::FactoryDevelopmentalConformanceManifest,
    ) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = temp.path().join("conformance-state.json");
        let manifest = create_developmental_conformance_state(&path).expect("conformance state");
        (temp, manifest)
    }

    pub(super) fn telemetry_args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    fn parse(document: &str) -> Value {
        serde_json::from_str(document).expect("structured telemetry document")
    }

    #[test]
    fn status_counts_what_the_state_actually_contains() {
        let (_temp, manifest) = conformance_state();
        let document = parse(
            &execute(
                &telemetry_args(&["status", &manifest.provider_state, "--json"]),
                true,
            )
            .expect("status runs"),
        );
        assert_eq!(document["contract"], FACTORY_TELEMETRY_STATUS_CONTRACT);
        assert_eq!(document["schema"], "factory.developmental-state/v1");
        assert_eq!(document["counts"]["executionCorrelations"], 1);
        assert_eq!(document["counts"]["journeys"], 1);
        assert_eq!(document["correlationCompleteness"]["total"], 1);
    }

    #[test]
    fn inspect_joins_the_reading_with_the_native_attempt_store() {
        let (_temp, manifest) = conformance_state();
        let document = parse(
            &execute(
                &telemetry_args(&[
                    "inspect",
                    &manifest.provider_state,
                    &manifest.telemetry_ref.to_string(),
                    "--json",
                ]),
                true,
            )
            .expect("inspect runs"),
        );
        assert_eq!(document["contract"], FACTORY_TELEMETRY_INSPECT_CONTRACT);
        // The conformance fixture records no NOW/day correlation and no
        // attempts; the inspection must say so instead of inventing joins.
        let reading = &document["reading"];
        assert_eq!(reading["telemetryRef"], manifest.telemetry_ref.to_string());
        assert_eq!(reading["relatedAttempts"], serde_json::json!([]));
        assert!(reading["temporal"]["childNowRef"].is_null());
        assert!(reading["gitBasis"].is_null());
    }

    #[test]
    fn search_discloses_an_unavailable_provider_instead_of_dying() {
        let (_temp, manifest) = conformance_state();
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let document = parse(
            &execute(
                &telemetry_args(&[
                    "search",
                    &manifest.provider_state,
                    "anything",
                    "--aikit",
                    "/nonexistent/aikit-binary",
                    "--json",
                ]),
                true,
            )
            .expect("search returns a structured answer"),
        );
        assert_eq!(document["contract"], FACTORY_TELEMETRY_SEARCH_CONTRACT);
        assert_eq!(document["status"], "provider-unavailable");
        assert!(document["absences"]
            .as_array()
            .expect("absences listed")
            .iter()
            .any(|absence| absence.as_str().unwrap_or_default().contains("remedy")));
    }

    #[test]
    fn stats_carry_explicit_denominators() {
        let (_temp, manifest) = conformance_state();
        let document = parse(
            &execute(
                &telemetry_args(&["stats", &manifest.provider_state, "--json"]),
                true,
            )
            .expect("stats runs"),
        );
        assert_eq!(document["contract"], FACTORY_TELEMETRY_STATS_CONTRACT);
        assert_eq!(document["correlations"]["total"], 1);
        assert_eq!(document["correlations"]["denominator"], 1);
    }

    #[test]
    fn export_emits_one_json_object_per_line() {
        let (_temp, manifest) = conformance_state();
        let body = execute(
            &telemetry_args(&["export", &manifest.provider_state, "--json"]),
            true,
        )
        .expect("export runs");
        let lines: Vec<Value> = body
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSONL line"))
            .collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["kind"], "execution-correlation");
    }

    #[test]
    fn doctor_reports_the_state_check_truthfully_and_structurally() {
        let (_temp, manifest) = conformance_state();
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let document = parse(
            &execute(
                &telemetry_args(&["doctor", &manifest.provider_state, "--json"]),
                true,
            )
            .expect("doctor runs"),
        );
        assert_eq!(document["contract"], FACTORY_TELEMETRY_DOCTOR_CONTRACT);
        let checks = document["checks"].as_array().expect("checks listed");
        let state_check = checks
            .iter()
            .find(|check| check["check"] == "state")
            .expect("state check present");
        assert_eq!(state_check["status"], "pass");
        // Every check names a remedy, even passed ones — the document is the
        // operator's, not the tool's.
        for check in checks {
            assert!(check["remedy"].is_string());
        }
    }

    #[test]
    fn record_execution_correlation_admits_replays_and_conflicts() {
        let (temp, manifest) = conformance_state();
        // The conformance state carries one correlation with an admitted
        // agency/execution; admit a second correlation of the same execution
        // under a new telemetry ref through the real mutation path.
        let state_text = std::fs::read_to_string(&manifest.provider_state).unwrap();
        let state_value: Value = serde_json::from_str(&state_text).unwrap();
        let original = &state_value["state"]["executionCorrelations"][0];
        let mut correlation = original.clone();
        correlation["correlationRef"] =
            serde_json::json!("execution-correlation:01M2RFTZT503G75Y7JRBQHPMFZ");
        correlation["telemetryRef"] = serde_json::json!("telemetry:01M2RFTZT5M0J1S2A28TQJCF88");
        let now = "2026-09-17T21:00:00Z";
        let request = |payload: &Value| {
            serde_json::json!({
                "contract": "factory.developmental-mutation-request/v1",
                "mutationRef": "mutation:record-correlation-test-1",
                "occurrenceRef": "occurrence:record-correlation-test-1",
                "source": {
                    "owner": "factory",
                    "reference": payload["correlationRef"].as_str().unwrap(),
                    "revision": "r1",
                    "standing": "owner-native-observation"
                },
                "observedAt": now,
                "mutation": {"kind": "record-execution-correlation", "correlation": payload}
            })
            .to_string()
        };
        let provider_path = temp.path().join("state.json");
        std::fs::copy(&manifest.provider_state, &provider_path).unwrap();
        let mut provider = FactoryDevelopmentalFileProvider::open(&provider_path).unwrap();
        let receipt = provider
            .apply_developmental_mutation(serde_json::from_str(&request(&correlation)).unwrap())
            .expect("the correlation is admitted through the real mutation path");
        assert_eq!(
            receipt.status,
            crate::commission::FactoryAdmissionStatus::Applied
        );
        // Idempotent replay of the identical request.
        let replay = provider
            .apply_developmental_mutation(serde_json::from_str(&request(&correlation)).unwrap())
            .expect("replay reads back");
        assert_eq!(
            replay.status,
            crate::commission::FactoryAdmissionStatus::AlreadyApplied
        );
        // Same mutation ref, different payload: a replay conflict, not silent
        // divergence.
        let mut divergent = correlation.clone();
        divergent["telemetryRef"] = serde_json::json!("telemetry:01M2RFTZT5M0J1S2A28TQJCF89");
        let error = provider
            .apply_developmental_mutation(serde_json::from_str(&request(&divergent)).unwrap())
            .expect_err("divergent replay conflicts");
        assert!(error.to_string().contains("conflict") || error.to_string().contains("Conflict"));
        // The admitted correlation is visible through the telemetry family.
        let document = parse(
            &execute(
                &telemetry_args(&[
                    "inspect",
                    provider_path.to_str().unwrap(),
                    "telemetry:01M2RFTZT5M0J1S2A28TQJCF88",
                    "--json",
                ]),
                true,
            )
            .expect("the admitted correlation inspects"),
        );
        assert_eq!(
            document["reading"]["telemetryRef"],
            "telemetry:01M2RFTZT5M0J1S2A28TQJCF88"
        );
    }

    #[test]
    fn usage_errors_exit_through_the_cli_error_path() {
        let error = execute(
            &telemetry_args(&["status", "/nonexistent/state.json"]),
            true,
        )
        .expect_err("missing state is an error");
        assert!(error.to_string().contains("does not exist"));
        let error = execute(&telemetry_args(&["wat", "x"]), true)
            .expect_err("unknown operation is an error");
        assert!(error.to_string().contains("unknown telemetry operation"));
    }
}

#[cfg(test)]
mod remainder_tests {
    use super::tests::telemetry_args;
    use super::*;
    use crate::conformance::create_developmental_conformance_state;

    fn parse(document: &str) -> Value {
        serde_json::from_str(document).expect("structured telemetry document")
    }

    #[test]
    fn compare_names_movement_between_two_bases_of_one_run() {
        let temp = tempfile::TempDir::new().unwrap();
        let original = temp.path().join("a.json");
        let manifest = create_developmental_conformance_state(&original).unwrap();
        let changed = temp.path().join("b.json");
        std::fs::copy(&original, &changed).unwrap();
        // B gains a correlation through the real mutation path (revision moves).
        let state_value: Value =
            serde_json::from_str(&std::fs::read_to_string(&original).unwrap()).unwrap();
        let mut correlation = state_value["state"]["executionCorrelations"][0].clone();
        correlation["correlationRef"] =
            serde_json::json!("execution-correlation:01M2S15SA8Q9CQ2SFN130Z5HSZ");
        correlation["telemetryRef"] = serde_json::json!("telemetry:01M2S15SA8YGJVA2RCSXWB2MNZ");
        let mut provider = FactoryDevelopmentalFileProvider::open(&changed).unwrap();
        let request = serde_json::json!({
            "contract": "factory.developmental-mutation-request/v1",
            "mutationRef": "mutation:compare-test-1",
            "occurrenceRef": "occurrence:compare-test-1",
            "source": {
                "owner": "factory",
                "reference": correlation["correlationRef"],
                "revision": "r1",
                "standing": "owner-native-observation"
            },
            "observedAt": "2026-09-18T00:00:00Z",
            "mutation": {"kind": "record-execution-correlation", "correlation": correlation}
        });
        provider
            .apply_developmental_mutation(serde_json::from_value(request).unwrap())
            .expect("correlation admitted");
        let document = parse(
            &execute(
                &telemetry_args(&[
                    "compare",
                    &manifest.provider_state,
                    changed.to_str().unwrap(),
                    "--json",
                ]),
                true,
            )
            .expect("compare runs"),
        );
        assert_eq!(document["verdict"], "moved");
        assert_eq!(
            document["movement"]["correlationsAdded"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn compare_names_identical_bases_and_names_its_verdict_vocabulary() {
        let temp = tempfile::TempDir::new().unwrap();
        let first = temp.path().join("one.json");
        let second = temp.path().join("two.json");
        let a = create_developmental_conformance_state(&first).unwrap();
        // The conformance generator is deterministic, so an independent copy
        // of the same fixture state is the same basis: identical, not moved.
        std::fs::copy(&first, &second).unwrap();
        let document = parse(
            &execute(
                &telemetry_args(&[
                    "compare",
                    &a.provider_state,
                    second.to_str().unwrap(),
                    "--json",
                ]),
                true,
            )
            .expect("compare runs"),
        );
        assert_eq!(document["verdict"], "identical");
    }

    #[test]
    fn templates_aggregate_with_explicit_denominators_and_drill_down() {
        let (_temp, manifest) = super::tests::conformance_state();
        let document = parse(
            &execute(
                &telemetry_args(&[
                    "stats",
                    &manifest.provider_state,
                    "--template",
                    "correlation-completeness",
                    "--drill-down",
                    "--json",
                ]),
                true,
            )
            .expect("template runs"),
        );
        // The conformance fixture carries no temporal provenance: the
        // template must name that gap, not report completeness.
        assert_eq!(document["template"], "correlation-completeness");
        assert_eq!(document["denominator"]["correlations"], 1);
        assert_eq!(document["denominator"]["incomplete"], 1);
        assert_eq!(
            document["incomplete"][0]["gaps"],
            serde_json::json!(["childNowRef", "dayRefs", "gitBasis"])
        );
    }

    #[test]
    fn watch_emits_a_cursor_a_resume_can_carry() {
        let (_temp, manifest) = super::tests::conformance_state();
        let output = execute(
            &telemetry_args(&[
                "watch",
                &manifest.provider_state,
                "--duration",
                "0.05",
                "--interval",
                "0.01",
                "--json",
            ]),
            true,
        )
        .expect("watch runs");
        let lines: Vec<Value> = output
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSONL line"))
            .collect();
        let cursor = lines.last().expect("cursor line");
        assert_eq!(cursor["type"], "cursor");
        assert!(cursor["cursor"]["stateRevision"].is_u64());
    }
}
