//! Checked inside the existing developmental transaction, before reservations
//! or dispatch intents become visible. No material provider state is invented.

use crate::attempt_runtime::{
    FactoryAttemptOperation, FactoryAttemptRecord, OwnerOperationPhase,
    SituatedExecutionDisposition,
};
use std::collections::BTreeMap;

fn blocked(record: &FactoryAttemptRecord, disposition: &SituatedExecutionDisposition) -> bool {
    let Some(world) = disposition.body.material_world_ref.as_deref() else {
        return false;
    };
    if record.disposition.body.workcell_ref != disposition.body.workcell_ref {
        return false;
    }
    let mut latest = BTreeMap::new();
    for receipt in &record.observations {
        if receipt.owner_ref == "factory"
            && receipt.contract == "factory.attempt-material-call/v1"
            && receipt.payload["worldRef"].as_str() == Some(world)
            && matches!(receipt.payload["operation"].as_str(), Some("recover" | "release"))
        {
            latest.insert(&receipt.operation_ref, receipt);
        }
    }
    latest.values().any(|receipt| {
        matches!(receipt.phase, OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain)
            || (receipt.payload["detail"]["validated"] == true
                && (receipt.phase == OwnerOperationPhase::Released
                    || (receipt.phase == OwnerOperationPhase::Recovered
                        && receipt.payload.pointer("/detail/ownerReceipt/payload/world/world_ref")
                            .and_then(serde_json::Value::as_str) != Some(world))))
    })
}

pub(crate) fn validate_new_work<'a>(
    operation: &FactoryAttemptOperation,
    addressed: &BTreeMap<String, FactoryAttemptRecord>,
    records: impl Iterator<Item = &'a FactoryAttemptRecord>,
) -> Result<(), String> {
    let dispositions: Vec<&SituatedExecutionDisposition> = match operation {
        FactoryAttemptOperation::StartSerial { disposition, .. }
        | FactoryAttemptOperation::Retry { disposition, .. } => vec![disposition],
        FactoryAttemptOperation::StartFork { attempts, .. } => {
            attempts.iter().map(|attempt| &attempt.disposition).collect()
        }
        FactoryAttemptOperation::BindDispatch { attempt_ref, .. } => {
            addressed.get(attempt_ref).map(|record| vec![&record.disposition]).unwrap_or_default()
        }
        FactoryAttemptOperation::RecordObservation { attempt_ref, receipt }
            if receipt.owner_ref == "factory"
                && receipt.contract == "factory.attempt-owner-transport/v1"
                && receipt.phase == OwnerOperationPhase::Dispatching
                && receipt.payload["action"] == "send" =>
        {
            addressed.get(attempt_ref).map(|record| vec![&record.disposition]).unwrap_or_default()
        }
        _ => return Ok(()),
    };
    for record in records {
        if dispositions.iter().any(|disposition| blocked(record, disposition)) {
            return Err("material lifecycle is unresolved, released or superseded; re-resolve before starting or dispatching work".into());
        }
    }
    Ok(())
}
