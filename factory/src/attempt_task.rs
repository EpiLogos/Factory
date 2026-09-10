//! Task-sized views of the existing native attempt and telemetry records.
//! This owns no task scheduler, copied Run, metric collector or completion flag.

use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_runtime::{FactoryAttemptReading, FactoryAttemptRecord, OwnerOperationPhase};
use crate::core::run::RunRef;
use crate::developmental_read::FactoryDevelopmentalState;
use crate::project_development_store::read_developmental_state;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const TASK_READING: &str = "factory.attempt-task-reading/v1";
pub const RETURN_READING: &str = "factory.attempt-return-reading/v1";
const MAX_PAGE: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCursor {
    pub revision: u64,
    pub after_attempt_ref: String,
}

/// The provider publishes by atomic replacement. Comparing before/after owner
/// state prevents a read from combining attempts and telemetry from two cuts.
fn coherent_read(
    path: &Path,
    run: &RunRef,
) -> Result<(FactoryDevelopmentalState, FactoryAttemptReading), String> {
    let store = FileAttemptStore::open_run(path, run.clone()).map_err(|error| error.to_string())?;
    for _ in 0..8 {
        let before = read_developmental_state(path).map_err(|error| error.to_string())?;
        let reading = store.reading().map_err(|error| error.to_string())?;
        let after = read_developmental_state(path).map_err(|error| error.to_string())?;
        if before == after {
            return Ok((after, reading));
        }
    }
    Err("native state changed throughout task read; retry a fresh bounded reading".into())
}

fn attempt_view(
    state: &FactoryDevelopmentalState,
    reading: &FactoryAttemptReading,
    attempt: &FactoryAttemptRecord,
) -> Value {
    let execution = attempt
        .execution_ref
        .as_deref()
        .unwrap_or(&attempt.reserved_execution_ref);
    let leg = reading.legs.get(&attempt.workflow_unit_ref);
    let current = leg.is_some_and(|leg| leg.execution_ref == execution);
    let historical = leg.and_then(|leg| {
        leg.attempts
            .iter()
            .find(|item| item.execution_ref == execution)
    });
    let status = if current {
        leg.map(|leg| leg.status)
    } else {
        historical.map(|item| item.status)
    };
    // A TrackingFact named model-usage is a reference, not measured usage. Only
    // the existing owner-validated telemetry joins can supply actual values.
    let correlations = state
        .execution_correlations
        .iter()
        .filter(|correlation| {
            correlation.run_ref == reading.run_ref
                && attempt.execution_ref.as_ref() == Some(&correlation.execution_ref)
                && correlation.workflow_unit_ref == attempt.workflow_unit_ref
        })
        .collect::<Vec<_>>();
    let model_observed = correlations
        .iter()
        .any(|value| !value.model_usage.observations.is_empty());
    let material_observed = correlations
        .iter()
        .any(|value| !value.material_usage.observations.is_empty());
    let mut latest = BTreeMap::new();
    for receipt in attempt.dispatch.iter().chain(&attempt.observations) {
        latest.insert((&receipt.owner_ref, &receipt.operation_ref), receipt);
    }
    let unresolved = latest.values().filter(|receipt| matches!(receipt.phase,
        OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain)).map(|receipt|
        json!({"ownerRef":receipt.owner_ref,"operationRef":receipt.operation_ref,"receiptRef":receipt.receipt_ref}))
        .collect::<Vec<_>>();
    let learning_refs = attempt
        .tracking
        .iter()
        .filter(|fact| fact.kind == "regression-observation")
        .map(|fact| fact.subject_ref.clone())
        .chain(
            attempt
                .readable_return
                .iter()
                .flat_map(|returned| returned.regression_observation_refs.iter().cloned()),
        )
        .collect::<BTreeSet<_>>();
    json!({
        "standing": if current {"current-attempt"} else {"historical-attempt"},
        "status":status,
        "record":attempt,
        "unresolvedOwnerOperations":unresolved,
        "ownerTelemetryCorrelations":correlations,
        "modelUsageStatus":if model_observed {"owner-observed"} else {"not-observed"},
        "materialUsageStatus":if material_observed {"owner-observed"} else {"not-observed"},
        "regressionObservationRefs":learning_refs,
        "receivingStanding":if attempt.readable_return.as_ref().and_then(|value| value.receiving_ref.as_ref()).is_some() {
            "owner-receipt-linked-not-recognition"
        } else {"not-linked"},
        "archiveStanding":if attempt.readable_return.as_ref().is_some_and(|value| !value.archive_refs.is_empty()) {
            "owner-references-retained-not-lifecycle-proof"
        } else {"not-linked"}
    })
}

