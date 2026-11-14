//! Public attempt Action admission. All mutations still use the one existing
//! FileAttemptStore and ExecutableOrchestration; this module has no state file,
//! scheduler, execution loop or substitute worker. The store's locked revision
//! check makes these current-reading preconditions race-safe: a concurrent
//! mutation after this read is refused, never applied against an older basis.

use crate::attempt_runtime::*;
use crate::core::run::{Run, WorkflowUnitRef};
use crate::execution_intelligence::{
    accept_aikit_selection, explicit_selection_refs, AikitModelRosterSelection, ExecutionDemand,
    ExecutionInteropError, ExplicitSelectionRefs,
};
use crate::orchestration::{LegStatus, RetryGrant};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Fresh public read on each call; retaining a provider object is not retaining
/// a lease on current Run, source, writer or retry state.
pub fn read_attempts(path: &Path) -> Result<FactoryAttemptReading, FactoryAttemptError> {
    let reading = FileAttemptStore::open(path)?.reading()?;
    validate_reading(&reading)?;
    Ok(reading)
}

pub fn apply_attempt_action(
    path: &Path,
    request: FactoryAttemptActionRequest,
) -> Result<FactoryAttemptActionReceipt, FactoryAttemptError> {
    FileAttemptStore::open_run(path, request.run_ref.clone())?.apply(request)
}

pub(crate) fn validate_native_action(
    reading: &FactoryAttemptReading,
    run: &Run,
    request: &FactoryAttemptActionRequest,
) -> Result<(), FactoryAttemptError> {
    validate_reading(reading)?;
    crate::attempt_delivery::validate_publisher(reading, request)?;
    if reading.revision != request.expected_revision {
        return Err(FactoryAttemptError::RevisionConflict {
            expected: request.expected_revision,
            actual: reading.revision,
        });
    }
    validate_operation(reading, run, &request.operation)
}

pub(crate) fn validate_reading(reading: &FactoryAttemptReading) -> Result<(), FactoryAttemptError> {
    let mut identities = BTreeSet::new();
    for record in &reading.attempts {
        let execution = execution_identity(record);
        if !identities.insert(execution) {
            return Err(invalid("two attempt records identify the same execution"));
        }
        let leg = reading
            .legs
            .get(&record.workflow_unit_ref)
            .ok_or_else(|| invalid("attempt record is not linked to a coordinator leg"))?;
        if !leg
            .attempts
            .iter()
            .any(|attempt| attempt.execution_ref == execution)
        {
            return Err(invalid(
                "attempt record is not linked to its execution history",
            ));
        }
    }
    for leg in reading.legs.values() {
        for attempt in &leg.attempts {
            if !identities.contains(attempt.execution_ref.as_str()) {
                return Err(invalid(
                    "coordinator history has an untracked native attempt",
                ));
            }
        }
    }
    Ok(())
}

