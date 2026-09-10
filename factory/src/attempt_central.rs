//! Native Central preparation for an existing Factory attempt. Preparation is
//! not worker enforcement: it retains the owner's actual policy, NOW and path
//! readings. Dispatch rechecks those same bases before contacting the worker.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_receiving::{CentralReceivingEndpoint, CENTRAL_CONTRACT_REVISION};
use crate::attempt_runtime::{
    AttemptTrackingFact, FactoryAttemptActionRequest, FactoryAttemptOperation,
    FactoryAttemptReading, FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt,
    FACTORY_ATTEMPT_ACTION,
};
use crate::core::run::RunRef;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const CENTRAL_ACTION: &str = "factory.attempt-central-action/v1";
pub const CENTRAL_RECEIPT: &str = "factory.attempt-central-receipt/v1";
const CALL: &str = "factory.attempt-central-call/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CentralAttemptRequest {
    pub contract: String,
    pub request_ref: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub attempt_ref: String,
    pub central: CentralReceivingEndpoint,
    pub working_directory: PathBuf,
    /// Exact intended destinations, not a grant for arbitrary sibling paths.
    pub destinations: BTreeSet<PathBuf>,
    /// Only explicit recovery can repeat Central's idempotent allocation.
    #[serde(default)]
    pub recover: bool,
    /// Total native preparation transport budget, never a remote worker lifetime.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("native Central result omitted {key}"))
}
fn now() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|error| error.to_string())
}
fn absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
}
fn endpoint_valid(endpoint: &CentralReceivingEndpoint) -> bool {
    endpoint.contract_revision == CENTRAL_CONTRACT_REVISION
        && absolute(&endpoint.root)
        && endpoint
            .project
            .as_ref()
            .is_none_or(|value| !value.trim().is_empty())
}
fn scoped(endpoint: &CentralReceivingEndpoint, mut input: Value) -> Value {
    if let Some(project) = &endpoint.project {
        input["project"] = json!(project);
    }
    input
}
fn call(
    endpoint: &CentralReceivingEndpoint,
    action: &str,
    input: &Value,
    deadline: Instant,
    observed: &mut Vec<Value>,
) -> Result<Value, String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or("Central preparation transport budget exhausted")?;
    let mut command = Command::new(&endpoint.binary);
    command
        .arg("--json")
        .arg("--root")
        .arg(&endpoint.root)
        .arg("action")
        .arg("run")
        .arg(action)
        .arg(serde_json::to_string(input).map_err(|error| error.to_string())?);
    // Native credentials remain in the host environment, never in the request.
    let output = crate::native_process::output(&mut command, remaining).map_err(|error| {
        observed.push(json!({"action":action,"request":input,"transportError":error.to_string()}));
        error.to_string()
    })?;
    let response: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| {
            observed.push(json!({"action":action,"request":input,"unreadableResponse":true,
                "responseDigest":blake3::hash(&output.stdout).to_hex().to_string(),"responseBytes":output.stdout.len(),"transportError":error.to_string()}));
            format!("Central ActionResult was unreadable: {error}")
        })?;
    observed.push(json!({"action":action,"request":input,"response":response}));
    if !output.status.success()
        || response["ok"] != true
        || response["status"] != "success"
        || response["action"] != action
    {
        return Err(format!("Central did not confirm {action}: {response}"));
    }
    Ok(response)
}
fn policy_valid(policy: &Value) -> Result<(), String> {
    if policy["schema"] != "central.effective-placement-policy/v1"
        || policy["outside_writes_prevented"] != false
        || !matches!(
            policy["enforcement"].as_str(),
            Some("native-actions" | "harness-interception" | "material-filesystem")
        )
        || policy["required_coverage"]
            .as_array()
            .is_none_or(|values| values.is_empty() || values.iter().any(|v| v.as_str().is_none()))
        || policy["sources"].as_array().is_none_or(Vec::is_empty)
    {
        return Err("Central omitted its actual policy, coverage or source standing".into());
    }
    text(policy, "scope_ref")?;
    text(policy, "revision")?;
    if policy["expires_at_unix_seconds"].as_u64().unwrap_or(0) <= now()? {
        return Err("Central placement lease expired before the operation".into());
    }
    Ok(())
}
fn record<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, String> {
    reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == reference)
        .ok_or_else(|| "attempt is not retained in this Run".into())
}
fn action(
    request: &CentralAttemptRequest,
    revision: u64,
    operation: FactoryAttemptOperation,
) -> FactoryAttemptActionRequest {
    FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: format!("{}:{}", request.projection_ref, request.request_ref),
        caller: request.caller.clone(),
        run_ref: request.run_ref.clone(),
        expected_revision: revision,
        authority: request.authority.clone(),
        operation,
    }
}
fn stamp(
    mut receipt: OwnerOperationReceipt,
    phase: OwnerOperationPhase,
    detail: Value,
) -> OwnerOperationReceipt {
    receipt.phase = phase;
    receipt.payload["detail"] = detail;
    receipt.receipt_ref = format!(
        "factory-central-call:{}",
        blake3::hash(
            &serde_json::to_vec(&(
                &receipt.operation_ref,
                &receipt.source_revision,
                phase,
                &receipt.payload
            ))
            .expect("JSON value")
        )
        .to_hex()
    );
    receipt
}
fn retain(
    store: &mut FileAttemptStore,
    request: &CentralAttemptRequest,
    receipt: &OwnerOperationReceipt,
) -> Result<(), String> {
    for _ in 0..8 {
        let reading = store.reading().map_err(|error| error.to_string())?;
        if let Some(existing) = record(&reading, &request.attempt_ref)?
            .observations
            .iter()
            .find(|existing| existing.receipt_ref == receipt.receipt_ref)
        {
            return if existing == receipt {
                Ok(())
            } else {
                Err("Central receipt identity conflicts".into())
            };
        }
        let operation = FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt: receipt.clone(),
        };
        match store.apply(action(request, reading.revision, operation)) {
            Ok(_) => return Ok(()),
            Err(error) => {
                if store.reading().map_err(|error| error.to_string())?.revision == reading.revision
                {
                    return Err(error.to_string());
                }
            }
        }
    }
    Err("Central result retention remained contended; explicitly recover, never dispatch from this result".into())
}
fn retain_fact(
    store: &mut FileAttemptStore,
    request: &CentralAttemptRequest,
    fact: AttemptTrackingFact,
) -> Result<(), String> {
    for _ in 0..8 {
        let reading = store.reading().map_err(|error| error.to_string())?;
        if let Some(existing) = record(&reading, &request.attempt_ref)?
            .tracking
            .iter()
            .find(|existing| existing.fact_ref == fact.fact_ref)
        {
            return if existing == &fact {
                Ok(())
            } else {
                Err("Central tracking identity conflicts".into())
            };
        }
        let operation = FactoryAttemptOperation::RecordTracking {
            attempt_ref: request.attempt_ref.clone(),
            fact: fact.clone(),
        };
        match store.apply(action(request, reading.revision, operation)) {
            Ok(_) => return Ok(()),
            Err(error) => {
                if store.reading().map_err(|error| error.to_string())?.revision == reading.revision
                {
                    return Err(error.to_string());
                }
            }
        }
    }
    Err("Central tracking retention remained contended".into())
}
fn allocation_valid(allocation: &Value, input: &Value, policy: &Value) -> Result<(), String> {
    let source = &allocation["source"];
    let retained = &allocation["record"];
    if allocation["schema"] != "central.now-allocation/v1"
        || retained["schema"] != "central.now-clearing/v1"
        || retained["scope_ref"] != policy["scope_ref"]
        || retained["task_ref"] != input["task_ref"]
        || retained["purpose"] != input["purpose"]
        || retained["participant_refs"] != input["participant_refs"]
        || retained["source_refs"] != input["source_refs"]
        || retained["source_ref"] != source["ref"]
        || retained["now_ref"] != allocation["now_ref"]
        || retained["lifecycle"] != "active"
        || allocation["policy"]["revision"] != policy["revision"]
        || allocation["automatic_agent_or_model_invocation"] != false
        || allocation["artifact_namespace"] != "T"
    {
        return Err(
            "Central NOW allocation changed the task, source, participant or policy basis".into(),
        );
    }
    text(allocation, "now_ref")?;
    text(source, "ref")?;
    text(&allocation["revision"], "revision")?;
    if !absolute(Path::new(text(allocation, "writable_destination")?)) {
        return Err("Central did not return an absolute NOW destination".into());
    }
    Ok(())
}
fn validations(
    endpoint: &CentralReceivingEndpoint,
    policy: &Value,
    allocation: &Value,
    destinations: &[Value],
    anchors: bool,
    deadline: Instant,
    observed: &mut Vec<Value>,
) -> Result<Vec<Value>, String> {
    let mut results = Vec::new();
    for destination in destinations {
        let path = if anchors {
            &destination["data"]["destination"]
        } else {
            destination
        };
        let mut input = scoped(
            endpoint,
            json!({
                "now_ref": allocation["now_ref"],
                "expected_now_revision": allocation["revision"]["revision"],
                "expected_policy_revision": policy["revision"],
                "destination": path
            }),
        );
        if anchors {
            input["expected_destination_anchor"] =
                destination["data"]["destination_anchor"].clone();
        }
        let response = call(
            endpoint,
            "central.work.validate",
            &input,
            deadline,
            observed,
        )?;
        let data = &response["data"];
        if data["schema"] != "central.work-placement-validation/v1"
            || data["allowed"] != true
            || data["destination"] != *path
            || data["now_ref"] != allocation["now_ref"]
            || data["now_revision"] != allocation["revision"]["revision"]
            || data["policy_revision"] != policy["revision"]
            || data["required_coverage"] != policy["required_coverage"]
            || data["required_enforcement"] != policy["enforcement"]
            || data["valid_now_destination"] != allocation["writable_destination"]
            || data["outside_writes_prevented"] != false
            || data["expires_at_unix_seconds"].as_u64().unwrap_or(0) <= now()?
            || !data["destination_anchor"].is_object()
        {
            return Err(
                "Central path validation changed or omitted the selected destination/basis".into(),
            );
        }
        results.push(response);
    }
    Ok(results)
}
fn prepare(
    request: &CentralAttemptRequest,
    attempt: &FactoryAttemptRecord,
    reading: &FactoryAttemptReading,
    previous: Option<&Value>,
    deadline: Instant,
    observed: &mut Vec<Value>,
) -> Result<Value, String> {
    let endpoint = &request.central;
    let policy_response = call(
        endpoint,
        "central.work.policy",
        &scoped(endpoint, json!({})),
        deadline,
        observed,
    )?;
    let policy = &policy_response["data"];
    policy_valid(policy)?;
    let leg = &reading.legs[&attempt.workflow_unit_ref];
    let input = scoped(
        endpoint,
        json!({
            "task_ref": attempt.task_ref,
            "purpose": leg.delegation.concern,
            "participant_refs": [attempt.disposition.participant.agent_ref, attempt.disposition.participant.agency_ref],
            "source_refs": [attempt.disposition.participant.source_ref],
            "expected_policy_revision": policy["revision"]
        }),
    );
    // Allocation is the owner's exact idempotent operation. No local path or
    // NOW identity is reconstructed from a task key or copied into another store.
    let response = call(endpoint, "central.now.allocate", &input, deadline, observed)?;
    let allocation = &response["data"];
    allocation_valid(allocation, &input, policy)?;
    if let Some(placement) = &attempt.disposition.placement {
        if allocation["now_ref"] != placement.now_ref
            || allocation["writable_destination"] != placement.now_path
            || policy["revision"] != placement.policy_revision
        {
            return Err("native NOW/policy differs from the selected disposition; explicitly re-resolve the attempt".into());
        }
    }
    if previous.is_some_and(|old| old["allocation"]["now_ref"] != allocation["now_ref"]) {
        return Err("recovery changed the original NOW identity".into());
    }
    let mut paths = request.destinations.clone();
    paths.insert(request.working_directory.clone());
    let destinations = paths.iter().map(|path| json!(path)).collect::<Vec<_>>();
    let checked = validations(
        endpoint,
        policy,
        allocation,
        &destinations,
        false,
        deadline,
        observed,
    )?;
    let checkpoint = json!({
        "central": endpoint,
        "workingDirectory": request.working_directory,
        "policy": policy,
        "allocation": allocation,
        "validations": checked,
        "allocationRequest": input,
        "policyResponse": policy_response,
        "allocationResponse": response,
        "workerEnforcementEstablished": false
    });
    if serde_json::to_vec(&checkpoint)
        .map_err(|error| error.to_string())?
        .len()
        > 2 * 1024 * 1024
    {
        return Err("Central preparation exceeds the bounded retained result size".into());
    }
    Ok(checkpoint)
}