pub fn read_task(
    path: &Path,
    run: &RunRef,
    task_ref: &str,
    limit: usize,
    cursor: Option<&TaskCursor>,
) -> Result<Value, String> {
    if task_ref.trim().is_empty() || limit == 0 || limit > MAX_PAGE {
        return Err("task ref and a page limit between 1 and 100 are required".into());
    }
    let (state, reading) = coherent_read(path, run)?;
    let mut attempts = reading
        .attempts
        .iter()
        .filter(|attempt| attempt.task_ref == task_ref)
        .collect::<Vec<_>>();
    attempts.sort_by(|left, right| left.attempt_ref.cmp(&right.attempt_ref));
    if attempts.is_empty() {
        return Err(format!("task {task_ref} has no retained attempt in {run}"));
    }
    let start = if let Some(cursor) = cursor {
        if cursor.revision != reading.revision {
            return Err("stale task cursor; restart at the current revision".into());
        }
        attempts
            .iter()
            .position(|attempt| attempt.attempt_ref == cursor.after_attempt_ref)
            .ok_or("task cursor does not belong to the addressed task")?
            + 1
    } else {
        0
    };
    let end = (start + limit).min(attempts.len());
    let next = if end < attempts.len() {
        Some(TaskCursor {
            revision: reading.revision,
            after_attempt_ref: attempts[end - 1].attempt_ref.clone(),
        })
    } else {
        None
    };
    let views = attempts[start..end]
        .iter()
        .map(|attempt| attempt_view(&state, &reading, attempt))
        .collect::<Vec<_>>();
    Ok(
        json!({"contract":TASK_READING,"projectRef":state.build.project().reference(),"runRef":run,
        "taskRef":task_ref,"revision":reading.revision,"runRevision":reading.run_revision,
        "topologyRevision":reading.topology_revision,"sourceCurrent":reading.source_current,
        "workflowSourceRef":reading.workflow_source_ref,"workflowSourceRevision":reading.workflow_source_revision,
        "workflowSourceDigest":reading.workflow_source_digest,"totalAttempts":attempts.len(),
        "attempts":views,"nextCursor":next}),
    )
}

pub fn read_return(path: &Path, run: &RunRef, attempt_ref: &str) -> Result<Value, String> {
    let (state, reading) = coherent_read(path, run)?;
    let attempt = reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == attempt_ref)
        .ok_or_else(|| format!("attempt {attempt_ref} is not retained in {run}"))?;
    Ok(
        json!({"contract":RETURN_READING,"projectRef":state.build.project().reference(),"runRef":run,
        "revision":reading.revision,"sourceCurrent":reading.source_current,
        "workflowSourceRef":reading.workflow_source_ref,"workflowSourceRevision":reading.workflow_source_revision,
        "workflowSourceDigest":reading.workflow_source_digest,"attempt":attempt_view(&state,&reading,attempt)}),
    )
}

fn strings(value: &Value) -> String {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "none retained".into())
}
fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("not available")
}

