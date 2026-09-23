use crate::core::identity::Revision;
use crate::core::run::{
    CommandOutcome, EdgeKind, NodeKind, NodeState, Project, ProjectRef, Run, RunContractError,
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

/// The one closed execution status vocabulary, shared with every host that
/// renders Factory executions (contract: `build-view.schema.json`
/// `$defs/executionStatus`). Factory refuses to admit an execution whose
/// status is outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionStatus {
    /// Admitted and not yet started.
    Queued,
    /// Carrying work now.
    Running,
    /// Cannot proceed until something outside it changes.
    Blocked,
    /// Came back with a readable return; recognition is still pending.
    Returned,
    /// Finished, and its return was accepted or verified.
    Success,
    /// Ended in failure.
    Fail,
    /// Stopped by decision before it finished.
    Cancelled,
    /// A conformance fixture execution, not real work.
    ContractFixture,
}

impl ExecutionStatus {
    pub const ALL: [Self; 8] = [
        Self::Queued,
        Self::Running,
        Self::Blocked,
        Self::Returned,
        Self::Success,
        Self::Fail,
        Self::Cancelled,
        Self::ContractFixture,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Blocked => "blocked",
            Self::Returned => "returned",
            Self::Success => "success",
            Self::Fail => "fail",
            Self::Cancelled => "cancelled",
            Self::ContractFixture => "contract-fixture",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
    }
}

impl Display for ExecutionStatus {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRecord {
    pub run_ref: RunRef,
    pub execution_ref: String,
    /// One of [`ExecutionStatus`]. Kept as text so previously persisted
    /// states still open; every admission path validates it.
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
    /// A Project exists before it commissions work. Initialization must not
    /// manufacture a Run merely to make the Project available to clients.
    pub fn empty(project: Project) -> Self {
        Self {
            project,
            runs: RunRegistry::default(),
            revision: Revision::INITIAL,
            claims: BTreeMap::new(),
            evidence: BTreeMap::new(),
            candidates: BTreeMap::new(),
            human_requests: BTreeMap::new(),
            agencies: BTreeMap::new(),
            executions: BTreeMap::new(),
            trajectories: BTreeMap::new(),
        }
    }

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

    /// Number of registered runs; read-only telemetry access.
    pub fn run_count(&self) -> usize {
        self.runs.len()
    }

    /// Registered run refs; read-only telemetry access.
    pub fn run_refs(&self) -> Vec<RunRef> {
        self.runs.refs()
    }

    pub fn run(&self, run_ref: &RunRef) -> Option<&Run> {
        self.runs.get(run_ref)
    }