/// A successful old preparation is evidence, not a lease on current source.
/// Called by the native owner dispatch itself, not only by the CLI projection.
pub(crate) fn preflight(
    attempt: &FactoryAttemptRecord,
    cwd: &Path,
) -> Result<Option<Value>, String> {
    let Some(receipt) = attempt
        .observations
        .iter()
        .rev()
        .find(|receipt| receipt.contract == CALL)
    else {
        return Ok(None);
    };
    if receipt.phase != OwnerOperationPhase::Observed {
        return Err(
            "Central preparation is unresolved; explicitly recover it before dispatch".into(),
        );
    }
    let mut observed = Vec::new();
    let timeout = attempt
        .disposition
        .budget
        .wall_clock_timeout_ms
        .unwrap_or(30_000)
        .min(30_000);
    if timeout == 0 {
        return Err("Central preflight transport budget exhausted".into());
    }
    let deadline = Instant::now() + Duration::from_millis(timeout);
    let checkpoint = &receipt.payload["detail"]["checkpoint"];
    let endpoint: CentralReceivingEndpoint =
        serde_json::from_value(checkpoint["central"].clone()).map_err(|error| error.to_string())?;
    if !endpoint_valid(&endpoint) || checkpoint["workingDirectory"] != json!(cwd) {
        return Err("dispatch cwd or Central endpoint differs from the prepared attempt".into());
    }
    let old_policy = &checkpoint["policy"];
    policy_valid(old_policy)?;
    let policy = call(
        &endpoint,
        "central.work.policy",
        &scoped(&endpoint, json!({})),
        deadline,
        &mut observed,
    )?;
    policy_valid(&policy["data"])?;
    if policy["data"]["revision"] != old_policy["revision"] {
        return Err(
            "Central placement policy changed after preparation; re-resolve before dispatch".into(),
        );
    }
    let allocation = &checkpoint["allocation"];
    let current = call(
        &endpoint,
        "central.now.read",
        &scoped(&endpoint, json!({"now_ref":allocation["now_ref"]})),
        deadline,
        &mut observed,
    )?;
    if current["data"]["schema"] != "central.now-reading/v1"
        || current["data"]["record"] != allocation["record"]
        || current["data"]["revision"] != allocation["revision"]
        || current["data"]["source"]["ref"] != allocation["source"]["ref"]
    {
        return Err(
            "NOW source/lifecycle changed after preparation; re-resolve before dispatch".into(),
        );
    }
    let destinations = checkpoint["validations"]
        .as_array()
        .ok_or("missing prepared destinations")?;
    let checked = validations(
        &endpoint,
        old_policy,
        allocation,
        destinations,
        true,
        deadline,
        &mut observed,
    )?;
    Ok(Some(
        json!({"preparationReceiptRef":receipt.receipt_ref,"policy":policy,"now":current,"validations":checked,"workerEnforcementEstablished":false}),
    ))
}

