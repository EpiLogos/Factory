//! Versioned public reads over Factory-owned Project, Journey, Run and compiled
//! WorkflowUnit state.
//!
//! The read models are deliberately bounded by subject. They expose canonical
//! Factory state without requiring a consumer to link to private storage or
//! reconstruct Journey/Run meaning from UI fixtures.

use crate::action_projection::{
    FactoryProjectedActionProvider, FACTORY_ACTION_PROJECTION_CONTRACT,
};
use crate::build::{
    AgencyRecord, CandidateRecord, EvidenceRecord, ExecutionRecord, FactoryActionAuthority,
    FactoryActionExecutor, FactoryActionInvocation, FactoryActionReceipt, FactoryBuildError,
    FactoryBuildSelection, FactoryBuildState, FactoryBuildViewProvider, HumanRequestRecord,
    FACTORY_NATIVE_OWNER,
};
use crate::core::identity::{Ref, Revision};
use crate::core::run::{ProjectRef, RunLifecycle, RunMap, RunMapAddress, RunRef, WorkflowUnitRef};
use crate::journey::{
    Journey, JourneyCommission, JourneyParticipant, JourneyRecognitionLink, JourneyRef,
    JourneyReturn, JourneyStatus,
};
use crate::workflow::{
    compile_workflow, CompiledAgentRequirements, CompiledWorkflow, CompiledWorkflowUnit,
    WorkflowSource,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const FACTORY_DEVELOPMENTAL_STATE_SCHEMA: &str = "factory.developmental-state/v1";
pub const FACTORY_PROJECT_READING_CONTRACT: &str = "factory.project-reading/v1";
pub const FACTORY_JOURNEY_READING_CONTRACT: &str = "factory.journey-reading/v1";
pub const FACTORY_RUN_READING_CONTRACT: &str = "factory.run-reading/v1";
pub const FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT: &str =
    "factory.workflow-unit-list-reading/v1";
pub const FACTORY_WORKFLOW_UNIT_READING_CONTRACT: &str = "factory.workflow-unit-reading/v1";
pub const FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT: &str =
    "factory.execution-telemetry-reading/v1";
pub const FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER: &str = "factory.developmental-local-provider/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryDevelopmentalState {
    pub schema: String,
    pub build: FactoryBuildState,
    pub journeys: Vec<Journey>,
    #[serde(default)]
    pub workflow_sources: Vec<WorkflowSource>,
    /// Factory-owned joins between developmental work and externally owned
    /// execution actuality. The joined observations remain references to their
    /// native owners; this is not a copied Activity or machine-metrics store.
    #[serde(default)]
    pub execution_correlations: Vec<FactoryExecutionCorrelation>,
}

impl FactoryDevelopmentalState {
    pub fn new(
        build: FactoryBuildState,
        mut journeys: Vec<Journey>,
    ) -> Result<Self, FactoryDevelopmentalReadError> {
        journeys.sort_by(|left, right| left.journey_ref.cmp(&right.journey_ref));
        let state = Self {
            schema: FACTORY_DEVELOPMENTAL_STATE_SCHEMA.into(),
            build,
            journeys,
            workflow_sources: Vec::new(),
            execution_correlations: Vec::new(),
        };
        state.validate()?;
        Ok(state)
    }

    /// Attach authoritative workflow source to the developmental owner state.
    /// The source digest is checked and the compiled artifact is regenerated
    /// here and again on reopen/read; caller-supplied compiled artifacts are
    /// never persisted as Factory truth.
    pub fn with_workflow_sources(
        mut self,
        mut workflow_sources: Vec<WorkflowSource>,
    ) -> Result<Self, FactoryDevelopmentalReadError> {
        workflow_sources.sort_by_key(workflow_source_identity);
        self.workflow_sources = workflow_sources;
        self.validate()?;
        Ok(self)
    }

    /// Attach Factory-owned execution correlations after validating every
    /// Factory edge and every external-owner provenance claim.
    pub fn with_execution_correlations(
        mut self,
        mut correlations: Vec<FactoryExecutionCorrelation>,
    ) -> Result<Self, FactoryDevelopmentalReadError> {
        correlations.sort_by(|left, right| left.correlation_ref.cmp(&right.correlation_ref));
        self.execution_correlations = correlations;
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), FactoryDevelopmentalReadError> {
        if self.schema != FACTORY_DEVELOPMENTAL_STATE_SCHEMA {
            return Err(FactoryDevelopmentalReadError::UnsupportedSchema(
                self.schema.clone(),
            ));
        }
        let project_ref = self.build.project().reference();
        let mut previous: Option<&JourneyRef> = None;
        for journey in &self.journeys {
            journey
                .validate()
                .map_err(|error| FactoryDevelopmentalReadError::InvalidJourney {
                    journey_ref: journey.journey_ref.to_string(),
                    detail: error.to_string(),
                })?;
            if &journey.project_ref != project_ref {
                return Err(FactoryDevelopmentalReadError::ProjectJourneyMismatch {
                    project_ref: project_ref.to_string(),
                    journey_ref: journey.journey_ref.to_string(),
                });
            }
            if previous == Some(&journey.journey_ref) {
                return Err(FactoryDevelopmentalReadError::DuplicateJourney(
                    journey.journey_ref.to_string(),
                ));
            }
            previous = Some(&journey.journey_ref);
            for link in &journey.runs {
                let run = self.build.run(&link.run_ref).ok_or_else(|| {
                    FactoryDevelopmentalReadError::RunNotFound(link.run_ref.to_string())
                })?;
                if run.project_ref() != project_ref {
                    return Err(FactoryDevelopmentalReadError::ProjectRunMismatch {
                        project_ref: project_ref.to_string(),
                        run_ref: link.run_ref.to_string(),
                    });
                }
            }
        }
        let compiled_workflows = self.compile_workflows()?;
        self.validate_execution_correlations(&compiled_workflows)?;
        Ok(())
    }

    pub fn project_reading(
        &self,
        project_ref: &ProjectRef,
    ) -> Result<FactoryProjectReading, FactoryDevelopmentalReadError> {
        self.ensure_project(project_ref)?;
        Ok(FactoryProjectReading {
            contract: FACTORY_PROJECT_READING_CONTRACT.into(),
            provenance: FactoryReadingProvenance::new(
                self.build.revision(),
                self.build.project().revision(),
                "canonical FactoryDevelopmentalState + Factory Journey registry",
            ),
            project_ref: project_ref.clone(),
            project_revision: self.build.project().revision(),
            journeys: self
                .journeys
                .iter()
                .map(|journey| FactoryJourneySummary {
                    journey_ref: journey.journey_ref.clone(),
                    revision: journey.revision,
                    status: journey.status,
                    frontier: journey.frontier.clone(),
                    run_refs: journey
                        .runs
                        .iter()
                        .map(|link| link.run_ref.clone())
                        .collect(),
                })
                .collect(),
        })
    }

    pub fn journey_reading(
        &self,
        journey_ref: &JourneyRef,
    ) -> Result<FactoryJourneyReading, FactoryDevelopmentalReadError> {
        let journey = self
            .journeys
            .iter()
            .find(|journey| &journey.journey_ref == journey_ref)
            .ok_or_else(|| {
                FactoryDevelopmentalReadError::JourneyNotFound(journey_ref.to_string())
            })?;
        Ok(FactoryJourneyReading {
            contract: FACTORY_JOURNEY_READING_CONTRACT.into(),
            provenance: FactoryReadingProvenance::new(
                self.build.revision(),
                journey.revision,
                "canonical Factory Journey",
            ),
            journey_ref: journey.journey_ref.clone(),
            revision: journey.revision,
            project_ref: journey.project_ref.clone(),
            related_projects: journey.related_projects.clone(),
            commission: journey.commission.clone(),
            status: journey.status,
            frontier: journey.frontier.clone(),
            participants: journey.participants.clone(),
            flow_refs: journey.flow_refs.clone(),
            run_refs: journey
                .runs
                .iter()
                .map(|link| link.run_ref.clone())
                .collect(),
            agent_session_refs: journey.agent_session_refs.clone(),
            activity_refs: journey.activity_refs.clone(),
            returns: journey.returns.clone(),
            recognitions: journey.recognitions.clone(),
            material_context_refs: journey.material_context_refs.clone(),
            started_at: journey.started_at.clone(),
            completed_at: journey.completed_at.clone(),
        })
    }

    pub fn run_reading(
        &self,
        run_ref: &RunRef,
    ) -> Result<FactoryRunReading, FactoryDevelopmentalReadError> {
        let run = self
            .build
            .run(run_ref)
            .ok_or_else(|| FactoryDevelopmentalReadError::RunNotFound(run_ref.to_string()))?;
        let selection = FactoryBuildSelection {
            project_ref: run.project_ref().clone(),
            run_ref: run_ref.clone(),
        };
        let snapshot = FactoryBuildViewProvider.snapshot(&self.build, &selection)?;
        let applicable_subject_refs = snapshot
            .view
            .candidates
            .iter()
            .map(|candidate| candidate.candidate_ref.clone())
            .collect::<Vec<_>>();
        let actions = snapshot
            .view
            .actions
            .iter()
            .map(|action| FactoryActionDescriptor {
                action_ref: action.action_ref.clone(),
                label: action.label.clone(),
                subject_kinds: action.subject_kinds.clone(),
                input_contract: FACTORY_ACTION_PROJECTION_CONTRACT.into(),
                result_contract: FACTORY_ACTION_PROJECTION_CONTRACT.into(),
                authority_owner: FACTORY_NATIVE_OWNER.into(),
                required_capability_ref: action.required_capability_ref.clone(),
                currently_applicable: !applicable_subject_refs.is_empty(),
                applicable_subject_refs: applicable_subject_refs.clone(),
            })
            .collect();
        Ok(FactoryRunReading {
            contract: FACTORY_RUN_READING_CONTRACT.into(),
            provenance: FactoryReadingProvenance::new(
                self.build.revision(),
                run.revision(),
                "canonical Factory Run/RunMap + Factory Build correlations",
            ),
            run_ref: run.reference().clone(),
            revision: run.revision(),
            project_ref: run.project_ref().clone(),
            owning_journey_refs: self
                .journeys
                .iter()
                .filter(|journey| journey.runs.iter().any(|link| &link.run_ref == run_ref))
                .map(|journey| journey.journey_ref.clone())
                .collect(),
            lifecycle: run.lifecycle(),
            destination: run.destination().into(),
            run_map: run.map().clone(),
            thought_available: !run.thought_field().thoughts().is_empty(),
            thought_refs: run
                .thought_field()
                .thoughts()
                .keys()
                .map(ToString::to_string)
                .collect(),
            agencies: snapshot.view.agencies,
            executions: snapshot.view.executions,
            evidence: snapshot.view.evidence,
            candidates: snapshot.view.candidates,
            human_requests: snapshot.view.human_requests,
            actions,
        })
    }

