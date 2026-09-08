use crate::core::run::RunRef;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const PROJECT_DEVELOPMENT_VERSION: &str = "factory.project-development/v1";

/// Run-scoped orientation supplied by native owners. Factory retains these refs;
/// it does not become the owner or resolver of ProjectCentral, Wiki, ProjectMap,
/// CodeIndex, or frontier state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectOrientationCondition {
    pub run_ref: RunRef,
    pub condition_ref: String,
    pub project_ref: String,
    pub projectcentral_contract_ref: Option<String>,
    pub projectcentral_revision: Option<String>,
    pub human_ground_refs: Vec<String>,
    pub semantic_wiki_refs: Vec<String>,
    pub structural_ground_ref: Option<String>,
    pub knowledge_application_ref: Option<String>,
    pub project_map_ref: Option<String>,
    pub frontier_refs: Vec<String>,
}

/// P3 / bounded present determination for one Run.
///
/// `intent_source_ref` is the existing ContextSource/source identity supplied by
/// Central or another native source owner; Factory does not create an Intent
/// identity or copy the source body. `context_resolution_ref` is an opaque ref to
/// the AIKit-owned P4 ContextResolution, which in turn contains activation truth.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundedIntentCondition {
    pub run_ref: RunRef,
    pub condition_ref: String,
    pub intent_source_ref: String,
    pub focus_ref: Option<String>,
    #[serde(default)]
    pub success_condition_refs: Vec<String>,
    #[serde(default)]
    pub constraint_refs: Vec<String>,
    pub context_resolution_ref: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IntentCriterionState {
    Satisfied,
    Unsatisfied,
    Indeterminate,
}

/// Evidence-bearing evaluation of one success condition from the bounded Intent.
/// Conclusive states require evidence; `Indeterminate` remains a truthful return
/// when the Run cannot yet determine the criterion.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentCriterionEvaluation {
    pub criterion_ref: String,
    pub state: IntentCriterionState,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// P5 / returned reality for the bounded Intent.
///
/// Artifact, Claim and Evidence identities remain their existing Factory records.
/// The return does not mutate Vision, Intent, Wiki or any other native source.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundedIntentReturn {
    pub run_ref: RunRef,
    pub return_ref: String,
    pub intent_source_ref: String,
    pub context_resolution_ref: String,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub claim_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub criterion_evaluations: Vec<IntentCriterionEvaluation>,
}

/// Aggregate return is derived rather than stored so Factory cannot retain a
/// summary which contradicts the criterion-level evidence.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BoundedIntentReturnState {
    Satisfied,
    Unsatisfied,
    Indeterminate,
}

/// One attributable semantic <-> source <-> code route returned by the native
/// reflection/knowledge provider. `local_source_ref` is source material, not an
/// assertion that the source is implementation truth.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReflectionAnchor {
    pub run_ref: RunRef,
    pub anchor_ref: String,
    pub semantic_ref: String,
    pub local_source_ref: Option<String>,
    pub code_refs: Vec<String>,
    pub verification_refs: Vec<String>,
    pub route_ref: Option<String>,
    pub provider_ref: String,
    pub provider_revision: String,
    pub relation: String,
}

/// Smallest sufficient resolved praxis condition for a consequential Run.
/// Resolution remains AIKit-owned; Factory records the returned condition.
/// `method_ref` is a compatibility projection label whose value is the
/// Method-classified Skill identity. It is never a Factory-owned Method source.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PraxisCondition {
    pub run_ref: RunRef,
    pub condition_ref: String,
    pub focus_ref: Option<String>,
    pub method_ref: Option<String>,
    pub skill_refs: Vec<String>,
    pub skill_set_refs: Vec<String>,
    pub profile_ref: Option<String>,
    pub usage_overlay_refs: Vec<String>,
    pub context_source_refs: Vec<String>,
    pub action_refs: Vec<String>,
    pub model_ref: Option<String>,
    pub harness_ref: Option<String>,
    pub harness_composition_ref: Option<String>,
    pub agency_ref: Option<String>,
    pub material_condition_refs: Vec<String>,
    pub resolution_ref: Option<String>,
    pub provider_ref: String,
    pub provider_revision: String,
}

/// Forward + returned capability/praxis relation. QL affinity is descriptive
/// metadata only and is never activation authority.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityPraxisRow {
    pub run_ref: RunRef,
    pub row_ref: String,
    pub capability_ref: String,
    pub skill_ref: Option<String>,
    /// Compatibility projection label for a Method-classified Skill ref.
    pub method_ref: Option<String>,
    pub project_target_refs: Vec<String>,
    pub ql_affinity: Option<String>,
    pub use_type: String,
    pub context_source_refs: Vec<String>,
    pub action_refs: Vec<String>,
    pub model_ref: Option<String>,
    pub harness_ref: Option<String>,
    pub agency_ref: Option<String>,
    pub material_condition_refs: Vec<String>,
    pub verification_expectation: String,
    pub observed_fitness_evidence_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DevelopmentObservationKind {
    ProjectReflectionDiscrepancy,
    PraxisFitness,
    InsufficientEvidence,
}

