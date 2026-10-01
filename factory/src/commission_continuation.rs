//! Commission continuation over the existing canonical developmental owner.
//!
//! The mutation history is the immutable continuation receipt. No new campaign,
//! Routine invocation, candidate acceptance or shadow source store is created.

use crate::commission::{
    CommissionContinuationRelation, CommissionError, FactoryDevelopmentalMutation,
    FactoryDevelopmentalMutationRequest,
};
use crate::core::identity::{Ref, Revision};
use crate::core::run::{Run, RunRef};
use crate::developmental_read::FactoryDevelopmentalState;
use crate::journey::{JourneyRef, JourneyStatus};
use crate::workflow::{compile_workflow, CompiledWorkflow, WorkflowSource};
use std::collections::BTreeSet;

struct Continuation<'a> {
    relation: CommissionContinuationRelation,
    commission: &'a str,
    journey: &'a JourneyRef,
    predecessor: &'a RunRef,
    journey_revision: Revision,
    predecessor_revision: Revision,
    source_ref: &'a Ref,
    source_revision: &'a str,
    source_digest: &'a str,
    successor: &'a RunRef,
    source: &'a WorkflowSource,
    reason: &'a str,
    basis: &'a [String],
}

fn fields(mutation: &FactoryDevelopmentalMutation) -> Result<Continuation<'_>, CommissionError> {
    let FactoryDevelopmentalMutation::ContinueCommission {
        continuation_relation,
        commission_ref,
        journey_ref,
        predecessor_run_ref,
        expected_journey_revision,
        expected_predecessor_run_revision,
        predecessor_source_ref,
        predecessor_source_revision,
        predecessor_source_digest,
        successor_run_ref,
        workflow_source,
        reason,
        basis_refs,
    } = mutation
    else {
        return Err(CommissionError::Invalid("continuation.kind".into()));
    };
    Ok(Continuation {
        relation: *continuation_relation,
        commission: commission_ref,
        journey: journey_ref,
        predecessor: predecessor_run_ref,
        journey_revision: *expected_journey_revision,
        predecessor_revision: *expected_predecessor_run_revision,
        source_ref: predecessor_source_ref,
        source_revision: predecessor_source_revision,
        source_digest: predecessor_source_digest,
        successor: successor_run_ref,
        source: workflow_source,
        reason,
        basis: basis_refs,
    })
}

fn conflict(detail: impl Into<String>) -> CommissionError {
    CommissionError::Conflict(detail.into())
}

fn validate_fields(basis: &Continuation<'_>) -> Result<(), CommissionError> {
    for (name, value) in [
        ("commissionRef", basis.commission),
        ("predecessorSourceRevision", basis.source_revision),
        ("predecessorSourceDigest", basis.source_digest),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_whitespace) {
            return Err(CommissionError::Invalid(name.into()));
        }
    }
    if basis.reason.trim().is_empty()
        || basis.reason != basis.reason.trim()
        || basis.reason.chars().any(char::is_control)
        || basis.predecessor == basis.successor
        || basis.basis.is_empty()
        || basis.basis.len() > 128
        || basis
            .basis
            .iter()
            .any(|value| value.trim().is_empty() || value.chars().any(char::is_whitespace))
        || basis.basis.iter().collect::<BTreeSet<_>>().len() != basis.basis.len()
        || basis.source_ref.kind() != "workflow-source"
        || basis.source.source.reference == *basis.source_ref
        || basis.source_digest.len() != 64
        || !basis
            .source_digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(CommissionError::Invalid("continuation.basis".into()));
    }
    Ok(())
}

pub(crate) fn validate_request(
    request: &FactoryDevelopmentalMutationRequest,
) -> Result<(), CommissionError> {
    let basis = fields(&request.mutation)?;
    validate_fields(&basis)?;
    if request.source.reference != basis.source.source.reference.to_string()
        || request.source.revision != basis.source.source.revision
    {
        return Err(CommissionError::Invalid("continuation.source".into()));
    }
    Ok(())
}

fn retained_source<'a>(
    state: &'a FactoryDevelopmentalState,
    basis: &Continuation<'_>,
) -> Result<&'a WorkflowSource, CommissionError> {
    let source = state
        .workflow_sources
        .iter()
        .find(|source| {
            source.source.reference == *basis.source_ref
                && source.source.revision == basis.source_revision
                && source.source.digest == basis.source_digest
        })
        .ok_or_else(|| conflict("predecessor source basis changed or is absent"))?;
    if state
        .attempt_states
        .get(basis.predecessor)
        .is_some_and(|attempts| attempts.workflow_source() != source)
    {
        return Err(conflict(
            "predecessor attempt retains a different source basis",
        ));
    }
    Ok(source)
}

fn compiled_membership(run: &Run, workflow: &CompiledWorkflow) -> bool {
    let members = run
        .map()
        .nodes()
        .values()
        .filter_map(|node| node.semantic_ref.as_ref())
        .collect::<BTreeSet<_>>();
    workflow
        .units
        .values()
        .all(|unit| members.contains(unit.reference.as_ref()))
}

fn ancestry(basis: &Continuation<'_>) -> Vec<String> {
    let mut refs = basis.basis.to_vec();
    refs.extend([
        basis.commission.into(),
        basis.predecessor.to_string(),
        basis.source_ref.to_string(),
        format!(
            "{}@{}:{}",
            basis.source_ref, basis.source_revision, basis.source_digest
        ),
        basis.source.source.reference.to_string(),
        format!(
            "{}@{}:{}",
            basis.source.source.reference, basis.source.source.revision, basis.source.source.digest
        ),
    ]);
    if basis.relation == CommissionContinuationRelation::BoundedContribution {
        refs.push("factory-continuation-relation:bounded-contribution".into());
    }
    refs.sort();
    refs.dedup();
    refs
}

