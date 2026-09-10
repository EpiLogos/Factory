//! Evidence-grounded intake into Factory's EXISTING ProjectDevelopmentLedger.
//! An observation/proposal does not promote a claim, rewrite a Method, recognise
//! a Return, or train a model. Intent/recovery joins the two existing stores.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::attempt_native_store::FileAttemptStore;
use crate::attempt_runtime::{
    AttemptTrackingFact, FactoryAttemptActionRequest, FactoryAttemptOperation,
    FactoryAttemptReading, FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt,
    FACTORY_ATTEMPT_ACTION,
};
use crate::core::run::RunRef;
use crate::project_development::{
    DevelopmentObservation, DevelopmentObservationKind, OwnerReturnProposal,
    ProjectDevelopmentLedger,
};
use crate::project_development_store::{FileProjectDevelopmentStore, ProjectDevelopmentStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const LEARNING_ACTION: &str = "factory.attempt-learning-action/v1";
pub const LEARNING_RECEIPT: &str = "factory.attempt-learning-receipt/v1";
const CALL_CONTRACT: &str = "factory.attempt-learning-call/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LearningRequest {
    pub contract: String,
    pub request_ref: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub attempt_ref: String,
    pub ledger_root: PathBuf,
    pub observation_ref: String,
    pub kind: DevelopmentObservationKind,
    pub statement: String,
    /// May select only evidence already retained on this exact attempt.
    pub evidence_refs: BTreeSet<String>,
    pub owner_return: Option<OwnerReturnProposal>,
    #[serde(default)]
    pub recover: bool,
}

fn attempt<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, String> {
    reading
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_ref == reference)
        .ok_or_else(|| "attempt not retained in the addressed Run".into())
}
fn native(
    request: &LearningRequest,
    revision: u64,
    suffix: &str,
    operation: FactoryAttemptOperation,
) -> FactoryAttemptActionRequest {
    FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: format!(
            "{}:{}:{suffix}",
            request.projection_ref, request.request_ref
        ),
        caller: request.caller.clone(),
        run_ref: request.run_ref.clone(),
        expected_revision: revision,
        authority: request.authority.clone(),
        operation,
    }
}
fn retain(
    store: &mut FileAttemptStore,
    request: &LearningRequest,
    suffix: &str,
    operation: FactoryAttemptOperation,
) -> Result<(), String> {
    for _ in 0..8 {
        let reading = store.reading().map_err(|error| error.to_string())?;
        let record = attempt(&reading, &request.attempt_ref)?;
        match &operation {
            FactoryAttemptOperation::RecordObservation { receipt, .. } => {
                if let Some(previous) = record
                    .observations
                    .iter()
                    .find(|previous| previous.receipt_ref == receipt.receipt_ref)
                {
                    return if previous == receipt {
                        Ok(())
                    } else {
                        Err("conflicting learning receipt identity".into())
                    };
                }
            }
            FactoryAttemptOperation::RecordTracking { fact, .. } => {
                if let Some(previous) = record
                    .tracking
                    .iter()
                    .find(|previous| previous.fact_ref == fact.fact_ref)
                {
                    return if previous == fact {
                        Ok(())
                    } else {
                        Err("conflicting learning correlation identity".into())
                    };
                }
            }
            _ => return Err("unsupported learning retention operation".into()),
        }
        match store.apply(native(request, reading.revision, suffix, operation.clone())) {
            Ok(_) => return Ok(()),
            Err(error) => {
                if store.reading().map_err(|error| error.to_string())?.revision == reading.revision
                {
                    return Err(error.to_string());
                }
            }
        }
    }
    Err("learning correlation remained contended; explicitly recover the same observation".into())
}
fn observation(
    request: &LearningRequest,
    reading: &FactoryAttemptReading,
    record: &FactoryAttemptRecord,
) -> Result<DevelopmentObservation, String> {
    let mut retained = record.failure_evidence_refs.clone();
    for receipt in record.dispatch.iter().chain(&record.observations) {
        retained.extend(receipt.evidence_refs.clone());
        retained.extend(receipt.partial_effect_refs.clone());
    }
    for verification in &record.verifications {
        retained.extend(verification.evidence_refs.clone());
    }
    if let Some(returned) = &record.readable_return {
        retained.extend(returned.evidence_refs.clone());
    }
    if !request.evidence_refs.is_subset(&retained) {
        return Err("learning evidence is not retained on the selected attempt".into());
    }
    if request.evidence_refs.is_empty()
        && request.kind != DevelopmentObservationKind::InsufficientEvidence
    {
        return Err("fitness/discrepancy observations require actual retained evidence".into());
    }
    if request.owner_return.as_ref().is_some_and(|proposal| {
        !proposal.recognition_required
            || proposal.owner_ref.trim().is_empty()
            || proposal.proposal_ref.trim().is_empty()
    }) {
        return Err("owner change proposals must preserve explicit Recognition and real owner/proposal identity".into());
    }
    let mut subjects = BTreeSet::from([
        request.run_ref.to_string(),
        record.task_ref.clone(),
        record.attempt_ref.clone(),
        record.workflow_unit_ref.to_string(),
        reading.workflow_source_ref.clone(),
        record.disposition.participant.source_ref.clone(),
        record.disposition.participant.agent_ref.clone(),
        record.disposition.body.model_ref.clone(),
        record.disposition.body.harness_ref.clone(),
        record.disposition.body.agent_session_ref.clone(),
    ]);
    subjects.extend(record.disposition.context_refs.clone());
    subjects.extend(record.disposition.praxis_refs.clone());
    if let Some(reference) = &record.execution_ref {
        subjects.insert(reference.clone());
    }
    if let Some(reference) = &record.disposition.body.material_world_ref {
        subjects.insert(reference.clone());
    }
    if let Some(placement) = &record.disposition.placement {
        subjects.insert(placement.now_ref.clone());
    }
    if let Some(returned) = &record.readable_return {
        subjects.insert(returned.return_ref.clone());
        subjects.extend(returned.artifact_refs.clone());
    }
    Ok(DevelopmentObservation {
        run_ref: request.run_ref.clone(),
        observation_ref: request.observation_ref.clone(),
        kind: request.kind,
        statement: request.statement.clone(),
        subject_refs: subjects.into_iter().collect(),
        evidence_refs: request.evidence_refs.iter().cloned().collect(),
        owner_return: request.owner_return.clone(),
    })
}
fn stamp(
    mut receipt: OwnerOperationReceipt,
    phase: OwnerOperationPhase,
    detail: Value,
) -> OwnerOperationReceipt {
    receipt.phase = phase;
    receipt.payload["detail"] = detail;
    receipt.receipt_ref = format!(
        "factory-learning-call:{}",
        blake3::hash(
            &serde_json::to_vec(&(&receipt.operation_ref, phase, &receipt.payload))
                .expect("JSON values serialize")
        )
        .to_hex()
    );
    receipt
}
fn response(
    store: &FileAttemptStore,
    request: &LearningRequest,
    observation: &DevelopmentObservation,
    receipt: OwnerOperationReceipt,
    recorded: bool,
    error: Option<String>,
    replayed: bool,
) -> Value {
    let reading = store.reading();
    json!({"contract":LEARNING_RECEIPT,"requestRef":request.request_ref,"runRef":request.run_ref,"attemptRef":request.attempt_ref,
        "observation":observation,"ledgerRoot":request.ledger_root,"recordedInExistingLedger":recorded,"replayed":replayed,
        "needsReconciliation":error.is_some()||reading.is_err()||receipt.phase!=OwnerOperationPhase::Observed,
        "transportObservation":receipt,"retentionError":error.or_else(||reading.as_ref().err().map(|error|error.to_string())),
        "reading":reading.ok(),"automaticPromotionPerformed":false,"humanRecognitionPerformed":false})
}