/// A pressure discovered by development that must return through the native
/// owner's mutation/Recognition path rather than being silently applied here.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnerReturnProposal {
    pub owner_ref: String,
    pub source_ref: Option<String>,
    pub proposal_ref: String,
    pub recognition_required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevelopmentObservation {
    pub run_ref: RunRef,
    pub observation_ref: String,
    pub kind: DevelopmentObservationKind,
    pub statement: String,
    pub subject_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub owner_return: Option<OwnerReturnProposal>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanProjectReview {
    pub run_ref: RunRef,
    pub intention_and_ground_refs: Vec<String>,
    pub semantic_refs: Vec<String>,
    pub local_source_refs: Vec<String>,
    pub code_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub discrepancies: Vec<String>,
    pub recognition_returns: Vec<OwnerReturnProposal>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectDevelopmentLedger {
    pub version: String,
    pub run_ref: RunRef,
    pub orientation: Option<ProjectOrientationCondition>,
    /// Additive P3 determination. Old v1 ledgers deserialize with no Intent.
    #[serde(default)]
    pub intent: Option<BoundedIntentCondition>,
    /// Additive P5 return against the active bounded Intent.
    #[serde(default)]
    pub intent_return: Option<BoundedIntentReturn>,
    pub reflection_anchors: Vec<ReflectionAnchor>,
    pub praxis: Option<PraxisCondition>,
    pub capability_rows: Vec<CapabilityPraxisRow>,
    pub observations: Vec<DevelopmentObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectDevelopmentError {
    WrongRun { expected: RunRef, actual: RunRef },
    DuplicateRef(String),
    MissingIntent,
    EmptyIntentSource,
    EmptyContextResolution,
    IntentSourceMismatch { expected: String, actual: String },
    ContextResolutionMismatch { expected: String, actual: String },
    UnknownSuccessCondition(String),
    DuplicateCriterionEvaluation(String),
    ConclusiveCriterionWithoutEvidence(String),
}

impl Display for ProjectDevelopmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongRun { expected, actual } => {
                write!(
                    formatter,
                    "development record belongs to {actual}, expected {expected}"
                )
            }
            Self::DuplicateRef(reference) => {
                write!(formatter, "duplicate development ref: {reference}")
            }
            Self::MissingIntent => write!(
                formatter,
                "bounded Intent must be retained before its return"
            ),
            Self::EmptyIntentSource => write!(formatter, "bounded Intent requires a source ref"),
            Self::EmptyContextResolution => {
                write!(
                    formatter,
                    "bounded Intent requires an AIKit ContextResolution ref"
                )
            }
            Self::IntentSourceMismatch { expected, actual } => write!(
                formatter,
                "Intent return belongs to source {actual}, expected {expected}"
            ),
            Self::ContextResolutionMismatch { expected, actual } => write!(
                formatter,
                "Intent return belongs to ContextResolution {actual}, expected {expected}"
            ),
            Self::UnknownSuccessCondition(reference) => write!(
                formatter,
                "Intent return evaluates unknown success condition: {reference}"
            ),
            Self::DuplicateCriterionEvaluation(reference) => write!(
                formatter,
                "Intent return evaluates success condition more than once: {reference}"
            ),
            Self::ConclusiveCriterionWithoutEvidence(reference) => write!(
                formatter,
                "conclusive Intent criterion requires evidence: {reference}"
            ),
        }
    }
}

impl Error for ProjectDevelopmentError {}

impl ProjectDevelopmentLedger {
    pub fn new(run_ref: RunRef) -> Self {
        Self {
            version: PROJECT_DEVELOPMENT_VERSION.to_owned(),
            run_ref,
            orientation: None,
            intent: None,
            intent_return: None,
            reflection_anchors: Vec::new(),
            praxis: None,
            capability_rows: Vec::new(),
            observations: Vec::new(),
        }
    }

    pub fn set_orientation(
        &mut self,
        condition: ProjectOrientationCondition,
    ) -> Result<(), ProjectDevelopmentError> {
        self.ensure_run(&condition.run_ref)?;
        self.ensure_unique(&condition.condition_ref)?;
        self.orientation = Some(condition);
        Ok(())
    }

