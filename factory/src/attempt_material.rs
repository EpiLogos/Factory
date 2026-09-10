//! Attempt-scoped use of the existing Workcell adapter and Factory store.
//! Material recovery is not Agent continuation; release is not cancellation.
//! Unknown consequential calls are observed, never implicitly sent again.

use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_owner_dispatch::{FactoryAttemptOwnerReceipt, FactoryAttemptOwnerRequest};
use crate::attempt_runtime::{
    FactoryAttemptActionRequest, FactoryAttemptOperation, FactoryAttemptReading,
    FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt, FACTORY_ATTEMPT_ACTION,
};
use crate::native_owner::{
    invoke_native_owner_bounded, NativeOwnerInvocation, WorkcellWorldOperation,
    DEFAULT_OWNER_TIMEOUT_MS, WORKCELL_CAW_CONTRACT_REVISION,
};
use crate::orchestration::LegStatus;
use crate::project_development_store::read_developmental_state;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MATERIAL_ACTION: &str = "factory.attempt-material-action/v1";
pub const MATERIAL_RECEIPT: &str = "factory.attempt-material-receipt/v1";
const CALL: &str = "factory.attempt-material-call/v1";
const WORLD: &str = "workcell.material-world/v1";
const MAX_RECEIPT: u64 = 4 * 1024 * 1024;

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("material response requires {key}"))
}

fn record<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, String> {
    reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == reference)
        .ok_or_else(|| "material call names an absent attempt".into())
}

fn pending(phase: OwnerOperationPhase) -> bool {
    matches!(
        phase,
        OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain
    )
}

fn calls(record: &FactoryAttemptRecord) -> BTreeMap<&str, &OwnerOperationReceipt> {
    let mut latest = BTreeMap::new();
    for receipt in &record.observations {
        if receipt.owner_ref == "factory" && receipt.contract == CALL {
            latest.insert(receipt.operation_ref.as_str(), receipt);
        }
    }
    latest
}

fn execution(record: &FactoryAttemptRecord) -> &str {
    record
        .execution_ref
        .as_deref()
        .unwrap_or(&record.reserved_execution_ref)
}

/// Follow only validated native recovery relations. Historical disposition is
/// never rewritten to pretend a replacement process is the original body.
fn current_world(record: &FactoryAttemptRecord) -> Option<&str> {
    let mut current = record.disposition.body.material_world_ref.as_deref()?;
    for receipt in &record.observations {
        if receipt.owner_ref == "factory"
            && receipt.contract == CALL
            && receipt.phase == OwnerOperationPhase::Recovered
            && receipt.payload["worldRef"].as_str() == Some(current)
            && receipt.payload["detail"]["validated"] == true
        {
            if let Some(next) = receipt
                .payload
                .pointer("/detail/ownerReceipt/payload/world/world_ref")
                .and_then(Value::as_str)
            {
                current = next;
            }
        }
    }
    Some(current)
}

fn known_world(record: &FactoryAttemptRecord, world: &str) -> bool {
    record.disposition.body.material_world_ref.as_deref() == Some(world)
        || record.observations.iter().any(|receipt| {
            receipt.owner_ref == "factory"
                && receipt.contract == CALL
                && receipt.phase == OwnerOperationPhase::Recovered
                && receipt.payload["detail"]["validated"] == true
                && receipt
                    .payload
                    .pointer("/detail/ownerReceipt/payload/world/world_ref")
                    .and_then(Value::as_str)
                    == Some(world)
        })
}

fn action(
    request: &FactoryAttemptOwnerRequest,
    revision: u64,
    receipt: OwnerOperationReceipt,
) -> FactoryAttemptActionRequest {
    FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: format!(
            "{}:material:{}",
            request.projection_ref, request.request_ref
        ),
        caller: request.caller.clone(),
        run_ref: request.run_ref.clone(),
        expected_revision: revision,
        authority: request.authority.clone(),
        operation: FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt,
        },
    }
}

