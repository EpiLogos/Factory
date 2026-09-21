//! Public attempt joins to the existing independent-review and barrier synthesis
//! engine. Returned native work, exact selected material and distinct actual
//! identities are necessary; this does not infer creative quality or Recognition.
use crate::attempt_runtime::{
    FactoryAttemptError, FactoryAttemptRecord, OwnerOperationPhase, StoredAttemptState,
    VerificationOutcome,
};
use crate::core::run::WorkflowUnitRef;
use crate::orchestration::{ExecutableOrchestration, LegStatus};
use std::collections::BTreeSet;
fn invalid(message: impl Into<String>) -> FactoryAttemptError {
    FactoryAttemptError::InvalidOperation(message.into())
}
fn settled<'a>(
    state: &'a StoredAttemptState,
    engine: &ExecutableOrchestration,
    id: &str,
) -> Result<&'a FactoryAttemptRecord, FactoryAttemptError> {
    let record = state
        .attempts
        .get(id)
        .ok_or_else(|| invalid("review/synthesis attempt is absent"))?;
    let execution = record
        .execution_ref
        .as_ref()
        .ok_or_else(|| invalid("review/synthesis requires a bound native execution"))?;
    let leg = engine
        .leg(&record.workflow_unit_ref)
        .ok_or_else(|| invalid("review/synthesis leg is absent"))?;
    if leg.status != LegStatus::Returned
        || &leg.execution_ref != execution
        || record.readable_return.is_none()
    {
        return Err(invalid("review/synthesis needs this exact current returned attempt, not a failed, pending or historical result"));
    }
    let verification = record
        .verifications
        .last()
        .ok_or_else(|| invalid("review/synthesis requires current verification"))?;
    if verification.outcome != VerificationOutcome::Passed
        || verification.evidence_refs.is_empty()
        || !record
            .disposition
            .verification_obligations
            .is_subset(&verification.obligations)
    {
        return Err(invalid(
            "failed, missing or superseded verification cannot establish review/synthesis",
        ));
    }
    let dispatch = record
        .dispatch
        .as_ref()
        .ok_or_else(|| invalid("review/synthesis has no native owner dispatch"))?;
    let observation = record
        .observations
        .iter()
        .rev()
        .find(|o| o.owner_ref == dispatch.owner_ref && o.operation_ref == dispatch.operation_ref)
        .unwrap_or(dispatch);
    if observation.phase != OwnerOperationPhase::Returned || observation.evidence_refs.is_empty() {
        return Err(invalid(
            "review/synthesis needs an evidence-bearing native returned observation",
        ));
    }
    let unit = engine
        .workflow()
        .units
        .values()
        .find(|u| u.reference == record.workflow_unit_ref)
        .ok_or_else(|| invalid("review unit absent from compiled source"))?;
    crate::workflow_inputs::validate_selected_inputs(
        engine,
        unit,
        &record.disposition.selected_inputs,
        &record.disposition.context_refs,
    )
    .map_err(invalid)?;
    Ok(record)
}
pub(crate) fn register(
    state: &StoredAttemptState,
    engine: &mut ExecutableOrchestration,
    attempt_ref: &str,
    review_of: BTreeSet<WorkflowUnitRef>,
) -> Result<(), FactoryAttemptError> {
    if review_of.is_empty() || review_of.len() > 128 {
        return Err(invalid(
            "independent review needs 1..128 actual producer units",
        ));
    }
    let reviewer = settled(state, engine, attempt_ref)?;
    for unit in &review_of {
        let leg = engine
            .leg(unit)
            .ok_or_else(|| invalid("reviewed producer has not executed"))?;
        let producer = state
            .attempts
            .values()
            .find(|a| a.execution_ref.as_deref() == Some(&leg.execution_ref))
            .ok_or_else(|| invalid("reviewed producer has no actual attempt"))?;
        settled(state, engine, &producer.attempt_ref)?;
        if reviewer.attempt_ref == producer.attempt_ref
            || reviewer.disposition.participant.agent_ref
                == producer.disposition.participant.agent_ref
            || reviewer.disposition.participant.agency_ref
                == producer.disposition.participant.agency_ref
            || reviewer.disposition.body.agent_session_ref
                == producer.disposition.body.agent_session_ref
        {
            return Err(invalid("reviewer must have a distinct actual Agent, Agency, Session and execution from every reviewed producer; changing a role label is not independence"));
        }
        if !reviewer.disposition.selected_inputs.iter().any(|input| {
            input.predecessor == *unit
                && input.execution_ref == leg.execution_ref
                && !input.artifacts.is_empty()
        }) {
            return Err(invalid(
                "reviewer did not receive selected material from every reviewed producer",
            ));
        }
    }
    engine.register_independent_reviewer(
        reviewer.execution_ref.as_ref().unwrap().clone(),
        &reviewer.disposition.selection,
        review_of,
    )?;
    Ok(())
}
pub(crate) struct SynthesisSelection {
    pub synthesis_ref: String,
    pub barrier_key: String,
    pub result_artifact_ref: String,
    pub artifact_refs: BTreeSet<String>,
}
pub(crate) fn synthesize(
    state: &StoredAttemptState,
    engine: &mut ExecutableOrchestration,
    attempt_ref: &str,
    reviewer_attempt_ref: &str,
    selection: SynthesisSelection,
) -> Result<(), FactoryAttemptError> {
    let result = settled(state, engine, attempt_ref)?;
    let reviewer = settled(state, engine, reviewer_attempt_ref)?;
    let barrier = engine.barrier_reading(&selection.barrier_key)?;
    let reviewed = engine
        .independent_reviewers()
        .get(reviewer.execution_ref.as_ref().unwrap())
        .ok_or_else(|| {
            invalid("independent review has not been admitted through the native operation")
        })?;
    if !barrier.required_units.is_subset(&reviewed.review_of) {
        return Err(invalid("review does not cover all required barrier legs"));
    }
    let received = result
        .disposition
        .selected_inputs
        .iter()
        .flat_map(|i| i.artifacts.iter())
        .map(|a| (a.artifact_ref.clone(), a))
        .collect::<std::collections::BTreeMap<_, _>>();
    if selection.artifact_refs.is_empty()
        || selection.artifact_refs.len() > 128
        || !selection
            .artifact_refs
            .iter()
            .all(|a| received.contains_key(a))
    {
        return Err(invalid(
            "synthesis must select actual predecessor artifacts delivered to its own context",
        ));
    }
    // Every required leg contributes. Selecting only the convenient successful
    // sibling is not accountable synthesis of the whole barrier.
    for unit in &barrier.required_units {
        let leg = engine
            .leg(unit)
            .ok_or_else(|| invalid("required synthesis leg missing"))?;
        if !leg
            .artifacts
            .iter()
            .any(|a| selection.artifact_refs.contains(&a.artifact_ref))
        {
            return Err(invalid("synthesis omitted material from a required leg"));
        }
    }
    let leg = engine.leg(&result.workflow_unit_ref).unwrap();
    let artifact = leg
        .artifacts
        .iter()
        .find(|a| {
            a.artifact_ref == selection.result_artifact_ref
                && result
                    .readable_return
                    .as_ref()
                    .unwrap()
                    .artifact_refs
                    .contains(&a.artifact_ref)
        })
        .ok_or_else(|| {
            invalid("synthesis result must be this attempt's actual returned artifact")
        })?;
    if artifact.semantic_difference.trim().is_empty() || artifact.evidence_refs.is_empty() {
        return Err(invalid(
            "synthesis result lacks its own difference/evidence",
        ));
    }
    let evidence = selection
        .artifact_refs
        .iter()
        .flat_map(|a| received[a].evidence_refs.clone())
        .collect();
    let difference = artifact.semantic_difference.clone();
    engine.synthesize(
        selection.synthesis_ref,
        &selection.barrier_key,
        reviewer.execution_ref.as_ref().unwrap(),
        selection.artifact_refs,
        evidence,
        difference,
    )?;
    Ok(())
}
