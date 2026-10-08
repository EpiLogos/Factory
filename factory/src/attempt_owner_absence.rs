//! Exact reconciliation of a disclosure-refused addressed request. Absence is
//! not generic permission to retry: the retained request, body, Task and source
//! must still agree, and this exact request must fail the native disclosure
//! guard before any durable reservation or provider prompt can occur.
use super::{FactoryAttemptOwnerRequest, TRANSPORT_OBSERVATION};
use crate::attempt_runtime::{FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt};
use crate::native_owner::{NativeOwnerError, AIKIT_DELIVERY_CONTRACT};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) const ABSENCE_PROOF: &str = "factory.aikit-no-delivery-reconciliation/v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AbsenceRequest {
    action: String,
    agent_session: String,
    delivery_ref: String,
    pub original_request: Box<FactoryAttemptOwnerRequest>,
    expected_connection_generation: String,
    /// The original compose/status observation is caller-selected evidence;
    /// EXPLICIT_PIN itself binds the native UUID, not connection generation.
    generation_observation_ref: String,
    #[serde(skip)]
    retained_requirements_digest: Option<Value>,
}

pub(super) fn validate(
    packet: &Value,
    request: &FactoryAttemptOwnerRequest,
    sent: &OwnerOperationReceipt,
) -> Result<AbsenceRequest, String> {
    let mut recovery: AbsenceRequest =
        serde_json::from_value(packet.clone()).map_err(|e| e.to_string())?;
    let original = &recovery.original_request;
    let digest = blake3::hash(&serde_json::to_vec(original).map_err(|e| e.to_string())?)
        .to_hex()
        .to_string();
    if recovery.action != "reconcile-no-delivery"
        || recovery.expected_connection_generation.trim().is_empty()
        || recovery.generation_observation_ref.trim().is_empty()
        || sent.contract != TRANSPORT_OBSERVATION
        || sent.operation_ref != super::intent_ref(original)
        || sent.payload["requestDigest"] != digest
        || original.run_ref != request.run_ref
        || original.attempt_ref != request.attempt_ref
        || original.execution_ref != request.execution_ref
        || sent.payload["agentSession"] != recovery.agent_session
        || sent.payload["deliveryRef"] != recovery.delivery_ref
    {
        return Err("No-delivery reconciliation must retain the exact original owner request/digest, attempt, Execution and delivery".into());
    }
    let crate::native_owner::NativeOwnerInvocation::AikitEncounter {
        binary: _old_binary,
        cwd: old_cwd,
        transport: old_transport,
        contract_revision: old_contract,
        request: original_packet,
    } = &original.invocation
    else {
        return Err("The retained original operation is not an AIKit encounter".into());
    };
    let crate::native_owner::NativeOwnerInvocation::AikitEncounter {
        binary,
        cwd,
        transport,
        contract_revision,
        ..
    } = &request.invocation
    else {
        unreachable!()
    };
    if !binary.is_absolute()
        || old_cwd != cwd
        || old_transport != transport
        || old_contract != contract_revision
        || original_packet["action"] != "send"
        || original_packet["agent_session"] != recovery.agent_session
        || original_packet["turn"]["delivery_ref"] != recovery.delivery_ref
        || original_packet.pointer("/turn/expected_task")
            != sent.payload.pointer("/taskAdmission/expectedTask")
        || original_packet
            .pointer("/turn/expected_task")
            .is_none_or(|v| !v.is_object())
    {
        return Err(
            "No-delivery recovery cannot replace its original transport or locked Task admission"
                .into(),
        );
    }
    recovery.retained_requirements_digest = sent
        .payload
        .pointer("/taskAdmission/requirementsDigest")
        .cloned()
        .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()));
    if recovery.retained_requirements_digest.is_none() {
        return Err("Original protected Task requirements digest is not retained".into());
    }
    Ok(recovery)
}

fn invalid(message: &str) -> NativeOwnerError {
    NativeOwnerError::InvalidResponse(message.into())
}

fn strings(value: &Value) -> Result<BTreeSet<&str>, NativeOwnerError> {
    value
        .as_array()
        .ok_or_else(|| invalid("Native disclosure reading is not an array"))?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| invalid("Native disclosure contains an empty/non-string identity"))
        })
        .collect()
}