fn stamped(
    mut receipt: OwnerOperationReceipt,
    phase: OwnerOperationPhase,
    detail: Value,
) -> OwnerOperationReceipt {
    receipt.phase = phase;
    receipt.payload["detail"] = detail;
    let bytes = serde_json::to_vec(&(&receipt.operation_ref, phase, &receipt.payload))
        .expect("JSON values serialize");
    receipt.receipt_ref = format!("factory-material-call:{}", blake3::hash(&bytes));
    receipt
}

fn retain(
    store: &mut FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    receipt: &OwnerOperationReceipt,
) -> Result<(), String> {
    for _ in 0..8 {
        let reading = store.reading().map_err(|error| error.to_string())?;
        let attempt = record(&reading, &request.attempt_ref)?;
        if let Some(existing) = attempt
            .observations
            .iter()
            .find(|existing| existing.receipt_ref == receipt.receipt_ref)
        {
            return if existing == receipt {
                Ok(())
            } else {
                Err("material receipt identity has conflicting content".into())
            };
        }
        match store.apply(action(request, reading.revision, receipt.clone())) {
            Ok(_) => return Ok(()),
            Err(error) => {
                if store.reading().map_err(|error| error.to_string())?.revision == reading.revision
                {
                    return Err(error.to_string());
                }
            }
        }
    }
    Err("material response retention remained contended; do not repeat the effect".into())
}

fn result(
    store: &FileAttemptStore,
    request: &FactoryAttemptOwnerRequest,
    receipt: OwnerOperationReceipt,
    replayed: bool,
    mut retention_error: Option<String>,
) -> FactoryAttemptOwnerReceipt {
    let owner_receipt = receipt
        .payload
        .pointer("/detail/ownerReceipt")
        .filter(|value| !value.is_null())
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    let reading = match store.reading() {
        Ok(reading) => Some(reading),
        Err(error) => {
            retention_error = Some(error.to_string());
            None
        }
    };
    let unresolved = reading
        .as_ref()
        .and_then(|reading| record(reading, &request.attempt_ref).ok())
        .is_none_or(|record| calls(record).values().any(|receipt| pending(receipt.phase)));
    FactoryAttemptOwnerReceipt {
        contract: MATERIAL_RECEIPT.into(),
        request_ref: request.request_ref.clone(),
        run_ref: request.run_ref.clone(),
        attempt_ref: request.attempt_ref.clone(),
        replayed,
        needs_reconciliation: unresolved || pending(receipt.phase) || retention_error.is_some(),
        transport_observation: receipt,
        owner_receipt,
        retention_error,
        reading,
    }
}

fn validate_world<'a>(value: &'a Value, workcell: &str) -> Result<&'a str, String> {
    if value["version"] != WORLD || text(value, "workcell_ref")? != workcell {
        return Err("material receipt has another contract or Workcell identity".into());
    }
    text(value, "demand_ref")?;
    if !value["subjects"].is_object() || !value["binding_graph"].is_object() {
        return Err("material receipt omits native subjects or binding graph".into());
    }
    text(value, "world_ref")
}

fn validate_response(
    receipt: &mut OwnerOperationReceipt,
    operation: WorkcellWorldOperation,
    input: &Value,
    workcell: &str,
    world: &str,
) -> Result<(), String> {
    let payload = &receipt.payload;
    if payload["ok"] != true
        || pending(receipt.phase)
        || receipt.phase == OwnerOperationPhase::Failed
    {
        return Err("native material operation did not confirm an attributable result".into());
    }
    match operation {
        WorkcellWorldOperation::Inspect => {
            let value = payload.get("receipt_world").unwrap_or(payload);
            if validate_world(value, workcell)? != world {
                return Err("material inspection belongs to another World".into());
            }
        }
        WorkcellWorldOperation::Recover => {
            validate_world(&payload["world"], workcell)?;
            if payload["previous_world_ref"].as_str() != Some(world)
                || payload["world"]["subjects"] != input["subjects"]
                || payload["world"]["demand_ref"] != input["demand_ref"]
            {
                return Err(
                    "native recovery changed caller subjects, demand or predecessor".into(),
                );
            }
        }
        _ => {
            if payload["world_ref"].as_str() != Some(world) {
                return Err("native material result belongs to another World".into());
            }
            let field = match operation {
                WorkcellWorldOperation::Observe => "observations",
                WorkcellWorldOperation::Expose => "surfaces",
                WorkcellWorldOperation::Collect => "outputs",
                WorkcellWorldOperation::Release => "disposition",
                _ => unreachable!(),
            };
            if operation == WorkcellWorldOperation::Release {
                // Workcell may retain material under its native policy. An OK
                // response alone is not evidence that everything was released.
                receipt.phase = if text(payload, field)? == "released" {
                    OwnerOperationPhase::Released
                } else {
                    OwnerOperationPhase::Observed
                };
            } else if !payload[field].is_array() {
                return Err(format!("native material result omits {field}"));
            }
        }
    }
    Ok(())
}