pub fn execute(path: &Path, request: LearningRequest) -> Result<Value, String> {
    if request.contract != LEARNING_ACTION
        || request.request_ref.trim().is_empty()
        || request.projection_ref.trim().is_empty()
        || request.observation_ref.trim().is_empty()
        || request.statement.trim().is_empty()
        || request.statement.len() > 64 * 1024
        || !request.ledger_root.is_absolute()
    {
        return Err("learning intake requires exact Action identity, bounded statement and an absolute existing native ledger root".into());
    }
    let mut store = FileAttemptStore::open_run(path, request.run_ref.clone())
        .map_err(|error| error.to_string())?;
    let reading = store.reading().map_err(|error| error.to_string())?;
    let record = attempt(&reading, &request.attempt_ref)?;
    let mut canonical = serde_json::to_value(&request).map_err(|error| error.to_string())?;
    for key in ["recover", "expectedRevision", "projectionRef"] {
        canonical
            .as_object_mut()
            .expect("request object")
            .remove(key);
    }
    let digest = blake3::hash(&serde_json::to_vec(&canonical).map_err(|error| error.to_string())?)
        .to_hex()
        .to_string();
    let operation_ref = format!("factory-attempt-learning:{}", request.request_ref);
    let previous = record
        .observations
        .iter()
        .rev()
        .find(|receipt| {
            receipt.owner_ref == "factory"
                && receipt.contract == CALL_CONTRACT
                && receipt.operation_ref == operation_ref
        })
        .cloned();
    if previous
        .as_ref()
        .is_some_and(|receipt| receipt.payload["requestDigest"].as_str() != Some(digest.as_str()))
    {
        return Err("learning request identity reused with changed content".into());
    }
    let observation = if let Some(previous) = &previous {
        serde_json::from_value(previous.payload["observation"].clone())
            .map_err(|error| error.to_string())?
    } else {
        observation(&request, &reading, record)?
    };
    let evidence = BTreeSet::from([request.observation_ref.clone()]);
    let intent = stamp(
        OwnerOperationReceipt {
            owner_ref: "factory".into(),
            contract: CALL_CONTRACT.into(),
            operation_ref,
            receipt_ref: String::new(),
            source_revision: format!("factory-state:{}", reading.revision),
            phase: OwnerOperationPhase::Dispatching,
            evidence_refs: evidence.clone(),
            partial_effect_refs: BTreeSet::new(),
            payload: json!({"requestDigest":digest,"observation":observation,"sourceRef":reading.workflow_source_ref,
            "sourceRevision":reading.workflow_source_revision,"sourceDigest":reading.workflow_source_digest}),
        },
        OwnerOperationPhase::Dispatching,
        json!({"meaning":"intake intent, not an established finding or owner mutation"}),
    );
    crate::attempt_runtime::validate_action_request(&native(
        &request,
        reading.revision,
        "admission",
        FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt: intent.clone(),
        },
    ))
    .map_err(|error| error.to_string())?;
    let ledger_store = FileProjectDevelopmentStore::new(&request.ledger_root);
    if let Some(previous) = previous {
        if !request.recover {
            let recorded = ledger_store
                .load(&request.run_ref)
                .map_err(|error| error.to_string())?
                .is_some_and(|ledger| ledger.observations.contains(&observation));
            return Ok(response(
                &store,
                &request,
                &observation,
                previous,
                recorded,
                None,
                true,
            ));
        }
    } else if request.recover {
        return Err("no retained learning request to recover".into());
    }
    if reading.revision != request.expected_revision {
        return Err("stale Factory revision before learning intake".into());
    }
    store
        .apply(native(
            &request,
            request.expected_revision,
            "intent",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: intent.clone(),
            },
        ))
        .map_err(|error| error.to_string())?;
    let mut recorded = false;
    let publication = (|| -> Result<(), String> {
        let mut ledger = ledger_store
            .load(&request.run_ref)
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| ProjectDevelopmentLedger::new(request.run_ref.clone()));
        if let Some(previous) = ledger
            .observations
            .iter()
            .find(|previous| previous.observation_ref == observation.observation_ref)
        {
            if previous != &observation {
                return Err("native learning observation identity has conflicting content".into());
            }
        } else {
            ledger
                .add_observation(observation.clone())
                .map_err(|error| error.to_string())?;
        }
        ledger_store
            .save(&ledger)
            .map_err(|error| error.to_string())?;
        recorded = ledger_store
            .load(&request.run_ref)
            .map_err(|error| error.to_string())?
            .is_some_and(|ledger| ledger.observations.contains(&observation));
        if !recorded {
            return Err("native learning readback did not retain the exact observation".into());
        }
        let observation_digest = format!(
            "blake3:{}",
            blake3::hash(&serde_json::to_vec(&observation).map_err(|error| error.to_string())?)
                .to_hex()
        );
        retain(
            &mut store,
            &request,
            "correlation",
            FactoryAttemptOperation::RecordTracking {
                attempt_ref: request.attempt_ref.clone(),
                fact: AttemptTrackingFact {
                    fact_ref: format!(
                        "factory-learning:{}:{}",
                        request.attempt_ref, request.observation_ref
                    ),
                    kind: "regression-observation".into(),
                    owner_ref: "factory".into(),
                    subject_ref: request.observation_ref.clone(),
                    source_revision: observation_digest,
                    evidence_refs: evidence,
                },
            },
        )
    })();
    let mut failure = publication.err();
    let phase = if failure.is_none() {
        OwnerOperationPhase::Observed
    } else {
        OwnerOperationPhase::Uncertain
    };
    let settled = stamp(
        intent,
        phase,
        json!({"recordedInExistingLedger":recorded,"failure":failure,"meaning":"observation/proposal intake only; no automatic promotion"}),
    );
    if let Err(error) = retain(
        &mut store,
        &request,
        "settled",
        FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt: settled.clone(),
        },
    ) {
        failure = Some(error);
    }
    Ok(response(
        &store,
        &request,
        &observation,
        settled,
        recorded,
        failure,
        false,
    ))
}

pub fn execute_cli(args: &[String], stdin: Option<&str>) -> Result<String, String> {
    let args = args
        .iter()
        .filter(|argument| argument.as_str() != "--json")
        .collect::<Vec<_>>();
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        return Ok(format!("factory attempt learn <native-state> <request-json|-> [--json]\nAction: {LEARNING_ACTION}\nAppends an evidence-grounded observation to the existing development ledger and correlates it to this attempt. Does not promote findings or modify owner sources. Explicit recover resumes a retained intake without duplicating it."));
    }
    if args.len() > 2 {
        return Err("unexpected learning command arguments".into());
    }
    let input = args.get(1).map(|value| value.as_str()).unwrap_or("-");
    let body = if input != "-" {
        std::fs::read_to_string(input).map_err(|error| error.to_string())?
    } else if let Some(body) = stdin {
        body.into()
    } else {
        let mut body = String::new();
        std::io::stdin()
            .read_to_string(&mut body)
            .map_err(|error| error.to_string())?;
        body
    };
    serde_json::to_string_pretty(&execute(
        Path::new(args[0]),
        serde_json::from_str(&body).map_err(|error| error.to_string())?,
    )?)
    .map_err(|error| error.to_string())
}
