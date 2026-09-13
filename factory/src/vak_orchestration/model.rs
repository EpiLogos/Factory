use crate::orchestration::OrchestrationError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const VAK_ORCHESTRATION_CONTRACT: &str = "factory.vak-orchestration/v1";
pub const QL_C_PRIME_PROFILE_CONTRACT: &str = "ql.vak-composition.profile/v1";
pub const AIKIT_OPERATIVE_SCOPE_CONTRACT: &str = "aikit.operative-scope/v1";

const PARTICIPATION: [&str; 2] = ["dialogical", "authorised-undertaking"];
const CONTENT: [&str; 7] = ["CT0", "CT1", "CT2", "CT3", "CT4", "CT4b′", "CT5"];
const POSITION: [&str; 6] = ["4.0", "4.1", "4.2", "4.3", "4.4", "4.5"];
const FRAME: [&str; 7] = ["CF1", "CF2", "CF3", "CF4", "CF5", "CF6", "CF7"];
const THREAD: [&str; 6] = ["CFP0", "CFP1", "CFP2", "CFP3", "CFP4", "CFP5"];
const SEQUENCE: [&str; 6] = ["CS0", "CS1", "CS2", "CS3", "CS4", "CS5"];
const DIRECTION: [&str; 2] = ["forward", "returning"];

pub(crate) fn required(value: &str, field: &'static str) -> Result<(), VakOrchestrationError> {
    if value.trim().is_empty() || value.len() > 16_384 || value.contains('\0') {
        Err(VakOrchestrationError::InvalidField(field))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CPrimeExecutionBinding {
    pub contract: String,
    pub ql_binding_ref: String,
    pub ql_binding_revision: String,
    pub actor_ref: String,
    pub whole_ref: String,
    pub subject_ref: String,
    pub participation: String,
    pub content: String,
    pub position: String,
    pub frame: String,
    pub thread: String,
    pub sequence: String,
    pub direction: String,
    pub ai_kit_scope_contract: String,
    pub ai_kit_resolve_path_ref: String,
    pub context_resolution_ref: String,
    pub source_refs: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undertaking_authority_ref: Option<String>,
}

impl CPrimeExecutionBinding {
    pub fn validate(&self) -> Result<(), VakOrchestrationError> {
        if self.contract != QL_C_PRIME_PROFILE_CONTRACT {
            return Err(VakOrchestrationError::WrongContract("QL C-prime"));
        }
        if self.ai_kit_scope_contract != AIKIT_OPERATIVE_SCOPE_CONTRACT {
            return Err(VakOrchestrationError::WrongContract(
                "AIKit operative scope",
            ));
        }
        for (value, field) in [
            (&self.ql_binding_ref, "qlBindingRef"),
            (&self.ql_binding_revision, "qlBindingRevision"),
            (&self.actor_ref, "actorRef"),
            (&self.whole_ref, "wholeRef"),
            (&self.subject_ref, "subjectRef"),
            (&self.ai_kit_resolve_path_ref, "aiKitResolvePathRef"),
            (&self.context_resolution_ref, "contextResolutionRef"),
        ] {
            required(value, field)?;
        }
        if !PARTICIPATION.contains(&self.participation.as_str())
            || !CONTENT.contains(&self.content.as_str())
            || !POSITION.contains(&self.position.as_str())
            || !FRAME.contains(&self.frame.as_str())
            || !THREAD.contains(&self.thread.as_str())
            || !SEQUENCE.contains(&self.sequence.as_str())
            || !DIRECTION.contains(&self.direction.as_str())
        {
            return Err(VakOrchestrationError::InvalidCPrimeProfile);
        }
        if self.source_refs.is_empty() || self.source_refs.len() > 256 {
            return Err(VakOrchestrationError::InvalidField("sourceRefs"));
        }
        for source in &self.source_refs {
            required(source, "sourceRef")?;
        }
        match self.participation.as_str() {
            "authorised-undertaking" => required(
                self.undertaking_authority_ref
                    .as_deref()
                    .unwrap_or_default(),
                "undertakingAuthorityRef",
            )?,
            "dialogical" if self.undertaking_authority_ref.is_some() => {
                return Err(VakOrchestrationError::UnexpectedAuthority);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn thread_form(&self) -> ThreadForm {
        match self.thread.as_str() {
            "CFP0" => ThreadForm::Single,
            "CFP1" => ThreadForm::Parallel,
            "CFP2" => ThreadForm::Chain,
            "CFP3" => ThreadForm::Fusion,
            "CFP4" => ThreadForm::Sustained,
            "CFP5" => ThreadForm::Nested,
            _ => unreachable!("validated C-prime thread"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreadForm {
    Single,
    Parallel,
    Chain,
    Fusion,
    Sustained,
    Nested,
}
impl ThreadForm {
    pub const fn musical_role(self) -> &'static str {
        match self {
            Self::Single => "single-voice",
            Self::Parallel => "chord",
            Self::Chain => "melody",
            Self::Fusion => "fusion",
            Self::Sustained => "drone",
            Self::Nested => "canon",
        }
    }
}

#[derive(Debug)]
pub enum VakOrchestrationError {
    Orchestration(OrchestrationError),
    InvalidField(&'static str),
    UnexpectedField(&'static str),
    WrongContract(&'static str),
    InvalidCPrimeProfile,
    UnexpectedAuthority,
    InvalidUnitShape,
    DuplicateUnit(String),
    UnknownUnit(String),
    UnknownPerformanceUnit(String),
    SubjectMismatch(String),
    SourceScopeWidening(String),
    NotIndependent {
        left: String,
        right: String,
    },
    InvalidChain {
        predecessor: String,
        successor: String,
    },
    PredecessorNotReturned(String),
    MissingPredecessorMaterial(String),
    InvalidPredecessorMaterial(String),
    FusionConcernMismatch,
    MissingBarrier(String),
    FusionBarrierMismatch,
    InvalidNestedTopology,
    NestedParentNotStarted(String),
    LaunchSetMismatch,
    LaunchUnitMismatch,
    MissingLaunchEvidence(String),
    SequenceComplete,
    WrongContinuationForm,
    WrongFusionForm,
    FusionNotReady,
    SustainedStopAlreadySatisfied,
    SustainedStopNotSatisfied,
    SustainedStopNotApplicable,
    PerformanceNotSettled,
    InvalidRunRef,
    InvalidZTransition,
    MissingPerformanceEvidence,
    RecompositionDidNotChangeBinding,
}
impl Display for VakOrchestrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Factory Vāk orchestration error: {self:?}")
    }
}
impl Error for VakOrchestrationError {}
impl From<OrchestrationError> for VakOrchestrationError {
    fn from(value: OrchestrationError) -> Self {
        Self::Orchestration(value)
    }
}
