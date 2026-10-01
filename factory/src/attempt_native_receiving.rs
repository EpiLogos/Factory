//! Live Central receiving admission. JSON observations and role labels cannot
//! construct the private witness consumed by the canonical Factory transaction.
use crate::attempt_native_store::{source_is_current, view_for, FileAttemptStore};
use crate::attempt_receiving::{CentralReceivingEndpoint, CENTRAL_CONTRACT_REVISION};
use crate::attempt_runtime::{
    FactoryAttemptActionRequest, FactoryAttemptOperation, FactoryAttemptRecord,
    OwnerOperationPhase, OwnerOperationReceipt, StoredAttemptState,
};
use crate::build::HumanRequestRecord;
use crate::core::run::{RunLifecycle, RunLifecycleOutcome, RunRef};
use crate::developmental_read::FactoryDevelopmentalState;
use crate::project_development_store::read_developmental_state;
use crate::run_lifecycle::{UnitDecisionOutcome, UnitDecisionResponse};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const QUESTION_CALL: &str = "factory.attempt-unit-decision-call/v1";
pub const DECISION_READING: &str = "factory.attempt-unit-decision-reading/v1";

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("native receiving omitted {field}"))
}
fn digest(value: &impl serde::Serialize) -> Result<String, String> {
    Ok(
        blake3::hash(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
            .to_hex()
            .to_string(),
    )
}
fn configured_endpoint() -> Result<CentralReceivingEndpoint, String> {
    let binary = std::env::var_os("FACTORY_NATIVE_CENTRAL_BINARY")
        .ok_or("host-configured FACTORY_NATIVE_CENTRAL_BINARY is unavailable")?;
    let root = std::env::var_os("FACTORY_NATIVE_CENTRAL_ROOT")
        .ok_or("host-configured FACTORY_NATIVE_CENTRAL_ROOT is unavailable")?;
    let binary = PathBuf::from(binary);
    let root = PathBuf::from(root);
    if !binary.is_absolute() || !root.is_absolute() || !binary.is_file() || !root.is_dir() {
        return Err(
            "native receiving requires the host's absolute installed Central binary and root"
                .into(),
        );
    }
    Ok(CentralReceivingEndpoint {
        binary: binary.canonicalize().map_err(|e| e.to_string())?,
        root: root.canonicalize().map_err(|e| e.to_string())?,
        contract_revision: CENTRAL_CONTRACT_REVISION.into(),
        project: std::env::var("FACTORY_NATIVE_CENTRAL_PROJECT")
            .ok()
            .filter(|v| !v.trim().is_empty()),
    })
}
fn read(endpoint: &CentralReceivingEndpoint, reference: &str) -> Result<Value, String> {
    let mut input = json!({"return_ref":reference});
    if let Some(project) = &endpoint.project {
        input["project"] = json!(project);
    }
    crate::attempt_receiving::call(endpoint, "central.receiving.read", &input)
}
fn checked_reading<'a>(
    response: &'a Value,
    reference: &str,
) -> Result<(&'a Value, &'a Value), String> {
    let data = &response["data"];
    let record = &data["record"];
    if data["schema"] != "central.receiving-reading/v1"
        || record["schema"] != "central.received-contribution/v1"
        || data["return_ref"] != reference
        || record["return_ref"] != reference
    {
        return Err("native receiving changed schema or exact channel identity".into());
    }
    text(data, "revision")?;
    text(record, "request_digest")?;
    Ok((data, record))
}
fn pending(
    native: &FactoryDevelopmentalState,
    view: &StoredAttemptState,
    reference: &str,
) -> Result<HumanRequestRecord, String> {
    native
        .build
        .human_requests_for_run(view.run.reference())
        .into_iter()
        .find(|record| {
            record.human_request_ref == reference
                && record.unit_decision_basis.is_some()
                && record.unit_decision_response.is_none()
                && record.unit_decision_retirement.is_none()
        })
        .ok_or_else(|| "current unresolved canonical unit decision is unavailable".into())
}
pub(crate) fn question_input(
    native: &FactoryDevelopmentalState,
    view: &StoredAttemptState,
    reference: &str,
    endpoint: &CentralReceivingEndpoint,
) -> Result<(Value, String), String> {
    let decision = pending(native, view, reference)?;
    let basis = decision
        .unit_decision_basis
        .as_ref()
        .expect("pending basis");
    let workflow = crate::workflow::compile_workflow(view.workflow_source.clone())
        .map_err(|e| e.to_string())?;
    let engine = view
        .snapshot
        .restore(workflow, view.run.clone())
        .map_err(|e| e.to_string())?;
    crate::run_lifecycle::validate_decision_basis(view, &engine, basis)
        .map_err(|e| e.to_string())?;
    let attempt = &view.attempts[&basis.attempt_ref];
    let now = attempt
        .disposition
        .placement
        .as_ref()
        .map(|p| p.now_ref.clone())
        .or_else(|| {
            attempt
                .tracking
                .iter()
                .rev()
                .find(|f| f.owner_ref == "central" && f.kind == "now")
                .map(|f| f.subject_ref.clone())
        })
        .ok_or("unit decision requires its actual retained Central NOW correlation")?;
    let basis_digest = digest(basis)?;
    let producer = digest(&(view.run.reference(), reference, basis))?;
    let mut evidence = decision
        .evidence_refs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    evidence.extend([
        decision.decision_ref.clone(),
        reference.into(),
        format!("factory-unit-decision-basis:{basis_digest}"),
        format!("factory-unit-decision-controlled:{}", basis.controlled),
    ]);
    let mut input = json!({"producer_key":format!("factory-unit-decision:{producer}"),
        "now_ref":now,"run_ref":view.run.reference(),"task_ref":attempt.task_ref,
        "session_ref":attempt.disposition.body.agent_session_ref,"reply_to":reference,
        "evidence_refs":evidence,
        "request":{"kind":"question","subject":decision.question,
            "body":format!("{}\nAffected Factory basis: {}", decision.why_human, serde_json::to_string(basis).map_err(|e|e.to_string())?),
            "options":["Resume bounded work","Cancel bounded work"]}});
    if let Some(project) = &endpoint.project {
        input["project"] = json!(project);
    }
    Ok((input, basis.attempt_ref.clone()))
}
fn validate_question(
    response: &Value,
    input: &Value,
    reference: &str,
    attempt: &FactoryAttemptRecord,
) -> Result<(), String> {
    let (_, record) = checked_reading(response, reference)?;
    let expected_digest = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_string(input)
                .map_err(|e| e.to_string())?
                .as_bytes()
        )
    );
    if record["kind"] != "request"
        || record["request_digest"] != expected_digest
        || record["request"] != input["request"]
        || record["reply_to"] != input["reply_to"]
        || record["evidence_refs"] != input["evidence_refs"]
        || record["run_ref"] != input["run_ref"]
        || record["task_ref"] != input["task_ref"]
        || record["session_ref"] != input["session_ref"]
        || record["now_ref"] != input["now_ref"]
        || record["author"]["principal_ref"] != attempt.disposition.participant.agent_ref
    {
        return Err("native question does not retain the exact affected request, source, unit, execution and producer basis".into());
    }
    Ok(())
}
fn reviewed_response(
    response: &Value,
    resolver: &str,
    controlled: bool,
) -> Result<UnitDecisionResponse, String> {
    let data = &response["data"];
    let record = &data["record"];
    let review = &record["review"];
    if record["status"] != "answered"
        || review["disposition"] != "answered"
        || review["reviewer_ref"] != resolver
    {
        return Err(
            "native question has no answer from this affected unit's authenticated resolver".into(),
        );
    }
    let authority = text(review, "authority_ref")?;
    let authority_revision = text(review, "authority_revision")?;
    let outcome = match review["answer"].as_str() {
        Some("Resume bounded work") => UnitDecisionOutcome::Resume,
        Some("Cancel bounded work") => UnitDecisionOutcome::Cancel,
        _ => return Err("native answer is not one of the exact declared bounded decisions".into()),
    };
    let reference = text(data, "return_ref")?;
    let revision = text(data, "revision")?;
    Ok(UnitDecisionResponse {
        response_ref: format!(
            "unit-decision-response:{}",
            digest(&(reference, revision, resolver, outcome))?
        ),
        resolver_ref: resolver.into(),
        channel_receipt_ref: reference.into(),
        source_revision: revision.into(),
        outcome,
        evidence_refs: BTreeSet::from([
            reference.into(),
            format!("{authority}@{authority_revision}"),
        ]),
        controlled,
    })
}

