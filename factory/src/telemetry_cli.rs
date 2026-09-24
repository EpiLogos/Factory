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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::CliError;
use crate::core::identity::Ref;
use crate::developmental_read::{
    FactoryDevelopmentalFileProvider, FactoryExecutionUsage, FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER,
};
use crate::telemetry_time::{parse_ms, CivilWindow};

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
            "missing telemetry operation; expected status|inspect|search|stats|watch|compare|export|doctor|collect|signals|classify|commission|return|digest|lookback|day|field|policy",
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
        "collect" | "signals" | "signal" | "classify" | "commission" | "return" | "digest"
        | "lookback" | "day" | "field" | "policy" => crate::sensing_cli::execute(args, json),
        other => Err(CliError::new(format!(
            "unknown telemetry operation `{other}`; expected status|inspect|search|stats|watch|compare|export|doctor|collect|signals|classify|commission|return|digest|lookback|day|field|policy"
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

/// One human line for normalised usage; unknown values say so, never `0`.
fn usage_line(usage: &FactoryExecutionUsage) -> String {
    if usage.observation_refs.is_empty() {
        return format!(
            "not observed ({})",
            usage.reason.as_deref().unwrap_or("no owner observation")
        );
    }
    let count = |value: Option<u64>| value.map_or_else(|| "unknown".into(), |v| v.to_string());
    format!(
        "in {} · out {} · cache read {} · cache write {} · cost {}",
        count(usage.input_tokens),
        count(usage.output_tokens),
        count(usage.cache_read_tokens),
        count(usage.cache_write_tokens),
        usage.cost.as_ref().map_or_else(
            || "unknown".into(),
            |cost| format!("{} {}", cost.amount, cost.currency)
        ),
    )
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
                .find(|correlation| correlation.telemetry_ref == telemetry_ref);
            let Some(recorded) = degraded else {
                return Err(CliError::new(error.to_string()));
            };
            let usage = FactoryExecutionUsage::from_correlation(recorded);
            let correlation = serde_json::to_value(recorded).unwrap_or(Value::Null);
            let document = json!({
                "contract": FACTORY_TELEMETRY_INSPECT_CONTRACT,
                "state": state_path.to_string_lossy(),
                "status": "correlation-recorded-owners-pending",
                "correlation": correlation,
                "usage": usage,
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
            "{}\nTelemetry: {}\nRun: {} · Unit: {} · Execution: {}\nAgency: {}\nChild NOW: {}\nDay refs: {}\nGit basis: {}\nUsage: {}\nReturned evidence: {}\nRelated attempts: {}",
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
            usage_line(&reading.usage),
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

fn attempt_has_any_time(record: &crate::attempt_runtime::FactoryAttemptRecord) -> bool {
    record.attempt_recorded_at.is_some()
        || record.return_recorded_at.is_some()
        || record.failure_recorded_at.is_some()
        || !record.verification_recorded_at.is_empty()
}

fn untimed_attempts(state: &crate::developmental_read::FactoryDevelopmentalState) -> usize {
    let mut by_ref = BTreeMap::<&str, bool>::new();
    for record in state
        .attempt_states
        .values()
        .flat_map(|r| r.attempts().values())
    {
        *by_ref.entry(record.attempt_ref.as_str()).or_default() |= attempt_has_any_time(record);
    }
    by_ref.values().filter(|has_time| !**has_time).count()
}

fn attempt_in_window(
    record: &crate::attempt_runtime::FactoryAttemptRecord,
    window: &CivilWindow,
) -> bool {
    record
        .attempt_recorded_at
        .as_deref()
        .and_then(|t| window.contains_rfc3339(t))
        .unwrap_or(false)
        || record
            .return_recorded_at
            .as_deref()
            .and_then(|t| window.contains_rfc3339(t))
            .unwrap_or(false)
        || record
            .failure_recorded_at
            .as_deref()
            .and_then(|t| window.contains_rfc3339(t))
            .unwrap_or(false)
        || record
            .verification_recorded_at
            .values()
            .any(|t| window.contains_rfc3339(t) == Some(true))
}

fn correlation_has_any_time(c: &crate::developmental_read::FactoryExecutionCorrelation) -> bool {
    [
        &c.temporal.started,
        &c.temporal.updated,
        &c.temporal.completed,
    ]
    .into_iter()
    .flatten()
    .any(|fact| parse_ms(&fact.value).is_some())
}

fn correlation_in_window(
    c: &crate::developmental_read::FactoryExecutionCorrelation,
    window: &CivilWindow,
) -> bool {
    let started = c.temporal.started.as_ref().and_then(|f| parse_ms(&f.value));
    let completed = c
        .temporal
        .completed
        .as_ref()
        .and_then(|f| parse_ms(&f.value));
    if let (Some(start), Some(end)) = (started, completed) {
        if window.overlaps_ms(start, end) {
            return true;
        }
    }
    [
        &c.temporal.started,
        &c.temporal.updated,
        &c.temporal.completed,
    ]
    .into_iter()
    .flatten()
    .any(|fact| parse_ms(&fact.value).is_some_and(|t| window.contains_ms(t)))
}

fn window_counts(
    state: &crate::developmental_read::FactoryDevelopmentalState,
    window: &CivilWindow,
) -> Value {
    let mut attempts = BTreeSet::new();
    let mut returns = BTreeSet::new();
    let mut verifications = BTreeSet::new();
    for runs in state.attempt_states.values() {
        for record in runs.attempts().values() {
            if !attempt_in_window(record, window) {
                continue;
            }
            attempts.insert(record.attempt_ref.as_str());
            if record
                .return_recorded_at
                .as_deref()
                .and_then(|t| window.contains_rfc3339(t))
                == Some(true)
            {
                returns.insert(record.attempt_ref.as_str());
            }
            for verification in &record.verifications {
                if record
                    .verification_recorded_at
                    .get(&verification.verification_ref)
                    .and_then(|t| window.contains_rfc3339(t))
                    == Some(true)
                {
                    verifications.insert(verification.verification_ref.as_str());
                }
            }
        }
    }
    json!({
        "window": window.as_json(), "uniqueAttempts": attempts.len(), "returns": returns.len(),
        "verifications": verifications.len(),
        "correlations": state.execution_correlations.iter().filter(|c| correlation_in_window(c, window)).count(),
        "timingUnavailable": {
            "attempts": untimed_attempts(state),
            "correlations": state.execution_correlations.iter().filter(|c| !correlation_has_any_time(c)).count(),
        },
    })
}

fn stats(args: &[String], json: bool) -> Result<String, CliError> {
    stats_inner(args, json)
}

fn stats_inner(args: &[String], json: bool) -> Result<String, CliError> {
    let state_path = require_state_path(args)?;
    let (window, remaining) = CivilWindow::from_args(&args[1..], &state_path)?;
    let provider = FactoryDevelopmentalFileProvider::open(&state_path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let state = provider.state();

    let mut dispositions: BTreeMap<String, usize> = BTreeMap::new();
    let mut observations = 0usize;
    let mut verifications = 0usize;
    let mut returned = 0usize;
    let mut seen_observations = BTreeSet::new();
    let mut seen_verifications = BTreeSet::new();
    let mut seen_returns = BTreeSet::new();
    let unknown_attempt_time = if window.is_some() {
        untimed_attempts(state)
    } else {
        0
    };
    let mut seen_attempts = BTreeSet::new();
    for run_attempts in state.attempt_states.values() {
        for record in run_attempts.attempts().values() {
            if let Some(window) = &window {
                if !attempt_in_window(record, window) {
                    continue;
                }
            }
            if seen_attempts.insert(record.attempt_ref.clone()) {
                // Group the stable attempt once, even if replay yields another view.
                let key = format!(
                    "{} @ {}",
                    record.disposition.participant.agency_ref, record.disposition.body.harness_ref
                );
                *dispositions.entry(key).or_default() += 1;
            }
            if window.is_none() {
                for observation in &record.observations {
                    if seen_observations.insert(observation.receipt_ref.as_str()) {
                        observations += 1;
                    }
                }
            }
            for verification in &record.verifications {
                if window.as_ref().is_none_or(|w| {
                    record
                        .verification_recorded_at
                        .get(&verification.verification_ref)
                        .and_then(|t| w.contains_rfc3339(t))
                        == Some(true)
                }) && seen_verifications.insert(verification.verification_ref.as_str())
                {
                    verifications += 1;
                }
            }
            if record.readable_return.is_some()
                && window.as_ref().is_none_or(|w| {
                    record
                        .return_recorded_at
                        .as_deref()
                        .and_then(|t| w.contains_rfc3339(t))
                        .unwrap_or(false)
                })
                && seen_returns.insert(record.attempt_ref.as_str())
            {
                returned += 1;
            }
        }
    }
    let attempt_total: usize = dispositions.values().sum();
    let correlations: Vec<_> = state
        .execution_correlations
        .iter()
        .filter(|c| window.as_ref().is_none_or(|w| correlation_in_window(c, w)))
        .collect();
    let correlation_total = correlations.len();
    let unknown_correlation_time = if window.is_some() {
        state
            .execution_correlations
            .iter()
            .filter(|c| !correlation_has_any_time(c))
            .count()
    } else {
        0
    };

    // Saved analysis templates (§6): deterministic aggregates over the real
    // records, each able to name its included occasions. A template that
    // cannot be answered from these records says so instead of inventing a
    // denominator.
    let mut template_name = String::new();
    let mut drill_down = false;
    let mut compare_previous = false;
    let mut iterator = remaining.iter().cloned();
    let mut positional: Vec<String> = Vec::new();
    while let Some(arg) = iterator.next() {
        match arg.as_str() {
            "--template" => template_name = iterator.next().unwrap_or_default(),
            "--drill-down" => drill_down = true,
            "--compare-previous" => compare_previous = true,
            other => positional.push(other.to_string()),
        }
    }
    if positional.is_empty() && !template_name.is_empty() {
        // `stats --template X` without a state path is a usage error handled
        // by require_state_path; keep the positional list as the argument tail.
    }
    if !template_name.is_empty() {
        if compare_previous {
            return Err(CliError::new(
                "--compare-previous applies to aggregate stats, not a saved template",
            ));
        }
        return stats_template(
            state,
            &state_path,
            &template_name,
            drill_down,
            window.as_ref(),
            json,
        );
    }
    let comparison = if compare_previous {
        let current = window
            .as_ref()
            .ok_or_else(|| CliError::new("--compare-previous requires a civil Day window"))?;
        let previous = current.previous(&state_path)?;
        Some(
            json!({"current": window_counts(state, current), "previous": window_counts(state, &previous)}),
        )
    } else {
        None
    };
    let document = json!({
        "contract": FACTORY_TELEMETRY_STATS_CONTRACT,
        "state": state_path.to_string_lossy(),
        "window": window.as_ref().map(CivilWindow::as_json).unwrap_or(json!("whole provider state")),
        "timingUnavailable": {"attempts": unknown_attempt_time, "correlations": unknown_correlation_time},
        "periodComparison": comparison,
        "attemptsByDisposition": dispositions,
        "attempts": {
            "total": attempt_total,
            "withReadableReturn": returned,
            "observations": if window.is_some() { Value::Null } else { json!(observations) },
            "observationTimeAvailability": if window.is_some() { "unavailable: owner observation receipts have no admission timestamp" } else { "whole-state count" },
            "verifications": verifications,
        },
        "correlations": {
            "total": correlation_total,
            "withChildNowRef": correlations.iter().filter(|c| c.temporal.child_now_ref.is_some()).count(),
            "withDayRefs": correlations.iter().filter(|c| !c.temporal.day_refs.is_empty()).count(),
            "withGitBasis": correlations.iter().filter(|c| c.git_basis.is_some()).count(),
            "denominator": correlation_total,
        },
    });
    render(&document, json, || {
        let observations_label = if window.is_some() {
            "unavailable".to_string()
        } else {
            observations.to_string()
        };
        let mut text = format!(
            "{}\nAttempts: {attempt_total} (returned {returned}, observations {observations_label}, verifications {verifications})\nBy disposition:",
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
    window: Option<&CivilWindow>,
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
            let mut seen = BTreeSet::new();
            let mut agency_for_attempt = BTreeMap::<&str, String>::new();
            let mut verified_attempts = BTreeSet::new();
            let mut returned_attempts = BTreeSet::new();
            let mut failed_attempts = BTreeSet::new();
            for run_attempts in state.attempt_states.values() {
                for (attempt_ref, record) in run_attempts.attempts() {
                    if window.is_some_and(|w| !attempt_in_window(record, w)) { continue; }
                    let key = agency_for_attempt.entry(attempt_ref.as_str()).or_insert_with(|| format!(
                        "{} @ {}", record.disposition.participant.agency_ref,
                        record.disposition.body.harness_ref
                    )).clone();
                    let first_view = seen.insert(attempt_ref.as_str());
                    if first_view { total += 1; included += 1; }
                    let group = groups.entry(key.clone()).or_insert_with(|| {
                        json!({
                            "attempts": 0,
                            "verified": 0,
                            "returned": 0,
                            "withFailureEvidence": 0,
                            "occasions": [],
                        })
                    });
                    if first_view { group["attempts"] = json!(group["attempts"].as_u64().unwrap_or(0) + 1); }
                    if record.verifications.iter().any(|v| window.is_none_or(|w| record.verification_recorded_at.get(&v.verification_ref).and_then(|t| w.contains_rfc3339(t)) == Some(true))) && verified_attempts.insert(attempt_ref.as_str()) {
                        group["verified"] = json!(group["verified"].as_u64().unwrap_or(0) + 1);
                    }
                    if record.readable_return.is_some() && window.is_none_or(|w| record.return_recorded_at.as_deref().and_then(|t| w.contains_rfc3339(t)) == Some(true)) && returned_attempts.insert(attempt_ref.as_str()) {
                        group["returned"] = json!(group["returned"].as_u64().unwrap_or(0) + 1);
                    }
                    if (!record.failure_evidence_refs.is_empty()
                        || record.dispatch.as_ref().is_some_and(|receipt| receipt.phase == crate::attempt_runtime::OwnerOperationPhase::Failed)
                        || record.observations.iter().any(|receipt| receipt.phase == crate::attempt_runtime::OwnerOperationPhase::Failed))
                        && window.is_none_or(|w| record.failure_recorded_at.as_deref()
                            .and_then(|t| w.contains_rfc3339(t)) == Some(true))
                        && failed_attempts.insert(attempt_ref.as_str()) {
                        group["withFailureEvidence"] =
                            json!(group["withFailureEvidence"].as_u64().unwrap_or(0) + 1);
                    }
                    if drill_down && first_view {
                        group["occasions"]
                            .as_array_mut()
                            .expect("occasions array")
                            .push(json!(attempt_ref));
                    }
                }
            }
            json!({
                "template": template,
                "denominator": {"attempts": total, "window": window.map(CivilWindow::as_json).unwrap_or(json!("whole provider state"))},
                "groups": groups,
            })
        }
        // §6: missing evidence — correlations whose temporal provenance or
        // Git basis was never recorded.
        "correlation-completeness" => {
            let mut incomplete: Vec<Value> = Vec::new();
            let total = state.execution_correlations.iter().filter(|c| window.is_none_or(|w| correlation_in_window(c, w))).count();
            for correlation in &state.execution_correlations {
                if window.is_some_and(|w| !correlation_in_window(correlation, w)) { continue; }
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
        // A later verification can measure Return-to-verification latency.
        // The initial verification precedes Return by contract; older records
        // and attempts without a later receipt remain unavailable.
        "return-to-verification" => {
            let mut attempts = 0usize;
            let mut with_verification = 0usize;
            let mut timed = 0usize;
            let mut total_ms: i128 = 0;
            let mut named: Vec<Value> = Vec::new();
            let mut views: BTreeMap<&str, Vec<(&crate::core::run::RunRef, &crate::attempt_runtime::FactoryAttemptRecord)>> = BTreeMap::new();
            for (run_ref, run_attempts) in &state.attempt_states {
                for (attempt_ref, record) in run_attempts.attempts() {
                    if window.is_some_and(|w| !attempt_in_window(record, w)) { continue; }
                    views.entry(attempt_ref.as_str()).or_default().push((run_ref, record));
                }
            }
            for (attempt_ref, records) in &views {
                    attempts += 1;
                    if records.iter().any(|(_, record)| record.verifications.iter().any(|v| window.is_none_or(|w| record.verification_recorded_at.get(&v.verification_ref).and_then(|t| w.contains_rfc3339(t)) == Some(true)))) { with_verification += 1; }
                    let later = records.iter().filter_map(|(run_ref, record)| {
                    let returned_at = record.return_recorded_at.as_deref().and_then(parse_ms);
                    returned_at.zip(record.verification_count_at_return).and_then(|(return_ms, prior_count)| {
                        record.verifications.iter().skip(prior_count).filter_map(|v| {
                            let at = record.verification_recorded_at.get(&v.verification_ref).and_then(|t| parse_ms(t))?;
                            (at >= return_ms && window.is_none_or(|w| w.contains_ms(at)))
                                .then_some((at, v))
                        }).min_by_key(|(at, _)| *at).map(|(at, v)| (at - return_ms, v, *run_ref, *record))
                    })
                    }).min_by_key(|(duration, _, _, _)| *duration);
                    if let Some((duration, _, _, _)) = later { timed += 1; total_ms += i128::from(duration); }
                    if drill_down {
                        let (first_run, first_record) = records[0];
                        named.push(json!({
                            "attemptRef": attempt_ref, "runRef": later.map(|(_, _, r, _)| r.to_string()).unwrap_or_else(|| first_run.to_string()),
                            "returnRecordedAt": later.map(|(_, _, _, record)| &record.return_recorded_at).unwrap_or(&first_record.return_recorded_at),
                            "laterVerificationRef": later.map(|(_, v, _, _)| &v.verification_ref),
                            "durationMs": later.map(|(duration, _, _, _)| duration),
                            "availability": if later.is_some() { "available" } else { "unavailable" },
                        }));
                    }
                    included += 1;
            }
            json!({
                "template": template,
                "denominator": {
                    "attempts": attempts,
                    "withVerification": with_verification,
                    "withTimedLaterVerification": timed,
                    "timingUnavailable": attempts - timed,
                },
                "verificationTargets": if drill_down { json!(named) } else { json!([]) },
                "meanDurationMs": if timed == 0 { Value::Null } else { json!(total_ms / timed as i128) },
                "disclosure": "Only Factory-admitted verification receipts after a readable Return establish this duration. Initial verification precedes Return; old or missing times remain unavailable.",
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
    full["window"] = window
        .map(CivilWindow::as_json)
        .unwrap_or(json!("whole provider state"));
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

/// Watch interval resolution: an explicit `--interval` always wins; the
/// applied `software-factory:telemetry:watch-interval-seconds` setting
/// replaces only the built-in default. The configured value is never compared
/// against a sentinel, so `--interval 2` stays 2 even when a setting is
/// applied — the same "flags win" law the search face states.
fn resolve_watch_interval(explicit: Option<f64>, configured: Option<f64>) -> f64 {
    explicit.or(configured).unwrap_or(2.0)
}

/// The state is a single revisioned document with one writer (the locked
/// owner mutations). A watcher therefore polls the revision and emits the
/// correlations of every revision past its cursor as JSONL — bounded by
/// `--max-events`, `--duration` or Ctrl-C — and always ends by naming the
/// cursor a resume should carry.
fn watch(args: &[String], _json: bool) -> Result<String, CliError> {
    let mut interval_secs: Option<f64> = None;
    let mut max_events = 100usize;
    let mut duration_secs = 30.0f64;
    let mut resume_revision: Option<u64> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut iterator = args.iter().cloned();
    while let Some(arg) = iterator.next() {
        match arg.as_str() {
            "--interval" => {
                interval_secs = Some(
                    iterator
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or_else(|| CliError::new("--interval requires seconds"))?,
                );
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
    let interval_secs = resolve_watch_interval(
        interval_secs,
        effective
            .get(crate::configuration::TELEMETRY_WATCH_INTERVAL_SETTING)
            .copied(),
    );
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
    fn public_cli_day_stats_exclude_untimed_native_records() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("Control/user")).unwrap();
        std::fs::create_dir_all(temp.path().join("Work")).unwrap();
        std::fs::write(temp.path().join("Control/user/civil-time-policy.json"),
            r#"{"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true}"#).unwrap();
        let state = temp.path().join("Work/native-state.json");
        create_developmental_conformance_state(&state).unwrap();
        let output = crate::cli::execute_cli(
            &[
                "telemetry".into(),
                "stats".into(),
                state.display().to_string(),
                "--day".into(),
                "2026-03-29".into(),
                "--json".into(),
            ],
            None,
        )
        .unwrap();
        let document = parse(&output);
        assert_eq!(document["window"]["timezone"], "Europe/London");
        assert_eq!(document["correlations"]["total"], 0);
        assert_eq!(document["timingUnavailable"]["correlations"], 1);
        assert!(document["attempts"]["observations"].is_null());

        let compared = crate::cli::execute_cli(
            &[
                "telemetry".into(),
                "stats".into(),
                state.display().to_string(),
                "--day".into(),
                "2026-03-29".into(),
                "--compare-previous".into(),
                "--json".into(),
            ],
            None,
        )
        .unwrap();
        let compared = parse(&compared);
        assert_eq!(
            compared["periodComparison"]["current"]["window"]["fromDay"],
            "2026-03-29"
        );
        assert_eq!(
            compared["periodComparison"]["previous"]["window"]["throughDay"],
            "2026-03-28"
        );
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
    fn explicit_watch_interval_beats_the_configured_default() {
        // The sentinel-free law: the flag wins, the setting is only a default.
        assert_eq!(resolve_watch_interval(Some(2.0), Some(30.0)), 2.0);
        assert_eq!(resolve_watch_interval(None, Some(30.0)), 30.0);
        assert_eq!(resolve_watch_interval(Some(5.0), None), 5.0);
        assert_eq!(resolve_watch_interval(None, None), 2.0);
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

    /// A public `actuation.model-usage/v1` observation re-bound to the
    /// conformance state's Agency/execution, as its owner would report it.
    fn observed_usage(input: u64, output: u64) -> Value {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../contracts/factory/fixtures/execution-telemetry-reading.json"
        ))
        .unwrap();
        let mut observation = fixture["modelUsage"]["observations"][0].clone();
        let usage = &mut observation["modelUsage"];
        usage["actuation_ref"] = json!("actuation:factory-conformance");
        usage["correlation"] = json!({
            "agent_ref": "agent:factory-conformance",
            "agency_ref": "agency:factory/conformance",
            "harness_ref": "harness:factory-native-cli",
            "external_refs": []
        });
        usage["tokens"] =
            json!({"standing": "provider-reported", "input": input, "output": output});
        usage["cost"] =
            json!({"standing": "provider-reported", "amount": 0.042, "currency": "USD"});
        usage["timing"] = json!({
            "started_at": "2026-09-23T09:00:00Z",
            "completed_at": "2026-09-23T09:02:30Z",
            "latency": {"standing": "observed", "milliseconds": 150000.0}
        });
        observation
    }

    fn record_correlation(correlation: &Value, suffix: &str) -> Value {
        json!({
            "contract": "factory.developmental-mutation-request/v1",
            "mutationRef": format!("mutation:usage-{suffix}"),
            "occurrenceRef": format!("occurrence:usage-{suffix}"),
            "source": {
                "owner": "factory",
                "reference": correlation["correlationRef"],
                "revision": "r1",
                "standing": "owner-native-observation"
            },
            "observedAt": "2026-09-23T09:03:00Z",
            "mutation": {"kind": "record-execution-correlation", "correlation": correlation}
        })
    }

    #[test]
    fn recorded_usage_is_validated_retained_and_read_back_normalised() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("state.json");
        create_developmental_conformance_state(&path).unwrap();
        let state: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let original = &state["state"]["executionCorrelations"][0];
        let mut provider = FactoryDevelopmentalFileProvider::open(&path).unwrap();

        // An owner observation that claims counts under an absent standing is
        // refused, and the state is unchanged.
        let mut dishonest = original.clone();
        dishonest["correlationRef"] = json!("execution-correlation:01M2S15SA8Q9CQ2SFN130Z5HT0");
        dishonest["telemetryRef"] = json!("telemetry:01M2S15SA8YGJVA2RCSXWB2MP0");
        let mut observation = observed_usage(1, 1);
        observation["modelUsage"]["tokens"] = json!({"standing": "not-reported", "input": 7});
        dishonest["modelUsage"] = json!({
            "owner": "actuation", "availability": "available", "observations": [observation]
        });
        let before = std::fs::read(&path).unwrap();
        let refused = provider
            .apply_developmental_mutation(
                serde_json::from_value(record_correlation(&dishonest, "dishonest")).unwrap(),
            )
            .expect_err("counts under an absent standing are refused");
        assert!(
            refused.to_string().contains("tokens cannot carry counts"),
            "{refused}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);

        // An honest observation is admitted through the real producer path.
        let mut correlation = original.clone();
        correlation["correlationRef"] = json!("execution-correlation:01M2S15SA8Q9CQ2SFN130Z5HT1");
        correlation["telemetryRef"] = json!("telemetry:01M2S15SA8YGJVA2RCSXWB2MP1");
        correlation["modelUsage"] = json!({
            "owner": "actuation", "availability": "available",
            "observations": [observed_usage(1200, 340)]
        });
        provider
            .apply_developmental_mutation(
                serde_json::from_value(record_correlation(&correlation, "honest")).unwrap(),
            )
            .expect("an honest owner observation is admitted");

        // Retained: a fresh open still carries the owner evidence.
        let reopened = FactoryDevelopmentalFileProvider::open(&path).unwrap();
        let retained = reopened
            .state()
            .execution_correlations
            .iter()
            .find(|c| c.telemetry_ref.to_string() == "telemetry:01M2S15SA8YGJVA2RCSXWB2MP1")
            .unwrap();
        assert_eq!(retained.model_usage.observations.len(), 1);

        // `factory telemetry inspect` exposes the normalised usage.
        let document = parse(
            &execute(
                &telemetry_args(&[
                    "inspect",
                    path.to_str().unwrap(),
                    "telemetry:01M2S15SA8YGJVA2RCSXWB2MP1",
                    "--json",
                ]),
                true,
            )
            .unwrap(),
        );
        let usage = &document["reading"]["usage"];
        assert_eq!(usage["contract"], "factory.execution-usage/v1");
        assert_eq!(usage["inputTokens"], 1200);
        assert_eq!(usage["outputTokens"], 340);
        assert_eq!(usage["cacheReadTokens"], 26788);
        assert_eq!(usage["cacheWriteTokens"], 53010);
        assert_eq!(usage["cost"], json!({"amount": 0.042, "currency": "USD"}));
        assert_eq!(usage["startedAt"], "2026-09-23T09:00:00Z");
        assert_eq!(usage["endedAt"], "2026-09-23T09:02:30Z");
        let text = execute(
            &telemetry_args(&[
                "inspect",
                path.to_str().unwrap(),
                "telemetry:01M2S15SA8YGJVA2RCSXWB2MP1",
            ]),
            false,
        )
        .unwrap();
        assert!(text.contains("Usage: in 1200 · out 340"), "{text}");

        // The build view exposes it per execution, next to the unobserved one.
        let run_ref = correlation["runRef"].as_str().unwrap();
        let project_ref = state["state"]["build"]["project"]["ref"].as_str().unwrap();
        let snapshot: Value = serde_json::from_str(
            &crate::cli::execute_cli(
                &[
                    "build".into(),
                    "snapshot".into(),
                    path.display().to_string(),
                    project_ref.into(),
                    run_ref.into(),
                    "--json".into(),
                ],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let entries = snapshot["view"]["executionUsage"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["inputTokens"] == 1200));
        assert!(entries
            .iter()
            .any(|e| e["inputTokens"].is_null() && e["availability"] == "unavailable"));
    }
}