fn validate_operation(
    reading: &FactoryAttemptReading,
    run: &Run,
    operation: &FactoryAttemptOperation,
) -> Result<(), FactoryAttemptError> {
    use FactoryAttemptOperation::*;
    match operation {
        StartSerial {
            disposition,
            retry_grant,
            tracking,
            parent_journey_ref,
            ..
        } => {
            validate_start(run, disposition, retry_grant.as_ref())?;
            nonempty(parent_journey_ref, "parent Journey")?;
            for fact in tracking {
                validate_fact(fact)?;
            }
        }
        StartFork {
            attempts,
            parent_journey_ref,
        } => {
            nonempty(parent_journey_ref, "parent Journey")?;
            let mut refs = BTreeSet::new();
            for start in attempts {
                if !refs.insert(&start.attempt_ref) {
                    return Err(invalid("fork repeats an attempt identity"));
                }
                validate_start(run, &start.disposition, start.retry_grant.as_ref())?;
                for fact in &start.tracking {
                    validate_fact(fact)?;
                }
            }
        }
        RequestCancellation { attempt_ref }
        | AcceptCancellation { attempt_ref }
        | RecordProcessTermination { attempt_ref }
        | MarkQuiescent { attempt_ref }
        | IncorporateLateResult { attempt_ref } => {
            let record = current_attempt(reading, attempt_ref)?;
            if matches!(operation, MarkQuiescent { .. }) && has_uncertain_operation(record) {
                return Err(invalid("unknown owner effects must be reconciled before quiescence releases the writer"));
            }
        }
        Fail {
            attempt_ref,
            evidence_refs,
            ..
        } => {
            let record = current_attempt(reading, attempt_ref)?;
            if has_uncertain_operation(record) {
                return Err(invalid(
                    "uncertain owner effects require reconciliation, not failure/writer release",
                ));
            }
            if !evidence_refs.is_superset(&partial_effects(record)) {
                return Err(invalid(
                    "failure evidence must retain every known partial effect",
                ));
            }
        }
        Retry {
            workflow_unit_ref,
            task_ref,
            parent_journey_ref,
            grant_ref,
            disposition,
            tracking,
            reresolution,
            ..
        } => {
            let prior = current_for_unit(reading, workflow_unit_ref)?;
            let leg = &reading.legs[workflow_unit_ref];
            if prior.task_ref != *task_ref
                || leg.delegation.parent_journey_ref != *parent_journey_ref
            {
                return Err(invalid("retry must retain its task and parent Journey"));
            }
            if prior.disposition.budget.retry_grant_ref.as_deref() != Some(grant_ref.as_str()) {
                return Err(invalid(
                    "retry cannot switch the previously consumed retry grant",
                ));
            }
            validate_start(run, disposition, None)?;
            if has_uncertain_operation(prior) {
                return Err(invalid("retry is blocked by an unreconciled owner outcome"));
            }
            if prior.disposition.participant.agent_ref != disposition.participant.agent_ref {
                return Err(invalid("retry cannot silently replace the enduring Agent"));
            }
            if prior.disposition.body.model_ref != disposition.body.model_ref
                || prior.disposition.body.provider_ref != disposition.body.provider_ref
            {
                return Err(invalid("explicit model/provider replacement requires its owner-authorised operation; retry is not that authority"));
            }
            if prior.disposition.budget.maximum_attempts != disposition.budget.maximum_attempts
                || prior.disposition.budget.retry_grant_ref != disposition.budget.retry_grant_ref
            {
                return Err(invalid("retry cannot replenish its total attempt budget"));
            }
            if let Some(limit) = prior.disposition.budget.wall_clock_timeout_ms {
                if disposition
                    .budget
                    .wall_clock_timeout_ms
                    .is_none_or(|next| next > limit)
                {
                    return Err(invalid("retry cannot remove or increase its time bound"));
                }
            }
            if let Some(limit) = prior.disposition.budget.cost_ceiling_usd {
                if disposition
                    .budget
                    .cost_ceiling_usd
                    .is_none_or(|next| next > limit)
                {
                    return Err(invalid("retry cannot remove or increase its cost ceiling"));
                }
            }
            match (&prior.disposition.placement, &disposition.placement) {
                (Some(before), Some(after)) => {
                    if !after.protected_paths.is_superset(&before.protected_paths)
                        || !after
                            .required_coverage
                            .is_superset(&before.required_coverage)
                    {
                        return Err(invalid(
                            "retry cannot drop protected sources or required coverage",
                        ));
                    }
                    if before.now_ref != after.now_ref
                        && reresolution
                            .as_ref()
                            .and_then(|value| value.replacement_now_ref.as_deref())
                            != Some(after.now_ref.as_str())
                    {
                        return Err(invalid(
                            "changed NOW requires explicit matching re-resolution",
                        ));
                    }
                }
                (Some(_), None) => return Err(invalid("retry cannot remove placement protection")),
                _ => {}
            }
            if prior.disposition.body.material_world_ref != disposition.body.material_world_ref
                && reresolution
                    .as_ref()
                    .and_then(|value| value.replacement_material_ref.as_deref())
                    != disposition.body.material_world_ref.as_deref()
            {
                return Err(invalid(
                    "changed material requires explicit matching re-resolution",
                ));
            }
            if prior.disposition.body.harness_ref != disposition.body.harness_ref
                && reresolution
                    .as_ref()
                    .and_then(|value| value.replacement_harness_ref.as_deref())
                    != Some(disposition.body.harness_ref.as_str())
            {
                return Err(invalid(
                    "changed harness requires explicit matching re-resolution",
                ));
            }
            if !partial_effects(prior).is_empty() || prior.disposition != *disposition {
                let resolution = reresolution.as_ref().ok_or_else(|| {
                    invalid("partial effects or changed conditions require explicit re-resolution")
                })?;
                if resolution.source_revision != disposition.participant.source_revision
                    || !resolution
                        .evidence_refs
                        .is_superset(&partial_effects(prior))
                {
                    return Err(invalid("re-resolution must retain current source basis and partial-effect evidence"));
                }
            }
            for fact in tracking {
                validate_fact(fact)?;
            }
        }
        BindDispatch {
            attempt_ref,
            execution_ref,
            receipt,
        } => {
            let record = current_attempt(reading, attempt_ref)?;
            if execution_ref == &record.disposition.body.agent_session_ref
                || execution_ref == &record.disposition.body.session_space_ref
                || execution_ref == &receipt.operation_ref
            {
                return Err(invalid(
                    "session, delivery and execution identities are not interchangeable",
                ));
            }
        }
        ReturnArtifact { attempt_ref, .. } => {
            let record = attempt(reading, attempt_ref)?;
            if has_uncertain_operation(record) {
                return Err(invalid(
                    "uncertain outcome cannot become a completed Return",
                ));
            }
            if !has_passing_verification(record) {
                return Err(invalid(
                    "a later failed/unknown verification supersedes an earlier pass",
                ));
            }
            let dispatch = record
                .dispatch
                .as_ref()
                .ok_or_else(|| invalid("Return requires an actual owner dispatch"))?;
            let last = record
                .observations
                .iter()
                .rev()
                .find(|receipt| {
                    receipt.owner_ref == dispatch.owner_ref
                        && receipt.operation_ref == dispatch.operation_ref
                })
                .unwrap_or(dispatch);
            if last.phase != OwnerOperationPhase::Returned {
                return Err(invalid(
                    "submitted, failed or cancelled delivery is not completed provider work",
                ));
            }
        }
        RegisterIndependentReview { attempt_ref, .. } => {
            current_attempt(reading, attempt_ref)?;
        }
        Synthesize {
            attempt_ref,
            reviewer_attempt_ref,
            ..
        } => {
            current_attempt(reading, attempt_ref)?;
            current_attempt(reading, reviewer_attempt_ref)?;
        }
        RecordTracking { fact, .. } => validate_fact(fact)?,
        TransitionRun { .. }
        | RequestUnitDecision { .. }
        | ResolveUnitDecision { .. }
        | RetireUnitDecision { .. } => {}
        RecordObservation { .. }
        | RecordVerification { .. }
        | RecordReresolution { .. }
        | AdvanceSubject { .. }
        | AttachReceiving { .. }
        | AttachArchive { .. } => {}
    }
    Ok(())
}