/// This value has private fields, no Deserialize, and only a live trusted owner
/// call can construct it. Persisted public observations never become admission.
pub(crate) struct NativeReceivingAdmission {
    action_digest: String,
    pub(crate) attempt_ref: String,
    pub(crate) observation: OwnerOperationReceipt,
}
impl NativeReceivingAdmission {
    pub(crate) fn validate(&self, request: &FactoryAttemptActionRequest) -> Result<(), String> {
        if self.action_digest != digest(request)? {
            return Err(
                "native receiving admission belongs to another exact Factory Action".into(),
            );
        }
        Ok(())
    }
}

fn closure_submission(
    view: &StoredAttemptState,
    closure: &crate::run_lifecycle::RunClosureBasis,
) -> Result<(Value, FactoryAttemptRecord), String> {
    let attempt = view
        .attempts
        .get(&closure.final_attempt_ref)
        .ok_or("final native attempt is unavailable")?;
    let returned = attempt
        .readable_return
        .as_ref()
        .ok_or("final attempt has no native Return")?;
    if returned.receiving_ref.as_deref() != Some(closure.receiving_ref.as_str()) {
        return Err("closure names another Return's receiving identity".into());
    }
    // Public observations retain proposal intent, but only a live owner read
    // may admit it. Recover the immutable Return before receiving added refs.
    let input = attempt
        .observations
        .iter()
        .rev()
        .find_map(|receipt| {
            (receipt.contract == "factory.attempt-receiving-call/v1")
                .then(|| receipt.payload.get("nativeRequest").cloned())
                .flatten()
        })
        .ok_or("final Return has no retained native receiving submission basis")?;
    let original = view
        .action_receipts
        .values()
        .find_map(|action| {
            let FactoryAttemptOperation::ReturnArtifact {
                attempt_ref,
                readable_return,
                ..
            } = &action.request.operation
            else {
                return None;
            };
            (attempt_ref == &attempt.attempt_ref
                && readable_return.return_ref == returned.return_ref)
                .then(|| readable_return.clone())
        })
        .ok_or("closure has no immutable original native Return Action")?;
    if original.summary != returned.summary
        || original.artifact_refs != returned.artifact_refs
        || !original.evidence_refs.is_subset(&returned.evidence_refs)
    {
        return Err("final Return material changed after native receiving submission".into());
    }
    let mut original_attempt = attempt.clone();
    original_attempt.readable_return = Some(original);
    Ok((input, original_attempt))
}