pub fn execute(path: &Path, request: CentralAttemptRequest) -> Result<Value, String> {
    if request.contract != CENTRAL_ACTION
        || request.request_ref.trim().is_empty()
        || request.projection_ref.trim().is_empty()
        || !endpoint_valid(&request.central)
        || !absolute(&request.working_directory)
        || request
            .timeout_ms
            .is_some_and(|timeout| timeout == 0 || timeout > 30_000)
        || request.destinations.len() > 64
        || request.destinations.iter().any(|path| !absolute(path))
    {
        return Err("Central preparation requires exact identities, pinned endpoint and bounded absolute destinations".into());
    }
    let mut store = FileAttemptStore::open_run(path, request.run_ref.clone())
        .map_err(|error| error.to_string())?;
    let reading = store.reading().map_err(|error| error.to_string())?;
    let attempt = record(&reading, &request.attempt_ref)?.clone();
    let mut identity = serde_json::to_value(&request).map_err(|error| error.to_string())?;
    for key in ["recover", "expectedRevision", "projectionRef"] {
        identity
            .as_object_mut()
            .expect("request object")
            .remove(key);
    }
    let digest = blake3::hash(&serde_json::to_vec(&identity).map_err(|error| error.to_string())?)
        .to_hex()
        .to_string();
    let operation_ref = format!("factory-attempt-central:{}", request.request_ref);
    let previous = attempt
        .observations
        .iter()
        .rev()
        .find(|receipt| receipt.contract == CALL && receipt.operation_ref == operation_ref)
        .cloned();
    let mut intent = stamp(
        OwnerOperationReceipt {
            owner_ref: "factory".into(),
            contract: CALL.into(),
            operation_ref,
            receipt_ref: String::new(),
            source_revision: format!("factory-state:{}", reading.revision),
            phase: OwnerOperationPhase::Dispatching,
            evidence_refs: BTreeSet::new(),
            partial_effect_refs: BTreeSet::new(),
            payload: json!({"requestDigest":digest,"request":identity}),
        },
        OwnerOperationPhase::Dispatching,
        json!({"meaning":"native Central preparation intent, not worker execution"}),
    );
    let operation = FactoryAttemptOperation::RecordObservation {
        attempt_ref: request.attempt_ref.clone(),
        receipt: intent.clone(),
    };
    crate::attempt_runtime::validate_action_request(&action(
        &request,
        reading.revision,
        operation.clone(),
    ))
    .map_err(|error| error.to_string())?;
    if let Some(previous) = &previous {
        if previous.payload["requestDigest"] != digest {
            return Err(
                "Central preparation request identity was reused with different content".into(),
            );
        }
        if !request.recover {
            return Ok(
                json!({"contract":CENTRAL_RECEIPT,"replayed":true,"needsReconciliation":previous.phase!=OwnerOperationPhase::Observed,"observation":previous,"reading":reading,"workerEnforcementEstablished":false}),
            );
        }
    }
    if reading.revision != request.expected_revision || !reading.source_current {
        return Err("stale Factory revision/source before Central preparation".into());
    }
    let leg = reading
        .legs
        .get(&attempt.workflow_unit_ref)
        .ok_or("attempt has no native workflow leg")?;
    let execution = attempt
        .execution_ref
        .as_deref()
        .unwrap_or(&attempt.reserved_execution_ref);
    if leg.execution_ref != execution
        || !matches!(
            leg.status,
            crate::orchestration::LegStatus::Active | crate::orchestration::LegStatus::Detached
        )
    {
        return Err("preparation cannot revive a historical or terminal attempt".into());
    }
    if previous.is_none() {
        store
            .apply(action(&request, reading.revision, operation))
            .map_err(|error| error.to_string())?;
    } else {
        intent = previous.clone().expect("previous checked");
        // Explicit refresh invalidates the old proof before any new owner call.
        // A crash must not leave an earlier successful checkpoint usable.
        intent.payload["refreshBasisRevision"] = json!(reading.revision);
        intent = stamp(
            intent,
            OwnerOperationPhase::Dispatching,
            json!({"meaning":"explicit Central refresh before native effects"}),
        );
        store
            .apply(action(
                &request,
                reading.revision,
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: intent.clone(),
                },
            ))
            .map_err(|error| error.to_string())?;
    }
    let deadline = Instant::now() + Duration::from_millis(request.timeout_ms.unwrap_or(30_000));
    let mut observed = Vec::new();
    let prepared = prepare(
        &request,
        &attempt,
        &reading,
        previous
            .as_ref()
            .and_then(|value| value.payload.pointer("/detail/checkpoint")),
        deadline,
        &mut observed,
    );
    let (phase, detail) = match prepared {
        Ok(checkpoint) => (
            OwnerOperationPhase::Observed,
            json!({"checkpoint":checkpoint,"nativeResponses":observed}),
        ),
        Err(error) => {
            let phase = if observed
                .iter()
                .any(|value| value.get("transportError").is_some())
            {
                OwnerOperationPhase::Uncertain
            } else {
                OwnerOperationPhase::Failed
            };
            (
                phase,
                json!({"error":error,"nativeResponses":observed,"instruction":"inspect the retained native results and explicitly recover or correct the preparation; no worker was dispatched"}),
            )
        }
    };
    let settled = stamp(intent, phase, detail);
    let retention = (|| {
        if phase == OwnerOperationPhase::Observed {
            // Keep per-owner source facts in the existing #222 tracking history.
            let checkpoint = &settled.payload["detail"]["checkpoint"];
            for (kind, subject, revision) in [
                (
                    "now",
                    checkpoint["allocation"]["now_ref"].clone(),
                    checkpoint["allocation"]["revision"]["revision"].clone(),
                ),
                (
                    "source-revision",
                    checkpoint["allocation"]["source"]["ref"].clone(),
                    checkpoint["allocation"]["revision"]["revision"].clone(),
                ),
                (
                    "placement-policy",
                    checkpoint["policy"]["scope_ref"].clone(),
                    checkpoint["policy"]["revision"].clone(),
                ),
            ] {
                let fact = AttemptTrackingFact {
                    fact_ref: format!(
                        "factory-central-fact:{}",
                        blake3::hash(
                            serde_json::to_string(&(
                                kind,
                                &subject,
                                &revision,
                                &settled.receipt_ref
                            ))
                            .map_err(|error| error.to_string())?
                            .as_bytes()
                        )
                        .to_hex()
                    ),
                    kind: kind.into(),
                    owner_ref: "central".into(),
                    subject_ref: subject
                        .as_str()
                        .ok_or("missing native tracking subject")?
                        .into(),
                    source_revision: revision
                        .as_str()
                        .ok_or("missing native tracking revision")?
                        .into(),
                    evidence_refs: BTreeSet::from([settled.receipt_ref.clone()]),
                };
                retain_fact(&mut store, &request, fact)?;
            }
        }
        // The ready checkpoint is published last, never before its tracking.
        retain(&mut store, &request, &settled)
    })();
    let current = store.reading();
    Ok(
        json!({"contract":CENTRAL_RECEIPT,"replayed":false,"requestRef":request.request_ref,"attemptRef":request.attempt_ref,
        "needsReconciliation":phase!=OwnerOperationPhase::Observed||retention.is_err()||current.is_err(),
        "observation":settled,"retentionError":retention.err(),"reading":current.ok(),"workerEnforcementEstablished":false}),
    )
}