pub(crate) fn apply(
    state: &mut FactoryDevelopmentalState,
    mutation: &FactoryDevelopmentalMutation,
) -> Result<(), CommissionError> {
    let basis = fields(mutation)?;
    validate_fields(&basis)?;
    let commission = state
        .commissions
        .iter()
        .find(|commission| commission.request.request_ref == basis.commission)
        .ok_or_else(|| CommissionError::ForeignReference(basis.commission.into()))?;
    if &commission.journey_ref != basis.journey {
        return Err(conflict(
            "continuation belongs to another Commission Journey",
        ));
    }
    let journey_index = state
        .journeys
        .iter()
        .position(|journey| {
            &journey.journey_ref == basis.journey
                && journey.commission.commission_ref.as_deref() == Some(basis.commission)
        })
        .ok_or_else(|| CommissionError::ForeignReference(basis.journey.to_string()))?;
    let journey = &state.journeys[journey_index];
    if !matches!(
        journey.status,
        JourneyStatus::Active | JourneyStatus::Paused
    ) || journey.revision != basis.journey_revision
        || !journey
            .runs
            .iter()
            .any(|link| &link.run_ref == basis.predecessor)
    {
        return Err(conflict(
            "Journey revision, standing or predecessor membership changed",
        ));
    }
    let predecessor = state
        .build
        .run(basis.predecessor)
        .ok_or_else(|| CommissionError::ForeignReference(basis.predecessor.to_string()))?;
    if predecessor.revision() != basis.predecessor_revision
        || predecessor.project_ref() != &commission.project_ref
    {
        return Err(conflict("predecessor Run revision or project changed"));
    }
    let previous_source = retained_source(state, &basis)?;
    let previous_workflow =
        compile_workflow(previous_source.clone()).map_err(|e| conflict(e.to_string()))?;
    if !compiled_membership(predecessor, &previous_workflow) {
        return Err(conflict(
            "predecessor Run does not own the supplied source units",
        ));
    }
    if state
        .attempt_states
        .get(basis.predecessor)
        .is_some_and(|attempts| !attempts.snapshot().writer_reservations().is_empty())
    {
        return Err(conflict(
            "predecessor source writer must quiesce before continuation",
        ));
    }
    if state.build.run(basis.successor).is_some()
        || state
            .workflow_sources
            .iter()
            .any(|source| source.source.reference == basis.source.source.reference)
    {
        return Err(conflict(
            "successor Run or workflow source identity already exists",
        ));
    }
    let workflow = compile_workflow(basis.source.clone()).map_err(|e| conflict(e.to_string()))?;
    crate::orchestration::validate_initial_subject_revisions(&workflow)
        .map_err(|e| conflict(e.to_string()))?;
    let mut successor = Run::new(
        basis.successor.clone(),
        predecessor.project_ref().clone(),
        predecessor.destination(),
        predecessor.write_authority().owner(),
    )
    .map_err(|e| conflict(e.to_string()))?;
    successor
        .apply_topology_command(
            &successor.mutation_authority(),
            workflow.topology_command(successor.revision()),
        )
        .map_err(|e| conflict(e.to_string()))?;
    state
        .build
        .insert_run(successor)
        .map_err(|e| conflict(e.to_string()))?;
    state.journeys[journey_index]
        .add_run(basis.successor.clone(), ancestry(&basis), Vec::new())
        .map_err(|e| conflict(e.to_string()))?;
    state.workflow_sources.push(basis.source.clone());
    state
        .workflow_sources
        .sort_by_key(|source| source.source.reference.to_string());
    Ok(())
}

pub(crate) fn validate_record(
    state: &FactoryDevelopmentalState,
    mutation: &FactoryDevelopmentalMutation,
) -> Result<(), CommissionError> {
    let basis = fields(mutation)?;
    let commission = state
        .commissions
        .iter()
        .find(|commission| {
            commission.request.request_ref == basis.commission
                && &commission.journey_ref == basis.journey
        })
        .ok_or(CommissionError::InvalidStored)?;
    let journey = state
        .journeys
        .iter()
        .find(|journey| &journey.journey_ref == basis.journey)
        .ok_or(CommissionError::InvalidStored)?;
    let link = journey
        .runs
        .iter()
        .find(|link| &link.run_ref == basis.successor)
        .ok_or(CommissionError::InvalidStored)?;
    let predecessor = state
        .build
        .run(basis.predecessor)
        .ok_or(CommissionError::InvalidStored)?;
    let successor = state
        .build
        .run(basis.successor)
        .ok_or(CommissionError::InvalidStored)?;
    let source = retained_source(state, &basis)?;
    let workflow =
        compile_workflow(basis.source.clone()).map_err(|_| CommissionError::InvalidStored)?;
    if journey.commission.commission_ref.as_deref() != Some(basis.commission)
        || link.journey_revision != basis.journey_revision
        || link.basis_refs != ancestry(&basis)
        || !journey
            .runs
            .iter()
            .any(|link| &link.run_ref == basis.predecessor)
        || predecessor.project_ref() != &commission.project_ref
        || successor.project_ref() != &commission.project_ref
        || successor.destination() != predecessor.destination()
        || predecessor.revision().get() < basis.predecessor_revision.get()
        || !state.workflow_sources.contains(basis.source)
        || !compiled_membership(
            predecessor,
            &compile_workflow(source.clone()).map_err(|_| CommissionError::InvalidStored)?,
        )
        || !compiled_membership(successor, &workflow)
    {
        return Err(CommissionError::InvalidStored);
    }
    crate::orchestration::validate_initial_subject_revisions(&workflow)
        .map_err(|_| CommissionError::InvalidStored)?;
    Ok(())
}