/// A retained completion belongs to an admitted canonical Finished Action, not
/// a lifecycle enum, row count, public observation or archived display state.
pub(crate) fn completion_verified(view: &StoredAttemptState) -> bool {
    view.action_receipts.iter().any(|(key, action)| {
        let FactoryAttemptOperation::TransitionRun {
            command,
            authority,
            closure: Some(closure),
        } = &action.request.operation
        else {
            return false;
        };
        if command.lifecycle != RunLifecycle::Finished
            || action.receipt.operation != "transition-run"
            || action.request.run_ref != *view.run.reference()
            || action.receipt.run_ref != *view.run.reference()
            || digest(&action.request).ok().as_ref() != Some(key)
            || action.request_digest != *key
        {
            return false;
        }
        let mut replay = view.run.clone();
        if !matches!(
            replay.apply_lifecycle_command(authority, command.clone()),
            Ok(RunLifecycleOutcome::AlreadyApplied(_))
        ) {
            return false;
        }
        let Ok((input, original_attempt)) = closure_submission(view, closure) else {
            return false;
        };
        view.attempts[&closure.final_attempt_ref]
            .observations
            .iter()
            .any(|observation| {
                let response = &observation.payload["centralResponse"];
                let expected_ref = digest(&(key, response))
                    .map(|digest| format!("native-receiving-admission:{digest}"));
                observation.owner_ref == "central"
                    && observation.contract == "central.receiving-reading/v1"
                    && observation.phase == OwnerOperationPhase::Observed
                    && observation.payload["factoryActionDigest"].as_str() == Some(key.as_str())
                    && expected_ref.as_ref().ok() == Some(&observation.receipt_ref)
                    && checked_reading(response, &closure.receiving_ref).is_ok_and(|(data, _)| {
                        data["revision"].as_str() == Some(observation.source_revision.as_str())
                    })
                    && crate::attempt_receiving::validate_closure_read(
                        response,
                        &input,
                        view.run.reference(),
                        &original_attempt,
                    )
                    .is_ok()
            })
    })
}