struct Reader<'a> {
    binary: &'a Path,
    cwd: &'a Path,
    transport: Option<&'a crate::native_aikit_route::AikitOwnerTransport>,
    deadline: Instant,
}
impl Reader<'_> {
    fn read(&self, args: &[String]) -> Result<Value, NativeOwnerError> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| invalid("No-delivery observation budget exhausted"))?;
        crate::native_owner::read_aikit_json(self.binary, self.cwd, self.transport, args, remaining)
    }
    fn encounter(&self, request: Value) -> Result<Value, NativeOwnerError> {
        let response = self.read(&[
            "session-space".into(),
            "encounter".into(),
            "--request-json".into(),
            serde_json::to_string(&request)?,
        ])?;
        if response["ok"] != true || response.get("data").is_none() {
            return Err(invalid("Missing complete native encounter observation"));
        }
        Ok(response)
    }
}

pub(super) fn observe(
    attempt: &FactoryAttemptRecord,
    recovery: &AbsenceRequest,
    observation: &crate::native_owner::NativeOwnerInvocation,
    timeout_ms: u64,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    let crate::native_owner::NativeOwnerInvocation::AikitEncounter {
        binary: original_binary,
        contract_revision,
        request: original_packet,
        ..
    } = &recovery.original_request.invocation
    else {
        unreachable!()
    };
    let crate::native_owner::NativeOwnerInvocation::AikitEncounter {
        binary,
        cwd,
        transport,
        ..
    } = observation
    else {
        unreachable!()
    };
    // Only the existing validated native explicit-selection body supplies the
    // UUID. A caller's new status label cannot substitute for that selection.
    let selection = &attempt.disposition.selection.selection;
    crate::execution_intelligence::explicit_selection_refs(selection)
        .map_err(|e| invalid(&format!("Invalid selected native body: {e:?}")))?
        .ok_or_else(|| {
            invalid("No-delivery reconciliation requires the original native explicit selection")
        })?;
    let basis = &selection.ranking_explanation["basis"];
    let native = &basis["native"];
    let body = &basis["composition_target_basis"]["resident_body_basis"];
    let participant = &attempt.disposition.participant;
    if native["agent_session"] != recovery.agent_session
        || native["native_session_id"]
            .as_str()
            .is_none_or(str::is_empty)
        || body["task_bound"] != true
    {
        return Err(invalid(
            "The original explicit selection is not this protected native Task/body",
        ));
    }
    let reader = Reader {
        binary,
        cwd,
        transport: transport.as_ref(),
        deadline: Instant::now() + Duration::from_millis(timeout_ms),
    };
    let status_request = json!({"action":"status","agent_session":recovery.agent_session});
    let status = reader.encounter(status_request.clone())?;
    let check_status = |reading: &Value| -> Result<(), NativeOwnerError> {
        let current = &reading["data"];
        if current["agent_session"] != recovery.agent_session
            || current["native_session_id"] != native["native_session_id"]
            || current["connection_generation"] != recovery.expected_connection_generation
            || current["state"] != "Resident"
            || !current["error"].is_null()
            || current["provider"]["id"] != body["owner_launcher_provider_id"]
        {
            return Err(invalid("The original native body/generation is unavailable, changed or active; absence remains uncertain"));
        }
        Ok(())
    };
    check_status(&status)?;
    let binding_args = [
        "session-space".into(),
        "encounter-agency-read".into(),
        "--agent-session".into(),
        recovery.agent_session.clone(),
    ];
    let binding = reader.read(&binding_args)?;
    let current = &binding["binding"];
    let turn = &original_packet["turn"];
    if binding["schema"] != "aikit.encounter-agency-source/v1"
        || binding["source_verified"] != true
        || binding["agent_session"] != recovery.agent_session
        || current["active"] != true
        || current["revision"] != turn["expected_binding_revision"]
        || current["agent_ref"] != participant.agent_ref
        || current["agency_ref"] != participant.agency_ref
        || current["world_binding_ref"] != participant.world_binding_ref
        || current["agency_source"]["source_ref"] != participant.source_ref
        || current["agency_source"]["revision"] != participant.source_revision
        || current["agency_source"]["content_digest"] != participant.source_digest
    {
        return Err(invalid(
            "Current native Agency/source differs from the retained addressed attempt",
        ));
    }
    // This unchanged binding can never admit this exact request, even if a
    // delayed IPC handler still exists. Do not settle an otherwise admissible
    // request solely because its delivery row has not appeared yet.
    let sources = strings(&turn["packet"]["source_refs"])?;
    let allowed = strings(&current["allowed_packet_sources"])?;
    let outside = sources.difference(&allowed).copied().collect::<Vec<_>>();
    if outside.is_empty()
        || !strings(&current["allowed_senders"])?
            .contains(turn["sender"].as_str().unwrap_or_default())
        || strings(&turn["packet"]["audience"])? != BTreeSet::from([participant.agent_ref.as_str()])
    {
        return Err(invalid("No exact pre-reservation source-disclosure refusal; missing delivery alone is insufficient"));
    }
    let task_args = [
        "session-space".into(),
        "encounter-task-read".into(),
        "--agent-session".into(),
        recovery.agent_session.clone(),
    ];
    let task = reader.read(&task_args)?;
    let expected = &turn["expected_task"];
    let allocation = &task["allocation"]["allocation"];
    if task["schema"] != "aikit.encounter-task/v1"
        || task["ready"] != true
        || task["revision"] != expected["revision"]
        || task["agency_revision"] != current["revision"]
        || task["inspection"]["requirements_digest"]
            != *recovery
                .retained_requirements_digest
                .as_ref()
                .expect("validated digest")
        || task["inspection"]["requirements"] != task["requirements"]
        || task["request"]["central"]["task_ref"] != attempt.task_ref
        || task["request"]["cwd"] != expected["cwd"]
        || allocation["now_ref"] != expected["now_ref"]
        || allocation["revision"]["revision"] != expected["now_revision"]
        || allocation["policy"]["revision"] != expected["policy_revision"]
        || expected["agent_ref"] != current["agent_ref"]
        || expected["agency_ref"] != current["agency_ref"]
        || expected["world_binding_ref"] != current["world_binding_ref"]
        || expected["source_ref"] != current["agency_source"]["source_ref"]
        || expected["source_revision"] != current["agency_source"]["revision"]
        || expected["source_digest"] != current["agency_source"]["content_digest"]
    {
        return Err(invalid(
            "Current Task/source/NOW differs from the original locked admission",
        ));
    }
    let delivery_request = json!({"action":"delivery","agent_session":recovery.agent_session,
        "delivery_ref":recovery.delivery_ref});
    let absent = reader.encounter(delivery_request.clone())?;
    if !absent["data"].is_null() {
        return Err(invalid(
            "Original delivery is retained; reconcile that actual record instead",
        ));
    }
    // All source and presence reads are repeated within the same finite wall
    // budget. A partial/moving observation cannot release Factory's fence.
    if reader.encounter(delivery_request)? != absent
        || reader.read(&binding_args)? != binding
        || reader.read(&task_args)? != task
    {
        return Err(invalid(
            "Original delivery or native source moved during absence observation",
        ));
    }
    check_status(&reader.encounter(status_request)?)?;
    let digest = blake3::hash(&serde_json::to_vec(&recovery.original_request)?)
        .to_hex()
        .to_string();
    let payload = json!({"schema":ABSENCE_PROOF,"agentSession":recovery.agent_session,
        "deliveryRef":recovery.delivery_ref,"originalRequestRef":recovery.original_request.request_ref,
        "originalRequestDigest":digest,"expectedTask":expected,
        "originalOwnerBinary":original_binary,"readOnlyObservationBinary":binary,
        "nativeIdentity":{"nativeSessionId":native["native_session_id"],
            "connectionGeneration":recovery.expected_connection_generation,
            "generationObservationRef":recovery.generation_observation_ref,
            "generationBasis":"caller-pinned original native observation; UUID is from retained explicit selection"},
        "agencyReading":binding,"taskReading":task,"deliveryReading":absent,
        "refusal":{"code":"encounter.disclosure_denied","derivedFromUnchangedNativeBinding":true,
            "outsideSourceCount":outside.len(),"outsideSourceRefs":outside.iter().take(16).collect::<Vec<_>>()},
        "standing":"original request cannot reach durable reservation or provider prompt; no replay, execution, Return or readiness inferred"});
    let proof_digest = blake3::hash(&serde_json::to_vec(&payload)?)
        .to_hex()
        .to_string();
    let receipt_ref = format!("aikit-no-delivery-observation:{proof_digest}");
    Ok(OwnerOperationReceipt {
        owner_ref: "aikit/session-space".into(),
        contract: AIKIT_DELIVERY_CONTRACT.into(),
        operation_ref: format!(
            "aikit-encounter-delivery:{}:{}",
            recovery.agent_session, recovery.delivery_ref
        ),
        receipt_ref: receipt_ref.clone(),
        source_revision: contract_revision.clone(),
        phase: OwnerOperationPhase::ReconciledNoReplay,
        evidence_refs: BTreeSet::from([receipt_ref, recovery.generation_observation_ref.clone()]),
        partial_effect_refs: BTreeSet::new(),
        payload,
    })
}
