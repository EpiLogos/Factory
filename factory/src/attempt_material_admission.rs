//! Checked inside the existing developmental transaction, before reservations
//! or dispatch intents become visible. No material provider state is invented.

use crate::attempt_runtime::{
    FactoryAttemptOperation, FactoryAttemptRecord, OwnerOperationPhase,
    SituatedExecutionDisposition,
};
use serde_json::Value;
use std::collections::BTreeMap;

const CALL: &str = "factory.attempt-material-call/v1";

fn target(disposition: &SituatedExecutionDisposition) -> Option<(&str, &str)> {
    Some((
        disposition.body.workcell_ref.as_deref()?,
        disposition.body.material_world_ref.as_deref()?,
    ))
}

fn blocked(record: &FactoryAttemptRecord, workcell: &str, world: &str) -> bool {
    let mut latest = BTreeMap::new();
    for receipt in &record.observations {
        if receipt.owner_ref == "factory"
            && receipt.contract == CALL
            && receipt.payload["worldRef"].as_str() == Some(world)
            && receipt.payload["workcellRef"].as_str() == Some(workcell)
            && matches!(
                receipt.payload["operation"].as_str(),
                Some("recover" | "release")
            )
        {
            latest.insert(&receipt.operation_ref, receipt);
        }
    }
    latest.values().any(|receipt| {
        matches!(
            receipt.phase,
            OwnerOperationPhase::Dispatching | OwnerOperationPhase::Uncertain
        ) || (receipt.payload["detail"]["validated"] == true
            && (receipt.phase == OwnerOperationPhase::Released
                || (receipt.phase == OwnerOperationPhase::Recovered
                    && receipt
                        .payload
                        .pointer("/detail/ownerReceipt/payload/world/world_ref")
                        .and_then(Value::as_str)
                        != Some(world))))
    })
}

pub(crate) fn validate_new_work<'a>(
    operation: &FactoryAttemptOperation,
    addressed: &BTreeMap<String, FactoryAttemptRecord>,
    records: impl Iterator<Item = &'a FactoryAttemptRecord>,
) -> Result<(), String> {
    let targets: Vec<(&str, &str)> = match operation {
        FactoryAttemptOperation::StartSerial { disposition, .. }
        | FactoryAttemptOperation::Retry { disposition, .. } => {
            target(disposition).into_iter().collect()
        }
        FactoryAttemptOperation::StartFork { attempts, .. } => attempts
            .iter()
            .filter_map(|attempt| target(&attempt.disposition))
            .collect(),
        FactoryAttemptOperation::BindDispatch { attempt_ref, .. } => addressed
            .get(attempt_ref)
            .and_then(|record| target(&record.disposition))
            .into_iter()
            .collect(),
        FactoryAttemptOperation::RecordObservation { attempt_ref, receipt }
            if receipt.owner_ref == "factory"
                && receipt.contract == "factory.attempt-owner-transport/v1"
                && receipt.phase == OwnerOperationPhase::Dispatching
                && receipt.payload["action"] == "send" =>
        {
            addressed
                .get(attempt_ref)
                .and_then(|record| target(&record.disposition))
                .into_iter()
                .collect()
        }
        FactoryAttemptOperation::RecordObservation { receipt, .. }
            if receipt.owner_ref == "factory"
                && receipt.contract == CALL
                && receipt.phase == OwnerOperationPhase::Dispatching
                && matches!(
                    receipt.payload["operation"].as_str(),
                    Some("recover" | "release")
                ) =>
        {
            vec![(
                receipt.payload["workcellRef"]
                    .as_str()
                    .ok_or("material intent omits Workcell identity")?,
                receipt.payload["worldRef"]
                    .as_str()
                    .ok_or("material intent omits World identity")?,
            )]
        }
        _ => return Ok(()),
    };
    for record in records {
        if targets
            .iter()
            .any(|(workcell, world)| blocked(record, workcell, world))
        {
            return Err("material lifecycle is unresolved, released or superseded; re-resolve before starting or dispatching work".into());
        }
    }
    Ok(())
}