/// Release cannot be used as a shortcut around cancellation/quiescence or stop
/// another still-active attempt sharing this material in the same provider.
fn validate_release(
    state_path: &Path,
    request: &FactoryAttemptOwnerRequest,
    world: &str,
) -> Result<(), String> {
    let native = read_developmental_state(state_path).map_err(|error| error.to_string())?;
    if native.build.revision().get() != request.expected_revision {
        return Err("Factory state changed before material release admission".into());
    }
    for run in native.attempt_states.keys() {
        let reading = FileAttemptStore::open_run(state_path, run.clone())
            .and_then(|store| store.reading())
            .map_err(|error| error.to_string())?;
        if reading.revision != request.expected_revision {
            return Err("Factory state changed while checking shared material users".into());
        }
        for candidate in &reading.attempts {
            if current_world(candidate) != Some(world) {
                continue;
            }
            let leg = &reading.legs[&candidate.workflow_unit_ref];
            let status = leg
                .attempts
                .iter()
                .find(|attempt| attempt.execution_ref == execution(candidate))
                .ok_or("material user has no canonical execution history")?
                .status;
            if run == &request.run_ref && candidate.attempt_ref == request.attempt_ref {
                if status != LegStatus::Quiescent {
                    return Err(
                        "material release requires this attempt's explicit quiescence".into(),
                    );
                }
            } else if !matches!(
                status,
                LegStatus::Quiescent | LegStatus::Failed | LegStatus::Returned
            ) {
                return Err("material is still used by another active Factory attempt".into());
            }
        }
    }
    Ok(())
}

/// An immutable transport copy prevents a swapped input path from directing
/// Workcell at another World after admission. It is not a new material store.
struct TransportReceipt(PathBuf);
impl TransportReceipt {
    fn new(bytes: &[u8]) -> Result<Self, String> {
        let directory =
            std::env::temp_dir().join(format!("factory-material-{}", ulid::Ulid::new()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&directory)
            .map_err(|error| error.to_string())?;
        let staging = Self(directory);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(staging.path())
            .map_err(|error| error.to_string())?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(staging)
    }
    fn path(&self) -> PathBuf {
        self.0.join("material-world.json")
    }
}
impl Drop for TransportReceipt {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path());
        let _ = fs::remove_dir(&self.0);
    }
}