    /// List compiled WorkflowUnits in deterministic source/locator order.
    /// An optional Run bounds correlation to that Run while preserving the
    /// complete Factory-owned unit definition.
    pub fn workflow_units_reading(
        &self,
        run_ref: Option<&RunRef>,
    ) -> Result<FactoryWorkflowUnitListReading, FactoryDevelopmentalReadError> {
        if let Some(run_ref) = run_ref {
            self.ensure_run(run_ref)?;
        }
        let compiled_workflows = self.compile_workflows()?;
        let mut units = compiled_workflows
            .iter()
            .flat_map(|workflow| {
                workflow
                    .units
                    .values()
                    .map(move |unit| self.workflow_unit_summary(workflow, unit, run_ref))
            })
            .collect::<Vec<_>>();
        units.sort_by(|left, right| left.locator.cmp(&right.locator));
        Ok(FactoryWorkflowUnitListReading {
            contract: FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT.into(),
            provenance: FactoryWorkflowUnitListProvenance {
                owner: FACTORY_NATIVE_OWNER.into(),
                build_state_revision: self.build.revision(),
                source_bases: compiled_workflows
                    .iter()
                    .map(|workflow| FactoryWorkflowSourceBasis {
                        source_ref: workflow.source.reference.clone(),
                        source_revision: workflow.source.revision.clone(),
                        source_digest: workflow.source.digest.clone(),
                    })
                    .collect(),
            },
            project_ref: self.build.project().reference().clone(),
            run_filter: run_ref.cloned(),
            units,
        })
    }

    /// Resolve one canonical WorkflowUnit without treating its requirements as
    /// evidence of runtime assignment, Agency, execution or telemetry.
    pub fn workflow_unit_reading(
        &self,
        workflow_unit_ref: &WorkflowUnitRef,
        run_ref: Option<&RunRef>,
    ) -> Result<FactoryWorkflowUnitReading, FactoryDevelopmentalReadError> {
        if let Some(run_ref) = run_ref {
            self.ensure_run(run_ref)?;
        }
        let compiled_workflows = self.compile_workflows()?;
        let (workflow, unit) = compiled_workflows
            .iter()
            .find_map(|workflow| {
                workflow
                    .units
                    .values()
                    .find(|unit| &unit.reference == workflow_unit_ref)
                    .map(|unit| (workflow, unit))
            })
            .ok_or_else(|| {
                FactoryDevelopmentalReadError::WorkflowUnitNotFound(workflow_unit_ref.to_string())
            })?;
        let summary = self.workflow_unit_summary(workflow, unit, run_ref);
        Ok(FactoryWorkflowUnitReading {
            contract: FACTORY_WORKFLOW_UNIT_READING_CONTRACT.into(),
            provenance: FactoryWorkflowUnitProvenance {
                owner: FACTORY_NATIVE_OWNER.into(),
                build_state_revision: self.build.revision(),
                source_ref: workflow.source.reference.clone(),
                source_revision: workflow.source.revision.clone(),
                source_digest: workflow.source.digest.clone(),
                identity_algorithm: workflow.identity_algorithm.clone(),
            },
            project_ref: self.build.project().reference().clone(),
            workflow_unit_ref: unit.reference.clone(),
            locator: summary.locator,
            key: unit.key.clone(),
            workflow_key: workflow.workflow_key.clone(),
            developmental_concern: unit.developmental_concern.clone(),
            subject_ref: unit.subject_ref.clone(),
            basis_revision: unit.basis_revision.clone(),
            required_difference: unit.required_difference.clone(),
            required_return: FactoryWorkflowUnitReturnRequirement {
                contract: unit.return_contract.clone(),
                address: unit.return_address.clone(),
            },
            required_verification: unit.verification_obligations.clone(),
            agent_requirements: unit.agent_requirements.clone(),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            dependencies: unit.dependencies.clone(),
            independence_from: unit.independence_from.clone(),
            barrier_relations: barrier_relations(workflow, unit),
            nesting: nesting_relations(workflow, unit),
            permitted_effects: unit.permitted_effects.clone(),
            stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(),
            current_correlation: summary.current_correlation,
        })
    }

    /// Read one Factory execution correlation as task-level telemetry. Values
    /// owned by Actuation, Central, AIKit or Workcell are never reconstructed:
    /// the reading carries their exact refs/revisions and explicit availability.
    pub fn execution_telemetry_reading(
        &self,
        telemetry_ref: &Ref,
    ) -> Result<FactoryExecutionTelemetryReading, FactoryDevelopmentalReadError> {
        let correlation = self
            .execution_correlations
            .iter()
            .find(|correlation| &correlation.telemetry_ref == telemetry_ref)
            .ok_or_else(|| {
                FactoryDevelopmentalReadError::ExecutionTelemetryNotFound(telemetry_ref.to_string())
            })?;
        let selection = FactoryBuildSelection {
            project_ref: self.build.project().reference().clone(),
            run_ref: correlation.run_ref.clone(),
        };
        let snapshot = FactoryBuildViewProvider.snapshot(&self.build, &selection)?;
        let agency = snapshot
            .view
            .agencies
            .iter()
            .find(|agency| agency.agency_ref == correlation.agency_ref)
            .ok_or_else(|| FactoryDevelopmentalReadError::AgencyNotFound {
                run_ref: correlation.run_ref.to_string(),
                agency_ref: correlation.agency_ref.clone(),
            })?;
        let execution = snapshot
            .view
            .executions
            .iter()
            .find(|execution| execution.execution_ref == correlation.execution_ref)
            .ok_or_else(|| FactoryDevelopmentalReadError::ExecutionNotFound {
                run_ref: correlation.run_ref.to_string(),
                execution_ref: correlation.execution_ref.clone(),
            })?;
        let evidence_refs = snapshot
            .view
            .evidence
            .iter()
            .filter(|evidence| {
                evidence.producing_execution_ref.as_deref()
                    == Some(correlation.execution_ref.as_str())
            })
            .map(|evidence| evidence.evidence_ref.clone())
            .collect();

        Ok(FactoryExecutionTelemetryReading {
            contract: FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT.into(),
            provenance: FactoryExecutionTelemetryProvenance {
                owner: FACTORY_NATIVE_OWNER.into(),
                factory_state_revision: self.build.revision(),
                correlation_ref: correlation.correlation_ref.clone(),
                source: "Factory execution correlation over owner-native refs".into(),
            },
            telemetry_ref: correlation.telemetry_ref.clone(),
            project_ref: self.build.project().reference().clone(),
            run_ref: correlation.run_ref.clone(),
            workflow_unit_ref: correlation.workflow_unit_ref.clone(),
            execution_ref: correlation.execution_ref.clone(),
            condition: FactoryExecutionCondition {
                agent_ref: agency.agent_ref.clone(),
                agency_ref: agency.agency_ref.clone(),
                actuation_ref: agency.actuation_ref.clone(),
                carrier: correlation.carrier.clone(),
                harness_ref: execution.harness_ref.clone(),
                harness_composition_ref: execution.harness_composition_ref.clone(),
                agent_session_ref: execution.agent_session_ref.clone(),
                session_space_ref: execution.session_space_ref.clone(),
                surface_refs: execution.surface_refs.clone(),
                workcell_binding_refs: execution.workcell_binding_refs.clone(),
            },
            temporal: correlation.temporal.clone(),
            model_usage: correlation.model_usage.clone(),
            material_usage: correlation.material_usage.clone(),
            handoff: correlation.handoff.clone(),
            return_state: FactoryExecutionReturnState {
                agency_return_ref: agency.return_ref.clone(),
                agency_return_state: agency.return_state.clone(),
                evidence_refs,
            },
        })
    }

