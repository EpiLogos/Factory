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

/// Hard bounds: a monitoring surface must not become an unbounded scan.
const SEARCH_DEFAULT_LIMIT: usize = 10;
const SEARCH_MAX_LIMIT: usize = 100;
const DOCTOR_NOW_REF_PROBES: usize = 5;
const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(15);

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
        "export" => export(rest, json),
        "doctor" => doctor(rest, json),
        other => Err(CliError::new(format!(
            "unknown telemetry operation `{other}`; expected status|inspect|search|stats|export|doctor"
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
    let mut limit = SEARCH_DEFAULT_LIMIT;
    let mut positional = Vec::new();
    let mut iterator = rest.into_iter();
    while let Some(arg) = iterator.next() {
        match arg.as_str() {
            "--regex" => regex = true,
            "--aikit" => aikit_bin = iterator.next(),
            "--limit" => {
                limit = iterator
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| CliError::new("--limit requires a number"))?;
            }
            other => positional.push(other.to_string()),
        }
    }
    rest = positional;
    let limit = limit.min(SEARCH_MAX_LIMIT);
    let state_path = require_state_path(&rest)?;
    let query = rest
        .get(1)
        .cloned()
        .ok_or_else(|| CliError::new("missing search query"))?;

    let provider = FactoryDevelopmentalFileProvider::open(&state_path)
        .map_err(|error| CliError::new(error.to_string()))?;

    let aikit = aikit_bin
        .or_else(|| std::env::var("FACTORY_AIKIT_BIN").ok())
        .unwrap_or_else(|| "aikit".into());
    let mut argv = vec![
        aikit.clone(),
        "--json".into(),
        "knowledge".into(),
        "search".into(),
        query.clone(),
        "--limit".into(),
        limit.to_string(),
    ];
    if regex {
        // AIKit's knowledge contract searches literal by default; the explicit
        // regex path is a deliberate caller decision mirrored flag-for-flag.
        argv.push("--regex".into());
    }
    let output = run_command(&argv);
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
    let start = std::time::Instant::now();
    let timeout = SUBPROCESS_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if status.success() {
                    return Ok(stdout);
                }
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Err(format!(
                    "exited with {status}{}",
                    if stderr.trim().is_empty() {
                        String::new()
                    } else {
                        format!(": {}", stderr.trim())
                    }
                ));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("timed out after {timeout:?}"));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(format!("wait failed: {error}")),
        }
    }
}

// ---------------------------------------------------------------------------
// stats — deterministic aggregates with explicit denominators
// ---------------------------------------------------------------------------

fn stats(args: &[String], json: bool) -> Result<String, CliError> {
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
            let key = serde_json::to_string(&record.disposition)
                .unwrap_or_else(|_| "unknown".into())
                .trim_matches('"')
                .to_string();
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
    let mut cursor = Some(from.to_path_buf());
    while let Some(current) = cursor {
        if current.join("Control").is_dir() && current.join("Work").is_dir() {
            return Some(current);
        }
        cursor = current.parent().map(Path::to_path_buf);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::create_developmental_conformance_state;
    use std::sync::Mutex;

    /// The doctor probes real executables; serialise the environment-touching
    /// tests so parallel cargo test threads cannot race a probe.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn conformance_state() -> (
        tempfile::TempDir,
        crate::conformance::FactoryDevelopmentalConformanceManifest,
    ) {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let path = temp.path().join("conformance-state.json");
        let manifest = create_developmental_conformance_state(&path).expect("conformance state");
        (temp, manifest)
    }

    fn telemetry_args(words: &[&str]) -> Vec<String> {
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