fn validate_start(
    run: &Run,
    disposition: &SituatedExecutionDisposition,
    grant: Option<&RetryGrant>,
) -> Result<(), FactoryAttemptError> {
    if disposition.selection.demand.project_ref != run.project_ref().to_string() {
        return Err(invalid(
            "Execution Intelligence demand names another Project",
        ));
    }
    if disposition
        .budget
        .cost_ceiling_usd
        .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
        || disposition.budget.wall_clock_timeout_ms == Some(0)
    {
        return Err(invalid("invalid finite execution budget"));
    }
    if let Some(grant) = grant {
        if grant.grant_ref.trim().is_empty() || grant.attempts_allowed == 0 {
            return Err(invalid(
                "retry grant must have an identity and a positive total bound",
            ));
        }
    }
    Ok(())
}

fn execution_identity(record: &FactoryAttemptRecord) -> &str {
    record
        .execution_ref
        .as_deref()
        .unwrap_or(&record.reserved_execution_ref)
}

fn attempt<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == reference)
        .ok_or_else(|| FactoryAttemptError::UnknownAttempt(reference.into()))
}

fn current_attempt<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    let record = attempt(reading, reference)?;
    let current = reading
        .legs
        .get(&record.workflow_unit_ref)
        .ok_or_else(|| invalid("attempt has no native leg"))?;
    if current.execution_ref != execution_identity(record) {
        return Err(invalid(
            "historical attempt cannot mutate its replacement; history remains readable",
        ));
    }
    Ok(record)
}

