//! Read configured native/GitHub sources with explicit enumeration coverage.
//! No arbitrary command from policy executes; unsupported integrations remain
//! unavailable until an actual provider owns their read contract.

use crate::developmental_read::FactoryDevelopmentalState;
use crate::sensing::{Coverage, CoverageState, Interval, Observation, Policy, Source};
use chrono::DateTime;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn now_ms() -> i64 {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    now.timestamp_millis()
}

/// Read-only/native owner transport. Pipe readers drain concurrently, including
/// failures, so full GitHub pages cannot deadlock the collector.
pub fn owner_json(program: &str, args: &[String], input: Option<&Value>) -> Result<Value, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let out = child.stdout.take();
    let err = child.stderr.take();
    let stdout = std::thread::spawn(move || match out {
        Some(pipe) => drain_bounded(pipe, 16 * 1024 * 1024 + 1),
        None => Ok(vec![]),
    });
    let stderr = std::thread::spawn(move || {
        err.map(|pipe| drain_bounded(pipe, 1024 * 1024).unwrap_or_default())
            .unwrap_or_default()
    });
    if let Some(input) = input {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input.to_string().as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            return Err(format!("{program}: owner read timed out after 60s"));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let bytes = stdout.join().map_err(|_| "owner stdout reader failed")??;
    let errors = stderr.join().map_err(|_| "owner stderr reader failed")?;
    if !status.success() {
        return Err(format!(
            "{program} exited {status}: {}",
            String::from_utf8_lossy(&errors)
                .chars()
                .take(400)
                .collect::<String>()
        ));
    }
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("owner response exceeded 16 MiB; coverage is partial".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("{program}: invalid owner JSON: {e}"))
}

fn drain_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        let keep = read.min(limit.saturating_sub(data.len()));
        data.extend_from_slice(&chunk[..keep]);
    }
    Ok(data)
}

pub fn read(
    source: &Source,
    window: &Interval,
    state: &FactoryDevelopmentalState,
) -> (Coverage, Vec<Observation>) {
    let mut coverage = Coverage {
        source_ref: source.source_ref.clone(),
        provider_ref: format!("provider/{}", source.provider),
        scope: source.scope.clone(),
        state: CoverageState::Complete,
        window: window.clone(),
        records: 0,
        pages: 0,
        cursor: None,
        reason: None,
        query_basis: json!({"arguments":source.arguments,"interval":"[since,until)"}),
    };
    let result = match source.provider.as_str() {
        "factory" => native(source, window, state, &mut coverage),
        "github" => github(source, window, &mut coverage),
        _ => Err(format!("{} has no connected native read adapter; configuration does not install an integration", source.provider)),
    };
    let observations = match result {
        Ok(observations) => observations,
        Err(error) => {
            coverage.state = CoverageState::Unavailable;
            coverage.reason = Some(error);
            vec![]
        }
    };
    coverage.records = observations.len();
    if observations.is_empty() && coverage.state == CoverageState::Complete {
        coverage.state = CoverageState::Empty;
    }
    (coverage, observations)
}

