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
) -> Result<(), crate::cli::CliError> {
    for _ in 0..8 {
        let reading = store.reading().map_err(crate::cli::CliError::from_native)?;
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
                if crate::native_publication_uncertainty(&error).is_some() {
                    return Err(crate::cli::CliError::from_native(error));
                }
                if store
                    .reading()
                    .map_err(crate::cli::CliError::from_native)?
                    .revision
                    == reading.revision
                {
                    return Err(crate::cli::CliError::from_native(error));
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
    // A recovery is a new observation of the SAME enduring intake request.
    // Include its actual local revision so it cannot collide with the first call.
    receipt.receipt_ref = format!(
        "factory-learning-call:{}",
        blake3::hash(
            &serde_json::to_vec(&(
                &receipt.operation_ref,
                &receipt.source_revision,
                phase,
                &receipt.payload
            ))
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
        "needsReconciliation":!recorded||error.is_some()||reading.is_err()||receipt.phase!=OwnerOperationPhase::Observed,
        "transportObservation":receipt,"retentionError":error.or_else(||reading.as_ref().err().map(|error|error.to_string())),
        "reading":reading.ok(),"automaticPromotionPerformed":false,"humanRecognitionPerformed":false})
}

pub fn execute(path: &Path, request: LearningRequest) -> Result<Value, crate::cli::CliError> {
    if request.contract != LEARNING_ACTION
        || request.request_ref.trim().is_empty()
        || request.projection_ref.trim().is_empty()
        || request.observation_ref.trim().is_empty()
        || request.statement.trim().is_empty()
        || request.statement.len() > 64 * 1024
        || !request.ledger_root.is_absolute()
    {
        return Err("learning intake requires exact Action identity, bounded statement and an absolute native ledger root".into());
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
        .map_err(crate::cli::CliError::from_native)?;
    let mut recorded = false;
    let mut publication_details = None;
    let mut attempt_publication_uncertain = false;
    let mut secondary_retention_failure = None;
    let mut secondary_publication_details = None;
    let publication = (|| -> Result<(), String> {
        ledger_store
            .transact(&request.run_ref, true, |ledger| {
                if let Some(previous) = ledger
                    .observations
                    .iter()
                    .find(|previous| previous.observation_ref == observation.observation_ref)
                {
                    if previous != &observation {
                        return Err(
                            crate::project_development_store::ProjectDevelopmentStoreError::Native(
                                "native learning observation identity has conflicting content"
                                    .into(),
                            ),
                        );
                    }
                } else {
                    ledger
                        .add_observation(observation.clone())
                        .map_err(|error| {
                            crate::project_development_store::ProjectDevelopmentStoreError::Native(
                                error.to_string(),
                            )
                        })?;
                }
                Ok(())
            })
            .map_err(|error| {
                publication_details =
                    crate::native_publication_uncertainty(&error).map(|failure| failure.details());
                error.to_string()
            })?;
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
        .map_err(|error| {
            attempt_publication_uncertain = crate::native_publication_uncertainty(&error).is_some();
            if publication_details.is_none() {
                publication_details = crate::native_publication_uncertainty(&error)
                    .map(|publication| publication.details());
            }
            error.to_string()
        })
    })();
    let mut failure = publication.err();
    let phase = if failure.is_none() {
        OwnerOperationPhase::Observed
    } else {
        OwnerOperationPhase::Uncertain
    };
    let mut payload = json!({"recordedInExistingLedger":recorded,"failure":failure,"meaning":"observation/proposal intake only; no automatic promotion"});
    if let Some(details) = &publication_details {
        payload["publicationUncertainty"] =
            serde_json::to_value(details).expect("native publication details serialize");
    }
    let settled = stamp(intent, phase, payload);
    // Ledger publication and attempt correlation have distinct native sources.
    // Only uncertainty from this attempt's correlation forbids this later
    // settlement write; it must not retry or overwrite unresolved continuity.
    if !attempt_publication_uncertain {
        if let Err(error) = retain(
            &mut store,
            &request,
            "settled",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: settled.clone(),
            },
        ) {
            let details = crate::native_publication_uncertainty(&error)
                .map(|publication| publication.details());
            if publication_details.is_none() {
                publication_details = details;
            } else {
                secondary_publication_details = details;
            }
            if failure.is_none() {
                failure = Some(error.to_string());
            } else {
                // Keep the original ledger failure primary and retain this actual
                // later attempt-source cause separately. There is no third write.
                secondary_retention_failure = Some(error);
            }
        }
    }
    let mut result = response(
        &store,
        &request,
        &observation,
        settled,
        recorded,
        failure,
        false,
    );
    if let Some(details) = publication_details {
        result["publicationUncertainty"] =
            serde_json::to_value(details).expect("native publication details serialize");
    }
    if let Some(failure) = secondary_retention_failure {
        result["secondaryRetentionError"] = json!(failure.to_string());
    }
    if let Some(details) = secondary_publication_details {
        result["secondaryPublicationUncertainty"] =
            serde_json::to_value(details).expect("native publication details serialize");
    }
    Ok(result)
}

pub fn execute_cli(args: &[String], stdin: Option<&str>) -> Result<String, crate::cli::CliError> {
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
        std::fs::read_to_string(input).map_err(crate::cli::CliError::from_native)?
    } else if let Some(body) = stdin {
        body.into()
    } else {
        let mut body = String::new();
        std::io::stdin()
            .read_to_string(&mut body)
            .map_err(crate::cli::CliError::from_native)?;
        body
    };
    serde_json::to_string_pretty(&execute(
        Path::new(args[0]),
        serde_json::from_str(&body).map_err(crate::cli::CliError::from_native)?,
    )?)
    .map_err(crate::cli::CliError::from_native)
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
pub(crate) mod publication_tests {
    use super::*;
    use crate::action_projection::FactoryActionProjectionKind;
    use crate::attempt_runtime::*;
    use crate::core::run::{Run, RunRef};
    use crate::execution_intelligence::{
        accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand,
        AIKIT_MODEL_ROSTER_VERSION,
    };
    use crate::project_development::ProjectDevelopmentLedger;
    use crate::workflow::{compile_workflow, WorkflowSource};
    use std::cell::Cell;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::rc::Rc;
    const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
    const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
    // Controlled native store data admits no real worker/model. The exercised
    // behavior is native transaction/retention, not participant execution.
    struct World {
        dir: tempfile::TempDir,
    }
    impl World {
        fn state(&self) -> String {
            self.dir.path().join("state.json").display().to_string()
        }
        fn ledger(&self) -> String {
            self.dir.path().join("ledger").display().to_string()
        }
        fn call(&self, operation: &str, body: Option<Value>) -> Value {
            let mut args = vec!["attempt".into(), operation.into(), self.state()];
            if body.is_some() {
                args.push("-".into());
            }
            args.push("--json".into());
            let body = body.map(|value| value.to_string());
            serde_json::from_str(&crate::attempt_cli::execute(&args, body.as_deref()).unwrap())
                .unwrap()
        }
        fn reading(&self) -> FactoryAttemptReading {
            serde_json::from_value(self.call("read", None)).unwrap()
        }
        fn action(&self, operation: FactoryAttemptOperation) {
            let request = FactoryAttemptActionRequest {
                contract: FACTORY_ATTEMPT_ACTION.into(),
                projection_ref: format!("projection:{}", self.reading().revision),
                caller: caller(),
                run_ref: RUN.parse().unwrap(),
                expected_revision: self.reading().revision,
                authority: authority(),
                operation,
            };
            self.call("action", Some(serde_json::to_value(request).unwrap()));
        }
        fn new() -> Self {
            let world = Self {
                dir: tempfile::tempdir().unwrap(),
            };
            let run = Run::new(
                RUN.parse::<RunRef>().unwrap(),
                PROJECT.parse().unwrap(),
                "learning controlled world",
                "factory-test",
            )
            .unwrap();
            let source: WorkflowSource = serde_json::from_str(include_str!(
                "../../contracts/factory/fixtures/agent-workflow-source.json"
            ))
            .unwrap();
            let workflow = compile_workflow(source.clone()).unwrap();
            let unit = workflow.unit("inspect-source").unwrap();
            world.call(
                "init",
                Some(
                    serde_json::to_value(FactoryAttemptSeed {
                        run,
                        workflow_source: source,
                    })
                    .unwrap(),
                ),
            );
            let selection = accept_aikit_selection(
                ExecutionDemand {
                    project_ref: PROJECT.into(),
                    run_ref: RUN.into(),
                    workflow_unit_ref: Some(unit.reference.to_string()),
                    agency_ref: Some("agency:test".into()),
                    profile_ref: None,
                    use_type: "test-only".into(),
                    required_capabilities: unit.capability_refs.clone(),
                    required_modalities: BTreeSet::from(["text".into()]),
                    required_actions: BTreeSet::new(),
                    required_tools: BTreeSet::new(),
                    context_characteristics: BTreeSet::new(),
                    independence_from: BTreeSet::new(),
                    cost_ceiling_usd: None,
                    latency_preference_ms: None,
                    requires_local_materialisation: false,
                },
                AikitModelRosterSelection {
                    roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
                    model_ref: "model:test".into(),
                    provider_ref: "provider:test".into(),
                    ranking_policy: "test".into(),
                    ranking_explanation: json!({"testOnly":true}),
                    provenance: vec!["selection:test".into()],
                },
                "2026-09-10T20:00:00+01:00",
            )
            .unwrap();
            let disposition = SituatedExecutionDisposition {
                selected_inputs: Vec::new(),
                selection,
                participant: SituatedParticipant {
                    agent_ref: unit
                        .agent_requirements
                        .agent_refs
                        .iter()
                        .next()
                        .unwrap()
                        .clone(),
                    agency_ref: "agency:test".into(),
                    world_binding_ref: "binding:test".into(),
                    profile_ref: None,
                    position_ref: None,
                    source_ref: workflow.source.reference.to_string(),
                    source_revision: workflow.source.revision.clone(),
                    source_digest: format!("blake3:{}", workflow.source.digest),
                },
                context_refs: BTreeSet::from(["context:test".into()]),
                praxis_refs: unit.praxis_refs.clone(),
                capability_refs: unit.capability_refs.clone(),
                body: ExecutionBody {
                    model_ref: "model:test".into(),
                    provider_ref: "provider:test".into(),
                    route_ref: "route:test".into(),
                    harness_ref: "harness:test".into(),
                    harness_composition_ref: "composition:test".into(),
                    agent_session_ref: "session:test".into(),
                    session_space_ref: "space:test".into(),
                    material_world_ref: None,
                    workcell_ref: None,
                },
                placement: None,
                permitted_effects: unit.permitted_effects.clone(),
                verification_obligations: unit.verification_obligations.clone(),
                return_address: unit.return_address.clone(),
                stop_conditions: unit.stop_conditions.clone(),
                escalation_conditions: unit.escalation_conditions.clone(),
                budget: ExecutionBudget {
                    cost_ceiling_usd: None,
                    latency_preference_ms: None,
                    wall_clock_timeout_ms: Some(10_000),
                    retry_grant_ref: None,
                    maximum_attempts: None,
                },
            };
            world.action(FactoryAttemptOperation::StartSerial {
                attempt_ref: "attempt:learning".into(),
                task_ref: "task:learning".into(),
                parent_journey_ref: "journey:test".into(),
                workflow_unit_ref: unit.reference.clone(),
                disposition,
                retry_grant: None,
                tracking: vec![],
                place_grant: None,
            });
            world
        }
        fn request(&self) -> Value {
            json!({"contract":"factory.attempt-learning-action/v1","requestRef":"learning-request:test","projectionRef":"projection:test","caller":caller(),"runRef":RUN,"expectedRevision":self.reading().revision,"authority":authority(),"attemptRef":"attempt:learning","ledgerRoot":self.ledger(),"observationRef":"observation:attempt-learning","kind":"insufficient-evidence","statement":"The controlled attempt did not produce enough evidence yet.","evidenceRefs":[],"ownerReturn":null,"recover":false})
        }
    }
    fn caller() -> FactoryActionCaller {
        FactoryActionCaller {
            caller_ref: "agent:test".into(),
            projection_kind: FactoryActionProjectionKind::Headless,
            lineage: vec!["agent:test".into()],
        }
    }
    fn authority() -> ProjectedFactoryActionAuthority {
        ProjectedFactoryActionAuthority {
            authority_ref: "authority:test".into(),
            native_owner: "factory".into(),
            capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
            capability_granted: true,
            action_authorised: true,
        }
    }

    pub(crate) fn native_retention_source() -> (tempfile::TempDir, FileAttemptStore, LearningRequest)
    {
        let world = World::new();
        let request: LearningRequest = serde_json::from_value(world.request()).unwrap();
        let store = FileAttemptStore::open_run(world.state(), request.run_ref.clone()).unwrap();
        (world.dir, store, request)
    }
    pub(crate) fn native_observation(store: &FileAttemptStore) -> OwnerOperationReceipt {
        let reading = store.reading().unwrap();
        OwnerOperationReceipt {
            owner_ref: "factory".into(),
            contract: "factory.native-retention-observation/v1".into(),
            operation_ref: "factory-retention:actual-source".into(),
            receipt_ref: "factory-retention:actual-source-reading".into(),
            source_revision: format!("factory-state:{}", reading.revision),
            phase: OwnerOperationPhase::Observed,
            evidence_refs: BTreeSet::new(),
            partial_effect_refs: BTreeSet::new(),
            payload: serde_json::to_value(reading).unwrap(),
        }
    }
    pub(crate) fn change_actual_published_privacy() {
        crate::native_file_transaction::observe_next_publication(|published| {
            std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777)).unwrap();
        });
    }
    #[derive(Clone)]
    struct NativePublicationSource {
        address: PathBuf,
        physical: PathBuf,
        parent_identity: (u64, u64),
    }
    impl NativePublicationSource {
        fn capture(address: PathBuf) -> Self {
            let physical = std::fs::canonicalize(&address).unwrap();
            let identity = |path: &Path| {
                let metadata = std::fs::metadata(path).unwrap();
                (metadata.dev(), metadata.ino())
            };
            // Retain the caller's actual native address. Before any rename,
            // prove its source and parent are the physical objects we will fault.
            assert_eq!(identity(&address), identity(&physical));
            let parent_identity = identity(address.parent().unwrap());
            assert_eq!(parent_identity, identity(physical.parent().unwrap()));
            Self {
                address,
                physical,
                parent_identity,
            }
        }
        fn matches(&self, published: &Path) -> bool {
            // The observer receives the owner's lexical address; uncertainty
            // details retain its pre-rename physical address. /var on Mac can
            // name the same held parent as /private/var without spelling alike.
            if published != self.address {
                return false;
            }
            let parent = std::fs::metadata(published.parent().unwrap()).unwrap();
            assert_eq!((parent.dev(), parent.ino()), self.parent_identity);
            assert_eq!(std::fs::canonicalize(published).unwrap(), self.physical);
            true
        }
        fn assert_published(&self, published: &Path) {
            assert_eq!(published, self.address);
            assert!(self.matches(published));
        }
    }
    fn change_after_source_publication(
        source: NativePublicationSource,
        occurrence: usize,
        seen: Rc<Cell<usize>>,
    ) {
        crate::native_file_transaction::observe_next_publication(move |published| {
            let matches = source.matches(published);
            if matches {
                seen.set(seen.get() + 1);
            }
            if matches && seen.get() == occurrence {
                std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                    .unwrap();
            } else {
                change_after_source_publication(source, occurrence, seen);
            }
        });
    }
    #[test]
    fn actual_attempt_correlation_uncertainty_stops_same_source_settlement() {
        let world = World::new();
        let request: LearningRequest = serde_json::from_value(world.request()).unwrap();
        let ledger = FileProjectDevelopmentStore::new(&request.ledger_root);
        ledger
            .save(&ProjectDevelopmentLedger::new(request.run_ref.clone()))
            .unwrap();
        let before = world.reading().revision;
        let source = NativePublicationSource::capture(PathBuf::from(world.state()));
        let seen = Rc::new(Cell::new(0));
        change_after_source_publication(source.clone(), 2, seen.clone());
        let args = vec![
            "attempt".into(),
            "learn".into(),
            world.state(),
            "-".into(),
            "--json".into(),
        ];
        let returned: Value = serde_json::from_str(
            &crate::attempt_cli::execute(&args, Some(&serde_json::to_string(&request).unwrap()))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(seen.get(), 2);
        assert_eq!(
            returned["publicationUncertainty"]["source_path"],
            source.physical.display().to_string()
        );
        assert_eq!(returned["publicationUncertainty"]["published"], true);
        assert_eq!(returned["publicationUncertainty"]["automatic_retry"], false);
        assert_eq!(returned["needsReconciliation"], true);
        let current = world.reading();
        assert_eq!(
            current.revision,
            before + 2,
            "intent and committed correlation only; no automatic settlement write"
        );
        let attempt = current
            .attempts
            .iter()
            .find(|a| a.attempt_ref == request.attempt_ref)
            .unwrap();
        assert_eq!(
            attempt
                .tracking
                .iter()
                .filter(|fact| fact.subject_ref == request.observation_ref)
                .count(),
            1
        );
        assert!(attempt
            .observations
            .iter()
            .filter(|r| r.contract == CALL_CONTRACT)
            .all(|r| r.phase == OwnerOperationPhase::Dispatching));
        assert_eq!(
            ledger
                .load(&request.run_ref)
                .unwrap()
                .unwrap()
                .observations
                .len(),
            1
        );
    }
    #[test]
    fn actual_separate_ledger_uncertainty_is_observed_without_retrying_ledger() {
        let world = World::new();
        let request: LearningRequest = serde_json::from_value(world.request()).unwrap();
        let ledger = FileProjectDevelopmentStore::new(&request.ledger_root);
        ledger
            .save(&ProjectDevelopmentLedger::new(request.run_ref.clone()))
            .unwrap();
        let before = world.reading().revision;
        let source = NativePublicationSource::capture(
            request
                .ledger_root
                .join(format!("{}.json", request.run_ref.as_ref().id())),
        );
        let seen = Rc::new(Cell::new(0));
        change_after_source_publication(source.clone(), 1, seen.clone());
        let returned = execute(Path::new(&world.state()), request.clone()).unwrap();
        assert_eq!(seen.get(), 1);
        assert_eq!(
            returned["publicationUncertainty"]["source_path"],
            source.physical.display().to_string()
        );
        assert_eq!(returned["needsReconciliation"], true);
        let current = world.reading();
        assert_eq!(
            current.revision,
            before + 2,
            "intent and separate-source uncertainty observation"
        );
        let attempt = current
            .attempts
            .iter()
            .find(|a| a.attempt_ref == request.attempt_ref)
            .unwrap();
        assert!(attempt.tracking.is_empty());
        assert!(attempt
            .observations
            .iter()
            .any(|r| r.contract == CALL_CONTRACT && r.phase == OwnerOperationPhase::Uncertain));
        assert_eq!(
            ledger
                .load(&request.run_ref)
                .unwrap()
                .unwrap()
                .observations
                .len(),
            1
        );
    }
    fn fault_two_native_sources(
        attempt_source: NativePublicationSource,
        ledger_source: NativePublicationSource,
        seen: Rc<Cell<usize>>,
    ) {
        crate::native_file_transaction::observe_next_publication(move |published| {
            let occurrence = seen.get() + 1;
            seen.set(occurrence);
            match occurrence {
                1 => attempt_source.assert_published(published), // intent
                2 => {
                    ledger_source.assert_published(published);
                    std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                        .unwrap();
                }
                3 => {
                    attempt_source.assert_published(published); // permitted observation
                    std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                        .unwrap();
                }
                _ => panic!("no later mutation is allowed after the attempt uncertainty"),
            }
            if occurrence < 3 {
                fault_two_native_sources(attempt_source, ledger_source, seen);
            }
        });
    }

    #[test]
    fn actual_dual_source_uncertainty_retains_both_native_causes_without_third_attempt_write() {
        let world = World::new();
        let request: LearningRequest = serde_json::from_value(world.request()).unwrap();
        let ledger = FileProjectDevelopmentStore::new(&request.ledger_root);
        ledger
            .save(&ProjectDevelopmentLedger::new(request.run_ref.clone()))
            .unwrap();
        let before = world.reading().revision;
        let attempt_source = NativePublicationSource::capture(PathBuf::from(world.state()));
        let ledger_source = NativePublicationSource::capture(
            request
                .ledger_root
                .join(format!("{}.json", request.run_ref.as_ref().id())),
        );
        let seen = Rc::new(Cell::new(0));
        fault_two_native_sources(attempt_source.clone(), ledger_source.clone(), seen.clone());
        let returned = execute(Path::new(&world.state()), request.clone()).unwrap();
        assert_eq!(
            seen.get(),
            3,
            "intent, ledger, one attempt observation only"
        );
        let first = &returned["publicationUncertainty"];
        let second = &returned["secondaryPublicationUncertainty"];
        assert_eq!(
            first["source_path"],
            ledger_source.physical.display().to_string()
        );
        assert_eq!(
            second["source_path"],
            attempt_source.physical.display().to_string()
        );
        for details in [first, second] {
            assert_eq!(details["published"], true);
            assert_eq!(details["outcome"], "unknown");
            assert_eq!(details["automatic_retry"], false);
            assert!(details["cause"]["kind"].is_string());
            assert!(!details["cause"]["message"].as_str().unwrap().is_empty());
        }
        let primary = returned["retentionError"].as_str().unwrap();
        let secondary = returned["secondaryRetentionError"].as_str().unwrap();
        assert!(!primary.is_empty() && !secondary.is_empty());
        assert_eq!(
            returned["transportObservation"]["payload"]["detail"]["failure"], primary,
            "original ledger failure stays primary in returned and retained facts"
        );
        assert_eq!(returned["needsReconciliation"], true);
        let current = world.reading();
        assert_eq!(
            current.revision,
            before + 2,
            "no third attempt-source write"
        );
        let attempt = current
            .attempts
            .iter()
            .find(|a| a.attempt_ref == request.attempt_ref)
            .unwrap();
        assert!(
            attempt.tracking.is_empty(),
            "unconfirmed ledger does not produce correlation"
        );
        assert!(attempt
            .observations
            .iter()
            .any(|r| r.contract == CALL_CONTRACT && r.phase == OwnerOperationPhase::Uncertain));
        assert!(
            attempt.readable_return.is_none(),
            "native persistence is not a worker Return"
        );
        assert_eq!(
            ledger
                .load(&request.run_ref)
                .unwrap()
                .unwrap()
                .observations
                .len(),
            1
        );
    }
}