fn current_for_unit<'a>(
    reading: &'a FactoryAttemptReading,
    unit: &WorkflowUnitRef,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    let leg = reading
        .legs
        .get(unit)
        .ok_or_else(|| invalid("no prior leg for retry"))?;
    reading
        .attempts
        .iter()
        .find(|record| {
            &record.workflow_unit_ref == unit && execution_identity(record) == leg.execution_ref
        })
        .ok_or_else(|| invalid("current coordinator execution has no attempt record"))
}

fn partial_effects(record: &FactoryAttemptRecord) -> BTreeSet<String> {
    record
        .dispatch
        .iter()
        .chain(record.observations.iter())
        .flat_map(|receipt| receipt.partial_effect_refs.iter().cloned())
        .collect()
}

fn has_uncertain_operation(record: &FactoryAttemptRecord) -> bool {
    let mut latest = BTreeMap::new();
    for receipt in record.dispatch.iter().chain(record.observations.iter()) {
        latest.insert((&receipt.owner_ref, &receipt.operation_ref), receipt.phase);
    }
    latest.values().any(|phase| {
        matches!(
            phase,
            OwnerOperationPhase::Uncertain | OwnerOperationPhase::Dispatching
        )
    })
}

fn validate_fact(fact: &AttemptTrackingFact) -> Result<(), FactoryAttemptError> {
    let owner = match fact.kind.as_str() {
        "now" | "day" | "flow" | "source-revision" | "source-change" | "receiving" => {
            Some("central")
        }
        "model-usage" | "activity" => Some("actuation"),
        "resource-usage" | "material" | "coverage" => Some("workcell"),
        "harness" | "session" | "context" | "praxis" => Some("aikit"),
        _ => None,
    };
    if owner.is_some_and(|expected| fact.owner_ref != expected) {
        return Err(invalid("tracking fact misattributes its native owner"));
    }
    Ok(())
}

fn nonempty(value: &str, field: &str) -> Result<(), FactoryAttemptError> {
    if value.trim().is_empty() {
        Err(invalid(&format!("{field} must be explicit")))
    } else {
        Ok(())
    }
}

fn invalid(message: &str) -> FactoryAttemptError {
    FactoryAttemptError::InvalidOperation(message.into())
}

// ---------------------------------------------------------------------------
// Launch (O-I #842, DLW build ruling 2026-11-05): start a unit from its
// owners' readings. The Run, the unit's law and the Journey come from
// Factory's own state; the model, the participant and the body come from
// AIKit's explicit selection receipt on the session that carries the unit;
// the praxis a unit names comes from AIKit's NOW preparation, and the
// placement of a writing unit from Central's. Factory assembles them into the
// disposition `start-serial` takes and invents no value: a reading that is
// missing is refused naming its owner, and the ordinary admission
// (`validate_disposition`) judges the rest in Factory's words.

