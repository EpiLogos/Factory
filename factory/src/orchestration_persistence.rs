//! Durable coordinator state. Run and Workflow source remain in their existing
//! owner records; a snapshot contains neither another Run nor mutation authority.

use super::*;
use crate::core::identity::Revision;
use crate::core::run::RunRef;
use crate::workflow::WorkflowSourceProvenance;

pub const ORCHESTRATION_SNAPSHOT: &str = "factory.orchestration-snapshot/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrchestrationSnapshot {
    contract: String,
    run_ref: RunRef,
    topology_revision: Revision,
    workflow_source: WorkflowSourceProvenance,
    workflow_key: String,
    legs: BTreeMap<WorkflowUnitRef, LegRecord>,
    subject_revisions: BTreeMap<String, String>,
    active_writers: BTreeMap<String, WorkflowUnitRef>,
    retry_grants: BTreeMap<String, RetryGrant>,
    reviewers: BTreeMap<String, IndependentReviewer>,
    syntheses: BTreeMap<String, SynthesisRecord>,
    owner_observations: BTreeMap<String, OwnerArchitectureObservationReceipt>,
    concatenation_attempts: BTreeSet<String>,
    self_review_attempts: BTreeSet<String>,
}

impl ExecutableOrchestration {
    pub fn snapshot(&self) -> OrchestrationSnapshot {
        OrchestrationSnapshot {
            contract: ORCHESTRATION_SNAPSHOT.into(),
            run_ref: self.run.reference().clone(),
            topology_revision: self.run.map().topology_revision(),
            workflow_source: self.workflow.source.clone(),
            workflow_key: self.workflow.workflow_key.clone(),
            legs: self.legs.clone(),
            subject_revisions: self.subject_revisions.clone(),
            active_writers: self.active_writers.clone(),
            retry_grants: self.retry_grants.clone(),
            reviewers: self.reviewers.clone(),
            syntheses: self.syntheses.clone(),
            owner_observations: self.owner_observations.clone(),
            concatenation_attempts: self.concatenation_attempts.clone(),
            self_review_attempts: self.self_review_attempts.clone(),
        }
    }

    pub fn current_subject_revision(&self, subject_ref: &str) -> Option<&str> {
        self.subject_revisions.get(subject_ref).map(String::as_str)
    }

    /// Bind a reserved Factory attempt to the execution identity returned by the
    /// native owner. This is not a claim that the admitted execution ran.
    pub fn bind_execution_identity(
        &mut self,
        unit: &WorkflowUnitRef,
        reserved_ref: &str,
        execution_ref: &str,
    ) -> Result<(), OrchestrationError> {
        if execution_ref.trim().is_empty()
            || reserved_ref == execution_ref
            || self.legs.values().any(|leg| {
                leg.attempts
                    .iter()
                    .any(|attempt| attempt.execution_ref == execution_ref)
            })
            || self.reviewers.contains_key(execution_ref)
        {
            return Err(OrchestrationError::DuplicateExecution(execution_ref.into()));
        }
        let leg = self.leg_mut(unit)?;
        if leg.execution_ref != reserved_ref
            || !leg.artifacts.is_empty()
            || !leg.late_artifacts.is_empty()
            || !reserved_ref.starts_with("factory-attempt:")
        {
            return Err(OrchestrationError::InvalidSnapshot(
                "execution identity is already bound or produced results".into(),
            ));
        }
        leg.execution_ref = execution_ref.into();
        leg.attempts
            .last_mut()
            .ok_or_else(|| OrchestrationError::InvalidSnapshot("missing reserved attempt".into()))?
            .execution_ref = execution_ref.into();
        Ok(())
    }
}