    /// Add one newly commissioned bounded Run to this Factory-owned Project.
    /// Callers must already have established the developmental reason for the
    /// Run; this method owns only the canonical Run registry mutation.
    pub fn insert_run(&mut self, run: Run) -> Result<(), FactoryBuildError> {
        if run.project_ref() != self.project.reference() {
            return Err(FactoryBuildError::ProjectRunMismatch);
        }
        self.runs.insert(run)?;
        self.bump_revision()
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

    /// Publish consumption through the same canonical Build/Run mutation path.
    /// Stage the Run so a Build revision overflow cannot partially retire input.
    pub fn apply_run_thought_consumption<P: crate::core::run::ThoughtConsumptionSources>(
        &mut self,
        run_ref: &RunRef,
        authority: &RunMutationAuthority,
        command: crate::core::run::RunThoughtConsumptionCommand,
        sources: &P,
    ) -> Result<RunThoughtOutcome, FactoryBuildError> {
        let mut run = self
            .runs
            .get(run_ref)
            .ok_or_else(|| FactoryBuildError::RunNotFound(run_ref.to_string()))?
            .clone();
        let outcome = run.apply_thought_consumption(authority, command, sources)?;
        if matches!(outcome, RunThoughtOutcome::Applied { .. }) {
            self.bump_revision()?;
            *self.runs.get_mut(run_ref).expect("validated Run") = run;
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

    /// Read-only lookups for the admission/validation paths.
    pub fn agency(&self, agency_ref: &str) -> Option<&AgencyRecord> {
        self.agencies.get(agency_ref)
    }

    pub fn execution(&self, execution_ref: &str) -> Option<&ExecutionRecord> {
        self.executions.get(execution_ref)
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
        if ExecutionStatus::parse(&execution.status).is_none() {
            return Err(FactoryBuildError::InvalidExecutionStatus(
                execution.status.clone(),
            ));
        }
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

    /// Commit the existing coordinator's Run under the provider's locked CAS.
    /// Semantic identity, writer ownership and unrelated cognition cannot drift.
    pub(crate) fn replace_attempt_run(
        &mut self,
        expected_revision: Revision,
        next: Run,
    ) -> Result<(), FactoryBuildError> {
        let reference = next.reference().clone();
        let current = self
            .runs
            .get(&reference)
            .ok_or_else(|| FactoryBuildError::RunNotFound(reference.to_string()))?;
        if current.revision() != expected_revision {
            return Err(RunContractError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision(),
            }
            .into());
        }
        if next.project_ref() != current.project_ref()
            || next.destination() != current.destination()
            || next.lifecycle() != current.lifecycle()
            || next.write_authority() != current.write_authority()
            || next.thought_field() != current.thought_field()
            || next.revision().get() < current.revision().get()
        {
            return Err(RunContractError::CorruptRun.into());
        }
        next.validate()?;
        let revision = self
            .revision
            .next()
            .ok_or(FactoryBuildError::RevisionOverflow)?;
        *self
            .runs
            .get_mut(&reference)
            .expect("canonical Run checked above") = next;
        self.revision = revision;
        Ok(())
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
    /// Normalised usage per correlated execution of this Run. Present only
    /// when the view is read from a developmental state that carries
    /// execution correlations; each entry says what is unknown as `null`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_usage: Vec<crate::developmental_read::FactoryExecutionUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub project_ref: String,
    /// Display name of the Project. Derived from the native project key when
    /// the owner state carries one (see [`project_label_from_key`]); otherwise
    /// the project ref itself — a name is never invented.
    pub label: String,
    /// The native project key the label was derived from, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_key: Option<String>,
}

/// A Project's display name together with the native key it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectName {
    pub label: String,
    /// Present when the name came from the native project key; absent when it
    /// came from a Central project link alone.
    pub project_key: Option<String>,
}

impl ProjectName {
    pub fn from_project_key(project_key: &str) -> Option<Self> {
        Some(Self {
            label: project_label_from_key(project_key)?,
            project_key: Some(project_key.to_owned()),
        })
    }

    pub fn from_central_project_ref(central_project_ref: &str) -> Option<Self> {
        Some(Self {
            label: central_project_name(central_project_ref)?,
            project_key: None,
        })
    }
}

/// Name a Project from its native project key without inventing one.
///
/// - `control:root` is the Central root world, named `Central`.
/// - `central-project:<id>` carries a percent-encoded Central project
///   identity; it is decoded and named by [`central_project_name`].
/// - Any other non-empty key is the owner's own stable name and is used as-is.
///
/// Returns `None` only for an empty key or an undecodable encoding.
pub fn project_label_from_key(project_key: &str) -> Option<String> {
    let key = project_key.trim();
    if key.is_empty() {
        return None;
    }
    if key == "control:root" {
        return Some("Central".into());
    }
    match key.strip_prefix("central-project:") {
        Some(encoded) => central_project_name(&percent_decode(encoded)?),
        None => Some(key.to_owned()),
    }
}

/// Name a Central project from its Central identity. Central world refs have
/// the grammar `project:<id>`; the `<id>` is the project's name. Bare
/// identities (`Factory`, `O-I`) are already names.
pub fn central_project_name(central_project_ref: &str) -> Option<String> {
    let reference = central_project_ref.trim();
    if reference == "control:root" {
        return Some("Central".into());
    }
    let name = reference
        .strip_prefix("project:")
        .unwrap_or(reference)
        .trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn percent_decode(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
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
    /// The Run's closure standing, read from its lifecycle alone:
    /// `open` | `closing` | `closed` | `aborted`. This is lifecycle standing,
    /// never a verification Closure (no closure envelope is implied).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closure_state: Option<String>,
    /// Standing of the Run Map gates the frontier node requires: `held` when
    /// any such gate still waits on unsatisfied work, `passed` when every one
    /// is satisfied. Absent when the frontier requires no gate or the map
    /// does not determine the gate's standing.
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
        self.snapshot_with_project_name(state, selection, None)
    }

    /// Materialise the view with the Project's native name, when the caller's
    /// owner state carries one (a Commission's `projectKey`, or a verified
    /// Central project link). Without one the Project label is its ref.
    pub fn snapshot_with_project_name(
        &self,
        state: &FactoryBuildState,
        selection: &FactoryBuildSelection,
        project_name: Option<ProjectName>,
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
            project: match project_name {
                Some(name) => ProjectView {
                    project_ref: state.project.reference().to_string(),
                    label: name.label,
                    project_key: name.project_key,
                },
                None => ProjectView {
                    project_ref: state.project.reference().to_string(),
                    label: state.project.reference().to_string(),
                    project_key: None,
                },
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
            execution_usage: Vec::new(),
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

/// The Run's closure standing from its lifecycle. See
/// [`FrontierView::closure_state`].
fn closure_state(run: &Run) -> &'static str {
    use crate::core::run::RunLifecycle;

    match run.lifecycle() {
        RunLifecycle::Seeded
        | RunLifecycle::Active
        | RunLifecycle::WaitingHuman
        | RunLifecycle::Suspended => "open",
        RunLifecycle::Finishing => "closing",
        RunLifecycle::Finished | RunLifecycle::Archived => "closed",
        RunLifecycle::Aborted => "aborted",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateStanding {
    Held,
    Passed,
}

/// The standing of the gates `node` requires. Gates carry no stored state in
/// the Run Map, so a gate's standing is derived from what it requires: passed
/// only when every prerequisite is satisfied, held while any prerequisite is
/// still planned, ready, running, blocked, waiting or returned. Anything the
/// map does not determine (superseded or abandoned prerequisites, stateless
/// decision/candidate/authority/nested-run prerequisites) yields no claim.
fn frontier_gate_state(run: &Run, node: &crate::core::run::NodeId) -> Option<&'static str> {
    let map = run.map();
    let gates = required_nodes(run, node)
        .filter(|id| map.nodes().get(*id).map(|n| n.kind) == Some(NodeKind::Gate))
        .collect::<Vec<_>>();
    if gates.is_empty() {
        return None;
    }
    let mut visiting = std::collections::BTreeSet::new();
    let standings = gates
        .into_iter()
        .map(|gate| gate_standing(run, gate, &mut visiting))
        .collect::<Vec<_>>();
    if standings.contains(&Some(GateStanding::Held)) {
        Some("held")
    } else if standings.iter().all(|s| *s == Some(GateStanding::Passed)) {
        Some("passed")
    } else {
        None
    }
}

fn required_nodes<'a>(
    run: &'a Run,
    node: &'a crate::core::run::NodeId,
) -> impl Iterator<Item = &'a crate::core::run::NodeId> + 'a {
    run.map()
        .edges()
        .iter()
        .filter(move |edge| edge.relation == EdgeKind::Requires && &edge.from == node)
        .map(|edge| &edge.to)
}

fn gate_standing<'a>(
    run: &'a Run,
    gate: &'a crate::core::run::NodeId,
    visiting: &mut std::collections::BTreeSet<&'a crate::core::run::NodeId>,
) -> Option<GateStanding> {
    if !visiting.insert(gate) {
        return None;
    }
    let mut standing = GateStanding::Passed;
    let mut undetermined = false;
    for prerequisite in required_nodes(run, gate) {
        let Some(node) = run.map().nodes().get(prerequisite) else {
            undetermined = true;
            continue;
        };
        let held = match (node.kind, node.state) {
            (NodeKind::Gate, _) => match gate_standing(run, prerequisite, visiting) {
                Some(GateStanding::Held) => true,
                Some(GateStanding::Passed) => false,
                None => {
                    undetermined = true;
                    false
                }
            },
            (_, Some(NodeState::Satisfied)) => false,
            (
                _,
                Some(
                    NodeState::Planned
                    | NodeState::Ready
                    | NodeState::Active
                    | NodeState::Blocked
                    | NodeState::Waiting
                    | NodeState::Returned,
                ),
            ) => true,
            _ => {
                undetermined = true;
                false
            }
        };
        if held {
            standing = GateStanding::Held;
        }
    }
    visiting.remove(gate);
    match standing {
        GateStanding::Held => Some(GateStanding::Held),
        GateStanding::Passed if undetermined => None,
        GateStanding::Passed => Some(GateStanding::Passed),
    }
}

/// One plain sentence for the frontier node's state, or nothing when the
/// node carries no state of its own.
fn frontier_summary(state: Option<NodeState>) -> &'static str {
    match state {
        Some(NodeState::Planned) => "Planned.",
        Some(NodeState::Ready) => "Ready to start.",
        Some(NodeState::Active) => "Running.",
        Some(NodeState::Blocked) => "Blocked.",
        Some(NodeState::Waiting) => "Waiting.",
        Some(NodeState::Satisfied) => "Done.",
        Some(NodeState::Returned) => "Returned — awaiting recognition.",
        Some(NodeState::Superseded) => "Superseded.",
        Some(NodeState::Abandoned) => "Abandoned.",
        None => "",
    }
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
            summary: frontier_summary(node.state).into(),
            closure_state: Some(closure_state(run).into()),
            gate_state: frontier_gate_state(run, &node.id).map(Into::into),
        },
        None => FrontierView {
            subject_ref: run.reference().to_string(),
            title: run.destination().to_owned(),
            mode: "work".into(),
            summary: "Nothing is ready, running, blocked, waiting or returned.".into(),
            closure_state: Some(closure_state(run).into()),
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
    /// An execution status outside [`ExecutionStatus`].
    InvalidExecutionStatus(String),
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

#[cfg(test)]
mod project_name_tests {
    use super::{central_project_name, project_label_from_key, ProjectName};

    #[test]
    fn project_key_names_decode_without_invention() {
        assert_eq!(
            project_label_from_key("central-project:Factory").as_deref(),
            Some("Factory")
        );
        assert_eq!(
            project_label_from_key("central-project:project%3Aquaternal-logic").as_deref(),
            Some("quaternal-logic")
        );
        assert_eq!(
            project_label_from_key("central-project:My%20Project").as_deref(),
            Some("My Project")
        );
        assert_eq!(
            project_label_from_key("control:root").as_deref(),
            Some("Central")
        );
        assert_eq!(
            project_label_from_key("factory-programme-195").as_deref(),
            Some("factory-programme-195")
        );
        // Nothing to name, or an encoding that does not decode: no label.
        assert_eq!(project_label_from_key("  "), None);
        assert_eq!(project_label_from_key("central-project:"), None);
        assert_eq!(project_label_from_key("central-project:bad%zz"), None);
        assert_eq!(project_label_from_key("central-project:cut%4"), None);
    }

    #[test]
    fn central_link_names_follow_the_central_ref_grammar() {
        assert_eq!(central_project_name("O-I").as_deref(), Some("O-I"));
        assert_eq!(
            central_project_name("project:quaternal-logic").as_deref(),
            Some("quaternal-logic")
        );
        assert_eq!(central_project_name("project:"), None);
        assert_eq!(
            ProjectName::from_central_project_ref("Factory"),
            Some(ProjectName {
                label: "Factory".into(),
                project_key: None
            })
        );
    }
}

#[cfg(test)]
mod frontier_tests {
    use super::*;
    use crate::core::run::{
        NodeId, RunTopologyCommand, TopologyEdge, TopologyMutation, TopologyNode,
    };

    const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAA";
    const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAB";

    fn work(id: &str, state: NodeState) -> TopologyNode {
        TopologyNode {
            id: NodeId::new(id).unwrap(),
            kind: NodeKind::Work,
            label: format!("Work {id}"),
            state: Some(state),
            semantic_ref: None,
        }
    }

    fn gate(id: &str) -> TopologyNode {
        TopologyNode {
            id: NodeId::new(id).unwrap(),
            kind: NodeKind::Gate,
            label: format!("Gate {id}"),
            state: None,
            semantic_ref: None,
        }
    }

    fn requires(from: &str, to: &str) -> TopologyEdge {
        TopologyEdge {
            from: NodeId::new(from).unwrap(),
            to: NodeId::new(to).unwrap(),
            relation: EdgeKind::Requires,
        }
    }

    /// A Build state whose single Run carries exactly these nodes and edges.
    fn snapshot_of(nodes: Vec<TopologyNode>, edges: Vec<TopologyEdge>) -> FactoryBuildSnapshot {
        let project_ref: ProjectRef = PROJECT.parse().unwrap();
        let run_ref: RunRef = RUN.parse().unwrap();
        let run = Run::new(
            run_ref.clone(),
            project_ref.clone(),
            "Frontier fixture",
            "factory",
        )
        .unwrap();
        let mut state = FactoryBuildState::new(Project::new(project_ref.clone()), run).unwrap();
        let authority = state.run_mutation_authority(&run_ref).unwrap();
        // Every node hangs from the destination unless an edge already
        // reaches it, so the fixture is a valid, reachable Run Map.
        let reached = edges
            .iter()
            .map(|edge| edge.to.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let mut edges = edges;
        for node in &nodes {
            if !reached.contains(&node.id) {
                edges.push(TopologyEdge {
                    from: NodeId::new("destination").unwrap(),
                    to: node.id.clone(),
                    relation: EdgeKind::Requires,
                });
            }
        }
        let mut mutations = nodes
            .into_iter()
            .map(|node| TopologyMutation::AddNode { node })
            .collect::<Vec<_>>();
        mutations.extend(
            edges
                .into_iter()
                .map(|edge| TopologyMutation::AddEdge { edge }),
        );
        state
            .apply_run_topology_command(
                &run_ref,
                &authority,
                RunTopologyCommand {
                    command_id: "frontier-fixture".into(),
                    expected_revision: state.run(&run_ref).unwrap().revision(),
                    mutation: TopologyMutation::Batch { mutations },
                },
            )
            .unwrap();
        FactoryBuildViewProvider
            .snapshot(
                &state,
                &FactoryBuildSelection {
                    project_ref,
                    run_ref,
                },
            )
            .unwrap()
    }

    #[test]
    fn frontier_summary_is_a_plain_sentence_never_debug_text() {
        for (state, sentence) in [
            (NodeState::Ready, "Ready to start."),
            (NodeState::Active, "Running."),
            (NodeState::Blocked, "Blocked."),
            (NodeState::Waiting, "Waiting."),
            (NodeState::Returned, "Returned — awaiting recognition."),
        ] {
            let snapshot = snapshot_of(vec![work("unit", state)], vec![]);
            assert_build_view_contract(&snapshot);
            assert_eq!(snapshot.view.frontier.summary, sentence);
            let json = snapshot.to_json().unwrap();
            assert!(!json.contains("Some("), "Debug text leaked: {json}");
            assert!(!json.contains("RunMap frontier"));
        }
        // Stateless nodes say nothing rather than print an Option.
        assert_eq!(frontier_summary(None), "");
        // No frontier node at all: one plain sentence.
        let idle = snapshot_of(vec![work("unit", NodeState::Satisfied)], vec![]);
        assert_eq!(
            idle.view.frontier.summary,
            "Nothing is ready, running, blocked, waiting or returned."
        );
    }

    #[test]
    fn closure_state_reads_the_run_lifecycle() {
        let snapshot = snapshot_of(vec![work("unit", NodeState::Ready)], vec![]);
        // A freshly seeded Run is open.
        assert_eq!(
            snapshot.view.frontier.closure_state.as_deref(),
            Some("open")
        );
        let idle = snapshot_of(vec![work("unit", NodeState::Satisfied)], vec![]);
        assert_eq!(idle.view.frontier.closure_state.as_deref(), Some("open"));
        // Every lifecycle maps to exactly one closure standing.
        let run = Run::new(
            RUN.parse().unwrap(),
            PROJECT.parse().unwrap(),
            "Closure fixture",
            "factory",
        )
        .unwrap();
        for (lifecycle, closure) in [
            ("seeded", "open"),
            ("active", "open"),
            ("waiting_human", "open"),
            ("suspended", "open"),
            ("finishing", "closing"),
            ("finished", "closed"),
            ("archived", "closed"),
            ("aborted", "aborted"),
        ] {
            let mut value = serde_json::to_value(&run).unwrap();
            value["lifecycle"] = lifecycle.into();
            let run: Run = serde_json::from_value(value).unwrap();
            assert_eq!(closure_state(&run), closure, "lifecycle {lifecycle}");
        }
    }

    #[test]
    fn gate_state_is_derived_from_what_the_frontier_gate_requires() {
        // The barrier waits for `left`; `next` is released by it. The
        // frontier picks the blocked `next` before the waiting `left`.
        let held = snapshot_of(
            vec![
                work("left", NodeState::Waiting),
                work("next", NodeState::Blocked),
                gate("barrier"),
            ],
            vec![requires("barrier", "left"), requires("next", "barrier")],
        );
        assert_eq!(held.view.frontier.title, "Work next");
        assert_eq!(held.view.frontier.gate_state.as_deref(), Some("held"));
        assert_build_view_contract(&held);

        let passed = snapshot_of(
            vec![
                work("left", NodeState::Satisfied),
                work("next", NodeState::Ready),
                gate("barrier"),
            ],
            vec![requires("barrier", "left"), requires("next", "barrier")],
        );
        assert_eq!(passed.view.frontier.title, "Work next");
        assert_eq!(passed.view.frontier.gate_state.as_deref(), Some("passed"));
        assert_build_view_contract(&passed);

        // No gate in front of the frontier: no claim.
        let ungated = snapshot_of(vec![work("unit", NodeState::Ready)], vec![]);
        assert_eq!(ungated.view.frontier.gate_state, None);

        // A superseded prerequisite leaves the gate undetermined: no claim.
        let undetermined = snapshot_of(
            vec![
                work("left", NodeState::Superseded),
                work("next", NodeState::Ready),
                gate("barrier"),
            ],
            vec![requires("barrier", "left"), requires("next", "barrier")],
        );
        assert_eq!(undetermined.view.frontier.gate_state, None);
    }
}

/// Validate a snapshot against the published build-view contract schema.
#[cfg(test)]
pub(crate) fn assert_build_view_contract(snapshot: &FactoryBuildSnapshot) {
    let schema: Value = serde_json::from_str(include_str!(
        "../../contracts/factory/build-view.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::Validator::new(&schema).expect("build-view schema compiles");
    let instance = serde_json::to_value(snapshot).unwrap();
    let errors = validator
        .iter_errors(&instance)
        .map(|error| format!("{} at {}", error, error.instance_path))
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "build view breaks its contract: {errors:#?}"
    );
}

#[cfg(test)]
mod execution_status_tests {
    use super::*;

    const SCHEMA: &str = include_str!("../../contracts/factory/build-view.schema.json");

    #[test]
    fn contract_schema_and_rust_vocabulary_are_the_same_closed_set() {
        let schema: Value = serde_json::from_str(SCHEMA).unwrap();
        let declared = schema["$defs"]["executionStatus"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                assert!(
                    entry["description"].as_str().is_some_and(|d| !d.is_empty()),
                    "every status is documented"
                );
                entry["const"].as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();
        let native = ExecutionStatus::ALL
            .iter()
            .map(|status| status.as_str().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(declared, native);
        for status in ExecutionStatus::ALL {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                Value::String(status.as_str().into())
            );
            assert_eq!(ExecutionStatus::parse(status.as_str()), Some(status));
        }
    }

    #[test]
    fn every_producer_value_is_in_the_vocabulary() {
        // Every value a Factory producer writes today (conformance fixture,
        // developmental telemetry fixtures, live Build fixtures).
        for written in ["contract-fixture", "returned", "running", "success"] {
            assert!(ExecutionStatus::parse(written).is_some(), "{written}");
        }
        // The conformance producer writes through the validated path, so a
        // generated state can only carry vocabulary values.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.json");
        crate::conformance::create_developmental_conformance_state(&path).unwrap();
        let state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let executions = state["state"]["build"]["executions"].as_object().unwrap();
        assert!(!executions.is_empty());
        for execution in executions.values() {
            assert!(ExecutionStatus::parse(execution["status"].as_str().unwrap()).is_some());
        }
    }

    #[test]
    fn the_contract_schema_refuses_a_status_outside_the_vocabulary() {
        let schema: Value = serde_json::from_str(SCHEMA).unwrap();
        let validator = jsonschema::Validator::new(&schema).unwrap();
        let execution = serde_json::json!({
            "runRef": "run:01ARZ3NDEKTSV4RRFFQ69G5FAB",
            "executionRef": "execution:x",
            "status": "done",
            "surfaceRefs": [],
            "workcellBindingRefs": []
        });
        let definition = serde_json::json!({
            "$ref": "#/$defs/execution",
            "$defs": schema["$defs"].clone()
        });
        let execution_validator = jsonschema::Validator::new(&definition).unwrap();
        assert!(!execution_validator.is_valid(&execution));
        let mut accepted = execution.clone();
        accepted["status"] = "returned".into();
        assert!(execution_validator.is_valid(&accepted));
        // The full document schema compiles too.
        assert!(!validator.is_valid(&serde_json::json!({})));
    }
}