pub(crate) fn prefetch(
    path: &Path,
    request: &FactoryAttemptActionRequest,
) -> Result<Option<NativeReceivingAdmission>, String> {
    let reference = match &request.operation {
        FactoryAttemptOperation::ResolveUnitDecision { response, .. } => {
            response.channel_receipt_ref.as_str()
        }
        FactoryAttemptOperation::TransitionRun {
            command,
            closure: Some(closure),
            ..
        } if command.lifecycle == RunLifecycle::Finished => closure.receiving_ref.as_str(),
        _ => return Ok(None),
    };
    let native = read_developmental_state(path).map_err(|e| e.to_string())?;
    let view = view_for(&native, &request.run_ref).map_err(|e| e.to_string())?;
    let request_digest = digest(request)?;
    if view.action_receipts.contains_key(&request_digest) {
        return Ok(None);
    }
    if request.expected_revision != view.revision {
        return Err("stale Factory provider revision before native receiving read".into());
    }
    if !source_is_current(&native, &view) {
        return Err("native receiving cannot admit a historical Factory source".into());
    }
    crate::attempt_application::validate_native_action(
        &crate::attempt_runtime::reading_for(&view).map_err(|e| e.to_string())?,
        &view.run,
        request,
    )
    .map_err(|e| e.to_string())?;
    // Refuse changed affected work and invalid closure before contacting the
    // foreign owner. The canonical transaction checks this basis again.
    let endpoint = configured_endpoint();
    let prepared = match &request.operation {
        FactoryAttemptOperation::ResolveUnitDecision {
            human_request_ref,
            response,
        } => {
            let decision = pending(&native, &view, human_request_ref)?;
            let basis = decision
                .unit_decision_basis
                .as_ref()
                .expect("pending basis");
            let workflow = crate::workflow::compile_workflow(view.workflow_source.clone())
                .map_err(|e| e.to_string())?;
            let engine = view
                .snapshot
                .restore(workflow, view.run.clone())
                .map_err(|e| e.to_string())?;
            crate::run_lifecycle::validate_decision_basis(&view, &engine, basis)
                .map_err(|e| e.to_string())?;
            crate::run_lifecycle::validate_decision_response(basis, response)
                .map_err(|e| e.to_string())?;
            let endpoint = endpoint.as_ref().map_err(|error| error.clone())?;
            let (input, attempt_ref) = question_input(&native, &view, human_request_ref, endpoint)?;
            (input, attempt_ref, None)
        }
        FactoryAttemptOperation::TransitionRun {
            command,
            authority,
            closure: Some(closure),
        } => {
            let workflow = crate::workflow::compile_workflow(view.workflow_source.clone())
                .map_err(|e| e.to_string())?;
            let engine = view
                .snapshot
                .restore(workflow, view.run.clone())
                .map_err(|e| e.to_string())?;
            let unresolved = native
                .build
                .human_requests_for_run(view.run.reference())
                .into_iter()
                .filter(|record| {
                    record.unit_decision_basis.is_some()
                        && record.unit_decision_response.is_none()
                        && record.unit_decision_retirement.is_none()
                })
                .map(|record| record.human_request_ref)
                .collect();
            crate::run_lifecycle::validate_closure(&view, &engine, closure, &unresolved)
                .map_err(|e| e.to_string())?;
            let mut next = view.run.clone();
            next.apply_lifecycle_command(authority, command.clone())
                .map_err(|e| e.to_string())?;
            let (input, original) = closure_submission(&view, closure)?;
            (input, closure.final_attempt_ref.clone(), Some(original))
        }
        _ => unreachable!(),
    };
    let endpoint = endpoint?;
    let response = read(&endpoint, reference)?;
    let (data, _) = checked_reading(&response, reference)?;
    let (input, attempt_ref, original) = prepared;
    match &request.operation {
        FactoryAttemptOperation::ResolveUnitDecision {
            human_request_ref,
            response: proposed,
        } => {
            let decision = pending(&native, &view, human_request_ref)?;
            let basis = decision
                .unit_decision_basis
                .as_ref()
                .expect("pending basis");
            validate_question(&response, &input, reference, &view.attempts[&attempt_ref])?;
            let actual = reviewed_response(&response, &basis.resolver_ref, basis.controlled)?;
            if proposed != &actual {
                return Err("proposed decision response differs from the exact native reviewed answer and channel revision".into());
            }
        }
        FactoryAttemptOperation::TransitionRun { .. } => {
            crate::attempt_receiving::validate_closure_read(
                &response,
                &input,
                &request.run_ref,
                original.as_ref().expect("prepared closure Return"),
            )?;
        }
        _ => unreachable!(),
    };
    let revision = text(data, "revision")?.to_owned();
    let observation = OwnerOperationReceipt {
        owner_ref: "central".into(),
        contract: "central.receiving-reading/v1".into(),
        operation_ref: format!("central.receiving:{reference}"),
        receipt_ref: format!(
            "native-receiving-admission:{}",
            digest(&(request_digest.clone(), &response))?
        ),
        source_revision: revision,
        phase: OwnerOperationPhase::Observed,
        evidence_refs: BTreeSet::from([reference.into()]),
        partial_effect_refs: BTreeSet::new(),
        payload: json!({"factoryActionDigest":request_digest,"centralResponse":response,"hostEndpoint":endpoint,"standing":"live native owner read; no caller JSON proof"}),
    };
    Ok(Some(NativeReceivingAdmission {
        action_digest: request_digest,
        attempt_ref,
        observation,
    }))
}

