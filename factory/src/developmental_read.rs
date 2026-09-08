//! Versioned public reads over Factory-owned Project, Journey and Run state.
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
use crate::core::identity::Revision;
use crate::core::run::{ProjectRef, RunLifecycle, RunMap, RunRef};
use crate::journey::{
    Journey, JourneyCommission, JourneyParticipant, JourneyRecognitionLink, JourneyRef,
    JourneyReturn, JourneyStatus,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const FACTORY_DEVELOPMENTAL_STATE_SCHEMA: &str = "factory.developmental-state/v1";
pub const FACTORY_PROJECT_READING_CONTRACT: &str = "factory.project-reading/v1";
pub const FACTORY_JOURNEY_READING_CONTRACT: &str = "factory.journey-reading/v1";
pub const FACTORY_RUN_READING_CONTRACT: &str = "factory.run-reading/v1";
pub const FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER: &str = "factory.developmental-local-provider/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryDevelopmentalState {
    pub schema: String,
    pub build: FactoryBuildState,
    pub journeys: Vec<Journey>,
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
        };
        state.validate()?;
        Ok(state)
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

#[derive(Debug)]
pub enum FactoryDevelopmentalReadError {
    UnsupportedSchema(String),
    ProjectNotFound(String),
    JourneyNotFound(String),
    RunNotFound(String),
    DuplicateJourney(String),
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
        CandidateRecord, FactoryActionAuthority, FactoryActionInvocation,
        REQUEST_MORE_EVIDENCE_ACTION_REF, REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
    };
    use crate::cli::execute_cli;
    use crate::core::run::{Project, Run};
    use crate::journey::JourneyRunLink;

    const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE";
    const JOURNEY: &str = "journey:01ARZ3NDEKTSV4RRFFQ69G5FAD";
    const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA";
    const SCHEMA: &str = include_str!("../../contracts/factory/developmental-read.schema.json");
    const CASES: &str =
        include_str!("../../contracts/factory/fixtures/developmental-read-cases.json");

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
    }
}