/// The Launch request (`factory attempt launch <state> <request-json|->`).
pub const FACTORY_ATTEMPT_LAUNCH: &str = "factory.attempt-launch/v1";
/// AIKit's realisation of a model on a resident session (`aikit compose
/// --realise`), which carries its explicit selection as `factory_selection`.
pub const AIKIT_MODEL_REALISATION: &str = "aikit.model-realisation/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAttemptLaunchRequest {
    pub contract: String,
    pub projection_ref: String,
    pub caller: crate::action_projection::FactoryActionCaller,
    pub run_ref: crate::core::run::RunRef,
    pub expected_revision: u64,
    pub authority: crate::action_projection::ProjectedFactoryActionAuthority,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub attempt_ref: String,
    pub task_ref: String,
    /// AIKit's `aikit.model-realisation/v2`, whole.
    pub aikit_realisation: Value,
    /// AIKit's NOW-preparation praxis receipt and the prepared NOW context it
    /// was resolved against, for a unit that names praxisRefs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub praxis_receipt: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_context_ref: Option<String>,
    /// Central's placement for a unit whose effects write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementProtection>,
}

fn interop_message(error: &ExecutionInteropError) -> String {
    match error {
        ExecutionInteropError::InvalidExplicitSelection(message) => message.clone(),
        ExecutionInteropError::WrongRosterVersion(version) => {
            format!("it names the roster {version}, not AIKit's model roster")
        }
        ExecutionInteropError::EmptyModelRef => "it names no model".into(),
        ExecutionInteropError::EmptyRunRef => "the demand names no Run".into(),
        ExecutionInteropError::InvalidWorkflowUnitRef(unit) => {
            format!("the demand names no workflow unit Factory knows ({unit})")
        }
    }
}

/// AIKit's explicit selection out of its realisation, verified against its
/// own basis digests, or the refusal naming what AIKit's reading lacks.
fn aikit_selection(
    realisation: &Value,
) -> Result<(AikitModelRosterSelection, ExplicitSelectionRefs), FactoryAttemptError> {
    let refuse = |message: String| FactoryAttemptError::InvalidDisposition(message);
    if realisation["schema"] != AIKIT_MODEL_REALISATION {
        return Err(refuse(format!(
            "AIKit's reading is not its model realisation ({AIKIT_MODEL_REALISATION}): a unit starts on the model AIKit opened on the session that carries it (aikit compose --realise)"
        )));
    }
    if realisation["selected"] != true {
        return Err(refuse("AIKit's realisation selected no model".into()));
    }
    let Some(selection) = realisation
        .get("factory_selection")
        .filter(|value| !value.is_null())
    else {
        let why = match realisation
            .get("resolution")
            .filter(|value| !value.is_null())
        {
            Some(resolution) => format!(
                "AIKit's roster chose {} and issued no explicit selection receipt",
                resolution["selected"]["model"]
                    .as_str()
                    .unwrap_or("the model")
            ),
            None => "AIKit's realisation carries no explicit selection receipt (factory_selection)"
                .into(),
        };
        return Err(refuse(format!(
            "{why}: a unit starts only on AIKit's explicit selection, whose basis names the route, harness and session that carry it (aikit compose --realise --model <ref>)"
        )));
    };
    let selection: AikitModelRosterSelection = serde_json::from_value(selection.clone())
        .map_err(|error| refuse(format!("AIKit's explicit selection is unreadable: {error}")))?;
    let refs = explicit_selection_refs(&selection)
        .map_err(|error| {
            refuse(format!(
                "AIKit's explicit selection does not verify: {}",
                interop_message(&error)
            ))
        })?
        .ok_or_else(|| {
            refuse(format!(
                "AIKit's selection is ranked ({}), not its explicit selection receipt: a unit starts only on the receipt whose basis names the route, harness and session",
                selection.ranking_policy
            ))
        })?;
    Ok((selection, refs))
}

