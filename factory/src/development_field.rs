//! Native Factory Development Field technique.
//!
//! The Development Field is not a second orchestration language or an owner of
//! source, Git, AIKit, Workcell, or Actuation state.  It is Factory's durable,
//! Run-scoped developmental relation: what difference is intended, which native
//! sources and capabilities it answers to, the exact Git basis on which work is
//! undertaken, which operative/material/actual conditions realised it, and what
//! evidence/Candidates/Recognition returned.

use crate::core::run::{ProjectRef, RunRef, WorkflowUnitRef};
use crate::git_development::GitDevelopmentWorld;
use crate::journey::JourneyRef;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const DEVELOPMENT_FIELD_SCHEMA: &str = "factory.development-field/v1";
pub const DEVELOPMENT_FIELD_READING_CONTRACT: &str = "factory.development-field-reading/v1";

/// Developmental proof is intentionally graded rather than collapsed to one
/// readiness scalar.  A stronger/later grade does not silently satisfy another.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DevelopmentEvidenceGrade {
    #[serde(rename = "D")]
    D,
    #[serde(rename = "C")]
    C,
    #[serde(rename = "P")]
    P,
    #[serde(rename = "M")]
    M,
    #[serde(rename = "H")]
    H,
}

/// Stable semantic targets for a developmental Run.  These are references to
/// owner-native source, never copied source authority.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentFieldTargets {
    pub plan_ref: String,
    #[serde(default)]
    pub ux_refs: Vec<String>,
    #[serde(default)]
    pub capability_refs: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
    #[serde(default)]
    pub self_description_refs: Vec<String>,
}

/// Exact source/Git basis retained from the existing Git developmental World.
/// Worktree identity is material/provider state and is deliberately not a Run,
/// Journey, Candidate, or Development Field identity.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentGitBasis {
    pub git_development_ref: String,
    pub project_ref: ProjectRef,
    pub run_ref: RunRef,
    pub repository_ref: String,
    pub base_revision: String,
    pub initial_worktree_clean: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_snapshot_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_worktree_ref: Option<String>,
    #[serde(default)]
    pub source_basis_refs: Vec<String>,
    #[serde(default)]
    pub structural_ground_refs: Vec<String>,
}

impl DevelopmentGitBasis {
    /// Consume the native Git developmental World rather than recreating Git
    /// semantics inside the Development Field.
    pub fn from_git_world(world: &GitDevelopmentWorld) -> Self {
        Self {
            git_development_ref: world.development_ref.clone(),
            project_ref: world.base.project_ref.clone(),
            run_ref: world.base.run_ref.clone(),
            repository_ref: world.base.repository_ref.clone(),
            base_revision: world.base.base_revision.clone(),
            initial_worktree_clean: world.base.base_worktree_clean,
            dirty_snapshot_ref: None,
            initial_worktree_ref: Some(world.binding.worktree_ref.clone()),
            source_basis_refs: world.base.source_basis_refs.clone(),
            structural_ground_refs: world.base.structural_ground_refs.clone(),
        }
    }
}

/// Reference-only capture of the AIKit-owned operative arrangement used for the
/// Run.  The fields identify what AIKit resolved; Factory does not copy profiles,
/// Skill/Method bodies, harness state, SessionSpace state, or provider registries.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AikitOperativeReferences {
    pub resolution_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub praxis_condition_ref: Option<String>,
    #[serde(default)]
    pub execution_disposition_refs: Vec<String>,
    #[serde(default)]
    pub agent_refs: Vec<String>,
    #[serde(default)]
    pub agent_set_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_composition_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_space_ref: Option<String>,
    #[serde(default)]
    pub agent_session_refs: Vec<String>,
    #[serde(default)]
    pub source_basis_refs: Vec<String>,
    pub provider_ref: String,
    pub provider_revision: String,
}

/// One material binding used during development.  `owner` is normally
/// `workcell`; an external owner is valid when current execution cannot truthfully
/// be represented by a Workcell binding.  Recording that distinction is better
/// than fabricating Workcell identity.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentMaterialBinding {
    pub owner: String,
    pub binding_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default)]
    pub provenance_refs: Vec<String>,
}