fn retain_question_observation(
    path: &Path,
    request: &FactoryAttemptActionRequest,
    attempt_ref: &str,
    receipt: OwnerOperationReceipt,
) -> Result<(), String> {
    let mut store =
        FileAttemptStore::open_run(path, request.run_ref.clone()).map_err(|e| e.to_string())?;
    for _ in 0..8 {
        let current = store.reading().map_err(|e| e.to_string())?;
        let attempt = current
            .attempts
            .iter()
            .find(|attempt| attempt.attempt_ref == attempt_ref)
            .ok_or("question's native attempt is no longer retained")?;
        if let Some(previous) = attempt
            .observations
            .iter()
            .find(|previous| previous.receipt_ref == receipt.receipt_ref)
        {
            return if previous == &receipt {
                Ok(())
            } else {
                Err("native question receipt identity conflicts".into())
            };
        }
        let mut retention = request.clone();
        retention.expected_revision = current.revision;
        retention.projection_ref = format!("{}:{}", request.projection_ref, receipt.receipt_ref);
        retention.operation = FactoryAttemptOperation::RecordObservation {
            attempt_ref: attempt_ref.into(),
            receipt: receipt.clone(),
        };
        match store.apply(retention) {
            Ok(_) => return Ok(()),
            Err(error) => {
                if store.reading().map_err(|e| e.to_string())?.revision == current.revision {
                    return Err(error.to_string());
                }
            }
        }
    }
    Err("question receipt retention remained contended; retry its same native producer key".into())
}

