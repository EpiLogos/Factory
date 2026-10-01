//! Native Run lifecycle eligibility over the existing attempt owner.
//!
//! Lifecycle is not a display fold of custody or worker prose. Closure consumes
//! current returned attempts, independent review and a retained receiving link.
//! Unit decision relations are embedded in the existing canonical HumanRequest
//! records by the provider; this module does not introduce another store.

use crate::attempt_runtime::{FactoryAttemptError, FactoryAttemptOperation, StoredAttemptState};
use crate::core::run::WorkflowUnitRef;
use crate::orchestration::{ExecutableOrchestration, SynthesisRecord, WholeRunState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunClosureBasis {
    pub final_attempt_ref: String,
    pub reviewer_attempt_refs: BTreeSet<String>,
    pub receiving_ref: String,
}

/// The immutable affected work basis of a human-authority request. A controlled
/// protocol case remains explicitly controlled throughout request and response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitDecisionBasis {
    pub workflow_unit_ref: WorkflowUnitRef,
    pub attempt_ref: String,
    pub execution_ref: String,
    pub subject_ref: String,
    pub subject_revision: String,
    pub workflow_source_ref: String,
    pub workflow_source_revision: String,
    pub workflow_source_digest: String,
    pub resolver_ref: String,
    #[serde(default)]
    pub controlled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitDecisionOutcome {
    Resume,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitDecisionResponse {
    pub response_ref: String,
    pub resolver_ref: String,
    pub channel_receipt_ref: String,
    pub source_revision: String,
    pub outcome: UnitDecisionOutcome,
    pub evidence_refs: BTreeSet<String>,
    #[serde(default)]
    pub controlled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitDecisionRequest {
    pub human_request_ref: String,
    pub decision_ref: String,
    pub question: String,
    pub why_human: String,
    pub basis: UnitDecisionBasis,
    pub evidence_refs: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnitDecisionRetirement {
    pub reason: String,
    pub replacement_basis_refs: BTreeSet<String>,
    pub observed_provider_revision: u64,
}

fn invalid(message: impl Into<String>) -> FactoryAttemptError {
    FactoryAttemptError::InvalidOperation(message.into())
}

pub(crate) fn validate_finishing(
    state: &StoredAttemptState,
    engine: &ExecutableOrchestration,
) -> Result<(), FactoryAttemptError> {
    if engine.whole_run_state() != WholeRunState::Complete {
        return Err(invalid(
            "Run cannot finish while a required leg is missing, failed or historical",
        ));
    }
    for unit in engine.workflow().units.values() {
        let leg = engine
            .leg(&unit.reference)
            .ok_or_else(|| invalid("required lifecycle leg is missing"))?;
        if !engine.is_current_return(&unit.reference) {
            return Err(invalid(
                "lifecycle leg Return has a historical source basis",
            ));
        }
        let attempt = state
            .attempts
            .values()
            .find(|attempt| attempt.execution_ref.as_ref() == Some(&leg.execution_ref))
            .ok_or_else(|| invalid("required lifecycle leg has no native attempt"))?;
        crate::attempt_review::settled(state, engine, &attempt.attempt_ref)?;
    }
    Ok(())
}

/// The caller derives unresolved requests from canonical HumanRequest records
/// under the provider transaction. This is not a caller-authored success list.
pub(crate) fn validate_closure(
    state: &StoredAttemptState,
    engine: &ExecutableOrchestration,
    basis: &RunClosureBasis,
    unresolved_decision_refs: &BTreeSet<String>,
) -> Result<(), FactoryAttemptError> {
    validate_finishing(state, engine)?;
    if !unresolved_decision_refs.is_empty() {
        return Err(invalid("Run has unresolved human-authority decisions"));
    }
    if basis.reviewer_attempt_refs.is_empty() || basis.reviewer_attempt_refs.len() > 128 {
        return Err(invalid(
            "Run closure needs 1..128 actual independent review attempts",
        ));
    }
    let final_attempt = crate::attempt_review::settled(state, engine, &basis.final_attempt_ref)?;
    let final_return = final_attempt
        .readable_return
        .as_ref()
        .ok_or_else(|| invalid("Run closure has no final readable Return"))?;
    if basis.receiving_ref.trim().is_empty()
        || final_return.receiving_ref.as_deref() != Some(&basis.receiving_ref)
        || final_return
            .receiving_source_revision
            .as_ref()
            .is_none_or(|revision| revision.trim().is_empty())
    {
        return Err(invalid(
            "Run closure requires this final Return's retained native receiving basis",
        ));
    }
    let mut reviewed = BTreeSet::new();
    let mut reviewer_units = BTreeSet::new();
    let mut reviewer_executions = BTreeSet::new();
    for id in &basis.reviewer_attempt_refs {
        let attempt = crate::attempt_review::settled(state, engine, id)?;
        let execution = attempt
            .execution_ref
            .as_ref()
            .ok_or_else(|| invalid("closure reviewer has no execution"))?;
        let reviewer = engine
            .independent_reviewers()
            .get(execution)
            .ok_or_else(|| invalid("closure reviewer was not admitted as independent"))?;
        for unit in &reviewer.review_of {
            let leg = engine
                .leg(unit)
                .ok_or_else(|| invalid("closure reviewed producer is missing"))?;
            if !engine.is_current_return(unit)
                || !reviewer
                    .independent_from_execution_refs
                    .contains(&leg.execution_ref)
            {
                return Err(invalid(
                    "closure review belongs to a different producer candidate",
                ));
            }
        }
        reviewer_units.insert(attempt.workflow_unit_ref.clone());
        reviewer_executions.insert(execution.clone());
        reviewed.extend(reviewer.review_of.iter().cloned());
    }
    let required_producers = engine
        .workflow()
        .units
        .values()
        .map(|unit| unit.reference.clone())
        .filter(|unit| unit != &final_attempt.workflow_unit_ref && !reviewer_units.contains(unit))
        .collect::<BTreeSet<_>>();
    if reviewed.is_empty() || !required_producers.is_subset(&reviewed) {
        return Err(invalid(
            "independent review does not cover the Run's producer units",
        ));
    }
    for barrier in &engine.workflow().barriers {
        if !engine.syntheses().values().any(|synthesis| {
            synthesis.barrier_key == barrier.key
                && synthesis_is_current(state, engine, synthesis, &reviewer_executions)
        }) {
            return Err(invalid(
                "required barrier has no native synthesis of this exact current producer/reviewer material",
            ));
        }
    }
    Ok(())
}

fn synthesis_is_current(
    state: &StoredAttemptState,
    engine: &ExecutableOrchestration,
    synthesis: &SynthesisRecord,
    reviewer_executions: &BTreeSet<String>,
) -> bool {
    let Ok(barrier) = engine.barrier_reading(&synthesis.barrier_key) else {
        return false;
    };
    if !reviewer_executions.contains(&synthesis.reviewer_execution_ref) {
        return false;
    }
    let Some(reviewer) = engine
        .independent_reviewers()
        .get(&synthesis.reviewer_execution_ref)
    else {
        return false;
    };
    if !barrier.required_units.is_subset(&reviewer.review_of) {
        return false;
    }
    let mut artifacts = std::collections::BTreeMap::new();
    for unit in &barrier.required_units {
        let Some(leg) = engine.leg(unit) else {
            return false;
        };
        if !engine.is_current_return(unit)
            || !reviewer
                .independent_from_execution_refs
                .contains(&leg.execution_ref)
            || !leg
                .artifacts
                .iter()
                .any(|artifact| synthesis.artifact_refs.contains(&artifact.artifact_ref))
        {
            return false;
        }
        for artifact in &leg.artifacts {
            if artifact.producing_execution_ref != leg.execution_ref
                || artifact.subject_ref != leg.delegation.subject_ref
                || artifact.subject_revision != leg.delegation.basis_revision
            {
                return false;
            }
            artifacts.insert(artifact.artifact_ref.clone(), artifact);
        }
    }
    let selected_evidence = synthesis
        .artifact_refs
        .iter()
        .filter_map(|reference| artifacts.get(reference))
        .flat_map(|artifact| artifact.evidence_refs.iter().cloned())
        .collect::<BTreeSet<_>>();
    if synthesis.artifact_refs.is_empty()
        || !synthesis
            .artifact_refs
            .iter()
            .all(|reference| artifacts.contains_key(reference))
        || selected_evidence.is_empty()
        || synthesis.evidence_refs != selected_evidence
    {
        return false;
    }
    // The synthesis owner record predates its explicit result-attempt link.
    // Recover that link from the retained native Action receipt, never from a
    // display-provided artifact or an old matching barrier name.
    state.action_receipts.values().any(|action| {
        let FactoryAttemptOperation::Synthesize {
            attempt_ref,
            reviewer_attempt_ref,
            synthesis_ref,
            barrier_key,
            result_artifact_ref,
            artifact_refs,
        } = &action.request.operation
        else {
            return false;
        };
        if synthesis_ref != &synthesis.synthesis_ref
            || barrier_key != &synthesis.barrier_key
            || artifact_refs != &synthesis.artifact_refs
        {
            return false;
        }
        let Ok(reviewer_attempt) =
            crate::attempt_review::settled(state, engine, reviewer_attempt_ref)
        else {
            return false;
        };
        let Ok(result) = crate::attempt_review::settled(state, engine, attempt_ref) else {
            return false;
        };
        if reviewer_attempt.execution_ref.as_deref() != Some(&synthesis.reviewer_execution_ref) {
            return false;
        }
        engine.leg(&result.workflow_unit_ref).is_some_and(|leg| {
            leg.artifacts.iter().any(|artifact| {
                &artifact.artifact_ref == result_artifact_ref
                    && artifact.semantic_difference == synthesis.integrated_difference
                    && result.readable_return.as_ref().is_some_and(|returned| {
                        returned.artifact_refs.contains(result_artifact_ref)
                    })
            })
        })
    })
}

pub(crate) fn validate_decision_basis(
    state: &StoredAttemptState,
    engine: &ExecutableOrchestration,
    basis: &UnitDecisionBasis,
) -> Result<(), FactoryAttemptError> {
    if basis.resolver_ref.trim().is_empty()
        || basis.subject_ref.trim().is_empty()
        || basis.subject_revision.trim().is_empty()
        || basis.workflow_source_ref != state.workflow_source.source.reference.to_string()
        || basis.workflow_source_revision != state.workflow_source.source.revision
        || basis.workflow_source_digest != state.workflow_source.source.digest
    {
        return Err(invalid(
            "human request has a missing or different workflow basis",
        ));
    }
    let attempt = state
        .attempts
        .get(&basis.attempt_ref)
        .ok_or_else(|| invalid("human request's affected native attempt is absent"))?;
    let execution = attempt
        .execution_ref
        .as_ref()
        .unwrap_or(&attempt.reserved_execution_ref);
    let leg = engine
        .leg(&basis.workflow_unit_ref)
        .ok_or_else(|| invalid("human request's affected execution leg is absent"))?;
    if attempt.workflow_unit_ref != basis.workflow_unit_ref
        || execution != &basis.execution_ref
        || leg.execution_ref != basis.execution_ref
        || leg.delegation.subject_ref != basis.subject_ref
        || leg.delegation.basis_revision != basis.subject_revision
        || engine.current_subject_revision(&basis.subject_ref)
            != Some(basis.subject_revision.as_str())
    {
        return Err(invalid(
            "human request belongs to a historical or replaced affected unit",
        ));
    }
    Ok(())
}

/// Compatibility guard after the native channel has established responder
/// authority. Strings in an Action payload do not establish a human identity.
pub(crate) fn validate_decision_response(
    basis: &UnitDecisionBasis,
    response: &UnitDecisionResponse,
) -> Result<(), FactoryAttemptError> {
    if response.response_ref.trim().is_empty()
        || response.resolver_ref != basis.resolver_ref
        || response.channel_receipt_ref.trim().is_empty()
        || response.source_revision.trim().is_empty()
        || response.evidence_refs.is_empty()
        || response.controlled != basis.controlled
    {
        return Err(invalid(
            "decision response lacks the expected resolver, native channel evidence or controlled standing",
        ));
    }
    Ok(())
}