/// Correlation to already-established Factory/Actuation actuality.  No Agency or
/// AgentSession is manufactured here: optional refs are present only when the
/// native execution relation actually supplied them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentActualityReferences {
    pub execution_correlation_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actuation_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agency_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_ref: Option<String>,
    #[serde(default)]
    pub activity_refs: Vec<String>,
    #[serde(default)]
    pub return_refs: Vec<String>,
}

/// Attributable evidence for exactly one proof grade.  H is reserved for actual
/// human experience and therefore requires an EX reference in addition to the
/// evidence record(s).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentEvidenceStanding {
    pub grade: DevelopmentEvidenceGrade,
    pub owner: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub source_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_ex_ref: Option<String>,
}

/// Candidate lineage remains explicit.  A Candidate can exist with no
/// Recognition, and multiple/rejected alternatives remain addressable history.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentCandidateLineage {
    pub candidate_ref: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub recognition_refs: Vec<String>,
}

/// Exact returned difference and the native actuality/evidence relations that
/// accompany it.  Remaining proof is derived from evidence standings; it is not
/// stored as a mutable readiness claim.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentFieldReturn {
    pub return_ref: String,
    pub git_development_ref: String,
    pub base_revision: String,
    pub result_revision: String,
    #[serde(default)]
    pub commits: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncommitted_diff_ref: Option<String>,
    #[serde(default)]
    pub verification_evidence_refs: Vec<String>,
    #[serde(default)]
    pub material_binding_refs: Vec<String>,
    #[serde(default)]
    pub actualities: Vec<DevelopmentActualityReferences>,
    #[serde(default)]
    pub evidence: Vec<DevelopmentEvidenceStanding>,
    #[serde(default)]
    pub candidates: Vec<DevelopmentCandidateLineage>,
    #[serde(default)]
    pub pressure_refs: Vec<String>,
}