    /// Retain the active P3 determination by native source identity. Factory does
    /// not parse a magic filename or become the Intent source owner.
    pub fn set_intent(
        &mut self,
        condition: BoundedIntentCondition,
    ) -> Result<(), ProjectDevelopmentError> {
        self.ensure_run(&condition.run_ref)?;
        self.ensure_unique(&condition.condition_ref)?;
        if condition.intent_source_ref.trim().is_empty() {
            return Err(ProjectDevelopmentError::EmptyIntentSource);
        }
        if condition.context_resolution_ref.trim().is_empty() {
            return Err(ProjectDevelopmentError::EmptyContextResolution);
        }
        self.intent = Some(condition);
        self.intent_return = None;
        Ok(())
    }

    /// Retain P5 returned reality against exactly the P3 source and P4 resolution
    /// that conditioned the Run. The result is evidential; it never mutates the
    /// source, Vision or Wiki by implication.
    pub fn set_intent_return(
        &mut self,
        returned: BoundedIntentReturn,
    ) -> Result<(), ProjectDevelopmentError> {
        self.ensure_run(&returned.run_ref)?;
        self.ensure_unique(&returned.return_ref)?;
        let intent = self
            .intent
            .as_ref()
            .ok_or(ProjectDevelopmentError::MissingIntent)?;
        if returned.intent_source_ref != intent.intent_source_ref {
            return Err(ProjectDevelopmentError::IntentSourceMismatch {
                expected: intent.intent_source_ref.clone(),
                actual: returned.intent_source_ref,
            });
        }
        if returned.context_resolution_ref != intent.context_resolution_ref {
            return Err(ProjectDevelopmentError::ContextResolutionMismatch {
                expected: intent.context_resolution_ref.clone(),
                actual: returned.context_resolution_ref,
            });
        }

        let mut seen = Vec::new();
        for evaluation in &returned.criterion_evaluations {
            if !intent
                .success_condition_refs
                .iter()
                .any(|reference| reference == &evaluation.criterion_ref)
            {
                return Err(ProjectDevelopmentError::UnknownSuccessCondition(
                    evaluation.criterion_ref.clone(),
                ));
            }
            if seen.contains(&evaluation.criterion_ref) {
                return Err(ProjectDevelopmentError::DuplicateCriterionEvaluation(
                    evaluation.criterion_ref.clone(),
                ));
            }
            seen.push(evaluation.criterion_ref.clone());
            if !matches!(evaluation.state, IntentCriterionState::Indeterminate)
                && evaluation.evidence_refs.is_empty()
            {
                return Err(ProjectDevelopmentError::ConclusiveCriterionWithoutEvidence(
                    evaluation.criterion_ref.clone(),
                ));
            }
        }

        self.intent_return = Some(returned);
        Ok(())
    }

    /// Derive the whole bounded-Intent return from criterion-level results.
    /// Missing criteria or an ungrounded/no-criteria Intent remain indeterminate.
    pub fn intent_return_state(&self) -> Option<BoundedIntentReturnState> {
        let intent = self.intent.as_ref()?;
        let returned = self.intent_return.as_ref()?;

        if returned
            .criterion_evaluations
            .iter()
            .any(|evaluation| evaluation.state == IntentCriterionState::Unsatisfied)
        {
            return Some(BoundedIntentReturnState::Unsatisfied);
        }
        if intent.success_condition_refs.is_empty()
            || returned
                .criterion_evaluations
                .iter()
                .any(|evaluation| evaluation.state == IntentCriterionState::Indeterminate)
            || intent.success_condition_refs.iter().any(|criterion| {
                !returned
                    .criterion_evaluations
                    .iter()
                    .any(|evaluation| &evaluation.criterion_ref == criterion)
            })
        {
            return Some(BoundedIntentReturnState::Indeterminate);
        }
        Some(BoundedIntentReturnState::Satisfied)
    }

    pub fn add_reflection_anchor(
        &mut self,
        anchor: ReflectionAnchor,
    ) -> Result<(), ProjectDevelopmentError> {
        self.ensure_run(&anchor.run_ref)?;
        self.ensure_unique(&anchor.anchor_ref)?;
        self.reflection_anchors.push(anchor);
        Ok(())
    }

    pub fn set_praxis(
        &mut self,
        condition: PraxisCondition,
    ) -> Result<(), ProjectDevelopmentError> {
        self.ensure_run(&condition.run_ref)?;
        self.ensure_unique(&condition.condition_ref)?;
        self.praxis = Some(condition);
        Ok(())
    }

    pub fn add_capability_row(
        &mut self,
        row: CapabilityPraxisRow,
    ) -> Result<(), ProjectDevelopmentError> {
        self.ensure_run(&row.run_ref)?;
        self.ensure_unique(&row.row_ref)?;
        self.capability_rows.push(row);
        Ok(())
    }