pub fn execute(
    state_path: &Path,
    request: FactoryAttemptOwnerRequest,
) -> Result<FactoryAttemptOwnerReceipt, String> {
    if request.contract != MATERIAL_ACTION || request.request_ref.trim().is_empty() {
        return Err("invalid material Action contract or request identity".into());
    }
    let NativeOwnerInvocation::WorkcellWorld {
        binary,
        receipt,
        world_operation,
        endpoint,
        authorization,
        contract_revision,
    } = &request.invocation
    else {
        return Err("material Action requires the existing Workcell World adapter".into());
    };
    if authorization.is_some() {
        return Err(
            "use WORKCELL_CONTROL_TOKEN; credentials cannot enter attempt JSON/state".into(),
        );
    }
    if contract_revision != WORKCELL_CAW_CONTRACT_REVISION
        || endpoint
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        || !receipt.is_absolute()
    {
        return Err("material Action requires the pinned owner, explicit endpoint and absolute receipt path".into());
    }
    let digest =
        blake3::hash(&serde_json::to_vec(&request).map_err(|error| error.to_string())?).to_string();
    let operation_ref = format!("factory-attempt-material:{}", request.request_ref);
    let mut store = FileAttemptStore::open_run(state_path, request.run_ref.clone())
        .map_err(|error| error.to_string())?;
    let reading = store.reading().map_err(|error| error.to_string())?;
    let attempt = record(&reading, &request.attempt_ref)?;
    let probe = OwnerOperationReceipt {
        owner_ref: "factory".into(),
        contract: CALL.into(),
        operation_ref: operation_ref.clone(),
        receipt_ref: "material:admission-only".into(),
        source_revision: format!("factory-state:{}", reading.revision),
        phase: OwnerOperationPhase::Dispatching,
        evidence_refs: BTreeSet::new(),
        partial_effect_refs: BTreeSet::new(),
        payload: Value::Null,
    };
    crate::attempt_runtime::validate_action_request(&action(
        &request,
        reading.revision,
        probe.clone(),
    ))
    .map_err(|error| error.to_string())?;
    if request.execution_ref != execution(attempt) {
        return Err("material Action execution does not belong to the addressed attempt".into());
    }
    if let Some(previous) = calls(attempt).get(operation_ref.as_str()) {
        if previous.payload["requestDigest"].as_str() != Some(&digest) {
            return Err("material Action identity was reused with different content".into());
        }
        return Ok(result(&store, &request, (*previous).clone(), true, None));
    }
    if reading.revision != request.expected_revision {
        return Err("stale Factory revision before material owner invocation".into());
    }
    let workcell = attempt
        .disposition
        .body
        .workcell_ref
        .as_deref()
        .ok_or("attempt has no Workcell binding")?;
    let mut bytes = Vec::new();
    fs::File::open(receipt)
        .map_err(|error| error.to_string())?
        .take(MAX_RECEIPT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_RECEIPT {
        return Err("material receipt exceeds four MiB".into());
    }
    let input: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let world = validate_world(&input, workcell)?;
    if !known_world(attempt, world) {
        return Err("material receipt is not bound to this attempt".into());
    }
    let effectful = matches!(
        world_operation,
        WorkcellWorldOperation::Recover | WorkcellWorldOperation::Release
    );
    if effectful {
        if current_world(attempt) != Some(world) {
            return Err(
                "superseded material cannot be recovered or released by its old receipt".into(),
            );
        }
        if calls(attempt).values().any(|call| pending(call.phase)) {
            return Err("unresolved material call must be reconciled before another effect".into());
        }
        if *world_operation == WorkcellWorldOperation::Recover && !reading.source_current {
            return Err("material recovery requires current source re-resolution".into());
        }
    }
    if *world_operation == WorkcellWorldOperation::Release {
        validate_release(state_path, &request, world)?;
    }
    let operation = serde_json::to_value(world_operation).map_err(|error| error.to_string())?;
    let mut intent = probe;
    intent.payload = json!({"requestDigest":digest,"worldRef":world,"workcellRef":workcell,
        "receiptDigest":format!("blake3:{}",blake3::hash(&bytes)),"operation":operation,
        "endpoint":endpoint,"executionRef":request.execution_ref});
    let intent = stamped(
        intent,
        OwnerOperationPhase::Dispatching,
        json!({"meaning":"Factory call intent, not a Workcell result"}),
    );
    store
        .apply(action(&request, request.expected_revision, intent.clone()))
        .map_err(|error| error.to_string())?;
    let staging = match TransportReceipt::new(&bytes) {
        Ok(staging) => staging,
        Err(error) => {
            let failed = stamped(
                intent,
                OwnerOperationPhase::Failed,
                json!({"failure":error,"ownerInvoked":false}),
            );
            let retained = retain(&mut store, &request, &failed).err();
            return Ok(result(&store, &request, failed, false, retained));
        }
    };
    let invocation = NativeOwnerInvocation::WorkcellWorld {
        binary: binary.clone(),
        receipt: staging.path(),
        world_operation: *world_operation,
        endpoint: endpoint.clone(),
        authorization: None,
        contract_revision: contract_revision.clone(),
    };
    let timeout = attempt
        .disposition
        .budget
        .wall_clock_timeout_ms
        .unwrap_or(DEFAULT_OWNER_TIMEOUT_MS)
        .min(DEFAULT_OWNER_TIMEOUT_MS);
    let outcome = match invoke_native_owner_bounded(&invocation, timeout) {
        Ok(mut owner) => {
            // These adapter identities use semantic refs, never the temporary
            // filename. The owner response payload remains byte-equivalent JSON.
            owner.operation_ref = format!("workcell-material:{workcell}:{world}:{operation}");
            owner.receipt_ref = format!(
                "workcell-material:{}",
                blake3::hash(
                    &serde_json::to_vec(&(&owner.operation_ref, &owner.payload))
                        .expect("JSON serializes")
                )
            );
            match validate_response(&mut owner, *world_operation, &input, workcell, world) {
                Ok(()) => {
                    let phase = owner.phase;
                    stamped(
                        intent,
                        phase,
                        json!({"validated":true,"ownerReceipt":owner,
                        "workerContinuationEstablished":false,"workerQuiescenceEstablished":false,
                        "taskReturnEstablished":false,"materialUsageAttribution":"shared-material-not-task-metrics"}),
                    )
                }
                Err(error) => stamped(
                    intent,
                    OwnerOperationPhase::Uncertain,
                    json!({"validated":false,"failure":error,"ownerReceipt":owner}),
                ),
            }
        }
        Err(error) => stamped(
            intent,
            OwnerOperationPhase::Uncertain,
            json!({"validated":false,"failure":error.to_string(),"instruction":"inspect native material; do not repeat a consequential call"}),
        ),
    };
    let retention_error = retain(&mut store, &request, &outcome).err();
    Ok(result(&store, &request, outcome, false, retention_error))
}

pub fn execute_cli(args: &[String], stdin: Option<&str>) -> Result<String, String> {
    let positional = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .collect::<Vec<_>>();
    if matches!(
        positional.first().map(|arg| arg.as_str()),
        None | Some("help" | "--help" | "-h")
    ) {
        return Ok(format!("Factory attempt material lifecycle\n\nUsage:\n  factory attempt material <state> <request-json|-> [--json]\n\nContract: {MATERIAL_ACTION}\nUses the existing Workcell World invocation and Factory owner-request fields. An explicit endpoint and bound material receipt are required. WORKCELL_CONTROL_TOKEN stays in the host environment. Exact replay never repeats an owner call; material recovery does not resume the Agent, and release requires explicit quiescence."));
    }
    if positional.len() > 2 {
        return Err("material expects a state and one request file".into());
    }
    let input_path = positional.get(1).map(|arg| arg.as_str()).unwrap_or("-");
    let input = if input_path != "-" {
        fs::read_to_string(input_path).map_err(|error| error.to_string())?
    } else if let Some(stdin) = stdin {
        stdin.to_owned()
    } else {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| error.to_string())?;
        input
    };
    let receipt = execute(
        Path::new(positional[0]),
        serde_json::from_str(&input).map_err(|error| error.to_string())?,
    )?;
    if args.iter().any(|arg| arg == "--json") {
        serde_json::to_string_pretty(&receipt).map_err(|error| error.to_string())
    } else {
        Ok(format!("{}\nAttempt: {}\nMaterial call: {}\nReplayed: {}\nNeeds reconciliation: {}\nWorker continuation, quiescence and task Return are separate operations.",
            receipt.contract,receipt.attempt_ref,receipt.request_ref,receipt.replayed,receipt.needs_reconciliation))
    }
}