pub fn execute_cli(args: &[String], input: Option<&str>) -> Result<String, String> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        return Ok(format!("Factory native Central preparation\n\nfactory attempt prepare <state> <request-json|-> [--json]\n\n{CENTRAL_ACTION}\nCalls native policy/NOW allocation/path validation. Dispatch rechecks the retained exact bases. This does not install a worker guard."));
    }
    let positional = args
        .iter()
        .filter(|value| value.as_str() != "--json")
        .collect::<Vec<_>>();
    if positional.is_empty() || positional.len() > 2 {
        return Err("expected state and one preparation request".into());
    }
    let request_path = positional.get(1).map(|value| value.as_str()).unwrap_or("-");
    let body = if request_path != "-" {
        std::fs::read_to_string(request_path).map_err(|error| error.to_string())?
    } else if let Some(input) = input {
        input.to_owned()
    } else {
        let mut body = String::new();
        std::io::stdin()
            .read_to_string(&mut body)
            .map_err(|error| error.to_string())?;
        body
    };
    let result = execute(
        Path::new(positional[0]),
        serde_json::from_str(&body).map_err(|error| error.to_string())?,
    )?;
    if args.iter().any(|value| value == "--json") {
        serde_json::to_string_pretty(&result).map_err(|error| error.to_string())
    } else {
        Ok(format!(
            "{CENTRAL_RECEIPT}\nNeeds reconciliation: {}\nWorker enforcement established: false",
            result["needsReconciliation"]
        ))
    }
}
