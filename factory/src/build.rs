use crate::core::identity::Revision;
use crate::core::run::{
    CommandOutcome, NodeKind, NodeState, Project, ProjectRef, Run, RunContractError,
    RunMutationAuthority, RunRef, RunRegistry, RunThoughtCommand, RunThoughtOutcome,
    RunTopologyCommand,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const FACTORY_BUILD_VIEW_CONTRACT: &str = "factory.build-view/v1";
pub const FACTORY_BUILD_PROVIDER_CONTRACT: &str = "factory.build-view-provider/v1";
pub const FACTORY_NATIVE_OWNER: &str = "factory";
pub const REQUEST_MORE_EVIDENCE_ACTION_REF: &str = "action:01ARZ3NDEKTSV4RRFFQ69G5FAP";
pub const REQUEST_MORE_EVIDENCE_CAPABILITY_REF: &str = "capability/factory/request-evidence";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRecord {
    pub run_ref: RunRef,
    pub claim_ref: String,
    pub statement: String,
    pub status: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRecord {
    pub run_ref: RunRef,
    pub evidence_ref: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assessment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producing_execution_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRecord {
    pub run_ref: RunRef,
    pub candidate_ref: String,
    pub revision: u64,
    pub label: String,
    pub status: String,
    #[serde(default)]
    pub producing_execution_refs: Vec<String>,
    #[serde(default)]
    pub claim_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_ref: Option<String>,
    #[serde(default)]
    pub tradeoffs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanRequestRecord {
    pub run_ref: RunRef,
    pub human_request_ref: String,
    pub decision_ref: String,
    pub question: String,
    pub why_human: String,
    #[serde(default)]
    pub blocked_execution_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgencyRecord {
    pub run_ref: RunRef,
    pub agency_ref: String,
    pub agent_ref: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_scope_ref: Option<String>,
    #[serde(default)]
    pub metagency_grant_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actuation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub run_ref: RunRef,
    pub execution_ref: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agency_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_composition_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    /// Opaque AIKit-owned SessionSpace identity. Factory never interprets the
    /// target's activation or authority state from this ref.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_space_ref: Option<String>,
    #[serde(default)]
    pub surface_refs: Vec<String>,
    #[serde(default)]
    pub workcell_binding_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_trajectory_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryRecord {
    pub run_ref: RunRef,
    pub execution_ref: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildState {
    project: Project,
    runs: RunRegistry,
    revision: Revision,
    claims: BTreeMap<String, ClaimRecord>,
    evidence: BTreeMap<String, EvidenceRecord>,
    candidates: BTreeMap<String, CandidateRecord>,
    human_requests: BTreeMap<String, HumanRequestRecord>,
    agencies: BTreeMap<String, AgencyRecord>,
    executions: BTreeMap<String, ExecutionRecord>,
    trajectories: BTreeMap<String, TrajectoryRecord>,
}

impl FactoryBuildState {
    pub fn new(project: Project, run: Run) -> Result<Self, FactoryBuildError> {
        if run.project_ref() != project.reference() {
            return Err(FactoryBuildError::ProjectRunMismatch);
        }
        let mut runs = RunRegistry::default();
        runs.insert(run)?;
        Ok(Self {
            project,
            runs,
            revision: Revision::INITIAL,
            claims: BTreeMap::new(),
            evidence: BTreeMap::new(),
            candidates: BTreeMap::new(),
            human_requests: BTreeMap::new(),
            agencies: BTreeMap::new(),
            executions: BTreeMap::new(),
            trajectories: BTreeMap::new(),
        })
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn run(&self, run_ref: &RunRef) -> Option<&Run> {
        self.runs.get(run_ref)
    }

    pub fn run_mutation_authority(&self, run_ref: &RunRef) -> Option<RunMutationAuthority> {
        self.runs.get(run_ref).map(Run::mutation_authority)
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn apply_run_topology_command(
        &mut self,
        run_ref: &RunRef,
        authority: &RunMutationAuthority,
        command: RunTopologyCommand,
    ) -> Result<CommandOutcome, FactoryBuildError> {
        let run = self
            .runs
            .get_mut(run_ref)
            .ok_or_else(|| FactoryBuildError::RunNotFound(run_ref.to_string()))?;
        let outcome = run.apply_topology_command(authority, command)?;
        if matches!(outcome, CommandOutcome::Applied { .. }) {
            self.bump_revision()?;
        }
        Ok(outcome)
    }

    pub fn apply_run_thought_command(
        &mut self,
        run_ref: &RunRef,
        authority: &RunMutationAuthority,
        command: RunThoughtCommand,
    ) -> Result<RunThoughtOutcome, FactoryBuildError> {
        let run = self
            .runs
            .get_mut(run_ref)
            .ok_or_else(|| FactoryBuildError::RunNotFound(run_ref.to_string()))?;
        let outcome = run.apply_thought_command(authority, command)?;
        if matches!(outcome, RunThoughtOutcome::Applied { .. }) {
            self.bump_revision()?;
        }
        Ok(outcome)
    }

    pub fn insert_claim(&mut self, claim: ClaimRecord) -> Result<(), FactoryBuildError> {
        self.ensure_run(&claim.run_ref)?;
        insert_unique(&mut self.claims, claim.claim_ref.clone(), claim, "claim")?;
        self.bump_revision()
    }

    pub fn insert_evidence(&mut self, evidence: EvidenceRecord) -> Result<(), FactoryBuildError> {
        self.ensure_run(&evidence.run_ref)?;
        insert_unique(
            &mut self.evidence,
            evidence.evidence_ref.clone(),
            evidence,
            "evidence",
        )?;
        self.bump_revision()
    }

    /// Admit evidence about the exact current declared artifact scope of a
    /// native Candidate. The Candidate revision and Run come from this owner,
    /// never from a caller's claimed current state. This does not finish a Run,
    /// verify artifact quality or grant human Recognition.
    pub fn insert_artifact_evidence(
        &mut self,
        mut evidence: EvidenceRecord,
        snapshot: &crate::artifact_evidence::ArtifactSnapshot,
        assessment: &crate::artifact_evidence::SubjectState,
    ) -> Result<(), crate::artifact_evidence::ArtifactEvidenceError> {
        use crate::artifact_evidence::ArtifactEvidenceError;
        let subject = &snapshot.subject_state().subject_ref;
        let candidate = self.candidates.get(&subject.to_string()).ok_or_else(|| {
            ArtifactEvidenceError::Build(FactoryBuildError::SubjectNotFound(subject.to_string()))
        })?;
        if subject.kind() != "candidate" || candidate.run_ref != evidence.run_ref {
            return Err(ArtifactEvidenceError::Build(
                FactoryBuildError::SubjectRunMismatch,
            ));
        }
        let revision =
            Revision::new(candidate.revision).ok_or(ArtifactEvidenceError::StaleSubject)?;
        snapshot.validate_current(assessment, subject, revision)?;
        evidence.native_ref = Some(snapshot.subject_state().state_ref.clone());
        evidence.assessment = Some("current-declared-artifact-scope".into());
        self.insert_evidence(evidence)
            .map_err(ArtifactEvidenceError::Build)
    }

    pub fn insert_candidate(
        &mut self,
        candidate: CandidateRecord,
    ) -> Result<(), FactoryBuildError> {
        self.ensure_run(&candidate.run_ref)?;
        insert_unique(
            &mut self.candidates,
            candidate.candidate_ref.clone(),
            candidate,
            "candidate",
        )?;
        self.bump_revision()
    }

    pub fn insert_human_request(
        &mut self,
        request: HumanRequestRecord,
    ) -> Result<(), FactoryBuildError> {
        self.ensure_run(&request.run_ref)?;
        insert_unique(
            &mut self.human_requests,
            request.human_request_ref.clone(),
            request,
            "human request",
        )?;
        self.bump_revision()
    }

    pub fn insert_agency(&mut self, agency: AgencyRecord) -> Result<(), FactoryBuildError> {
        self.ensure_run(&agency.run_ref)?;
        insert_unique(
            &mut self.agencies,
            agency.agency_ref.clone(),
            agency,
            "agency",
        )?;
        self.bump_revision()
    }

    pub fn insert_execution(
        &mut self,
        execution: ExecutionRecord,
    ) -> Result<(), FactoryBuildError> {
        self.ensure_run(&execution.run_ref)?;
        insert_unique(
            &mut self.executions,
            execution.execution_ref.clone(),
            execution,
            "execution",
        )?;
        self.bump_revision()
    }

    pub fn insert_trajectory(
        &mut self,
        trajectory: TrajectoryRecord,
    ) -> Result<(), FactoryBuildError> {
        self.ensure_run(&trajectory.run_ref)?;
        insert_unique(
            &mut self.trajectories,
            trajectory.execution_ref.clone(),
            trajectory,
            "trajectory",
        )?;
        self.bump_revision()
    }

    fn request_more_evidence(
        &mut self,
        run_ref: &RunRef,
        candidate_ref: &str,
    ) -> Result<String, FactoryBuildError> {
        let candidate = self
            .candidates
            .get(candidate_ref)
            .ok_or_else(|| FactoryBuildError::SubjectNotFound(candidate_ref.to_owned()))?;
        if &candidate.run_ref != run_ref {
            return Err(FactoryBuildError::SubjectRunMismatch);
        }
        let human_request_ref = format!("human-request/request-evidence/{candidate_ref}");
        if self.human_requests.contains_key(&human_request_ref) {
            return Err(FactoryBuildError::ActionAlreadyApplied(human_request_ref));
        }
        let request = HumanRequestRecord {
            run_ref: run_ref.clone(),
            human_request_ref: human_request_ref.clone(),
            decision_ref: format!("decision/request-evidence/{candidate_ref}"),
            question: format!("What additional evidence should `{candidate_ref}` provide?"),
            why_human: "The Candidate needs additional evidence before recognition can proceed."
                .into(),
            blocked_execution_refs: Vec::new(),
            evidence_refs: candidate.evidence_refs.clone(),
        };
        self.human_requests
            .insert(human_request_ref.clone(), request);
        self.bump_revision()?;
        Ok(human_request_ref)
    }

    fn ensure_run(&self, run_ref: &RunRef) -> Result<(), FactoryBuildError> {
        self.runs
            .get(run_ref)
            .map(|_| ())
            .ok_or_else(|| FactoryBuildError::RunNotFound(run_ref.to_string()))
    }

    fn bump_revision(&mut self) -> Result<(), FactoryBuildError> {
        self.revision = self
            .revision
            .next()
            .ok_or(FactoryBuildError::RevisionOverflow)?;
        Ok(())
    }
}

fn insert_unique<T>(
    map: &mut BTreeMap<String, T>,
    key: String,
    value: T,
    label: &'static str,
) -> Result<(), FactoryBuildError> {
    if map.contains_key(&key) {
        return Err(FactoryBuildError::DuplicateRecord {
            kind: label,
            reference: key,
        });
    }
    map.insert(key, value);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryBuildSelection {
    pub project_ref: ProjectRef,
    pub run_ref: RunRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildSnapshot {
    pub contract: String,
    pub provider_contract: String,
    pub revision: u64,
    pub provenance: FactoryBuildProvenance,
    pub view: FactoryBuildView,
}

impl FactoryBuildSnapshot {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildProvenance {
    pub owner: String,
    pub factory_state_revision: u64,
    pub run_revision: u64,
    pub run_map_revision: u64,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryBuildView {
    pub project: ProjectView,
    pub run: RunView,
    pub frontier: FrontierView,
    pub claims: Vec<ClaimRecord>,
    pub evidence: Vec<EvidenceRecord>,
    pub candidates: Vec<CandidateRecord>,
    pub human_requests: Vec<HumanRequestRecord>,
    pub agencies: Vec<AgencyRecord>,
    pub executions: Vec<ExecutionRecord>,
    pub trajectories: Vec<Value>,
    pub actions: Vec<FactoryActionView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub project_ref: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    pub run_ref: String,
    pub run_map_ref: String,
    pub label: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontierView {
    pub subject_ref: String,
    pub title: String,
    pub mode: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryActionView {
    pub action_ref: String,
    pub label: String,
    pub subject_kinds: Vec<String>,
    pub required_capability_ref: String,
}

#[derive(Debug, Default)]
pub struct FactoryBuildViewProvider;

impl FactoryBuildViewProvider {
    pub fn snapshot(
        &self,
        state: &FactoryBuildState,
        selection: &FactoryBuildSelection,
    ) -> Result<FactoryBuildSnapshot, FactoryBuildError> {
        if state.project.reference() != &selection.project_ref {
            return Err(FactoryBuildError::ProjectNotFound(
                selection.project_ref.to_string(),
            ));
        }
        let run = state
            .runs
            .get(&selection.run_ref)
            .ok_or_else(|| FactoryBuildError::RunNotFound(selection.run_ref.to_string()))?;
        if run.project_ref() != state.project.reference() {
            return Err(FactoryBuildError::ProjectRunMismatch);
        }

        let view = FactoryBuildView {
            project: ProjectView {
                project_ref: state.project.reference().to_string(),
                label: state.project.reference().to_string(),
            },
            run: RunView {
                run_ref: run.reference().to_string(),
                run_map_ref: run.map().address().to_string(),
                label: run.destination().to_owned(),
                status: run_status(run),
            },
            frontier: materialise_frontier(run),
            claims: records_for_run(&state.claims, &selection.run_ref),
            evidence: records_for_run(&state.evidence, &selection.run_ref),
            candidates: records_for_run(&state.candidates, &selection.run_ref),
            human_requests: records_for_run(&state.human_requests, &selection.run_ref),
            agencies: records_for_run(&state.agencies, &selection.run_ref),
            executions: records_for_run(&state.executions, &selection.run_ref),
            trajectories: state
                .trajectories
                .values()
                .filter(|record| record.run_ref == selection.run_ref)
                .map(|record| record.value.clone())
                .collect(),
            actions: vec![FactoryActionView {
                action_ref: REQUEST_MORE_EVIDENCE_ACTION_REF.into(),
                label: "Request more evidence".into(),
                subject_kinds: vec!["candidate".into()],
                required_capability_ref: REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into(),
            }],
        };

        Ok(FactoryBuildSnapshot {
            contract: FACTORY_BUILD_VIEW_CONTRACT.into(),
            provider_contract: FACTORY_BUILD_PROVIDER_CONTRACT.into(),
            revision: state.revision.get(),
            provenance: FactoryBuildProvenance {
                owner: FACTORY_NATIVE_OWNER.into(),
                factory_state_revision: state.revision.get(),
                run_revision: run.revision().get(),
                run_map_revision: run.map().topology_revision().get(),
                source: "canonical FactoryBuildState + canonical Run/RunMap".into(),
            },
            view,
        })
    }
}

trait RunScopedRecord {
    fn run_ref(&self) -> &RunRef;
}

macro_rules! run_scoped_record {
    ($type:ty) => {
        impl RunScopedRecord for $type {
            fn run_ref(&self) -> &RunRef {
                &self.run_ref
            }
        }
    };
}

run_scoped_record!(ClaimRecord);
run_scoped_record!(EvidenceRecord);
run_scoped_record!(CandidateRecord);
run_scoped_record!(HumanRequestRecord);
run_scoped_record!(AgencyRecord);
run_scoped_record!(ExecutionRecord);

fn records_for_run<T>(records: &BTreeMap<String, T>, run_ref: &RunRef) -> Vec<T>
where
    T: Clone + RunScopedRecord,
{
    records
        .values()
        .filter(|record| record.run_ref() == run_ref)
        .cloned()
        .collect()
}

fn run_status(run: &Run) -> String {
    use crate::core::run::RunLifecycle;

    match run.lifecycle() {
        RunLifecycle::Seeded => "queued",
        RunLifecycle::Active | RunLifecycle::Finishing => "running",
        RunLifecycle::WaitingHuman | RunLifecycle::Suspended => "blocked",
        RunLifecycle::Finished | RunLifecycle::Archived => "success",
        RunLifecycle::Aborted => "fail",
    }
    .into()
}

fn materialise_frontier(run: &Run) -> FrontierView {
    let nodes = run.map().nodes().values().collect::<Vec<_>>();
    let selected = [
        NodeState::Active,
        NodeState::Ready,
        NodeState::Blocked,
        NodeState::Waiting,
        NodeState::Returned,
    ]
    .iter()
    .find_map(|state| {
        nodes
            .iter()
            .find(|node| node.state == Some(*state))
            .copied()
    });

    match selected {
        Some(node) => FrontierView {
            subject_ref: node
                .semantic_ref
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("run-map-node/{}/{}", run.reference(), node.id)),
            title: node.label.clone(),
            mode: match node.kind {
                NodeKind::Decision => "decision",
                NodeKind::Candidate => "recognition",
                _ if node.state == Some(NodeState::Returned) => "return",
                _ => "work",
            }
            .into(),
            summary: format!("RunMap frontier: {:?}", node.state),
            closure_state: None,
            gate_state: None,
        },
        None => FrontierView {
            subject_ref: run.reference().to_string(),
            title: run.destination().to_owned(),
            mode: "work".into(),
            summary: "RunMap has no active/ready/blocked/waiting frontier node.".into(),
            closure_state: None,
            gate_state: None,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryActionInvocation {
    pub action_ref: String,
    pub subject_ref: String,
    pub run_ref: RunRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryActionAuthority {
    pub authority_ref: String,
    pub native_owner: String,
    pub capability_ref: Option<String>,
    pub capability_granted: bool,
    pub action_authorised: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryActionReceipt {
    pub action_ref: String,
    pub subject_ref: String,
    pub authority_ref: String,
    pub previous_revision: u64,
    pub next_revision: u64,
    pub created_human_request_ref: String,
}

#[derive(Debug, Default)]
pub struct FactoryActionExecutor;

impl FactoryActionExecutor {
    pub fn execute(
        &self,
        state: &mut FactoryBuildState,
        invocation: &FactoryActionInvocation,
        authority: &FactoryActionAuthority,
    ) -> Result<FactoryActionReceipt, FactoryBuildError> {
        if invocation.action_ref != REQUEST_MORE_EVIDENCE_ACTION_REF {
            return Err(FactoryBuildError::UnknownAction(
                invocation.action_ref.clone(),
            ));
        }
        if authority.native_owner != FACTORY_NATIVE_OWNER {
            return Err(FactoryBuildError::WrongNativeOwner(
                authority.native_owner.clone(),
            ));
        }
        if authority.authority_ref.trim().is_empty() {
            return Err(FactoryBuildError::MissingAuthority);
        }
        if authority.capability_ref.as_deref() != Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF)
            || !authority.capability_granted
        {
            return Err(FactoryBuildError::MissingCapabilityGrant);
        }
        if !authority.action_authorised {
            return Err(FactoryBuildError::MissingActionAuthority);
        }
        state.ensure_run(&invocation.run_ref)?;
        let previous_revision = state.revision.get();
        let human_request_ref =
            state.request_more_evidence(&invocation.run_ref, &invocation.subject_ref)?;
        Ok(FactoryActionReceipt {
            action_ref: invocation.action_ref.clone(),
            subject_ref: invocation.subject_ref.clone(),
            authority_ref: authority.authority_ref.clone(),
            previous_revision,
            next_revision: state.revision.get(),
            created_human_request_ref: human_request_ref,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FactoryBuildError {
    ProjectRunMismatch,
    ProjectNotFound(String),
    RunNotFound(String),
    SubjectNotFound(String),
    SubjectRunMismatch,
    DuplicateRecord {
        kind: &'static str,
        reference: String,
    },
    RevisionOverflow,
    UnknownAction(String),
    WrongNativeOwner(String),
    MissingAuthority,
    MissingCapabilityGrant,
    MissingActionAuthority,
    ActionAlreadyApplied(String),
    RunContract(RunContractError),
}

impl From<RunContractError> for FactoryBuildError {
    fn from(error: RunContractError) -> Self {
        Self::RunContract(error)
    }
}

impl Display for FactoryBuildError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Factory Build error: {self:?}")
    }
}

impl Error for FactoryBuildError {}