pub fn execute_cli(args: &[String], stdin: Option<&str>) -> Result<String, String> {
    let args = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .collect::<Vec<_>>();
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        return Ok("factory attempt decision submit <native-state> <request-json|-> --json\nfactory attempt decision read <native-state> <run-ref> <human-request-ref> <receiving-ref> --json\nNative receiving uses host-configured FACTORY_NATIVE_CENTRAL_BINARY/ROOT; the human reviews through Central. Submit does not answer a question. Read returns the exact response payload for the existing ResolveUnitDecision Action.".into());
    }
    let endpoint = configured_endpoint()?;
    let (path, run, request_ref, submission_request) = match args[0].as_str() {
        "read" if args.len() == 5 => (
            PathBuf::from(args[1]),
            args[2].parse::<RunRef>().map_err(|e| e.to_string())?,
            args[3].to_string(),
            None,
        ),
        "submit" if args.len() == 3 => {
            let body = if args[2].as_str() == "-" {
                if let Some(stdin) = stdin {
                    stdin.to_owned()
                } else {
                    use std::io::Read;
                    let mut body = String::new();
                    std::io::stdin()
                        .read_to_string(&mut body)
                        .map_err(|e| e.to_string())?;
                    body
                }
            } else {
                std::fs::read_to_string(args[2]).map_err(|e| e.to_string())?
            };
            let request: FactoryAttemptActionRequest =
                serde_json::from_str(&body).map_err(|e| e.to_string())?;
            let FactoryAttemptOperation::RequestUnitDecision { request: decision } =
                &request.operation
            else {
                return Err("submit requires the retained RequestUnitDecision Action".into());
            };
            (
                PathBuf::from(args[1]),
                request.run_ref.clone(),
                decision.human_request_ref.clone(),
                Some(request),
            )
        }
        _ => return Err("invalid native unit decision command".into()),
    };
    let native = read_developmental_state(&path).map_err(|e| e.to_string())?;
    let view = view_for(&native, &run).map_err(|e| e.to_string())?;
    if !source_is_current(&native, &view) {
        return Err("unit decision source is historical".into());
    }
    let (input, attempt_ref) = question_input(&native, &view, &request_ref, &endpoint)?;
    let response = if let Some(request) = &submission_request {
        crate::attempt_runtime::validate_action_request(request).map_err(|e| e.to_string())?;
        let actual = pending(&native, &view, &request_ref)?;
        let FactoryAttemptOperation::RequestUnitDecision { request: decision } = &request.operation
        else {
            unreachable!()
        };
        if actual.question != decision.question
            || actual.why_human != decision.why_human
            || actual.decision_ref != decision.decision_ref
            || actual.unit_decision_basis.as_ref() != Some(&decision.basis)
            || actual
                .evidence_refs
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                != decision.evidence_refs
        {
            return Err("submission differs from the immutable canonical unit decision".into());
        }
        if view.revision != request.expected_revision {
            return Err("stale Factory revision before native question submission".into());
        }
        crate::attempt_application::validate_native_action(
            &crate::attempt_runtime::reading_for(&view).map_err(|e| e.to_string())?,
            &view.run,
            request,
        )
        .map_err(|e| e.to_string())?;
        let operation_ref = format!("factory-unit-decision:{request_ref}");
        let identity = digest(&(&input, &endpoint))?;
        let previous = view.attempts[&attempt_ref]
            .observations
            .iter()
            .filter(|receipt| {
                receipt.contract == QUESTION_CALL && receipt.operation_ref == operation_ref
            })
            .collect::<Vec<_>>();
        if previous
            .iter()
            .any(|receipt| receipt.payload["requestDigest"] != identity)
        {
            return Err(
                "native question identity reused with a changed submission or host endpoint".into(),
            );
        }
        if let Some(response) = previous
            .iter()
            .rev()
            .filter(|receipt| receipt.phase == OwnerOperationPhase::Observed)
            .find_map(|receipt| receipt.payload.get("centralResponse").cloned())
        {
            // Submission replay reads retained delivery facts. It grants no
            // authority to answer; Resolve always reads the live owner again.
            response
        } else {
            let intent = previous.iter()
                .find(|receipt| receipt.phase == OwnerOperationPhase::Dispatching)
                .map(|receipt| (**receipt).clone())
                .unwrap_or_else(|| OwnerOperationReceipt {
                owner_ref: "factory".into(),
                contract: QUESTION_CALL.into(),
                operation_ref: operation_ref.clone(),
                receipt_ref: format!("factory-unit-decision-intent:{identity}"),
                source_revision: format!("factory-state:{}", view.revision),
                phase: OwnerOperationPhase::Dispatching,
                evidence_refs: BTreeSet::new(),
                partial_effect_refs: BTreeSet::new(),
                payload: json!({"requestDigest":identity,"nativeRequest":input,"hostEndpoint":endpoint}),
                });
            // Retain the immutable intent before the call. A lost response
            // resumes this same Central producer key after process replacement.
            if !previous
                .iter()
                .any(|receipt| receipt.receipt_ref == intent.receipt_ref)
            {
                retain_question_observation(&path, request, &attempt_ref, intent.clone())?;
            }
            match crate::attempt_receiving::call(&endpoint, "central.receiving.submit", &input) {
                Ok(response) => response,
                Err(failure) => {
                    let mut uncertain = intent;
                    uncertain.phase = OwnerOperationPhase::Uncertain;
                    uncertain.receipt_ref = format!(
                        "factory-unit-decision-uncertain:{}",
                        digest(&(&identity, &failure))?
                    );
                    uncertain.payload["failure"] = json!(failure);
                    let retained =
                        retain_question_observation(&path, request, &attempt_ref, uncertain).err();
                    return Err(format!("native question outcome is unresolved; retry the same producer key: {failure}; retention: {retained:?}"));
                }
            }
        }
    } else {
        read(&endpoint, args[4])?
    };
    let reference = text(&response["data"], "return_ref")?;
    validate_question(&response, &input, reference, &view.attempts[&attempt_ref])?;
    let decision = pending(&native, &view, &request_ref)?;
    let basis = decision
        .unit_decision_basis
        .as_ref()
        .expect("pending basis");
    let reviewed = reviewed_response(&response, &basis.resolver_ref, basis.controlled);
    if let Some(request) = submission_request {
        let identity = digest(&(&input, &endpoint))?;
        retain_question_observation(
            &path,
            &request,
            &attempt_ref,
            OwnerOperationReceipt {
                owner_ref: "factory".into(),
                contract: QUESTION_CALL.into(),
                operation_ref: format!("factory-unit-decision:{request_ref}"),
                receipt_ref: format!("factory-unit-decision-call:{}", digest(&response)?),
                source_revision: text(&response["data"], "revision")?.into(),
                phase: OwnerOperationPhase::Observed,
                evidence_refs: BTreeSet::from([reference.into()]),
                partial_effect_refs: BTreeSet::new(),
                payload: json!({"requestDigest":identity,"nativeRequest":input,"centralResponse":response,"hostEndpoint":endpoint}),
            },
        )?;
    }
    serde_json::to_string_pretty(&json!({"contract":DECISION_READING,"runRef":run,"humanRequestRef":request_ref,
        "nativeRequest":input,"centralResponse":response,"decisionResponse":reviewed.as_ref().ok(),"unresolved":reviewed.err()})).map_err(|e|e.to_string())
}