impl OrchestrationSnapshot {
    /// Reopen only against the canonical Run and freshly compiled authored
    /// source. Authority is reconstituted by Run, never deserialized from input.
    pub fn restore(
        &self,
        workflow: CompiledWorkflow,
        run: Run,
    ) -> Result<ExecutableOrchestration, OrchestrationError> {
        let invalid = |detail: &str| OrchestrationError::InvalidSnapshot(detail.into());
        if self.contract != ORCHESTRATION_SNAPSHOT
            || self.run_ref != *run.reference()
            || self.topology_revision != run.map().topology_revision()
            || self.workflow_source != workflow.source
            || self.workflow_key != workflow.workflow_key
        {
            return Err(invalid("Run/topology/source basis changed"));
        }
        let authority = run.mutation_authority();
        let restored = ExecutableOrchestration {
            workflow,
            run,
            authority,
            legs: self.legs.clone(),
            subject_revisions: self.subject_revisions.clone(),
            active_writers: self.active_writers.clone(),
            retry_grants: self.retry_grants.clone(),
            reviewers: self.reviewers.clone(),
            syntheses: self.syntheses.clone(),
            owner_observations: self.owner_observations.clone(),
            concatenation_attempts: self.concatenation_attempts.clone(),
            self_review_attempts: self.self_review_attempts.clone(),
        };
        let mut executions = BTreeSet::new();
        let mut writers = BTreeMap::new();
        let mut grant_uses = BTreeMap::<String, u32>::new();
        for unit in restored.workflow.units.values() {
            restored.node_for_unit(&unit.reference)?;
            if restored
                .subject_revisions
                .get(&unit.subject_ref.to_string())
                .is_none_or(|revision| revision.trim().is_empty())
            {
                return Err(invalid("missing current subject basis"));
            }
        }
        for (unit_ref, leg) in &restored.legs {
            let unit = restored.unit(unit_ref)?;
            let latest = leg
                .attempts
                .last()
                .ok_or_else(|| invalid("empty attempt history"))?;
            if latest.execution_ref != leg.execution_ref
                || latest.delegation != leg.delegation
                || latest.status != leg.status
                || latest.status_history != leg.status_history
                || latest.artifacts != leg.artifacts
                || latest.late_artifacts != leg.late_artifacts
                || latest.failure_reason != leg.failure_reason
            {
                return Err(invalid("current leg differs from its latest attempt"));
            }
            for attempt in &leg.attempts {
                if attempt.execution_ref.trim().is_empty()
                    || !executions.insert(attempt.execution_ref.clone())
                    || attempt.status_history.first() != Some(&LegStatus::Active)
                    || attempt.status_history.last() != Some(&attempt.status)
                    || attempt.delegation.parent_run_ref != self.run_ref.to_string()
                    || attempt.delegation.execution_unit_ref != *unit_ref
                    || attempt.delegation.subject_ref != unit.subject_ref.to_string()
                    || attempt.delegation.basis_revision.trim().is_empty()
                    || attempt.delegation.permitted_effects != unit.permitted_effects
                    || attempt.delegation.verification_obligations != unit.verification_obligations
                {
                    return Err(invalid("invalid attempt identity, history or delegation"));
                }
                if let Some(grant) = &attempt.delegation.retry_grant {
                    let uses = grant_uses.entry(grant.grant_ref.clone()).or_default();
                    *uses = uses
                        .checked_add(1)
                        .ok_or_else(|| invalid("retry use overflow"))?;
                }
                for artifact in attempt.artifacts.iter().chain(&attempt.late_artifacts) {
                    artifact.validate()?;
                    if artifact.producing_execution_ref != attempt.execution_ref
                        || artifact.subject_ref != attempt.delegation.subject_ref
                        || artifact.subject_revision != attempt.delegation.basis_revision
                    {
                        return Err(invalid(
                            "artifact is attributed to another attempt or basis",
                        ));
                    }
                }
            }
            let released = matches!(
                leg.status,
                LegStatus::Failed
                    | LegStatus::Returned
                    | LegStatus::ProcessTerminated
                    | LegStatus::Quiescent
            ) || (leg.status == LegStatus::LateResult
                && leg.status_history.iter().any(|status| {
                    matches!(status, LegStatus::ProcessTerminated | LegStatus::Quiescent)
                }));
            if is_writer(unit)
                && !released
                && writers
                    .insert(unit.subject_ref.to_string(), unit_ref.clone())
                    .is_some()
            {
                return Err(invalid("two active writers hold one subject"));
            }
            let node_id = restored.node_for_unit(unit_ref)?;
            let state = restored.run.map().nodes()[&node_id].state;
            let expected = match leg.status {
                LegStatus::Returned => NodeState::Returned,
                LegStatus::Failed => NodeState::Abandoned,
                _ => NodeState::Active,
            };
            if state != Some(expected) {
                return Err(invalid("canonical RunMap disagrees with the current leg"));
            }
        }
        if writers != restored.active_writers {
            return Err(invalid("writer reservations disagree with attempt state"));
        }
        for (key, grant) in &restored.retry_grants {
            if key != &grant.grant_ref
                || key.trim().is_empty()
                || grant.attempts_allowed == 0
                || grant.attempts_spent > grant.attempts_allowed
                || grant_uses.get(key).copied().unwrap_or(0) > grant.attempts_spent
            {
                return Err(invalid("invalid or replenished retry grant"));
            }
        }
        if grant_uses
            .keys()
            .any(|reference| !restored.retry_grants.contains_key(reference))
        {
            return Err(invalid("attempt names an absent retry grant"));
        }
        for (key, reviewer) in &restored.reviewers {
            if key != &reviewer.execution_ref
                || executions.contains(key)
                || reviewer.disposition_run_ref != self.run_ref.to_string()
                || reviewer.review_of.is_empty()
                || reviewer
                    .review_of
                    .iter()
                    .any(|unit| restored.unit(unit).is_err())
            {
                return Err(invalid("invalid reviewer identity or scope"));
            }
        }
        Ok(restored)
    }
}