/// Re-read the configured native owner before consequential work. The caller
/// still binds the retained signal revision inside its subsequent transaction.
pub fn verify_current(
    observation: &Observation,
    state: &FactoryDevelopmentalState,
    policy: &Policy,
) -> Result<(), String> {
    policy.validate()?;
    let flow = policy
        .workflows
        .get("collect")
        .filter(|flow| flow.enabled)
        .ok_or("current source verification requires enabled collection policy")?;
    let sources = flow
        .sources
        .iter()
        .filter_map(|id| policy.sources.iter().find(|s| &s.id == id));
    for source in sources {
        if source.provider != observation.provider_ref {
            continue;
        }
        if source.provider == "factory" {
            if source.scope != policy.project_world_ref {
                continue;
            }
            let mut coverage = Coverage {
                source_ref: source.source_ref.clone(),
                provider_ref: "provider/factory".into(),
                scope: source.scope.clone(),
                state: CoverageState::Complete,
                window: Interval {
                    since_unix_ms: i64::MIN,
                    until_unix_ms: i64::MAX,
                },
                records: 0,
                pages: 0,
                cursor: None,
                reason: None,
                query_basis: json!({}),
            };
            let records = native(source, &coverage.window.clone(), state, &mut coverage)?;
            if records.iter().any(|current| {
                current.source_ref == observation.source_ref
                    && current.source_revision == observation.source_revision
            }) {
                return Ok(());
            }
        } else if source.provider == "github" {
            if !valid_github_scope(&source.scope) {
                return Err("GitHub scope must be an exact owner/repository".into());
            }
            let kind = source
                .arguments
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("issues");
            let (prefix, endpoint) = match kind {
                "issues" => (
                    format!("https://github.com/{}/issues/", source.scope),
                    "issues",
                ),
                "actions" => (
                    format!("https://github.com/{}/actions/runs/", source.scope),
                    "actions/runs",
                ),
                _ => continue,
            };
            let Some(number) = observation
                .source_ref
                .strip_prefix(&prefix)
                .filter(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
            else {
                continue;
            };
            let value = owner_json(
                "gh",
                &[
                    "api".into(),
                    "--method".into(),
                    "GET".into(),
                    format!("repos/{}/{endpoint}/{number}", source.scope),
                ],
                None,
            )?;
            if value["html_url"] != observation.source_ref {
                return Err(
                    "live GitHub source URL no longer matches the configured repository".into(),
                );
            }
            if kind == "issues" && value.get("pull_request").is_some() {
                return Err("configured issue source now resolves to a pull request".into());
            }
            if kind == "actions"
                && !["failure", "timed_out", "action_required", "startup_failure"]
                    .contains(&value["conclusion"].as_str().unwrap_or(""))
            {
                return Err("live GitHub run no longer has a configured failure conclusion".into());
            }
            let current_revision = github_fingerprint(kind, &value)?;
            if current_revision == observation.source_revision {
                return Ok(());
            }
            return Err("live GitHub source revision changed; recollect and reclassify before commissioning".into());
        }
    }
    Err("retained observation has no matching current configured owner source/revision".into())
}

/// The list and detail endpoints publish different peripheral fields. Hash the
/// same material source facts on both paths so an exact detail re-read can
/// verify a collected observation without accepting a stale changed record.
fn github_fingerprint(kind: &str, row: &Value) -> Result<String, String> {
    let url = row["html_url"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("GitHub source URL absent")?;
    let id = row["id"].as_u64().ok_or("GitHub source ID absent")?;
    let updated = row["updated_at"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("GitHub source update time absent")?;
    let basis = match kind {
        "issues" => {
            let number = row["number"].as_u64().ok_or("GitHub issue number absent")?;
            let labels = row["labels"]
                .as_array()
                .ok_or("GitHub issue labels absent")?
                .iter()
                .map(|label| {
                    label["name"]
                        .as_str()
                        .ok_or("GitHub issue label name absent")
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut labels = labels;
            labels.sort_unstable();
            json!({"kind":kind,"id":id,"url":url,"number":number,"state":row["state"],
                "title":row["title"],"body":row["body"],"created_at":row["created_at"],
                "updated_at":updated,"closed_at":row["closed_at"],"labels":labels})
        }
        "actions" => json!({"kind":kind,"id":id,"url":url,"head_sha":row["head_sha"],
            "status":row["status"],"conclusion":row["conclusion"],"created_at":row["created_at"],
            "updated_at":updated,"run_attempt":row["run_attempt"]}),
        _ => return Err("unsupported GitHub source kind".into()),
    };
    Ok(format!(
        "blake3:{}",
        blake3::hash(basis.to_string().as_bytes()).to_hex()
    ))
}

fn observation(
    reference: String,
    revision: String,
    time: Option<i64>,
    summary: String,
    dimension: &str,
    refs: Vec<String>,
    provider: &str,
) -> Observation {
    Observation {
        source_ref: reference,
        provider_ref: provider.into(),
        source_revision: revision,
        occurred_at_unix_ms: time,
        observed_at_unix_ms: now_ms(),
        summary: summary.chars().take(600).collect(),
        dimension: dimension.into(),
        standing: if provider == "factory" {
            "observed"
        } else {
            "provider-reported"
        }
        .into(),
        relation_refs: refs,
    }
}

fn native(
    source: &Source,
    window: &Interval,
    state: &FactoryDevelopmentalState,
    coverage: &mut Coverage,
) -> Result<Vec<Observation>, String> {
    let mut result = vec![];
    let kind = source
        .arguments
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("all");
    if !["all", "attempts", "custody", "correlations"].contains(&kind) {
        return Err(format!("unsupported Factory source kind {kind}"));
    }
    coverage.pages = 1;
    coverage.query_basis["temporal_basis"] =
        json!("timestamped events inside interval; untimed current anomalies disclosed separately");
    coverage.query_basis["stalled_basis"] =
        json!("explicit blocked custody only; age or silence alone does not establish a stall");
    let mut unknown_time = 0;
    let mut current_outside_interval = 0;
    if kind == "all" || kind == "attempts" {
        for (run, attempts) in &state.attempt_states {
            for attempt in attempts.attempts().values() {
                let refs = vec![run.to_string(), attempt.workflow_unit_ref.to_string()];
                let failed_dispatch = attempt.dispatch.as_ref().filter(|receipt| {
                    receipt.phase == crate::attempt_runtime::OwnerOperationPhase::Failed
                });
                let failed_observations = attempt
                    .observations
                    .iter()
                    .filter(|receipt| {
                        receipt.phase == crate::attempt_runtime::OwnerOperationPhase::Failed
                    })
                    .collect::<Vec<_>>();
                if !attempt.failure_evidence_refs.is_empty()
                    || failed_dispatch.is_some()
                    || !failed_observations.is_empty()
                {
                    let time = attempt.failure_recorded_at.as_deref().and_then(parse_time);
                    if time.is_none_or(|t| window.contains(t)) {
                        if time.is_none() {
                            unknown_time += 1;
                        }
                        // Only the owner-admitted failure facts define this
                        // source revision. A later successful verification or
                        // Return cannot silently erase its classification.
                        let failure_basis = json!({
                            "failureEvidenceRefs": attempt.failure_evidence_refs,
                            "failedDispatch": failed_dispatch,
                            "failedObservations": failed_observations,
                            "failureRecordedAt": attempt.failure_recorded_at,
                        });
                        let mut failure_refs = refs.clone();
                        failure_refs.extend(attempt.failure_evidence_refs.iter().cloned());
                        failure_refs.extend(failed_dispatch.map(|r| r.receipt_ref.clone()));
                        failure_refs
                            .extend(failed_observations.iter().map(|r| r.receipt_ref.clone()));
                        failure_refs.sort();
                        failure_refs.dedup();
                        result.push(observation(
                            attempt.attempt_ref.clone(),
                            format!(
                                "blake3:{}",
                                blake3::hash(failure_basis.to_string().as_bytes()).to_hex()
                            ),
                            time,
                            "Attempt has owner-admitted failure evidence".into(),
                            "developmental",
                            failure_refs,
                            "factory",
                        ));
                    }
                }
                for verification in &attempt.verifications {
                    if verification.outcome != crate::attempt_runtime::VerificationOutcome::Failed {
                        continue;
                    }
                    let time = attempt
                        .verification_recorded_at
                        .get(&verification.verification_ref)
                        .and_then(|t| parse_time(t));
                    if time.is_some_and(|t| !window.contains(t)) {
                        continue;
                    }
                    if time.is_none() {
                        unknown_time += 1;
                    }
                    let revision = format!(
                        "blake3:{}",
                        blake3::hash(
                            serde_json::to_string(verification)
                                .map_err(|e| e.to_string())?
                                .as_bytes()
                        )
                        .to_hex()
                    );
                    let mut relation = refs.clone();
                    relation.push(attempt.attempt_ref.clone());
                    relation.extend(verification.evidence_refs.iter().cloned());
                    result.push(observation(
                        verification.verification_ref.clone(),
                        revision,
                        time,
                        "Native verification failed".into(),
                        "developmental",
                        relation,
                        "factory",
                    ));
                }
            }
            for leg in attempts.snapshot().legs().values() {
                for pair in leg.attempts.windows(2) {
                    let prior = attempts.attempts().values().find(|record| {
                        record
                            .execution_ref
                            .as_deref()
                            .unwrap_or(&record.reserved_execution_ref)
                            == pair[0].execution_ref
                    });
                    let retried = attempts.attempts().values().find(|record| {
                        record
                            .execution_ref
                            .as_deref()
                            .unwrap_or(&record.reserved_execution_ref)
                            == pair[1].execution_ref
                    });
                    let (Some(prior), Some(retried), Some(grant)) = (
                        prior,
                        retried,
                        retried
                            .and_then(|record| record.disposition.budget.retry_grant_ref.as_ref()),
                    ) else {
                        continue;
                    };
                    let time = retried.attempt_recorded_at.as_deref().and_then(parse_time);
                    if time.is_some_and(|time| !window.contains(time)) {
                        continue;
                    }
                    if time.is_none() {
                        unknown_time += 1;
                    }
                    let basis = json!({"runRef":run.to_string(),"priorAttemptRef":prior.attempt_ref,
                        "retryAttemptRef":retried.attempt_ref,"grantRef":grant,"admittedAt":retried.attempt_recorded_at});
                    result.push(observation(
                        format!("factory:attempt-retry:{}", retried.attempt_ref),
                        format!(
                            "blake3:{}",
                            blake3::hash(basis.to_string().as_bytes()).to_hex()
                        ),
                        time,
                        "Native Attempt retried after an earlier execution".into(),
                        "developmental",
                        vec![
                            run.to_string(),
                            prior.attempt_ref.clone(),
                            retried.attempt_ref.clone(),
                            grant.clone(),
                        ],
                        "factory",
                    ));
                }
            }
        }
    }
    if kind == "all" || kind == "custody" {
        for custody in &state.work_custody {
            if custody.state == crate::work_custody::CustodyState::Blocked
                && window.contains(custody.updated_at_unix_ms)
            {
                result.push(observation(
                    custody.custody_ref.clone(),
                    format!("revision:{}", custody.revision),
                    Some(custody.updated_at_unix_ms),
                    format!("Blocked custody: {}", custody.reason),
                    "developmental",
                    vec![custody.work_ref.clone(), custody.position_ref.clone()],
                    "factory",
                ));
            }
            if custody.state == crate::work_custody::CustodyState::Completed
                && window.contains(custody.updated_at_unix_ms)
            {
                if let Some(run_ref) = custody.run_ref.as_ref() {
                    let attempts = state.attempt_states.get(run_ref);
                    let has_return = attempts.is_some_and(|attempts| {
                        attempts
                            .attempts()
                            .values()
                            .any(|record| record.readable_return.is_some())
                    });
                    if !has_return {
                        let mut refs = vec![
                            custody.custody_ref.clone(),
                            custody.work_ref.clone(),
                            custody.position_ref.clone(),
                            run_ref.to_string(),
                        ];
                        if let Some(attempts) = attempts {
                            refs.extend(attempts.attempts().keys().cloned());
                        }
                        refs.sort();
                        refs.dedup();
                        let basis = json!({"custodyRef":custody.custody_ref,"revision":custody.revision,
                            "runRef":run_ref.to_string(),"state":custody.state,"attemptRefs":refs});
                        result.push(observation(
                            format!("factory:missing-return:{}", custody.custody_ref),
                            format!(
                                "blake3:{}",
                                blake3::hash(basis.to_string().as_bytes()).to_hex()
                            ),
                            Some(custody.updated_at_unix_ms),
                            "Completed custody has no native readable Attempt Return".into(),
                            "developmental",
                            refs,
                            "factory",
                        ));
                    }
                }
            }
        }
        for signal in state.sensing.signals.values() {
            let Some(work) = signal.work.as_ref().filter(|work| work.now_ref.is_none()) else {
                continue;
            };
            let occurred = work.created_at_unix_ms;
            if !window.contains(occurred) {
                // The missing join is current, but its known onset predates
                // this interval. Keep the owner's timestamp and disclose the
                // out-of-window current-state inclusion separately.
                current_outside_interval += 1;
            }
            let source_ref = format!(
                "factory:signal-now:{}",
                signal.signal_ref.trim_start_matches("factory:signal:")
            );
            let basis = json!({"signalRef":signal.signal_ref,"custodyRef":work.custody_ref,
                "workRef":work.work_ref,"runRef":work.run_ref,"positionRef":work.position_ref,
                "workCreatedAt":work.created_at_unix_ms,"childNowRef":null});
            let mut relation_refs = vec![
                signal.signal_ref.clone(),
                signal.observation.source_ref.clone(),
                work.custody_ref.clone(),
                work.work_ref.clone(),
                work.position_ref.clone(),
            ];
            relation_refs.extend(work.run_ref.iter().cloned());
            relation_refs.sort();
            relation_refs.dedup();
            result.push(observation(
                source_ref,
                format!(
                    "blake3:{}",
                    blake3::hash(basis.to_string().as_bytes()).to_hex()
                ),
                Some(occurred),
                "Commissioned work has no joined child NOW".into(),
                "developmental",
                relation_refs,
                "factory",
            ));
        }
    }
    if kind == "all" || kind == "correlations" {
        for c in &state.execution_correlations {
            let value = serde_json::to_value(c).map_err(|e| e.to_string())?;
            if c.temporal.child_now_ref.is_some() && !c.temporal.day_refs.is_empty() {
                continue;
            }
            let reference = value["telemetryRef"]
                .as_str()
                .or(value["executionRef"].as_str())
                .unwrap_or("");
            if reference.is_empty() {
                continue;
            }
            unknown_time += 1;
            result.push(observation(
                reference.into(),
                format!(
                    "blake3:{}",
                    blake3::hash(value.to_string().as_bytes()).to_hex()
                ),
                None,
                "Execution correlation lacks child NOW or Day provenance".into(),
                "contextual",
                vec![],
                "factory",
            ));
        }
    }
    if unknown_time > 0 || current_outside_interval > 0 {
        coverage.state = CoverageState::Truncated;
        coverage.reason = Some(format!(
            "{unknown_time} current anomalies lack owner occurrence times; {current_outside_interval} current anomalies began before the interval and retain their known occurrence times; this read cannot establish complete interval coverage"
        ));
    }
    coverage.cursor = Some(format!("build:{}", state.build.revision().get()));
    Ok(result)
}

fn parse_time(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.timestamp_millis())
}

fn valid_github_scope(scope: &str) -> bool {
    let parts: Vec<_> = scope.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

fn github(
    source: &Source,
    window: &Interval,
    coverage: &mut Coverage,
) -> Result<Vec<Observation>, String> {
    if !valid_github_scope(&source.scope) {
        return Err("GitHub scope must be an exact owner/repository".into());
    }
    let kind = source
        .arguments
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("issues");
    if !["issues", "actions"].contains(&kind) {
        return Err("GitHub adapter supports issue headers and Actions runs; other reads remain unsupported".into());
    }
    let since = chrono::DateTime::from_timestamp_millis(window.since_unix_ms)
        .ok_or("invalid interval")?
        .to_rfc3339();
    let mut observations = vec![];
    let max = source.page_limit.unwrap_or(10_000);
    for page in 1..=max {
        let endpoint = if kind == "issues" {
            format!("repos/{}/issues", source.scope)
        } else {
            format!("repos/{}/actions/runs", source.scope)
        };
        let mut args = vec![
            "api".into(),
            "--method".into(),
            "GET".into(),
            endpoint,
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            format!("page={page}"),
        ];
        if kind == "issues" {
            args.extend([
                "-f".into(),
                "state=all".into(),
                "-f".into(),
                format!("since={since}"),
                "-f".into(),
                "sort=updated".into(),
                "-f".into(),
                "direction=asc".into(),
            ]);
        } else {
            args.extend(["-f".into(), format!("created=>={since}")]);
        }
        let value = match owner_json("gh", &args, None) {
            Ok(value) => value,
            Err(error) if page > 1 => {
                coverage.state = CoverageState::Truncated;
                coverage.reason = Some(error);
                coverage.cursor = Some(format!("page:{page}"));
                return Ok(observations);
            }
            Err(error) => return Err(error),
        };
        let rows = match if kind == "issues" {
            value.as_array()
        } else {
            value["workflow_runs"].as_array()
        } {
            Some(rows) => rows,
            None => {
                coverage.state = if page > 1 {
                    CoverageState::Truncated
                } else {
                    CoverageState::Unavailable
                };
                coverage.reason =
                    Some(format!("GitHub page {page} returned no enumerable records"));
                return Ok(observations);
            }
        };
        coverage.pages = page;
        coverage.query_basis["record_kind"] = json!(if kind == "issues" {
            "issue headers (discussion remains at source)"
        } else {
            "Actions workflow runs"
        });
        for row in rows {
            if kind == "issues" && row.get("pull_request").is_some() {
                continue;
            }
            // GitHub's Actions filter is on creation, not completion. Keep
            // that same clock in the observation instead of silently omitting
            // runs created before the window but completed inside it.
            coverage.query_basis["time_basis"] = json!(if kind == "actions" {
                "workflow run created_at; latest conclusion, not failure completion time"
            } else {
                "issue updated_at"
            });
            let occurred = if kind == "actions" {
                row["created_at"].as_str()
            } else {
                row["updated_at"].as_str()
            }
            .and_then(parse_time);
            if occurred.is_some_and(|t| !window.contains(t)) {
                continue;
            }
            if kind == "actions"
                && !["failure", "timed_out", "action_required", "startup_failure"]
                    .contains(&row["conclusion"].as_str().unwrap_or(""))
            {
                continue;
            }
            let Some(reference) = row["html_url"].as_str().filter(|reference| {
                reference.starts_with(&format!("https://github.com/{}/", source.scope))
            }) else {
                coverage.state = CoverageState::Truncated;
                coverage.reason=Some(format!("GitHub page {page} contained a missing or out-of-scope source URL; rejected record, retained scoped prefix"));
                continue;
            };
            let reference = reference.to_owned();
            if occurred.is_none() {
                coverage.state = CoverageState::Truncated;
                coverage.reason = Some(
                    "GitHub occurrence time is absent; interval completeness cannot be established"
                        .into(),
                );
            }
            let revision = match github_fingerprint(kind, row) {
                Ok(revision) => revision,
                Err(error) => {
                    coverage.state = CoverageState::Truncated;
                    coverage.reason = Some(format!(
                        "GitHub page {page} has no stable source fingerprint: {error}"
                    ));
                    continue;
                }
            };
            let summary = row["title"]
                .as_str()
                .or(row["display_title"].as_str())
                .or(row["name"].as_str())
                .unwrap_or("GitHub evidence")
                .to_owned();
            let refs = row["head_sha"]
                .as_str()
                .map(|sha| vec![format!("git:{}@{sha}", source.scope)])
                .unwrap_or_default();
            observations.push(observation(
                reference,
                revision,
                occurred,
                summary,
                if kind == "actions" {
                    "delivery"
                } else {
                    "product"
                },
                refs,
                "github",
            ));
        }
        coverage.cursor = Some(format!("page:{page}"));
        if rows.len() < 100 {
            return Ok(observations);
        }
        if page == max {
            coverage.state = CoverageState::Truncated;
            coverage.reason = Some(format!(
                "stopped at configured page limit {max}; next page was not read"
            ));
        }
    }
    Ok(observations)
}