    pub fn add_observation(
        &mut self,
        observation: DevelopmentObservation,
    ) -> Result<(), ProjectDevelopmentError> {
        self.ensure_run(&observation.run_ref)?;
        self.ensure_unique(&observation.observation_ref)?;
        self.observations.push(observation);
        Ok(())
    }

    /// Reverse traversal over retained AIKit/provider results. This is not a
    /// Factory CodeIndex lookup and does not infer new mappings.
    pub fn anchors_for_code(&self, code_ref: &str) -> Vec<&ReflectionAnchor> {
        self.reflection_anchors
            .iter()
            .filter(|anchor| {
                anchor
                    .code_refs
                    .iter()
                    .any(|candidate| candidate == code_ref)
            })
            .collect()
    }

    /// Human-altitude review: intention -> meaning -> changed executable refs ->
    /// evidence -> discrepancy -> Recognition return. Exact refs remain available
    /// for progressive inspection without making graph topology the review surface.
    pub fn human_review(&self) -> HumanProjectReview {
        let mut intention_and_ground_refs = self
            .orientation
            .as_ref()
            .map(|orientation| orientation.human_ground_refs.clone())
            .unwrap_or_default();
        if let Some(intent) = &self.intent {
            push_unique(
                &mut intention_and_ground_refs,
                intent.intent_source_ref.clone(),
            );
        }

        let mut semantic_refs = Vec::new();
        let mut local_source_refs = Vec::new();
        let mut code_refs = Vec::new();
        let mut evidence_refs = Vec::new();

        for anchor in &self.reflection_anchors {
            push_unique(&mut semantic_refs, anchor.semantic_ref.clone());
            if let Some(source_ref) = &anchor.local_source_ref {
                push_unique(&mut local_source_refs, source_ref.clone());
            }
            for code_ref in &anchor.code_refs {
                push_unique(&mut code_refs, code_ref.clone());
            }
            for evidence_ref in &anchor.verification_refs {
                push_unique(&mut evidence_refs, evidence_ref.clone());
            }
        }

        if let Some(returned) = &self.intent_return {
            for evidence_ref in &returned.evidence_refs {
                push_unique(&mut evidence_refs, evidence_ref.clone());
            }
            for evaluation in &returned.criterion_evaluations {
                for evidence_ref in &evaluation.evidence_refs {
                    push_unique(&mut evidence_refs, evidence_ref.clone());
                }
            }
        }

        let mut discrepancies = Vec::new();
        let mut recognition_returns = Vec::new();
        for observation in &self.observations {
            for evidence_ref in &observation.evidence_refs {
                push_unique(&mut evidence_refs, evidence_ref.clone());
            }
            if matches!(
                observation.kind,
                DevelopmentObservationKind::ProjectReflectionDiscrepancy
            ) {
                discrepancies.push(observation.statement.clone());
            }
            if let Some(owner_return) = &observation.owner_return {
                if owner_return.recognition_required {
                    recognition_returns.push(owner_return.clone());
                }
            }
        }

        HumanProjectReview {
            run_ref: self.run_ref.clone(),
            intention_and_ground_refs,
            semantic_refs,
            local_source_refs,
            code_refs,
            evidence_refs,
            discrepancies,
            recognition_returns,
        }
    }

    fn ensure_run(&self, actual: &RunRef) -> Result<(), ProjectDevelopmentError> {
        if actual == &self.run_ref {
            Ok(())
        } else {
            Err(ProjectDevelopmentError::WrongRun {
                expected: self.run_ref.clone(),
                actual: actual.clone(),
            })
        }
    }

    fn ensure_unique(&self, candidate: &str) -> Result<(), ProjectDevelopmentError> {
        let duplicate = self
            .orientation
            .as_ref()
            .map(|record| record.condition_ref.as_str() == candidate)
            .unwrap_or(false)
            || self
                .intent
                .as_ref()
                .map(|record| record.condition_ref.as_str() == candidate)
                .unwrap_or(false)
            || self
                .intent_return
                .as_ref()
                .map(|record| record.return_ref.as_str() == candidate)
                .unwrap_or(false)
            || self
                .praxis
                .as_ref()
                .map(|record| record.condition_ref.as_str() == candidate)
                .unwrap_or(false)
            || self
                .reflection_anchors
                .iter()
                .any(|record| record.anchor_ref == candidate)
            || self
                .capability_rows
                .iter()
                .any(|record| record.row_ref == candidate)
            || self
                .observations
                .iter()
                .any(|record| record.observation_ref == candidate);

        if duplicate {
            Err(ProjectDevelopmentError::DuplicateRef(candidate.to_owned()))
        } else {
            Ok(())
        }
    }
}

fn push_unique(target: &mut Vec<String>, value: String) {
    if !target.contains(&value) {
        target.push(value);
    }
}