    fn validate_execution_correlations(
        &self,
        workflows: &[CompiledWorkflow],
    ) -> Result<(), FactoryDevelopmentalReadError> {
        let unit_refs = workflows
            .iter()
            .flat_map(|workflow| workflow.units.values().map(|unit| unit.reference.clone()))
            .collect::<BTreeSet<_>>();
        let mut correlation_refs = BTreeSet::new();
        let mut telemetry_refs = BTreeSet::new();
        for correlation in &self.execution_correlations {
            correlation.validate()?;
            if !correlation_refs.insert(correlation.correlation_ref.clone()) {
                return Err(
                    FactoryDevelopmentalReadError::DuplicateExecutionCorrelation(
                        correlation.correlation_ref.to_string(),
                    ),
                );
            }
            if !telemetry_refs.insert(correlation.telemetry_ref.clone()) {
                return Err(FactoryDevelopmentalReadError::DuplicateExecutionTelemetry(
                    correlation.telemetry_ref.to_string(),
                ));
            }
            if !unit_refs.contains(&correlation.workflow_unit_ref) {
                return Err(FactoryDevelopmentalReadError::WorkflowUnitNotFound(
                    correlation.workflow_unit_ref.to_string(),
                ));
            }
            let selection = FactoryBuildSelection {
                project_ref: self.build.project().reference().clone(),
                run_ref: correlation.run_ref.clone(),
            };
            let snapshot = FactoryBuildViewProvider.snapshot(&self.build, &selection)?;
            let run = self.build.run(&correlation.run_ref).ok_or_else(|| {
                FactoryDevelopmentalReadError::RunNotFound(correlation.run_ref.to_string())
            })?;
            if !run.map().nodes().values().any(|node| {
                node.semantic_ref.as_ref() == Some(correlation.workflow_unit_ref.as_ref())
            }) {
                return Err(FactoryDevelopmentalReadError::ExecutionUnitNotInRun {
                    run_ref: correlation.run_ref.to_string(),
                    workflow_unit_ref: correlation.workflow_unit_ref.to_string(),
                });
            }
            let agency = snapshot
                .view
                .agencies
                .iter()
                .find(|agency| agency.agency_ref == correlation.agency_ref)
                .ok_or_else(|| FactoryDevelopmentalReadError::AgencyNotFound {
                    run_ref: correlation.run_ref.to_string(),
                    agency_ref: correlation.agency_ref.clone(),
                })?;
            let execution = snapshot
                .view
                .executions
                .iter()
                .find(|execution| execution.execution_ref == correlation.execution_ref)
                .ok_or_else(|| FactoryDevelopmentalReadError::ExecutionNotFound {
                    run_ref: correlation.run_ref.to_string(),
                    execution_ref: correlation.execution_ref.clone(),
                })?;
            if execution.agency_ref.as_deref() != Some(correlation.agency_ref.as_str()) {
                return Err(FactoryDevelopmentalReadError::ExecutionAgencyMismatch {
                    execution_ref: correlation.execution_ref.clone(),
                    agency_ref: correlation.agency_ref.clone(),
                });
            }
            if execution
                .agent_ref
                .as_deref()
                .is_some_and(|agent_ref| agent_ref != agency.agent_ref)
            {
                return Err(FactoryDevelopmentalReadError::ExecutionAgentMismatch {
                    execution_ref: correlation.execution_ref.clone(),
                    agent_ref: agency.agent_ref.clone(),
                });
            }
            if let Some(handoff) = &correlation.handoff {
                let source = snapshot
                    .view
                    .agencies
                    .iter()
                    .find(|agency| agency.agency_ref == handoff.from_agency_ref)
                    .ok_or_else(|| FactoryDevelopmentalReadError::AgencyNotFound {
                        run_ref: correlation.run_ref.to_string(),
                        agency_ref: handoff.from_agency_ref.clone(),
                    })?;
                if source.return_ref.as_deref() != Some(handoff.return_ref.as_str()) {
                    return Err(FactoryDevelopmentalReadError::HandoffReturnMismatch {
                        agency_ref: handoff.from_agency_ref.clone(),
                        return_ref: handoff.return_ref.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn workflow_unit_summary(
        &self,
        workflow: &CompiledWorkflow,
        unit: &CompiledWorkflowUnit,
        run_filter: Option<&RunRef>,
    ) -> FactoryWorkflowUnitSummary {
        FactoryWorkflowUnitSummary {
            workflow_unit_ref: unit.reference.clone(),
            locator: workflow_unit_locator(workflow, unit),
            key: unit.key.clone(),
            workflow_key: workflow.workflow_key.clone(),
            source_ref: workflow.source.reference.clone(),
            source_revision: workflow.source.revision.clone(),
            source_digest: workflow.source.digest.clone(),
            subject_ref: unit.subject_ref.clone(),
            basis_revision: unit.basis_revision.clone(),
            current_correlation: self.workflow_unit_correlation(&unit.reference, run_filter),
        }
    }

    fn workflow_unit_correlation(
        &self,
        workflow_unit_ref: &WorkflowUnitRef,
        run_filter: Option<&RunRef>,
    ) -> FactoryWorkflowUnitCurrentCorrelation {
        let candidate_runs = if let Some(run_ref) = run_filter {
            vec![run_ref.clone()]
        } else {
            self.journeys
                .iter()
                .flat_map(|journey| journey.runs.iter().map(|link| link.run_ref.clone()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        let mut runs = candidate_runs
            .into_iter()
            .filter_map(|run_ref| {
                let run = self.build.run(&run_ref)?;
                let present = run
                    .map()
                    .nodes()
                    .values()
                    .any(|node| node.semantic_ref.as_ref() == Some(workflow_unit_ref.as_ref()));
                present.then(|| FactoryWorkflowUnitRunCorrelation {
                    run_ref: run_ref.clone(),
                    run_revision: run.revision(),
                    run_map_address: RunMapAddress::for_run(run_ref.clone()),
                    topology_revision: run.map().topology_revision(),
                    journey_refs: self
                        .journeys
                        .iter()
                        .filter(|journey| journey.runs.iter().any(|link| link.run_ref == run_ref))
                        .map(|journey| journey.journey_ref.clone())
                        .collect(),
                })
            })
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| left.run_ref.cmp(&right.run_ref));
        let run_refs = runs
            .iter()
            .map(|correlation| correlation.run_ref.clone())
            .collect::<BTreeSet<_>>();
        let lower = self
            .execution_correlations
            .iter()
            .filter(|correlation| {
                &correlation.workflow_unit_ref == workflow_unit_ref
                    && run_refs.contains(&correlation.run_ref)
            })
            .collect::<Vec<_>>();
        let values = |select: fn(&FactoryExecutionCorrelation) -> String| {
            let values = lower
                .iter()
                .map(|correlation| select(correlation))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            (!values.is_empty()).then_some(values)
        };
        FactoryWorkflowUnitCurrentCorrelation {
            run_journey_status: if runs.is_empty() {
                FactoryWorkflowUnitCorrelationStatus::Absent
            } else {
                FactoryWorkflowUnitCorrelationStatus::OwnerEstablished
            },
            run_journey_basis: "RunMap Work node semanticRef plus Factory Journey run link".into(),
            runs,
            lower_correlation_status: if lower.is_empty() {
                FactoryLowerCorrelationStatus::NotOwnerEstablished
            } else {
                FactoryLowerCorrelationStatus::OwnerEstablished
            },
            agency_refs: values(|correlation| correlation.agency_ref.clone()),
            execution_refs: values(|correlation| correlation.execution_ref.clone()),
            telemetry_refs: values(|correlation| correlation.telemetry_ref.to_string()),
        }
    }

    fn compile_workflows(&self) -> Result<Vec<CompiledWorkflow>, FactoryDevelopmentalReadError> {
        let mut identities = BTreeSet::new();
        let mut unit_refs = BTreeSet::new();
        let mut compiled_workflows = Vec::new();
        for source in &self.workflow_sources {
            let workflow = compile_workflow(source.clone()).map_err(|error| {
                FactoryDevelopmentalReadError::InvalidWorkflowSource {
                    source_ref: source.source.reference.to_string(),
                    detail: error.to_string(),
                }
            })?;
            let identity = workflow_identity(&workflow);
            if !identities.insert(identity.clone()) {
                return Err(FactoryDevelopmentalReadError::DuplicateCompiledWorkflow(
                    format!(
                        "{}@{}:{}#{}",
                        identity.0, identity.1, identity.2, identity.3
                    ),
                ));
            }
            for unit in workflow.units.values() {
                if !unit_refs.insert(unit.reference.clone()) {
                    return Err(FactoryDevelopmentalReadError::DuplicateWorkflowUnit(
                        unit.reference.to_string(),
                    ));
                }
            }
            compiled_workflows.push(workflow);
        }
        compiled_workflows.sort_by_key(workflow_identity);
        Ok(compiled_workflows)
    }

    fn ensure_project(
        &self,
        project_ref: &ProjectRef,
    ) -> Result<(), FactoryDevelopmentalReadError> {
        if self.build.project().reference() != project_ref {
            return Err(FactoryDevelopmentalReadError::ProjectNotFound(
                project_ref.to_string(),
            ));
        }
        Ok(())
    }

    fn ensure_run(&self, run_ref: &RunRef) -> Result<(), FactoryDevelopmentalReadError> {
        if self.build.run(run_ref).is_none() {
            return Err(FactoryDevelopmentalReadError::RunNotFound(
                run_ref.to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredFactoryDevelopmentalState {
    schema: String,
    state: FactoryDevelopmentalState,
}

/// First-party local application boundary for public developmental reads and
/// canonical Factory Action invocation. The file is provider state, never a
/// consumer-owned Journey or Run store.
#[derive(Debug)]
pub struct FactoryDevelopmentalFileProvider {
    path: PathBuf,
    state: FactoryDevelopmentalState,
}

impl FactoryDevelopmentalFileProvider {
    pub fn create(
        path: impl Into<PathBuf>,
        state: FactoryDevelopmentalState,
    ) -> Result<Self, FactoryDevelopmentalProviderError> {
        state.validate()?;
        let provider = Self {
            path: path.into(),
            state,
        };
        provider.persist()?;
        Ok(provider)
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self, FactoryDevelopmentalProviderError> {
        let path = path.into();
        let stored: StoredFactoryDevelopmentalState = serde_json::from_slice(&fs::read(&path)?)?;
        if stored.schema != FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER {
            return Err(
                FactoryDevelopmentalProviderError::UnsupportedProviderSchema(stored.schema),
            );
        }
        stored.state.validate()?;
        Ok(Self {
            path,
            state: stored.state,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn project_reading(
        &self,
        project_ref: &ProjectRef,
    ) -> Result<FactoryProjectReading, FactoryDevelopmentalProviderError> {
        Ok(self.state.project_reading(project_ref)?)
    }

    pub fn journey_reading(
        &self,
        journey_ref: &JourneyRef,
    ) -> Result<FactoryJourneyReading, FactoryDevelopmentalProviderError> {
        Ok(self.state.journey_reading(journey_ref)?)
    }

    pub fn run_reading(
        &self,
        run_ref: &RunRef,
    ) -> Result<FactoryRunReading, FactoryDevelopmentalProviderError> {
        Ok(self.state.run_reading(run_ref)?)
    }

    pub fn workflow_units_reading(
        &self,
        run_ref: Option<&RunRef>,
    ) -> Result<FactoryWorkflowUnitListReading, FactoryDevelopmentalProviderError> {
        Ok(self.state.workflow_units_reading(run_ref)?)
    }

    pub fn workflow_unit_reading(
        &self,
        workflow_unit_ref: &WorkflowUnitRef,
        run_ref: Option<&RunRef>,
    ) -> Result<FactoryWorkflowUnitReading, FactoryDevelopmentalProviderError> {
        Ok(self
            .state
            .workflow_unit_reading(workflow_unit_ref, run_ref)?)
    }

    pub fn execution_telemetry_reading(
        &self,
        telemetry_ref: &Ref,
    ) -> Result<FactoryExecutionTelemetryReading, FactoryDevelopmentalProviderError> {
        Ok(self.state.execution_telemetry_reading(telemetry_ref)?)
    }

    pub fn execute_action(
        &mut self,
        invocation: &FactoryActionInvocation,
        authority: &FactoryActionAuthority,
    ) -> Result<FactoryActionReceipt, FactoryDevelopmentalProviderError> {
        let receipt =
            FactoryActionExecutor.execute(&mut self.state.build, invocation, authority)?;
        self.persist()?;
        Ok(receipt)
    }

    fn persist(&self) -> Result<(), FactoryDevelopmentalProviderError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let stored = StoredFactoryDevelopmentalState {
            schema: FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER.into(),
            state: self.state.clone(),
        };
        let temporary = self.path.with_file_name(format!(
            ".{}.tmp-{}",
            self.path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("factory-developmental-state.json"),
            std::process::id()
        ));
        fs::write(&temporary, serde_json::to_vec_pretty(&stored)?)?;
        fs::rename(&temporary, &self.path).inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })?;
        Ok(())
    }
}

impl FactoryProjectedActionProvider for FactoryDevelopmentalFileProvider {
    type Error = FactoryDevelopmentalProviderError;

    fn execute_projected_action(
        &mut self,
        invocation: &FactoryActionInvocation,
        authority: &FactoryActionAuthority,
    ) -> Result<FactoryActionReceipt, Self::Error> {
        self.execute_action(invocation, authority)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryReadingProvenance {
    pub owner: String,
    pub factory_state_revision: Revision,
    pub subject_revision: Revision,
    pub source: String,
}

impl FactoryReadingProvenance {
    fn new(factory_state_revision: Revision, subject_revision: Revision, source: &str) -> Self {
        Self {
            owner: FACTORY_NATIVE_OWNER.into(),
            factory_state_revision,
            subject_revision,
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryProjectReading {
    pub contract: String,
    pub provenance: FactoryReadingProvenance,
    pub project_ref: ProjectRef,
    pub project_revision: Revision,
    pub journeys: Vec<FactoryJourneySummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryJourneySummary {
    pub journey_ref: JourneyRef,
    pub revision: Revision,
    pub status: JourneyStatus,
    pub frontier: String,
    pub run_refs: Vec<RunRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryJourneyReading {
    pub contract: String,
    pub provenance: FactoryReadingProvenance,
    pub journey_ref: JourneyRef,
    pub revision: Revision,
    pub project_ref: ProjectRef,
    pub related_projects: Vec<ProjectRef>,
    pub commission: JourneyCommission,
    pub status: JourneyStatus,
    pub frontier: String,
    pub participants: Vec<JourneyParticipant>,
    pub flow_refs: Vec<String>,
    pub run_refs: Vec<RunRef>,
    pub agent_session_refs: Vec<String>,
    pub activity_refs: Vec<String>,
    pub returns: Vec<JourneyReturn>,
    pub recognitions: Vec<JourneyRecognitionLink>,
    pub material_context_refs: Vec<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryRunReading {
    pub contract: String,
    pub provenance: FactoryReadingProvenance,
    pub run_ref: RunRef,
    pub revision: Revision,
    pub project_ref: ProjectRef,
    pub owning_journey_refs: Vec<JourneyRef>,
    pub lifecycle: RunLifecycle,
    pub destination: String,
    pub run_map: RunMap,
    pub thought_available: bool,
    pub thought_refs: Vec<String>,
    pub agencies: Vec<AgencyRecord>,
    pub executions: Vec<ExecutionRecord>,
    pub evidence: Vec<EvidenceRecord>,
    pub candidates: Vec<CandidateRecord>,
    pub human_requests: Vec<HumanRequestRecord>,
    pub actions: Vec<FactoryActionDescriptor>,
}

/// Factory-owned relation from one stable WorkflowUnit to one concrete
/// Execution. All non-Factory actuality remains linked to its native owner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryExecutionCorrelation {
    pub correlation_ref: Ref,
    pub telemetry_ref: Ref,
    pub run_ref: RunRef,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub execution_ref: String,
    pub agency_ref: String,
    pub carrier: FactoryAgencyCarrier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<FactoryAgencyHandoff>,
    pub temporal: FactoryTemporalCorrelation,
    pub model_usage: FactoryOwnerTelemetryLink,
    pub material_usage: FactoryOwnerTelemetryLink,
}

impl FactoryExecutionCorrelation {
    fn validate(&self) -> Result<(), FactoryDevelopmentalReadError> {
        if self.correlation_ref.kind() != "execution-correlation" {
            return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                correlation_ref: self.correlation_ref.to_string(),
                detail: "correlationRef must have kind execution-correlation".into(),
            });
        }
        if self.telemetry_ref.kind() != "telemetry" {
            return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                correlation_ref: self.correlation_ref.to_string(),
                detail: "telemetryRef must have kind telemetry".into(),
            });
        }
        require_correlation_text(&self.execution_ref, "executionRef", &self.correlation_ref)?;
        require_correlation_text(&self.agency_ref, "agencyRef", &self.correlation_ref)?;
        self.carrier.validate(&self.correlation_ref)?;
        if let Some(handoff) = &self.handoff {
            handoff.validate(&self.correlation_ref)?;
            if handoff.from_agency_ref == self.agency_ref {
                return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                    correlation_ref: self.correlation_ref.to_string(),
                    detail: "handoff must cross two distinct Agencies".into(),
                });
            }
        }
        self.temporal.validate(&self.correlation_ref)?;
        self.model_usage.validate(
            FactoryTelemetryOwner::Actuation,
            "modelUsage",
            &self.correlation_ref,
        )?;
        self.material_usage.validate(
            FactoryTelemetryOwner::Workcell,
            "materialUsage",
            &self.correlation_ref,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAgencyCarrier {
    pub mechanism: FactoryAgencyCarrierMechanism,
    /// Native provider/session/team/carrier identity; never an AgentRef.
    pub carrier_ref: String,
    /// Actuation-owned determination which established the situated Agency.
    pub determination_ref: String,
    pub source: FactoryRevisionedOwnerRef,
}

impl FactoryAgencyCarrier {
    fn validate(&self, correlation_ref: &Ref) -> Result<(), FactoryDevelopmentalReadError> {
        require_correlation_text(&self.carrier_ref, "carrier.carrierRef", correlation_ref)?;
        require_correlation_text(
            &self.determination_ref,
            "carrier.determinationRef",
            correlation_ref,
        )?;
        if self.source.owner != FactoryTelemetryOwner::Actuation {
            return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                correlation_ref: correlation_ref.to_string(),
                detail: "carrier source must be owned by Actuation".into(),
            });
        }
        self.source.validate("carrier.source", correlation_ref)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryAgencyCarrierMechanism {
    NativeHarness,
    ProviderSubagent,
    ProviderTeam,
    Acp,
    A2a,
    AikitGateway,
    PersistentAgent,
    SharedField,
    SourceProvenAdapter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryAgencyHandoff {
    pub from_agency_ref: String,
    /// Actuation-owned delegation/authority evidence. A message ref is never
    /// accepted in this field.
    pub delegation_ref: String,
    pub return_ref: String,
    #[serde(default)]
    pub message_refs: Vec<String>,
    pub source: FactoryRevisionedOwnerRef,
}

impl FactoryAgencyHandoff {
    fn validate(&self, correlation_ref: &Ref) -> Result<(), FactoryDevelopmentalReadError> {
        require_correlation_text(
            &self.from_agency_ref,
            "handoff.fromAgencyRef",
            correlation_ref,
        )?;
        require_correlation_text(
            &self.delegation_ref,
            "handoff.delegationRef",
            correlation_ref,
        )?;
        require_correlation_text(&self.return_ref, "handoff.returnRef", correlation_ref)?;
        for message_ref in &self.message_refs {
            require_correlation_text(message_ref, "handoff.messageRefs", correlation_ref)?;
            if message_ref == &self.delegation_ref {
                return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                    correlation_ref: correlation_ref.to_string(),
                    detail: "message/address identity cannot also be delegation authority".into(),
                });
            }
        }
        if self.source.owner != FactoryTelemetryOwner::Actuation {
            return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                correlation_ref: correlation_ref.to_string(),
                detail: "handoff source must be owned by Actuation".into(),
            });
        }
        self.source.validate("handoff.source", correlation_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryTemporalCorrelation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<FactoryTemporalFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<FactoryTemporalFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<FactoryTemporalFact>,
    #[serde(default)]
    pub activity_refs: Vec<FactoryRevisionedOwnerRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<FactoryRevisionedOwnerRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_horizon: Option<FactoryRevisionedOwnerRef>,
    #[serde(default)]
    pub source_changes: Vec<FactoryRevisionedOwnerRef>,
    #[serde(default)]
    pub day_refs: Vec<FactoryRevisionedOwnerRef>,
}

impl FactoryTemporalCorrelation {
    fn validate(&self, correlation_ref: &Ref) -> Result<(), FactoryDevelopmentalReadError> {
        for (field, fact) in [
            ("temporal.started", self.started.as_ref()),
            ("temporal.updated", self.updated.as_ref()),
            ("temporal.completed", self.completed.as_ref()),
        ] {
            if let Some(fact) = fact {
                require_correlation_text(&fact.value, field, correlation_ref)?;
                fact.source.validate(field, correlation_ref)?;
            }
        }
        for (field, references, expected_owner) in [
            (
                "temporal.activityRefs",
                self.activity_refs.as_slice(),
                FactoryTelemetryOwner::Actuation,
            ),
            (
                "temporal.sourceChanges",
                self.source_changes.as_slice(),
                FactoryTelemetryOwner::Central,
            ),
            (
                "temporal.dayRefs",
                self.day_refs.as_slice(),
                FactoryTelemetryOwner::Central,
            ),
        ] {
            for reference in references {
                if reference.owner != expected_owner {
                    return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                        correlation_ref: correlation_ref.to_string(),
                        detail: format!("{field} must retain {expected_owner:?} ownership"),
                    });
                }
                reference.validate(field, correlation_ref)?;
            }
        }
        for (field, reference) in [
            ("temporal.flow", self.flow.as_ref()),
            ("temporal.changeHorizon", self.change_horizon.as_ref()),
        ] {
            if let Some(reference) = reference {
                if reference.owner != FactoryTelemetryOwner::Central {
                    return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                        correlation_ref: correlation_ref.to_string(),
                        detail: format!("{field} must retain Central ownership"),
                    });
                }
                reference.validate(field, correlation_ref)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryTemporalFact {
    pub value: String,
    pub source: FactoryRevisionedOwnerRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryRevisionedOwnerRef {
    pub owner: FactoryTelemetryOwner,
    #[serde(rename = "ref")]
    pub reference: String,
    pub revision: String,
    pub standing: FactoryObservationStanding,
}

impl FactoryRevisionedOwnerRef {
    fn validate(
        &self,
        field: &str,
        correlation_ref: &Ref,
    ) -> Result<(), FactoryDevelopmentalReadError> {
        require_correlation_text(&self.reference, &format!("{field}.ref"), correlation_ref)?;
        require_correlation_text(
            &self.revision,
            &format!("{field}.revision"),
            correlation_ref,
        )
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryTelemetryOwner {
    Factory,
    Central,
    Actuation,
    Aikit,
    Workcell,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryObservationStanding {
    Observed,
    ProviderReported,
    NormalizedFromNative,
    Derived,
    Estimated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FactoryOwnerTelemetryLink {
    pub owner: FactoryTelemetryOwner,
    pub availability: FactoryTelemetryAvailability,
    #[serde(default)]
    pub observations: Vec<FactoryRevisionedOwnerRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl FactoryOwnerTelemetryLink {
    fn validate(
        &self,
        expected_owner: FactoryTelemetryOwner,
        field: &str,
        correlation_ref: &Ref,
    ) -> Result<(), FactoryDevelopmentalReadError> {
        if self.owner != expected_owner {
            return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                correlation_ref: correlation_ref.to_string(),
                detail: format!("{field} must retain {expected_owner:?} ownership"),
            });
        }
        match self.availability {
            FactoryTelemetryAvailability::Available if self.observations.is_empty() => {
                return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                    correlation_ref: correlation_ref.to_string(),
                    detail: format!("{field} is available but has no owner observation refs"),
                });
            }
            FactoryTelemetryAvailability::Unavailable
            | FactoryTelemetryAvailability::Unsupported
                if !self.observations.is_empty() =>
            {
                return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                    correlation_ref: correlation_ref.to_string(),
                    detail: format!("{field} cannot carry observations while unavailable"),
                });
            }
            _ => {}
        }
        if !matches!(self.availability, FactoryTelemetryAvailability::Available)
            && self
                .reason
                .as_deref()
                .is_none_or(|reason| reason.trim().is_empty())
        {
            return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                correlation_ref: correlation_ref.to_string(),
                detail: format!("{field} absence requires a reason"),
            });
        }
        for observation in &self.observations {
            if observation.owner != expected_owner {
                return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
                    correlation_ref: correlation_ref.to_string(),
                    detail: format!("{field} observation has the wrong native owner"),
                });
            }
            observation.validate(field, correlation_ref)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryTelemetryAvailability {
    Available,
    Unavailable,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryExecutionTelemetryReading {
    pub contract: String,
    pub provenance: FactoryExecutionTelemetryProvenance,
    pub telemetry_ref: Ref,
    pub project_ref: ProjectRef,
    pub run_ref: RunRef,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub execution_ref: String,
    pub condition: FactoryExecutionCondition,
    pub temporal: FactoryTemporalCorrelation,
    pub model_usage: FactoryOwnerTelemetryLink,
    pub material_usage: FactoryOwnerTelemetryLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<FactoryAgencyHandoff>,
    pub return_state: FactoryExecutionReturnState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryExecutionTelemetryProvenance {
    pub owner: String,
    pub factory_state_revision: Revision,
    pub correlation_ref: Ref,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryExecutionCondition {
    pub agent_ref: String,
    pub agency_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actuation_ref: Option<String>,
    pub carrier: FactoryAgencyCarrier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_composition_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_space_ref: Option<String>,
    #[serde(default)]
    pub surface_refs: Vec<String>,
    #[serde(default)]
    pub workcell_binding_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryExecutionReturnState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agency_return_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agency_return_state: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

fn require_correlation_text(
    value: &str,
    field: &str,
    correlation_ref: &Ref,
) -> Result<(), FactoryDevelopmentalReadError> {
    if value.trim().is_empty() {
        return Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation {
            correlation_ref: correlation_ref.to_string(),
            detail: format!("{field} cannot be empty"),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryActionDescriptor {
    pub action_ref: String,
    pub label: String,
    pub subject_kinds: Vec<String>,
    pub input_contract: String,
    pub result_contract: String,
    pub authority_owner: String,
    pub required_capability_ref: String,
    pub currently_applicable: bool,
    pub applicable_subject_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitListReading {
    pub contract: String,
    pub provenance: FactoryWorkflowUnitListProvenance,
    pub project_ref: ProjectRef,
    pub run_filter: Option<RunRef>,
    pub units: Vec<FactoryWorkflowUnitSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitListProvenance {
    pub owner: String,
    /// Revision of the independent Build state consulted only for current
    /// RunMap correlations. Workflow source attachment does not advance it.
    pub build_state_revision: Revision,
    pub source_bases: Vec<FactoryWorkflowSourceBasis>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowSourceBasis {
    pub source_ref: Ref,
    pub source_revision: String,
    pub source_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitSummary {
    pub workflow_unit_ref: WorkflowUnitRef,
    pub locator: String,
    pub key: String,
    pub workflow_key: String,
    pub source_ref: Ref,
    pub source_revision: String,
    pub source_digest: String,
    pub subject_ref: Ref,
    pub basis_revision: String,
    pub current_correlation: FactoryWorkflowUnitCurrentCorrelation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitReading {
    pub contract: String,
    pub provenance: FactoryWorkflowUnitProvenance,
    pub project_ref: ProjectRef,
    pub workflow_unit_ref: WorkflowUnitRef,
    pub locator: String,
    pub key: String,
    pub workflow_key: String,
    pub developmental_concern: String,
    pub subject_ref: Ref,
    pub basis_revision: String,
    pub required_difference: String,
    pub required_return: FactoryWorkflowUnitReturnRequirement,
    pub required_verification: BTreeSet<String>,
    pub agent_requirements: CompiledAgentRequirements,
    pub praxis_refs: BTreeSet<String>,
    pub capability_refs: BTreeSet<String>,
    pub dependencies: BTreeSet<WorkflowUnitRef>,
    pub independence_from: BTreeSet<WorkflowUnitRef>,
    pub barrier_relations: Vec<FactoryWorkflowBarrierRelation>,
    pub nesting: FactoryWorkflowUnitNestingRelations,
    pub permitted_effects: BTreeSet<String>,
    pub stop_conditions: String,
    pub escalation_conditions: String,
    pub current_correlation: FactoryWorkflowUnitCurrentCorrelation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitProvenance {
    pub owner: String,
    /// Revision of the independent Build state consulted only for current
    /// RunMap correlations. The source revision/digest is the unit change basis.
    pub build_state_revision: Revision,
    pub source_ref: Ref,
    pub source_revision: String,
    pub source_digest: String,
    pub identity_algorithm: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitReturnRequirement {
    pub contract: String,
    pub address: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowBarrierRelation {
    pub barrier_key: String,
    pub relation: FactoryWorkflowBarrierUnitRelation,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryWorkflowBarrierUnitRelation {
    AwaitedByBarrier,
    ReleasedByBarrier,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitNestingRelations {
    pub parent_unit_refs: BTreeSet<WorkflowUnitRef>,
    pub child_unit_refs: BTreeSet<WorkflowUnitRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitCurrentCorrelation {
    pub run_journey_status: FactoryWorkflowUnitCorrelationStatus,
    pub run_journey_basis: String,
    pub runs: Vec<FactoryWorkflowUnitRunCorrelation>,
    pub lower_correlation_status: FactoryLowerCorrelationStatus,
    pub agency_refs: Option<Vec<String>>,
    pub execution_refs: Option<Vec<String>>,
    pub telemetry_refs: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryWorkflowUnitCorrelationStatus {
    OwnerEstablished,
    Absent,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FactoryLowerCorrelationStatus {
    OwnerEstablished,
    NotOwnerEstablished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryWorkflowUnitRunCorrelation {
    pub run_ref: RunRef,
    pub run_revision: Revision,
    pub run_map_address: RunMapAddress,
    pub topology_revision: Revision,
    pub journey_refs: Vec<JourneyRef>,
}

fn workflow_identity(workflow: &CompiledWorkflow) -> (Ref, String, String, String) {
    (
        workflow.source.reference.clone(),
        workflow.source.revision.clone(),
        workflow.source.digest.clone(),
        workflow.workflow_key.clone(),
    )
}

fn workflow_source_identity(source: &WorkflowSource) -> (Ref, String, String, String) {
    (
        source.source.reference.clone(),
        source.source.revision.clone(),
        source.source.digest.clone(),
        source.workflow_key.clone(),
    )
}

fn workflow_unit_locator(workflow: &CompiledWorkflow, unit: &CompiledWorkflowUnit) -> String {
    format!(
        "{}#{}/{}",
        workflow.source.reference, workflow.workflow_key, unit.key
    )
}

fn barrier_relations(
    workflow: &CompiledWorkflow,
    unit: &CompiledWorkflowUnit,
) -> Vec<FactoryWorkflowBarrierRelation> {
    let mut relations = workflow
        .barriers
        .iter()
        .flat_map(|barrier| {
            let mut relations = Vec::new();
            if barrier.waits_for.contains(&unit.reference) {
                relations.push(FactoryWorkflowBarrierRelation {
                    barrier_key: barrier.key.clone(),
                    relation: FactoryWorkflowBarrierUnitRelation::AwaitedByBarrier,
                });
            }
            if barrier.releases.contains(&unit.reference) {
                relations.push(FactoryWorkflowBarrierRelation {
                    barrier_key: barrier.key.clone(),
                    relation: FactoryWorkflowBarrierUnitRelation::ReleasedByBarrier,
                });
            }
            relations
        })
        .collect::<Vec<_>>();
    relations.sort();
    relations
}

fn nesting_relations(
    workflow: &CompiledWorkflow,
    unit: &CompiledWorkflowUnit,
) -> FactoryWorkflowUnitNestingRelations {
    FactoryWorkflowUnitNestingRelations {
        parent_unit_refs: workflow
            .nesting
            .iter()
            .filter(|relation| relation.child == unit.reference)
            .map(|relation| relation.parent.clone())
            .collect(),
        child_unit_refs: workflow
            .nesting
            .iter()
            .filter(|relation| relation.parent == unit.reference)
            .map(|relation| relation.child.clone())
            .collect(),
    }
}

#[derive(Debug)]
pub enum FactoryDevelopmentalReadError {
    UnsupportedSchema(String),
    ProjectNotFound(String),
    JourneyNotFound(String),
    RunNotFound(String),
    WorkflowUnitNotFound(String),
    ExecutionTelemetryNotFound(String),
    DuplicateJourney(String),
    DuplicateCompiledWorkflow(String),
    DuplicateWorkflowUnit(String),
    DuplicateExecutionCorrelation(String),
    DuplicateExecutionTelemetry(String),
    AgencyNotFound {
        run_ref: String,
        agency_ref: String,
    },
    ExecutionNotFound {
        run_ref: String,
        execution_ref: String,
    },
    ExecutionUnitNotInRun {
        run_ref: String,
        workflow_unit_ref: String,
    },
    ExecutionAgencyMismatch {
        execution_ref: String,
        agency_ref: String,
    },
    ExecutionAgentMismatch {
        execution_ref: String,
        agent_ref: String,
    },
    HandoffReturnMismatch {
        agency_ref: String,
        return_ref: String,
    },
    InvalidExecutionCorrelation {
        correlation_ref: String,
        detail: String,
    },
    ProjectJourneyMismatch {
        project_ref: String,
        journey_ref: String,
    },
    ProjectRunMismatch {
        project_ref: String,
        run_ref: String,
    },
    InvalidJourney {
        journey_ref: String,
        detail: String,
    },
    InvalidWorkflowSource {
        source_ref: String,
        detail: String,
    },
    Build(FactoryBuildError),
}

impl From<FactoryBuildError> for FactoryDevelopmentalReadError {
    fn from(error: FactoryBuildError) -> Self {
        Self::Build(error)
    }
}

impl Display for FactoryDevelopmentalReadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Factory developmental read error: {self:?}")
    }
}

impl Error for FactoryDevelopmentalReadError {}

#[derive(Debug)]
pub enum FactoryDevelopmentalProviderError {
    Io(io::Error),
    Json(serde_json::Error),
    Read(FactoryDevelopmentalReadError),
    Build(FactoryBuildError),
    UnsupportedProviderSchema(String),
}

impl From<io::Error> for FactoryDevelopmentalProviderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for FactoryDevelopmentalProviderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<FactoryDevelopmentalReadError> for FactoryDevelopmentalProviderError {
    fn from(error: FactoryDevelopmentalReadError) -> Self {
        Self::Read(error)
    }
}

impl From<FactoryBuildError> for FactoryDevelopmentalProviderError {
    fn from(error: FactoryBuildError) -> Self {
        Self::Build(error)
    }
}

impl Display for FactoryDevelopmentalProviderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Factory developmental provider error: {self:?}")
    }
}

impl Error for FactoryDevelopmentalProviderError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{
        AgencyRecord, CandidateRecord, ExecutionRecord, FactoryActionAuthority,
        FactoryActionInvocation, REQUEST_MORE_EVIDENCE_ACTION_REF,
        REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
    };
    use crate::cli::execute_cli;
    use crate::core::run::{Project, Run};
    use crate::journey::JourneyRunLink;
    use crate::workflow::{
        compile_workflow, workflow_source_digest, WorkflowNestingSource, WorkflowSource,
        WORKFLOW_UNIT_IDENTITY_ALGORITHM,
    };

    const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE";
    const JOURNEY: &str = "journey:01ARZ3NDEKTSV4RRFFQ69G5FAD";
    const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA";
    const SCHEMA: &str = include_str!("../../contracts/factory/developmental-read.schema.json");
    const CASES: &str =
        include_str!("../../contracts/factory/fixtures/developmental-read-cases.json");
    const WORKFLOW_SOURCE: &str =
        include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");

    fn state() -> FactoryDevelopmentalState {
        let project_ref: ProjectRef = PROJECT.parse().unwrap();
        let run_ref: RunRef = RUN.parse().unwrap();
        let project = Project::new(project_ref.clone());
        let run = Run::new(
            run_ref.clone(),
            project_ref.clone(),
            "Publish the Factory developmental field",
            "factory",
        )
        .unwrap();
        let mut build = FactoryBuildState::new(project, run).unwrap();
        build
            .insert_candidate(CandidateRecord {
                run_ref: run_ref.clone(),
                candidate_ref: "candidate:publication-v1".into(),
                revision: 1,
                label: "Factory public reading contract".into(),
                status: "awaiting-evidence".into(),
                producing_execution_refs: vec!["execution:publication".into()],
                claim_refs: vec!["claim:public-contract".into()],
                evidence_refs: vec![],
                artifact_refs: vec!["artifact:developmental-read".into()],
                preview_ref: None,
                tradeoffs: vec![],
            })
            .unwrap();

        let mut journey = Journey::new(
            JOURNEY.parse().unwrap(),
            project_ref,
            JourneyCommission {
                purpose: "Publish real Journey and Run state from Factory.".into(),
                commission_ref: Some("commission:factory-199-a".into()),
                why_refs: vec!["issue:EpiLogos/agent-system-design#199".into()],
            },
            "Owner publication Part A",
            "2026-09-08T19:00:00Z",
        )
        .unwrap();
        journey
            .add_run(
                run_ref,
                vec!["git:947ce7a".into()],
                vec!["agent-session:publication".into()],
            )
            .unwrap();
        journey.correlate_activity("activity:publication").unwrap();
        journey
            .correlate_material_context("workcell:local")
            .unwrap();
        FactoryDevelopmentalState::new(build, vec![journey]).unwrap()
    }

    fn workflow_source() -> WorkflowSource {
        let mut source: WorkflowSource = serde_json::from_str(WORKFLOW_SOURCE).unwrap();
        source.nesting.push(WorkflowNestingSource {
            parent: "inspect-source".into(),
            child: "implement-compiler".into(),
        });
        source.source.digest = workflow_source_digest(&source).unwrap();
        source
    }

    fn state_with_workflow(applied_to_run: bool) -> FactoryDevelopmentalState {
        let source = workflow_source();
        let workflow = compile_workflow(source.clone()).unwrap();
        let mut state = state();
        if applied_to_run {
            let run_ref: RunRef = RUN.parse().unwrap();
            let authority = state.build.run_mutation_authority(&run_ref).unwrap();
            state
                .build
                .apply_run_topology_command(
                    &run_ref,
                    &authority,
                    workflow.topology_command(Revision::INITIAL),
                )
                .unwrap();
        }
        state.with_workflow_sources(vec![source]).unwrap()
    }

    fn owner_ref(
        owner: FactoryTelemetryOwner,
        reference: &str,
        revision: &str,
    ) -> FactoryRevisionedOwnerRef {
        FactoryRevisionedOwnerRef {
            owner,
            reference: reference.into(),
            revision: revision.into(),
            standing: FactoryObservationStanding::Observed,
        }
    }

    fn unavailable(owner: FactoryTelemetryOwner, reason: &str) -> FactoryOwnerTelemetryLink {
        FactoryOwnerTelemetryLink {
            owner,
            availability: FactoryTelemetryAvailability::Unavailable,
            observations: vec![],
            reason: Some(reason.into()),
        }
    }

    fn correlated_state() -> FactoryDevelopmentalState {
        let mut state = state_with_workflow(true);
        let workflow = state.compile_workflows().unwrap().remove(0);
        let inspect = workflow.unit("inspect-source").unwrap().reference.clone();
        let implement = workflow
            .unit("implement-compiler")
            .unwrap()
            .reference
            .clone();
        let run_ref: RunRef = RUN.parse().unwrap();

        state
            .build
            .insert_agency(AgencyRecord {
                run_ref: run_ref.clone(),
                agency_ref: "agency:parasakti/research".into(),
                agent_ref: "agent:parasakti".into(),
                label: "Parāśakti research Agency".into(),
                position: None,
                root_scope_ref: None,
                metagency_grant_refs: vec![],
                actuation_ref: Some("actuation:inspect-source".into()),
                return_ref: Some("return:inspect-source".into()),
                return_state: Some("returned".into()),
            })
            .unwrap();
        state
            .build
            .insert_agency(AgencyRecord {
                run_ref: run_ref.clone(),
                agency_ref: "agency:parasakti/implementation".into(),
                agent_ref: "agent:parasakti".into(),
                label: "Parāśakti implementation Agency".into(),
                position: None,
                root_scope_ref: None,
                metagency_grant_refs: vec![],
                actuation_ref: Some("actuation:implement-compiler".into()),
                return_ref: None,
                return_state: None,
            })
            .unwrap();
        state
            .build
            .insert_execution(ExecutionRecord {
                run_ref: run_ref.clone(),
                execution_ref: "execution:inspect-source".into(),
                status: "returned".into(),
                agency_ref: Some("agency:parasakti/research".into()),
                agent_ref: Some("agent:parasakti".into()),
                harness_ref: Some("harness:codex".into()),
                harness_composition_ref: Some("harness-composition:research".into()),
                agent_session_ref: Some("agent-session:research".into()),
                session_space_ref: Some("session-space:research".into()),
                surface_refs: vec!["surface:codex".into()],
                workcell_binding_refs: vec!["binding:local-research".into()],
                native_trajectory_ref: None,
            })
            .unwrap();
        state
            .build
            .insert_execution(ExecutionRecord {
                run_ref: run_ref.clone(),
                execution_ref: "execution:implement-compiler".into(),
                status: "running".into(),
                agency_ref: Some("agency:parasakti/implementation".into()),
                agent_ref: Some("agent:parasakti".into()),
                harness_ref: Some("harness:pi".into()),
                harness_composition_ref: Some("harness-composition:implementation".into()),
                agent_session_ref: Some("agent-session:implementation".into()),
                session_space_ref: Some("session-space:implementation".into()),
                surface_refs: vec!["surface:acp".into()],
                workcell_binding_refs: vec!["binding:lan-gpu".into()],
                native_trajectory_ref: None,
            })
            .unwrap();

        let actuation_source = owner_ref(
            FactoryTelemetryOwner::Actuation,
            "actuation-stream:factory-199",
            "b6ed67e57ef45c5b0434ad63e296a31ce56d2e99",
        );
        let central_flow = owner_ref(
            FactoryTelemetryOwner::Central,
            "flow:factory-agentic-w4",
            "flow-revision:12",
        );
        let unavailable_model = unavailable(
            FactoryTelemetryOwner::Actuation,
            "actuation.model-usage observation is not published at owner revision b6ed67e",
        );
        let unavailable_material = unavailable(
            FactoryTelemetryOwner::Workcell,
            "workcell resource-usage observation is not published at owner revision ab7540b",
        );
        let correlations = vec![
            FactoryExecutionCorrelation {
                correlation_ref: "execution-correlation:01ARZ3NDEKTSV4RRFFQ69G5FA1"
                    .parse()
                    .unwrap(),
                telemetry_ref: "telemetry:01ARZ3NDEKTSV4RRFFQ69G5FA2".parse().unwrap(),
                run_ref: run_ref.clone(),
                workflow_unit_ref: inspect,
                execution_ref: "execution:inspect-source".into(),
                agency_ref: "agency:parasakti/research".into(),
                carrier: FactoryAgencyCarrier {
                    mechanism: FactoryAgencyCarrierMechanism::NativeHarness,
                    carrier_ref: "harness-instance:codex-local".into(),
                    determination_ref: "determination:research".into(),
                    source: actuation_source.clone(),
                },
                handoff: None,
                temporal: FactoryTemporalCorrelation {
                    started: Some(FactoryTemporalFact {
                        value: "2026-09-07T23:55:00+01:00".into(),
                        source: actuation_source.clone(),
                    }),
                    updated: None,
                    completed: Some(FactoryTemporalFact {
                        value: "2026-09-08T00:05:00+01:00".into(),
                        source: actuation_source.clone(),
                    }),
                    activity_refs: vec![owner_ref(
                        FactoryTelemetryOwner::Actuation,
                        "activity:inspect-source",
                        "stream-cursor:41",
                    )],
                    flow: Some(central_flow.clone()),
                    change_horizon: Some(owner_ref(
                        FactoryTelemetryOwner::Central,
                        "change-horizon:factory-w4",
                        "cursor:18",
                    )),
                    source_changes: vec![owner_ref(
                        FactoryTelemetryOwner::Central,
                        "source-change:compiler-inspection",
                        "change:7",
                    )],
                    day_refs: vec![
                        owner_ref(
                            FactoryTelemetryOwner::Central,
                            "day:2026-09-07",
                            "day-revision:1",
                        ),
                        owner_ref(
                            FactoryTelemetryOwner::Central,
                            "day:2026-09-08",
                            "day-revision:1",
                        ),
                    ],
                },
                model_usage: unavailable_model.clone(),
                material_usage: unavailable_material.clone(),
            },
            FactoryExecutionCorrelation {
                correlation_ref: "execution-correlation:01ARZ3NDEKTSV4RRFFQ69G5FA3"
                    .parse()
                    .unwrap(),
                telemetry_ref: "telemetry:01ARZ3NDEKTSV4RRFFQ69G5FA4".parse().unwrap(),
                run_ref,
                workflow_unit_ref: implement,
                execution_ref: "execution:implement-compiler".into(),
                agency_ref: "agency:parasakti/implementation".into(),
                carrier: FactoryAgencyCarrier {
                    mechanism: FactoryAgencyCarrierMechanism::Acp,
                    carrier_ref: "acp-session:implementation".into(),
                    determination_ref: "determination:implementation".into(),
                    source: actuation_source.clone(),
                },
                handoff: Some(FactoryAgencyHandoff {
                    from_agency_ref: "agency:parasakti/research".into(),
                    delegation_ref: "delegation:research-to-implementation".into(),
                    return_ref: "return:inspect-source".into(),
                    message_refs: vec!["message:handoff-context".into()],
                    source: actuation_source.clone(),
                }),
                temporal: FactoryTemporalCorrelation {
                    started: Some(FactoryTemporalFact {
                        value: "2026-09-08T00:06:00+01:00".into(),
                        source: actuation_source,
                    }),
                    updated: None,
                    completed: None,
                    activity_refs: vec![owner_ref(
                        FactoryTelemetryOwner::Actuation,
                        "activity:implement-compiler",
                        "stream-cursor:42",
                    )],
                    flow: Some(central_flow),
                    change_horizon: None,
                    source_changes: vec![],
                    day_refs: vec![owner_ref(
                        FactoryTelemetryOwner::Central,
                        "day:2026-09-08",
                        "day-revision:1",
                    )],
                },
                model_usage: unavailable_model,
                material_usage: unavailable_material,
            },
        ];
        state.with_execution_correlations(correlations).unwrap()
    }

    #[test]
    fn bounded_reads_publish_real_native_identity_revision_and_correlations() {
        let state = state();
        let project = state.project_reading(&PROJECT.parse().unwrap()).unwrap();
        assert_eq!(project.contract, FACTORY_PROJECT_READING_CONTRACT);
        assert_eq!(project.journeys[0].journey_ref.to_string(), JOURNEY);
        assert_eq!(project.journeys[0].run_refs[0].to_string(), RUN);

        let journey = state.journey_reading(&JOURNEY.parse().unwrap()).unwrap();
        assert_eq!(journey.contract, FACTORY_JOURNEY_READING_CONTRACT);
        assert_eq!(
            journey.commission.commission_ref.as_deref(),
            Some("commission:factory-199-a")
        );
        assert_eq!(journey.activity_refs, vec!["activity:publication"]);
        assert_eq!(journey.material_context_refs, vec!["workcell:local"]);

        let run = state.run_reading(&RUN.parse().unwrap()).unwrap();
        assert_eq!(run.contract, FACTORY_RUN_READING_CONTRACT);
        assert_eq!(run.owning_journey_refs[0].to_string(), JOURNEY);
        assert_eq!(run.run_map.run_ref().to_string(), RUN);
        assert_eq!(run.run_map.topology_revision(), Revision::INITIAL);
        assert!(!run.thought_available);
        assert_eq!(run.candidates[0].candidate_ref, "candidate:publication-v1");
        assert_eq!(run.actions[0].action_ref, REQUEST_MORE_EVIDENCE_ACTION_REF);
        assert!(run.actions[0].currently_applicable);
        assert_eq!(
            run.actions[0].applicable_subject_refs,
            vec!["candidate:publication-v1"]
        );
        assert_eq!(run.actions[0].authority_owner, FACTORY_NATIVE_OWNER);
        assert_eq!(run.provenance.owner, FACTORY_NATIVE_OWNER);
    }

    #[test]
    fn provider_persists_canonical_action_result_and_cli_reads_same_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("developmental.json");
        let mut provider = FactoryDevelopmentalFileProvider::create(&path, state()).unwrap();

        let before = provider.run_reading(&RUN.parse().unwrap()).unwrap();
        assert!(before.human_requests.is_empty());
        let receipt = provider
            .execute_action(
                &FactoryActionInvocation {
                    action_ref: REQUEST_MORE_EVIDENCE_ACTION_REF.into(),
                    subject_ref: "candidate:publication-v1".into(),
                    run_ref: RUN.parse().unwrap(),
                },
                &FactoryActionAuthority {
                    authority_ref: "authority:owner".into(),
                    native_owner: FACTORY_NATIVE_OWNER.into(),
                    capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
                    capability_granted: true,
                    action_authorised: true,
                },
            )
            .unwrap();
        assert_eq!(receipt.next_revision, receipt.previous_revision + 1);

        let reopened = FactoryDevelopmentalFileProvider::open(&path).unwrap();
        let after = reopened.run_reading(&RUN.parse().unwrap()).unwrap();
        assert_eq!(after.human_requests.len(), 1);

        let cli = execute_cli(
            &[
                "development".into(),
                "journey".into(),
                path.to_string_lossy().into_owned(),
                JOURNEY.into(),
                "--json".into(),
            ],
            None,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_str(&cli).unwrap();
        assert_eq!(value["contract"], FACTORY_JOURNEY_READING_CONTRACT);
        assert_eq!(value["journeyRef"], JOURNEY);
    }

    #[test]
    fn state_rejects_journey_links_to_nonexistent_native_run() {
        let mut state = state();
        state.journeys[0].runs.push(JourneyRunLink {
            run_ref: "run:01ARZ3NDEKTSV4RRFFQ69G5FAB".parse().unwrap(),
            journey_revision: Revision::INITIAL,
            basis_refs: vec!["git:947ce7a".into()],
            agent_session_refs: vec![],
        });
        assert!(matches!(
            state.validate(),
            Err(FactoryDevelopmentalReadError::RunNotFound(_))
        ));
    }

    #[test]
    fn workflow_unit_reads_publish_compiled_semantics_and_only_owner_established_correlations() {
        let state = state_with_workflow(true);
        let list = state
            .workflow_units_reading(Some(&RUN.parse().unwrap()))
            .unwrap();
        assert_eq!(list.contract, FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT);
        assert_eq!(list.units.len(), 4);
        assert!(list
            .units
            .windows(2)
            .all(|pair| pair[0].locator < pair[1].locator));

        let workflows = state.compile_workflows().unwrap();
        let workflow = &workflows[0];
        let unit = workflow.unit("implement-compiler").unwrap();
        let reading = state
            .workflow_unit_reading(&unit.reference, Some(&RUN.parse().unwrap()))
            .unwrap();
        assert_eq!(reading.contract, FACTORY_WORKFLOW_UNIT_READING_CONTRACT);
        assert_eq!(reading.workflow_unit_ref, unit.reference);
        assert_eq!(reading.provenance.source_ref, workflow.source.reference);
        assert_eq!(reading.provenance.source_revision, workflow.source.revision);
        assert_eq!(reading.provenance.source_digest, workflow.source.digest);
        assert_eq!(
            reading.provenance.identity_algorithm,
            WORKFLOW_UNIT_IDENTITY_ALGORITHM
        );
        assert_eq!(reading.key, "implement-compiler");
        assert_eq!(reading.subject_ref, unit.subject_ref);
        assert_eq!(reading.basis_revision, unit.basis_revision);
        assert_eq!(reading.required_difference, unit.required_difference);
        assert_eq!(reading.required_return.contract, unit.return_contract);
        assert_eq!(reading.required_return.address, unit.return_address);
        assert_eq!(reading.required_verification, unit.verification_obligations);
        assert_eq!(reading.agent_requirements, unit.agent_requirements);
        assert_eq!(reading.praxis_refs, unit.praxis_refs);
        assert_eq!(reading.capability_refs, unit.capability_refs);
        assert_eq!(reading.dependencies, unit.dependencies);
        assert_eq!(reading.independence_from, unit.independence_from);
        assert_eq!(
            reading.barrier_relations,
            vec![FactoryWorkflowBarrierRelation {
                barrier_key: "implementation-reviewed".into(),
                relation: FactoryWorkflowBarrierUnitRelation::AwaitedByBarrier,
            }]
        );
        assert_eq!(
            reading.nesting.parent_unit_refs,
            BTreeSet::from([workflow.unit("inspect-source").unwrap().reference.clone()])
        );
        assert_eq!(
            reading.current_correlation.run_journey_status,
            FactoryWorkflowUnitCorrelationStatus::OwnerEstablished
        );
        assert_eq!(reading.current_correlation.runs[0].run_ref.to_string(), RUN);
        assert_eq!(
            reading.current_correlation.runs[0].journey_refs[0].to_string(),
            JOURNEY
        );
        assert_eq!(
            reading.current_correlation.lower_correlation_status,
            FactoryLowerCorrelationStatus::NotOwnerEstablished
        );
        assert_eq!(reading.current_correlation.agency_refs, None);
        assert_eq!(reading.current_correlation.execution_refs, None);
        assert_eq!(reading.current_correlation.telemetry_refs, None);
    }

    #[test]
    fn heterogeneous_agency_and_temporal_telemetry_preserve_owner_boundaries() {
        let state = correlated_state();
        let workflows = state.compile_workflows().unwrap();
        let inspect = workflows[0].unit("inspect-source").unwrap();
        let implement = workflows[0].unit("implement-compiler").unwrap();

        let inspect_reading = state
            .workflow_unit_reading(&inspect.reference, Some(&RUN.parse().unwrap()))
            .unwrap();
        let implement_reading = state
            .workflow_unit_reading(&implement.reference, Some(&RUN.parse().unwrap()))
            .unwrap();
        assert_eq!(
            inspect_reading.current_correlation.lower_correlation_status,
            FactoryLowerCorrelationStatus::OwnerEstablished
        );
        assert_eq!(
            inspect_reading.current_correlation.agency_refs,
            Some(vec!["agency:parasakti/research".into()])
        );
        assert_eq!(
            implement_reading.current_correlation.execution_refs,
            Some(vec!["execution:implement-compiler".into()])
        );

        let first = state
            .execution_telemetry_reading(&"telemetry:01ARZ3NDEKTSV4RRFFQ69G5FA2".parse().unwrap())
            .unwrap();
        let second = state
            .execution_telemetry_reading(&"telemetry:01ARZ3NDEKTSV4RRFFQ69G5FA4".parse().unwrap())
            .unwrap();
        assert_eq!(first.condition.agent_ref, second.condition.agent_ref);
        assert_ne!(first.condition.agency_ref, second.condition.agency_ref);
        assert_eq!(
            first.condition.carrier.mechanism,
            FactoryAgencyCarrierMechanism::NativeHarness
        );
        assert_eq!(
            second.condition.carrier.mechanism,
            FactoryAgencyCarrierMechanism::Acp
        );
        assert_eq!(first.temporal.day_refs.len(), 2);
        assert_eq!(first.run_ref, second.run_ref);
        assert_eq!(
            second.handoff.as_ref().unwrap().delegation_ref,
            "delegation:research-to-implementation"
        );
        assert_eq!(
            second.handoff.as_ref().unwrap().return_ref,
            "return:inspect-source"
        );
        assert_eq!(
            first.model_usage.availability,
            FactoryTelemetryAvailability::Unavailable
        );
        assert!(first.model_usage.observations.is_empty());
        assert_eq!(first.model_usage.owner, FactoryTelemetryOwner::Actuation);
        assert_eq!(
            first.material_usage.availability,
            FactoryTelemetryAvailability::Unavailable
        );
        assert!(first.material_usage.observations.is_empty());
        assert_eq!(first.material_usage.owner, FactoryTelemetryOwner::Workcell);
    }

    #[test]
    fn message_identity_cannot_satisfy_handoff_delegation() {
        let mut state = correlated_state();
        let handoff = state.execution_correlations[1].handoff.as_mut().unwrap();
        handoff.delegation_ref = handoff.message_refs[0].clone();
        assert!(matches!(
            state.validate(),
            Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation { detail, .. })
                if detail.contains("message/address identity")
        ));
    }

    #[test]
    fn telemetry_links_cannot_change_owner_or_attach_values_to_absence() {
        let mut wrong_owner = correlated_state();
        wrong_owner.execution_correlations[0].material_usage.owner =
            FactoryTelemetryOwner::Actuation;
        assert!(matches!(
            wrong_owner.validate(),
            Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation { detail, .. })
                if detail.contains("Workcell ownership")
        ));

        let mut counterfeit_observation = correlated_state();
        counterfeit_observation.execution_correlations[0]
            .model_usage
            .observations
            .push(owner_ref(
                FactoryTelemetryOwner::Actuation,
                "model-usage:counterfeit",
                "unknown",
            ));
        assert!(matches!(
            counterfeit_observation.validate(),
            Err(FactoryDevelopmentalReadError::InvalidExecutionCorrelation { detail, .. })
                if detail.contains("cannot carry observations while unavailable")
        ));
    }

    #[test]
    fn provider_reopens_and_cli_reads_the_same_execution_telemetry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("developmental.json");
        FactoryDevelopmentalFileProvider::create(&path, correlated_state()).unwrap();
        let output = execute_cli(
            &[
                "development".into(),
                "execution-telemetry".into(),
                path.to_string_lossy().into_owned(),
                "telemetry:01ARZ3NDEKTSV4RRFFQ69G5FA4".into(),
                "--json".into(),
            ],
            None,
        )
        .unwrap();
        let reading: FactoryExecutionTelemetryReading = serde_json::from_str(&output).unwrap();
        assert_eq!(
            reading.contract,
            FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT
        );
        assert_eq!(
            reading.condition.carrier.source.reference,
            "actuation-stream:factory-199"
        );
        assert_eq!(
            reading.condition.workcell_binding_refs,
            vec!["binding:lan-gpu"]
        );
        assert_eq!(
            reading.temporal.flow.as_ref().unwrap().reference,
            "flow:factory-agentic-w4"
        );
    }

    #[test]
    fn workflow_source_attachment_preserves_build_revision_and_publishes_exact_change_basis() {
        let source = workflow_source();
        let expected_build_revision = state().build.revision();
        let state = state().with_workflow_sources(vec![source.clone()]).unwrap();
        let list = state.workflow_units_reading(None).unwrap();

        assert_eq!(state.build.revision(), expected_build_revision);
        assert_eq!(
            list.provenance.build_state_revision,
            expected_build_revision
        );
        assert_eq!(list.provenance.source_bases.len(), 1);
        assert_eq!(
            list.provenance.source_bases[0],
            FactoryWorkflowSourceBasis {
                source_ref: source.source.reference.clone(),
                source_revision: source.source.revision.clone(),
                source_digest: source.source.digest.clone(),
            }
        );

        let unit_ref = state.compile_workflows().unwrap()[0]
            .unit("inspect-source")
            .unwrap()
            .reference
            .clone();
        let reading = state.workflow_unit_reading(&unit_ref, None).unwrap();
        assert_eq!(
            reading.provenance.build_state_revision,
            expected_build_revision
        );
        assert_eq!(reading.provenance.source_ref, source.source.reference);
        assert_eq!(reading.provenance.source_revision, source.source.revision);
        assert_eq!(reading.provenance.source_digest, source.source.digest);
    }

    #[test]
    fn workflow_unit_absence_and_cli_resolution_are_truthful_and_persistent() {
        let state = state_with_workflow(false);
        let unit_ref = state.compile_workflows().unwrap()[0]
            .unit("inspect-source")
            .unwrap()
            .reference
            .clone();
        let reading = state.workflow_unit_reading(&unit_ref, None).unwrap();
        assert_eq!(
            reading.current_correlation.run_journey_status,
            FactoryWorkflowUnitCorrelationStatus::Absent
        );
        assert!(reading.current_correlation.runs.is_empty());

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("developmental-workflow.json");
        FactoryDevelopmentalFileProvider::create(&path, state).unwrap();
        let list_output = execute_cli(
            &[
                "development".into(),
                "workflow-units".into(),
                path.to_string_lossy().into_owned(),
                "--json".into(),
            ],
            None,
        )
        .unwrap();
        let list: serde_json::Value = serde_json::from_str(&list_output).unwrap();
        assert_eq!(
            list["contract"],
            FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT
        );
        assert_eq!(list["units"].as_array().unwrap().len(), 4);

        let unit_output = execute_cli(
            &[
                "development".into(),
                "workflow-unit".into(),
                path.to_string_lossy().into_owned(),
                unit_ref.to_string(),
                RUN.into(),
                "--json".into(),
            ],
            None,
        )
        .unwrap();
        let resolved: serde_json::Value = serde_json::from_str(&unit_output).unwrap();
        assert_eq!(resolved["workflowUnitRef"], unit_ref.to_string());
        assert_eq!(resolved["currentCorrelation"]["runJourneyStatus"], "absent");
        assert_eq!(
            resolved["currentCorrelation"]["lowerCorrelationStatus"],
            "not-owner-established"
        );
        assert!(resolved["currentCorrelation"]["agencyRefs"].is_null());
        assert!(resolved["currentCorrelation"]["executionRefs"].is_null());
        assert!(resolved["currentCorrelation"]["telemetryRefs"].is_null());
    }

    #[test]
    fn provider_reopen_rejects_workflow_source_semantic_tampering_without_restamped_digest() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("tampered-developmental-workflow.json");
        FactoryDevelopmentalFileProvider::create(&path, state_with_workflow(false)).unwrap();

        let mut persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        persisted["state"]["workflowSources"][0]["units"][0]["requiredDifference"] =
            serde_json::Value::String("caller-tampered semantic difference".into());
        fs::write(&path, serde_json::to_vec_pretty(&persisted).unwrap()).unwrap();

        assert!(matches!(
            FactoryDevelopmentalFileProvider::open(&path),
            Err(FactoryDevelopmentalProviderError::Read(
                FactoryDevelopmentalReadError::InvalidWorkflowSource { .. }
            ))
        ));
    }

    #[test]
    fn cross_language_schema_and_cases_pin_version_owner_and_public_relations() {
        let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
        let cases: serde_json::Value = serde_json::from_str(CASES).unwrap();
        assert_eq!(schema["$id"], "factory.developmental-read.schema/v1");
        assert_eq!(cases["schema"], "factory.developmental-read-cases/v1");

        let state = state();
        let project =
            serde_json::to_value(state.project_reading(&PROJECT.parse().unwrap()).unwrap())
                .unwrap();
        let journey =
            serde_json::to_value(state.journey_reading(&JOURNEY.parse().unwrap()).unwrap())
                .unwrap();
        let run = serde_json::to_value(state.run_reading(&RUN.parse().unwrap()).unwrap()).unwrap();
        assert_eq!(project["contract"], cases["contracts"]["project"]);
        assert_eq!(journey["contract"], cases["contracts"]["journey"]);
        assert_eq!(run["contract"], cases["contracts"]["run"]);
        assert_eq!(run["provenance"]["owner"], cases["requiredOwner"]);
        for relation in cases["requiredRunRelations"].as_array().unwrap() {
            assert!(run.get(relation.as_str().unwrap()).is_some());
        }

        let workflow_state = state_with_workflow(true);
        let list =
            serde_json::to_value(workflow_state.workflow_units_reading(None).unwrap()).unwrap();
        let unit_ref = workflow_state.compile_workflows().unwrap()[0]
            .unit("implement-compiler")
            .unwrap()
            .reference
            .clone();
        let unit = serde_json::to_value(
            workflow_state
                .workflow_unit_reading(&unit_ref, None)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(list["contract"], cases["contracts"]["workflowUnitList"]);
        assert_eq!(unit["contract"], cases["contracts"]["workflowUnit"]);
        for field in cases["requiredWorkflowUnitFields"].as_array().unwrap() {
            assert!(unit.get(field.as_str().unwrap()).is_some());
        }
        for field in cases["workflowUnitProvenanceFields"].as_array().unwrap() {
            assert!(unit["provenance"].get(field.as_str().unwrap()).is_some());
        }
        for field in cases["workflowUnitListProvenanceFields"]
            .as_array()
            .unwrap()
        {
            assert!(list["provenance"].get(field.as_str().unwrap()).is_some());
        }
        assert_eq!(
            unit["currentCorrelation"]["runJourneyBasis"],
            cases["correlationLaw"]["runJourneyBasis"]
        );
        assert_eq!(
            unit["currentCorrelation"]["lowerCorrelationStatus"],
            cases["correlationLaw"]["lower"]
        );

        let correlated = correlated_state();
        let telemetry = serde_json::to_value(
            correlated
                .execution_telemetry_reading(
                    &"telemetry:01ARZ3NDEKTSV4RRFFQ69G5FA4".parse().unwrap(),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            telemetry["contract"],
            cases["contracts"]["executionTelemetry"]
        );
        assert_eq!(
            telemetry["modelUsage"]["owner"],
            cases["executionCorrelationLaw"]["modelUsageOwner"]
        );
        assert_eq!(
            telemetry["materialUsage"]["owner"],
            cases["executionCorrelationLaw"]["materialUsageOwner"]
        );
        assert_eq!(
            telemetry["modelUsage"]["availability"],
            cases["executionCorrelationLaw"]["unavailable"]
        );
        assert!(telemetry.get("tokens").is_none());
        assert!(telemetry.get("cost").is_none());
        assert!(telemetry.get("cpu").is_none());
        assert!(telemetry.get("memory").is_none());
    }
}