impl DevelopmentFieldReturn {
    /// Seed a return from the existing Git developmental World.  Git facts remain
    /// native to that world; callers then attach Factory/Actuation/evidence refs.
    pub fn from_git_world(
        return_ref: impl Into<String>,
        world: &GitDevelopmentWorld,
    ) -> Result<Self, DevelopmentFieldError> {
        let returned = world
            .returned
            .as_ref()
            .ok_or(DevelopmentFieldError::MissingGitReturn)?;
        Ok(Self {
            return_ref: return_ref.into(),
            git_development_ref: world.development_ref.clone(),
            base_revision: returned.base_revision.clone(),
            result_revision: returned.result_revision.clone(),
            commits: returned.commits.clone(),
            diff_ref: returned.diff_ref.clone(),
            uncommitted_diff_ref: returned.uncommitted_diff_ref.clone(),
            verification_evidence_refs: returned.verification_evidence_refs.clone(),
            material_binding_refs: Vec::new(),
            actualities: Vec::new(),
            evidence: Vec::new(),
            candidates: Vec::new(),
            pressure_refs: Vec::new(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentField {
    pub schema: String,
    pub field_ref: String,
    pub project_ref: ProjectRef,
    pub run_ref: RunRef,
    pub journey_ref: JourneyRef,
    pub commission_ref: String,
    #[serde(default)]
    pub workflow_unit_refs: Vec<WorkflowUnitRef>,
    pub required_difference: String,
    pub targets: DevelopmentFieldTargets,
    pub required_proof: BTreeSet<DevelopmentEvidenceGrade>,
    pub git_basis: DevelopmentGitBasis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aikit_operative: Option<AikitOperativeReferences>,
    #[serde(default)]
    pub material_bindings: Vec<DevelopmentMaterialBinding>,
    #[serde(default)]
    pub returns: Vec<DevelopmentFieldReturn>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevelopmentFieldReading {
    pub contract: String,
    pub field: DevelopmentField,
    pub satisfied_proof: BTreeSet<DevelopmentEvidenceGrade>,
    pub remaining_proof: BTreeSet<DevelopmentEvidenceGrade>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_return_ref: Option<String>,
}

impl DevelopmentField {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        field_ref: impl Into<String>,
        project_ref: ProjectRef,
        run_ref: RunRef,
        journey_ref: JourneyRef,
        commission_ref: impl Into<String>,
        workflow_unit_refs: Vec<WorkflowUnitRef>,
        required_difference: impl Into<String>,
        targets: DevelopmentFieldTargets,
        required_proof: BTreeSet<DevelopmentEvidenceGrade>,
        git_basis: DevelopmentGitBasis,
    ) -> Result<Self, DevelopmentFieldError> {
        let field = Self {
            schema: DEVELOPMENT_FIELD_SCHEMA.to_owned(),
            field_ref: field_ref.into(),
            project_ref,
            run_ref,
            journey_ref,
            commission_ref: commission_ref.into(),
            workflow_unit_refs,
            required_difference: required_difference.into(),
            targets,
            required_proof,
            git_basis,
            aikit_operative: None,
            material_bindings: Vec::new(),
            returns: Vec::new(),
        };
        field.validate()?;
        Ok(field)
    }

    pub fn validate(&self) -> Result<(), DevelopmentFieldError> {
        if self.schema != DEVELOPMENT_FIELD_SCHEMA {
            return Err(DevelopmentFieldError::WrongSchema(self.schema.clone()));
        }
        stable_ref(&self.field_ref, "fieldRef")?;
        stable_ref(&self.commission_ref, "commissionRef")?;
        required_text(&self.required_difference, "requiredDifference")?;
        if self.required_proof.is_empty() {
            return Err(DevelopmentFieldError::EmptyRequiredProof);
        }
        self.targets.validate()?;
        self.git_basis.validate(&self.project_ref, &self.run_ref)?;
        unique_typed_refs(&self.workflow_unit_refs, "workflowUnitRefs")?;
        if let Some(operative) = &self.aikit_operative {
            operative.validate()?;
        }

        let mut bindings = BTreeSet::new();
        for binding in &self.material_bindings {
            binding.validate()?;
            if !bindings.insert(binding.binding_ref.as_str()) {
                return Err(DevelopmentFieldError::DuplicateRef(
                    binding.binding_ref.clone(),
                ));
            }
            if self
                .git_basis
                .initial_worktree_ref
                .as_deref()
                .is_some_and(|worktree_ref| worktree_ref == binding.binding_ref)
            {
                return Err(DevelopmentFieldError::IdentityCollapse(
                    "material binding cannot also be the Git worktree identity".into(),
                ));
            }
        }

        let mut returns = BTreeSet::new();
        for returned in &self.returns {
            if !returns.insert(returned.return_ref.as_str()) {
                return Err(DevelopmentFieldError::DuplicateRef(
                    returned.return_ref.clone(),
                ));
            }
            self.validate_return(returned)?;
        }
        Ok(())
    }

    pub fn set_aikit_operative(
        &mut self,
        operative: AikitOperativeReferences,
    ) -> Result<(), DevelopmentFieldError> {
        operative.validate()?;
        self.aikit_operative = Some(operative);
        Ok(())
    }

    /// Append material history rather than rebasing semantic identity when a
    /// development World moves between machines/VMs/sandboxes.
    pub fn record_material_binding(
        &mut self,
        binding: DevelopmentMaterialBinding,
    ) -> Result<(), DevelopmentFieldError> {
        binding.validate()?;
        if self
            .material_bindings
            .iter()
            .any(|existing| existing.binding_ref == binding.binding_ref)
        {
            return Err(DevelopmentFieldError::DuplicateRef(binding.binding_ref));
        }
        if self
            .git_basis
            .initial_worktree_ref
            .as_deref()
            .is_some_and(|worktree_ref| worktree_ref == binding.binding_ref)
        {
            return Err(DevelopmentFieldError::IdentityCollapse(
                "material binding cannot also be the Git worktree identity".into(),
            ));
        }
        self.material_bindings.push(binding);
        Ok(())
    }

    pub fn record_return(
        &mut self,
        returned: DevelopmentFieldReturn,
    ) -> Result<(), DevelopmentFieldError> {
        if self.aikit_operative.is_none() {
            return Err(DevelopmentFieldError::MissingOperativeResolution);
        }
        if self
            .returns
            .iter()
            .any(|existing| existing.return_ref == returned.return_ref)
        {
            return Err(DevelopmentFieldError::DuplicateRef(returned.return_ref));
        }
        self.validate_return(&returned)?;
        self.returns.push(returned);
        Ok(())
    }

    pub fn satisfied_proof_grades(&self) -> BTreeSet<DevelopmentEvidenceGrade> {
        self.returns
            .iter()
            .flat_map(|returned| returned.evidence.iter().map(|evidence| evidence.grade))
            .collect()
    }

    pub fn remaining_proof_grades(&self) -> BTreeSet<DevelopmentEvidenceGrade> {
        let satisfied = self.satisfied_proof_grades();
        self.required_proof
            .iter()
            .filter(|grade| !satisfied.contains(grade))
            .copied()
            .collect()
    }

    pub fn reading(&self) -> DevelopmentFieldReading {
        DevelopmentFieldReading {
            contract: DEVELOPMENT_FIELD_READING_CONTRACT.to_owned(),
            field: self.clone(),
            satisfied_proof: self.satisfied_proof_grades(),
            remaining_proof: self.remaining_proof_grades(),
            latest_return_ref: self.returns.last().map(|returned| returned.return_ref.clone()),
        }
    }

    fn validate_return(&self, returned: &DevelopmentFieldReturn) -> Result<(), DevelopmentFieldError> {
        stable_ref(&returned.return_ref, "returnRef")?;
        stable_ref(&returned.git_development_ref, "gitDevelopmentRef")?;
        stable_ref(&returned.base_revision, "baseRevision")?;
        stable_ref(&returned.result_revision, "resultRevision")?;
        if returned.git_development_ref != self.git_basis.git_development_ref {
            return Err(DevelopmentFieldError::GitDevelopmentMismatch {
                expected: self.git_basis.git_development_ref.clone(),
                actual: returned.git_development_ref.clone(),
            });
        }
        if returned.base_revision != self.git_basis.base_revision {
            return Err(DevelopmentFieldError::ReturnBaseMismatch {
                expected: self.git_basis.base_revision.clone(),
                actual: returned.base_revision.clone(),
            });
        }
        unique_refs(&returned.commits, "commits")?;
        unique_refs(
            &returned.verification_evidence_refs,
            "verificationEvidenceRefs",
        )?;
        unique_refs(&returned.material_binding_refs, "materialBindingRefs")?;
        for binding_ref in &returned.material_binding_refs {
            if !self
                .material_bindings
                .iter()
                .any(|binding| &binding.binding_ref == binding_ref)
            {
                return Err(DevelopmentFieldError::UnknownMaterialBinding(
                    binding_ref.clone(),
                ));
            }
        }
        for actuality in &returned.actualities {
            actuality.validate()?;
        }
        for evidence in &returned.evidence {
            evidence.validate()?;
        }
        for candidate in &returned.candidates {
            candidate.validate()?;
        }
        unique_refs(&returned.pressure_refs, "pressureRefs")?;
        Ok(())
    }
}

impl DevelopmentFieldTargets {
    fn validate(&self) -> Result<(), DevelopmentFieldError> {
        stable_ref(&self.plan_ref, "planRef")?;
        unique_refs(&self.ux_refs, "uxRefs")?;
        unique_refs(&self.capability_refs, "capabilityRefs")?;
        unique_refs(&self.source_refs, "sourceRefs")?;
        unique_refs(&self.self_description_refs, "selfDescriptionRefs")
    }
}

impl DevelopmentGitBasis {
    fn validate(
        &self,
        project_ref: &ProjectRef,
        run_ref: &RunRef,
    ) -> Result<(), DevelopmentFieldError> {
        if &self.project_ref != project_ref {
            return Err(DevelopmentFieldError::WrongProject {
                expected: project_ref.clone(),
                actual: self.project_ref.clone(),
            });
        }
        if &self.run_ref != run_ref {
            return Err(DevelopmentFieldError::WrongRun {
                expected: run_ref.clone(),
                actual: self.run_ref.clone(),
            });
        }
        stable_ref(&self.git_development_ref, "gitDevelopmentRef")?;
        stable_ref(&self.repository_ref, "repositoryRef")?;
        stable_ref(&self.base_revision, "baseRevision")?;
        if !self.initial_worktree_clean && self.dirty_snapshot_ref.is_none() {
            return Err(DevelopmentFieldError::DirtyBasisWithoutSnapshot);
        }
        if let Some(reference) = self.dirty_snapshot_ref.as_deref() {
            stable_ref(reference, "dirtySnapshotRef")?;
        }
        if let Some(reference) = self.initial_worktree_ref.as_deref() {
            stable_ref(reference, "initialWorktreeRef")?;
        }
        unique_refs(&self.source_basis_refs, "sourceBasisRefs")?;
        unique_refs(&self.structural_ground_refs, "structuralGroundRefs")
    }
}

impl AikitOperativeReferences {
    fn validate(&self) -> Result<(), DevelopmentFieldError> {
        stable_ref(&self.resolution_ref, "aikitOperative.resolutionRef")?;
        stable_ref(&self.provider_ref, "aikitOperative.providerRef")?;
        stable_ref(&self.provider_revision, "aikitOperative.providerRevision")?;
        for (name, value) in [
            ("praxisConditionRef", self.praxis_condition_ref.as_deref()),
            ("profileRef", self.profile_ref.as_deref()),
            ("harnessRef", self.harness_ref.as_deref()),
            (
                "harnessCompositionRef",
                self.harness_composition_ref.as_deref(),
            ),
            ("sessionSpaceRef", self.session_space_ref.as_deref()),
        ] {
            if let Some(value) = value {
                stable_ref(value, name)?;
            }
        }
        unique_refs(
            &self.execution_disposition_refs,
            "executionDispositionRefs",
        )?;
        unique_refs(&self.agent_refs, "agentRefs")?;
        unique_refs(&self.agent_set_refs, "agentSetRefs")?;
        unique_refs(&self.agent_session_refs, "agentSessionRefs")?;
        unique_refs(&self.source_basis_refs, "operativeSourceBasisRefs")
    }
}

impl DevelopmentMaterialBinding {
    fn validate(&self) -> Result<(), DevelopmentFieldError> {
        stable_ref(&self.owner, "materialBinding.owner")?;
        stable_ref(&self.binding_ref, "materialBinding.bindingRef")?;
        if let Some(reference) = self.provider_ref.as_deref() {
            stable_ref(reference, "materialBinding.providerRef")?;
        }
        if let Some(revision) = self.revision.as_deref() {
            stable_ref(revision, "materialBinding.revision")?;
        }
        unique_refs(&self.provenance_refs, "materialBinding.provenanceRefs")
    }
}

impl DevelopmentActualityReferences {
    fn validate(&self) -> Result<(), DevelopmentFieldError> {
        stable_ref(
            &self.execution_correlation_ref,
            "actuality.executionCorrelationRef",
        )?;
        for (name, value) in [
            ("actuality.actuationRef", self.actuation_ref.as_deref()),
            ("actuality.agencyRef", self.agency_ref.as_deref()),
            (
                "actuality.agentSessionRef",
                self.agent_session_ref.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                stable_ref(value, name)?;
            }
        }
        unique_refs(&self.activity_refs, "actuality.activityRefs")?;
        unique_refs(&self.return_refs, "actuality.returnRefs")
    }
}

impl DevelopmentEvidenceStanding {
    fn validate(&self) -> Result<(), DevelopmentFieldError> {
        stable_ref(&self.owner, "evidence.owner")?;
        if self.evidence_refs.is_empty() {
            return Err(DevelopmentFieldError::EmptyEvidence(self.grade));
        }
        unique_refs(&self.evidence_refs, "evidenceRefs")?;
        unique_refs(&self.source_refs, "evidenceSourceRefs")?;
        if self.grade == DevelopmentEvidenceGrade::H {
            let ex_ref = self
                .human_ex_ref
                .as_deref()
                .ok_or(DevelopmentFieldError::HumanProofWithoutEx)?;
            stable_ref(ex_ref, "humanExRef")?;
        }
        Ok(())
    }
}

impl DevelopmentCandidateLineage {
    fn validate(&self) -> Result<(), DevelopmentFieldError> {
        stable_ref(&self.candidate_ref, "candidateRef")?;
        unique_refs(&self.evidence_refs, "candidateEvidenceRefs")?;
        unique_refs(&self.recognition_refs, "recognitionRefs")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevelopmentFieldError {
    WrongSchema(String),
    InvalidField(&'static str),
    DuplicateRef(String),
    WrongProject { expected: ProjectRef, actual: ProjectRef },
    WrongRun { expected: RunRef, actual: RunRef },
    EmptyRequiredProof,
    DirtyBasisWithoutSnapshot,
    IdentityCollapse(String),
    MissingOperativeResolution,
    MissingGitReturn,
    GitDevelopmentMismatch { expected: String, actual: String },
    ReturnBaseMismatch { expected: String, actual: String },
    UnknownMaterialBinding(String),
    EmptyEvidence(DevelopmentEvidenceGrade),
    HumanProofWithoutEx,
}

impl Display for DevelopmentFieldError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSchema(schema) => write!(formatter, "unsupported Development Field schema: {schema}"),
            Self::InvalidField(field) => write!(formatter, "invalid Development Field field: {field}"),
            Self::DuplicateRef(reference) => write!(formatter, "duplicate Development Field ref: {reference}"),
            Self::WrongProject { expected, actual } => write!(formatter, "Development Field project {actual} does not match {expected}"),
            Self::WrongRun { expected, actual } => write!(formatter, "Development Field Run {actual} does not match {expected}"),
            Self::EmptyRequiredProof => write!(formatter, "Development Field requires at least one explicit proof grade"),
            Self::DirtyBasisWithoutSnapshot => write!(formatter, "dirty Git basis requires an explicit snapshot ref"),
            Self::IdentityCollapse(detail) => write!(formatter, "Development Field identity collapse: {detail}"),
            Self::MissingOperativeResolution => write!(formatter, "returned development requires an AIKit operative-resolution reference"),
            Self::MissingGitReturn => write!(formatter, "Git developmental World has no returned difference"),
            Self::GitDevelopmentMismatch { expected, actual } => write!(formatter, "returned Git development {actual} does not match {expected}"),
            Self::ReturnBaseMismatch { expected, actual } => write!(formatter, "returned Git base {actual} does not match exact basis {expected}"),
            Self::UnknownMaterialBinding(reference) => write!(formatter, "return refers to unknown material binding {reference}"),
            Self::EmptyEvidence(grade) => write!(formatter, "{grade:?} evidence standing has no evidence refs"),
            Self::HumanProofWithoutEx => write!(formatter, "H-grade proof requires an actual human EX ref"),
        }
    }
}

impl Error for DevelopmentFieldError {}

fn required_text(value: &str, field: &'static str) -> Result<(), DevelopmentFieldError> {
    if value.trim().is_empty() {
        Err(DevelopmentFieldError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn stable_ref(value: &str, field: &'static str) -> Result<(), DevelopmentFieldError> {
    if value.trim().is_empty() || value.chars().any(char::is_whitespace) {
        Err(DevelopmentFieldError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn unique_refs(values: &[String], field: &'static str) -> Result<(), DevelopmentFieldError> {
    let mut seen = BTreeSet::new();
    for value in values {
        stable_ref(value, field)?;
        if !seen.insert(value) {
            return Err(DevelopmentFieldError::DuplicateRef(value.clone()));
        }
    }
    Ok(())
}

fn unique_typed_refs(
    values: &[WorkflowUnitRef],
    field: &'static str,
) -> Result<(), DevelopmentFieldError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(DevelopmentFieldError::DuplicateRef(value.to_string()));
        }
    }
    if field.is_empty() {
        return Err(DevelopmentFieldError::InvalidField("workflowUnitRefs"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_development::{
        GitDevelopmentBase, GitReturnEvidence, GitWorktreeBinding, GIT_DEVELOPMENT_WORLD_SCHEMA,
        GIT_RETURN_EVIDENCE_SCHEMA,
    };
    use std::str::FromStr;

    fn project() -> ProjectRef {
        ProjectRef::from_str("project:01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    fn run() -> RunRef {
        RunRef::from_str("run:01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap()
    }

    fn journey() -> JourneyRef {
        JourneyRef::from_str("journey:01ARZ3NDEKTSV4RRFFQ69G5FAX").unwrap()
    }

    fn unit() -> WorkflowUnitRef {
        WorkflowUnitRef::from_str("workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY").unwrap()
    }

    fn git_world() -> GitDevelopmentWorld {
        GitDevelopmentWorld {
            schema: GIT_DEVELOPMENT_WORLD_SCHEMA.into(),
            development_ref: "git-development:field-215".into(),
            base: GitDevelopmentBase {
                project_ref: project(),
                run_ref: run(),
                candidate_ref: "candidate:field-215".into(),
                repository_ref: "repo:agent-system-design".into(),
                base_revision: "885f3c29726dc6cb9397eca246abdfb1c65eff86".into(),
                base_worktree_clean: true,
                source_basis_refs: vec!["source:factory-plan".into()],
                structural_ground_refs: vec!["source:factory-architecture".into()],
            },
            binding: GitWorktreeBinding {
                provider_ref: "aikit:git-provider".into(),
                worktree_ref: "git-worktree:field-215".into(),
                repository_ref: "repo:agent-system-design".into(),
                current_revision: "885f3c29726dc6cb9397eca246abdfb1c65eff86".into(),
                branch: Some("agent/development-field-s4-215".into()),
                material_host_ref: Some("workcell:host-a".into()),
                locator: None,
                clean: true,
                conflicts: Vec::new(),
            },
            returned: Some(GitReturnEvidence {
                schema: GIT_RETURN_EVIDENCE_SCHEMA.into(),
                base_revision: "885f3c29726dc6cb9397eca246abdfb1c65eff86".into(),
                result_revision: "revision:result".into(),
                commits: vec!["commit:implementation".into()],
                diff_ref: Some("diff:field-215".into()),
                uncommitted_diff_ref: None,
                changed_paths: vec!["factory/src/development_field.rs".into()],
                verification_evidence_refs: vec!["evidence:cargo-test".into()],
                claim_refs: Vec::new(),
                conflicts: Vec::new(),
                provider_ref: "aikit:git-provider".into(),
                material_host_ref: Some("workcell:host-b".into()),
            }),
            recognition: None,
        }
    }

    fn field() -> DevelopmentField {
        let world = git_world();
        DevelopmentField::new(
            "development-field:215",
            project(),
            run(),
            journey(),
            "commission:215",
            vec![unit()],
            "Make Development Field the native Factory software-development technique",
            DevelopmentFieldTargets {
                plan_ref: "plan:development-field-s4".into(),
                ux_refs: vec!["ux:factory-development-inspection".into()],
                capability_refs: vec!["capability/factory/development-field".into()],
                source_refs: vec!["source:issue-215".into()],
                self_description_refs: vec!["source:project-self".into()],
            },
            BTreeSet::from([
                DevelopmentEvidenceGrade::D,
                DevelopmentEvidenceGrade::C,
                DevelopmentEvidenceGrade::P,
                DevelopmentEvidenceGrade::M,
                DevelopmentEvidenceGrade::H,
            ]),
            DevelopmentGitBasis::from_git_world(&world),
        )
        .unwrap()
    }

    fn operative() -> AikitOperativeReferences {
        AikitOperativeReferences {
            resolution_ref: "aikit:context-resolution:215".into(),
            praxis_condition_ref: Some("factory:praxis-condition:215".into()),
            execution_disposition_refs: vec!["factory:execution-disposition:215".into()],
            agent_refs: vec!["agent:mahamaya".into()],
            agent_set_refs: vec!["agent-set:development".into()],
            profile_ref: Some("profile:rust".into()),
            harness_ref: Some("harness:codex".into()),
            harness_composition_ref: Some("harness-composition:215".into()),
            session_space_ref: Some("session-space:215".into()),
            agent_session_refs: vec!["agent-session:215".into()],
            source_basis_refs: vec!["aikit:versioned-world:215".into()],
            provider_ref: "aikit".into(),
            provider_revision: "revision:aikit".into(),
        }
    }

    fn evidence(grade: DevelopmentEvidenceGrade, reference: &str) -> DevelopmentEvidenceStanding {
        DevelopmentEvidenceStanding {
            grade,
            owner: "factory".into(),
            evidence_refs: vec![reference.into()],
            source_refs: Vec::new(),
            human_ex_ref: None,
        }
    }

    #[test]
    fn proof_grades_remain_distinct_and_h_requires_human_ex() {
        let mut field = field();
        field.set_aikit_operative(operative()).unwrap();
        field
            .record_material_binding(DevelopmentMaterialBinding {
                owner: "workcell".into(),
                binding_ref: "workcell:binding:b".into(),
                provider_ref: Some("workcell:provider:local".into()),
                revision: Some("revision:b".into()),
                provenance_refs: vec!["workcell:observation:b".into()],
            })
            .unwrap();

        let mut returned = DevelopmentFieldReturn::from_git_world("return:215", &git_world()).unwrap();
        returned.material_binding_refs = vec!["workcell:binding:b".into()];
        returned.evidence = vec![
            evidence(DevelopmentEvidenceGrade::D, "evidence:D"),
            evidence(DevelopmentEvidenceGrade::C, "evidence:C"),
            evidence(DevelopmentEvidenceGrade::P, "evidence:P"),
            evidence(DevelopmentEvidenceGrade::M, "evidence:M"),
        ];
        returned.candidates = vec![DevelopmentCandidateLineage {
            candidate_ref: "candidate:215".into(),
            evidence_refs: vec!["evidence:D".into()],
            recognition_refs: Vec::new(),
        }];
        field.record_return(returned).unwrap();

        assert_eq!(
            field.remaining_proof_grades(),
            BTreeSet::from([DevelopmentEvidenceGrade::H])
        );
        assert!(field.returns[0].candidates[0].recognition_refs.is_empty());

        let mut invalid_h = evidence(DevelopmentEvidenceGrade::H, "evidence:H");
        assert_eq!(
            invalid_h.validate().unwrap_err(),
            DevelopmentFieldError::HumanProofWithoutEx
        );
        invalid_h.human_ex_ref = Some("ex:human-acceptance".into());
        invalid_h.validate().unwrap();
    }

    #[test]
    fn material_relocation_preserves_semantic_and_git_identity() {
        let mut field = field();
        let run_before = field.run_ref.clone();
        let journey_before = field.journey_ref.clone();
        let base_before = field.git_basis.base_revision.clone();
        for suffix in ["a", "b"] {
            field
                .record_material_binding(DevelopmentMaterialBinding {
                    owner: "workcell".into(),
                    binding_ref: format!("workcell:binding:{suffix}"),
                    provider_ref: Some("workcell:provider:local".into()),
                    revision: Some(format!("revision:{suffix}")),
                    provenance_refs: vec![format!("workcell:observation:{suffix}")],
                })
                .unwrap();
        }
        assert_eq!(field.run_ref, run_before);
        assert_eq!(field.journey_ref, journey_before);
        assert_eq!(field.git_basis.base_revision, base_before);
        assert_eq!(field.material_bindings.len(), 2);
    }

    #[test]
    fn direct_execution_is_not_given_fabricated_agency_ancestry() {
        let mut field = field();
        field.set_aikit_operative(operative()).unwrap();
        field
            .record_material_binding(DevelopmentMaterialBinding {
                owner: "external".into(),
                binding_ref: "material:chatgpt-github".into(),
                provider_ref: Some("provider:github".into()),
                revision: None,
                provenance_refs: vec!["provider-run:215".into()],
            })
            .unwrap();
        let mut returned = DevelopmentFieldReturn::from_git_world("return:direct", &git_world()).unwrap();
        returned.material_binding_refs = vec!["material:chatgpt-github".into()];
        returned.actualities = vec![DevelopmentActualityReferences {
            execution_correlation_ref: "execution-correlation:direct".into(),
            actuation_ref: Some("actuation:direct".into()),
            agency_ref: None,
            agent_session_ref: None,
            activity_refs: vec!["activity:direct".into()],
            return_refs: vec!["actuation-return:direct".into()],
        }];
        returned.evidence = vec![evidence(DevelopmentEvidenceGrade::D, "evidence:direct")];
        field.record_return(returned).unwrap();
        assert!(field.returns[0].actualities[0].agency_ref.is_none());
        assert!(field.returns[0].actualities[0].agent_session_ref.is_none());
    }
}