/// The attempts a task may spend, its first included: the one budget rule a
/// launch starts every task under (`factory.attempt.launch`, and
/// `factory.task.run` on a session, which launches through it).
pub const LAUNCH_ATTEMPTS: u32 = 3;

/// The `start-serial` Action a Launch assembles, at the state revision the
/// caller read. Nothing in it is the caller's but the Run, the unit, the
/// attempt and task refs and the owners' readings it carries.
pub fn launch_action(
    path: &Path,
    request: FactoryAttemptLaunchRequest,
) -> Result<FactoryAttemptActionRequest, FactoryAttemptError> {
    if request.contract != FACTORY_ATTEMPT_LAUNCH {
        return Err(FactoryAttemptError::UnsupportedContract(request.contract));
    }
    nonempty(&request.attempt_ref, "attemptRef")?;
    nonempty(&request.task_ref, "taskRef")?;
    let store = FileAttemptStore::open_run(path, request.run_ref.clone())?;
    let basis = store.launch_basis(&request.workflow_unit_ref)?;
    let parent_journey_ref = basis.journey_ref.clone().ok_or_else(|| {
        invalid(&format!(
            "Factory links {} to no Journey: a unit starts under the Journey its Run was commissioned from",
            request.run_ref
        ))
    })?;
    let (selection, refs) = aikit_selection(&request.aikit_realisation)?;
    // The grant a retry of this task spends (factory.attempt.retry): its first attempt consumes it.
    let retry_grant = RetryGrant::new(format!("retry-grant:{}", request.task_ref), LAUNCH_ATTEMPTS)
        .map_err(|error| {
            invalid(&format!(
                "Factory refused the task's retry grant: {error:?}"
            ))
        })?;
    let unit = &basis.unit;
    let contribution = unit.contribution.as_ref();
    let from_contribution =
        |pick: fn(&crate::workflow::WorkflowContribution) -> &BTreeSet<String>| {
            contribution.map(pick).cloned().unwrap_or_default()
        };
    // Factory's own demand: why this unit needs execution, named by the unit.
    let demand = ExecutionDemand {
        project_ref: basis.run.project_ref().to_string(),
        run_ref: request.run_ref.to_string(),
        workflow_unit_ref: Some(unit.reference.to_string()),
        agency_ref: Some(refs.agency_ref.clone()),
        profile_ref: None,
        use_type: unit.key.clone(),
        required_capabilities: unit.capability_refs.clone(),
        required_modalities: from_contribution(|c| &c.required_modalities),
        required_actions: from_contribution(|c| &c.required_actions),
        required_tools: from_contribution(|c| &c.required_tools),
        context_characteristics: BTreeSet::new(),
        independence_from: unit
            .independence_from
            .iter()
            .map(ToString::to_string)
            .collect(),
        cost_ceiling_usd: None,
        latency_preference_ms: None,
        requires_local_materialisation: false,
    };
    let model_ref = selection.model_ref.clone();
    let provider_ref = selection.provider_ref.clone();
    let decided =
        accept_aikit_selection(demand, selection, factory_admission_time()).map_err(|error| {
            FactoryAttemptError::InvalidDisposition(format!(
                "AIKit's selection does not meet the unit's demand: {}",
                interop_message(&error)
            ))
        })?;
    let disposition = SituatedExecutionDisposition {
        selected_inputs: Vec::new(),
        selection: decided,
        participant: SituatedParticipant {
            agent_ref: refs.agent_ref,
            agency_ref: refs.agency_ref,
            world_binding_ref: refs.world_binding_ref,
            profile_ref: None,
            position_ref: None,
            source_ref: refs.source_ref,
            source_revision: refs.source_revision,
            source_digest: refs.source_digest,
        },
        context_refs: from_contribution(|c| &c.context_refs),
        praxis_refs: unit.praxis_refs.clone(),
        capability_refs: unit.capability_refs.clone(),
        praxis_receipt: request.praxis_receipt,
        now_context_ref: request.now_context_ref,
        body: ExecutionBody {
            model_ref,
            provider_ref,
            route_ref: refs.route_ref,
            harness_ref: refs.harness_ref,
            harness_composition_ref: refs.harness_composition_ref,
            agent_session_ref: refs.agent_session_ref,
            session_space_ref: refs.session_space_ref,
            material_world_ref: None,
            workcell_ref: None,
        },
        placement: request.placement,
        permitted_effects: unit.permitted_effects.clone(),
        verification_obligations: unit.verification_obligations.clone(),
        return_address: unit.return_address.clone(),
        stop_conditions: unit.stop_conditions.clone(),
        escalation_conditions: unit.escalation_conditions.clone(),
        budget: ExecutionBudget {
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            wall_clock_timeout_ms: None,
            retry_grant_ref: Some(retry_grant.grant_ref.clone()),
            maximum_attempts: Some(retry_grant.attempts_allowed),
        },
    };
    Ok(FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: request.projection_ref,
        caller: request.caller,
        run_ref: request.run_ref,
        expected_revision: request.expected_revision,
        authority: request.authority,
        operation: FactoryAttemptOperation::StartSerial {
            attempt_ref: request.attempt_ref,
            task_ref: request.task_ref,
            parent_journey_ref,
            workflow_unit_ref: unit.reference.clone(),
            disposition,
            retry_grant: Some(retry_grant),
            tracking: Vec::new(),
            place_grant: None,
        },
    })
}