pub fn render_reading(value: &Value) -> String {
    let mut lines = vec![format!("{}\nProject: {}\nRun: {}\nRevision: {}\nSource: {} @ {}\nSource digest: {}\nSource current: {}",
        text(&value["contract"]),text(&value["projectRef"]),text(&value["runRef"]),value["revision"],
        text(&value["workflowSourceRef"]),text(&value["workflowSourceRevision"]),text(&value["workflowSourceDigest"]),value["sourceCurrent"])];
    let views = if let Some(views) = value["attempts"].as_array() {
        views.iter().collect::<Vec<_>>()
    } else {
        vec![&value["attempt"]]
    };
    for view in views {
        let record = &view["record"];
        lines.push(format!("\nTask: {}\nAttempt: {} ({}, {})\nExecution: {}\nAgent: {}\nContext: {}\nPraxis: {}\nNOW: {}\nModel usage: {}\nMaterial usage: {}",
            text(&record["taskRef"]),text(&record["attemptRef"]),text(&view["standing"]),text(&view["status"]),
            record["executionRef"].as_str().unwrap_or("not dispatched"),text(&record["disposition"]["participant"]["agentRef"]),
            strings(&record["disposition"]["contextRefs"]),strings(&record["disposition"]["praxisRefs"]),
            record["disposition"]["placement"]["nowRef"].as_str().unwrap_or("not allocated"),
            text(&view["modelUsageStatus"]),text(&view["materialUsageStatus"])));
        let returned = &record["readableReturn"];
        if returned.is_null() {
            lines.push(
                "Return: not recorded (provider acknowledgement is not task completion)".into(),
            );
        } else {
            lines.push(format!("Return: {}\n{}\nArtifacts: {}\nEvidence: {}\nReceiving: {} ({})\nArchive references: {}\nLearning intake references: {}",
                text(&returned["returnRef"]),text(&returned["summary"]),strings(&returned["artifactRefs"]),strings(&returned["evidenceRefs"]),
                returned["receivingRef"].as_str().unwrap_or("not linked"),text(&view["receivingStanding"]),strings(&returned["archiveRefs"]),strings(&view["regressionObservationRefs"])));
        }
        for fact in record["tracking"].as_array().into_iter().flatten() {
            lines.push(format!(
                "Correlation: {} / {} — {} @ {}",
                text(&fact["kind"]),
                text(&fact["ownerRef"]),
                text(&fact["subjectRef"]),
                text(&fact["sourceRevision"])
            ));
        }
        for receipt in view["unresolvedOwnerOperations"]
            .as_array()
            .into_iter()
            .flatten()
        {
            lines.push(format!(
                "Unresolved owner effect: {} / {}",
                text(&receipt["ownerRef"]),
                text(&receipt["operationRef"])
            ));
        }
    }
    if !value["nextCursor"].is_null() {
        lines.push(format!("Next cursor: {}", value["nextCursor"]));
    }
    lines.join("\n")
}

pub fn execute_cli(args: &[String]) -> Result<String, String> {
    if args.len() < 4 {
        return Err("Usage: factory attempt task|return <state> <run-ref> <task-ref|attempt-ref> [--json] [--limit N] [--cursor JSON]".into());
    }
    let run: RunRef = args[2]
        .parse()
        .map_err(|error| format!("invalid Run: {error}"))?;
    let mut limit = 50;
    let mut cursor = None;
    let mut json = false;
    let mut index = 4;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => json = true,
            "--limit" => {
                index += 1;
                limit = args
                    .get(index)
                    .ok_or("missing page limit")?
                    .parse::<usize>()
                    .map_err(|error| error.to_string())?;
            }
            "--cursor" => {
                index += 1;
                cursor = Some(
                    serde_json::from_str::<TaskCursor>(args.get(index).ok_or("missing cursor")?)
                        .map_err(|error| error.to_string())?,
                );
            }
            other => return Err(format!("unexpected task read argument {other}")),
        }
        index += 1;
    }
    let reading = match args[0].as_str() {
        "task" => read_task(Path::new(&args[1]), &run, &args[3], limit, cursor.as_ref())?,
        "return" if cursor.is_none() && limit == 50 => {
            read_return(Path::new(&args[1]), &run, &args[3])?
        }
        _ => return Err("unsupported read operation or pagination on a single Return".into()),
    };
    if json {
        serde_json::to_string_pretty(&reading).map_err(|error| error.to_string())
    } else {
        Ok(render_reading(&reading))
    }
}