/// Launch one unit: assemble its `start-serial` and apply it through the one
/// attempt store, as `attempt action` does.
pub fn launch_attempt(
    path: &Path,
    request: FactoryAttemptLaunchRequest,
) -> Result<FactoryAttemptActionReceipt, FactoryAttemptError> {
    let action = launch_action(path, request)?;
    apply_attempt_action(path, action)
}

/// Same public command grammar and native receipt type. Only admission and
/// current readback are added; all semantic mutation remains in the coordinator.
pub fn execute_attempt_cli(
    args: &[String],
    stdin_override: Option<&str>,
) -> Result<String, FactoryAttemptError> {
    let positional = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .collect::<Vec<_>>();
    // `factory attempt scope <state> [run-ref] [--attempt <ref>] [--json]`: what each attempt may write, read from its record.
    if positional.first().map(|value| value.as_str()) == Some("scope") {
        let rest: Vec<&str> = positional[1..].iter().map(|value| value.as_str()).collect();
        let mut path = None;
        let mut run_ref = None;
        let mut attempt = None;
        let mut index = 0;
        while index < rest.len() {
            match rest[index] {
                "--attempt" => {
                    attempt = Some(
                        *rest
                            .get(index + 1)
                            .ok_or_else(|| invalid("--attempt names an attempt ref"))?,
                    );
                    index += 2;
                    continue;
                }
                value if path.is_none() => path = Some(value),
                value if run_ref.is_none() => run_ref = Some(value),
                value => return Err(invalid(&format!("unexpected argument `{value}`"))),
            }
            index += 1;
        }
        let path = path.ok_or_else(|| invalid("missing state path"))?;
        let reading = match run_ref {
            Some(run_ref) => FileAttemptStore::open_run(
                Path::new(path),
                run_ref
                    .parse::<crate::core::run::RunRef>()
                    .map_err(|error| invalid(&error.to_string()))?,
            )?
            .reading()?,
            None => read_attempts(Path::new(path))?,
        };
        let scope = crate::attempt_scope::scope_reading(&reading, attempt)?;
        return if args.iter().any(|arg| arg == "--json") {
            Ok(serde_json::to_string_pretty(&scope)?)
        } else {
            let mut output = format!(
                "{}\nRun: {}",
                crate::attempt_scope::FACTORY_ATTEMPT_SCOPE_READING,
                reading.run_ref
            );
            for row in scope["scopes"].as_array().into_iter().flatten() {
                let paths = row["writes"]["writablePaths"]
                    .as_array()
                    .map(|paths| {
                        paths
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "nowhere".into());
                output.push_str(&format!(
                    "\nAttempt {}: writes {}",
                    row["attemptRef"].as_str().unwrap_or("?"),
                    if paths.is_empty() {
                        "nowhere".into()
                    } else {
                        paths
                    }
                ));
            }
            Ok(output)
        };
    }
    let launch = positional.first().map(|value| value.as_str()) == Some("launch");
    if positional.first().map(|value| value.as_str()) != Some("action") && !launch {
        if positional.first().map(|value| value.as_str()) == Some("read") {
            let path = positional
                .get(1)
                .ok_or_else(|| invalid("missing state path"))?;
            let reading = if let Some(run_ref) = positional.get(2) {
                FileAttemptStore::open_run(
                    Path::new(path),
                    run_ref
                        .parse::<crate::core::run::RunRef>()
                        .map_err(|error| invalid(&error.to_string()))?,
                )?
                .reading()?
            } else {
                read_attempts(Path::new(path))?
            };
            if args.iter().any(|arg| arg == "--json") {
                return Ok(serde_json::to_string_pretty(&reading)?);
            }
            let mut output = format!(
                "{}\nRun: {} @ {}\nState revision: {}",
                reading.contract, reading.run_ref, reading.run_revision, reading.revision
            );
            for record in &reading.attempts {
                let leg = &reading.legs[&record.workflow_unit_ref];
                let historic = leg.execution_ref != execution_identity(record);
                let status = if historic {
                    "historical".to_owned()
                } else {
                    format!("{:?}", leg.status)
                };
                output.push_str(&format!(
                    "\n\nTask: {}\nAttempt: {} ({})\nAgent: {}\nSource: {} @ {}\nExecution: {}",
                    record.task_ref,
                    record.attempt_ref,
                    status,
                    record.disposition.participant.agent_ref,
                    record.disposition.participant.source_ref,
                    record.disposition.participant.source_revision,
                    record.execution_ref.as_deref().unwrap_or("not dispatched")
                ));
                if let Some(result) = &record.readable_return {
                    output.push_str(&format!(
                        "\nReturn {}: {}",
                        result.return_ref, result.summary
                    ));
                }
                if has_uncertain_operation(record) {
                    output.push_str("\nOwner outcome uncertain; reconciliation required.");
                }
                if !historic && leg.status != LegStatus::Returned {
                    output.push_str("\nNot a completed Factory Return.");
                }
            }
            return Ok(output);
        }
        return crate::attempt_runtime::execute_attempt_cli(args, stdin_override);
    }
    let path = positional
        .get(1)
        .ok_or_else(|| invalid("missing state path"))?;
    let input_path = positional.get(2).map(|value| value.as_str()).unwrap_or("-");
    let input = if input_path != "-" {
        fs::read_to_string(input_path)?
    } else if let Some(input) = stdin_override {
        input.to_owned()
    } else {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        input
    };
    let receipt = if launch {
        launch_attempt(Path::new(path), serde_json::from_str(&input)?)?
    } else {
        apply_attempt_action(Path::new(path), serde_json::from_str(&input)?)?
    };
    if args.iter().any(|arg| arg == "--json") {
        return Ok(serde_json::to_string_pretty(&receipt)?);
    }
    Ok(format!(
        "{}\nRun: {}\nOperation: {}\nRevision: {} -> {}\n{}",
        receipt.contract,
        receipt.run_ref,
        receipt.operation,
        receipt.previous_revision,
        receipt.next_revision,
        receipt.standing
    ))
}
