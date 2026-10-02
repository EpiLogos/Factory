use crate::core::identity::{Ref, RefParseError, Revision};
use crate::core::run::{ProjectRef, RunRef};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub const JOURNEY_SCHEMA: &str = "factory.journey/v1";

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct JourneyRef(Ref);

impl JourneyRef {
    pub fn new(reference: Ref) -> Result<Self, JourneyError> {
        if reference.kind() != "journey" {
            return Err(JourneyError::WrongRefKind {
                expected: "journey",
                actual: reference.kind().to_owned(),
            });
        }
        Ok(Self(reference))
    }

    pub fn as_ref(&self) -> &Ref {
        &self.0
    }
}

impl Display for JourneyRef {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl FromStr for JourneyRef {
    type Err = JourneyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value.parse::<Ref>()?)
    }
}

impl Serialize for JourneyRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JourneyRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let reference = Ref::deserialize(deserializer)?;
        Self::new(reference).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JourneyStatus {
    Proposed,
    Active,
    Paused,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct JourneyCommission {
    pub purpose: String,
    #[serde(default)]
    pub commission_ref: Option<String>,
    #[serde(default)]
    pub why_refs: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct JourneyParticipant {
    /// Opaque semantic participant ref owned outside Journey. Typical owners are
    /// Central human/Agent/AgentSet identity and O:I SharedField projection.
    pub participant_ref: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct JourneyRunLink {
    pub run_ref: RunRef,
    /// Exact Journey revision from which the bounded Run was commissioned.
    pub journey_revision: Revision,
    #[serde(default)]
    pub basis_refs: Vec<String>,
    #[serde(default)]
    pub agent_session_refs: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct JourneyReturn {
    pub return_ref: String,
    #[serde(default)]
    pub run_refs: Vec<RunRef>,
    #[serde(default)]
    pub basis_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    /// Recognition is an external Factory authority correlation. Journey never
    /// converts a Return into recognised source merely because it was returned.
    #[serde(default)]
    pub recognition_ref: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct JourneyRecognitionLink {
    pub recognition_ref: String,
    pub subject_ref: String,
    #[serde(default)]
    pub basis_refs: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Journey {
    pub schema: String,
    #[serde(rename = "ref")]
    pub journey_ref: JourneyRef,
    pub revision: Revision,
    pub project_ref: ProjectRef,
    #[serde(default)]
    pub related_projects: Vec<ProjectRef>,
    pub commission: JourneyCommission,
    #[serde(default)]
    pub knowledge_refs: Vec<String>,
    #[serde(default)]
    pub architecture_refs: Vec<String>,
    #[serde(default)]
    pub design_refs: Vec<String>,
    #[serde(default)]
    pub wayfinder_refs: Vec<String>,
    #[serde(default)]
    pub participants: Vec<JourneyParticipant>,
    #[serde(default)]
    pub flow_refs: Vec<String>,
    #[serde(default)]
    pub runs: Vec<JourneyRunLink>,
    #[serde(default)]
    pub agent_session_refs: Vec<String>,
    #[serde(default)]
    pub activity_refs: Vec<String>,
    #[serde(default)]
    pub returns: Vec<JourneyReturn>,
    #[serde(default)]
    pub recognitions: Vec<JourneyRecognitionLink>,
    /// External material/context correlations (for example Workcell placement
    /// evidence). They are deliberately not part of Journey identity.
    #[serde(default)]
    pub material_context_refs: Vec<String>,
    pub status: JourneyStatus,
    pub frontier: String,
    pub started_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
}

impl Journey {
    pub fn new(
        journey_ref: JourneyRef,
        project_ref: ProjectRef,
        commission: JourneyCommission,
        frontier: impl Into<String>,
        started_at: impl Into<String>,
    ) -> Result<Self, JourneyError> {
        validate_text(&commission.purpose, "Journey commission purpose")?;
        validate_refs(&commission.why_refs, "Journey commission why_refs")?;
        if let Some(reference) = commission.commission_ref.as_deref() {
            validate_ref_text(reference, "Journey commission_ref")?;
        }
        let frontier = frontier.into();
        let started_at = started_at.into();
        validate_text(&frontier, "Journey frontier")?;
        validate_timestamp(&started_at, "Journey started_at")?;
        Ok(Self {
            schema: JOURNEY_SCHEMA.into(),
            journey_ref,
            revision: Revision::INITIAL,
            project_ref,
            related_projects: Vec::new(),
            commission,
            knowledge_refs: Vec::new(),
            architecture_refs: Vec::new(),
            design_refs: Vec::new(),
            wayfinder_refs: Vec::new(),
            participants: Vec::new(),
            flow_refs: Vec::new(),
            runs: Vec::new(),
            agent_session_refs: Vec::new(),
            activity_refs: Vec::new(),
            returns: Vec::new(),
            recognitions: Vec::new(),
            material_context_refs: Vec::new(),
            status: JourneyStatus::Active,
            frontier,
            started_at,
            completed_at: None,
        })
    }

    pub fn add_participant(&mut self, participant: JourneyParticipant) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        validate_ref_text(&participant.participant_ref, "Journey participant_ref")?;
        if let Some(role) = participant.role.as_deref() {
            validate_text(role, "Journey participant role")?;
        }
        upsert_by(&mut self.participants, participant, |item| {
            item.participant_ref.clone()
        });
        self.bump_revision()
    }

    pub fn add_run(
        &mut self,
        run_ref: RunRef,
        basis_refs: Vec<String>,
        agent_session_refs: Vec<String>,
    ) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        validate_refs(&basis_refs, "Journey Run basis_refs")?;
        validate_refs(&agent_session_refs, "Journey Run AgentSession refs")?;
        if self.runs.iter().any(|link| link.run_ref == run_ref) {
            return Err(JourneyError::DuplicateRun(run_ref.to_string()));
        }
        let journey_revision = self.revision;
        for session in &agent_session_refs {
            push_unique(&mut self.agent_session_refs, session.clone());
        }
        self.runs.push(JourneyRunLink {
            run_ref,
            journey_revision,
            basis_refs,
            agent_session_refs,
        });
        self.bump_revision()
    }

    pub(crate) fn correlate_flow(&mut self, flow_ref: String) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        validate_ref_text(&flow_ref, "Journey Flow source ref")?;
        push_unique(&mut self.flow_refs, flow_ref);
        self.bump_revision()
    }

    pub fn correlate_activity(
        &mut self,
        activity_ref: impl Into<String>,
    ) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        let activity_ref = activity_ref.into();
        validate_ref_text(&activity_ref, "Journey Activity ref")?;
        push_unique(&mut self.activity_refs, activity_ref);
        self.bump_revision()
    }

    pub fn correlate_material_context(
        &mut self,
        material_context_ref: impl Into<String>,
    ) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        let material_context_ref = material_context_ref.into();
        validate_ref_text(&material_context_ref, "Journey material context ref")?;
        push_unique(&mut self.material_context_refs, material_context_ref);
        self.bump_revision()
    }

    pub fn record_return(&mut self, returned: JourneyReturn) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        validate_ref_text(&returned.return_ref, "Journey Return ref")?;
        validate_text(&returned.summary, "Journey Return summary")?;
        validate_refs(&returned.basis_refs, "Journey Return basis_refs")?;
        validate_refs(&returned.evidence_refs, "Journey Return evidence_refs")?;
        if let Some(reference) = returned.recognition_ref.as_deref() {
            validate_ref_text(reference, "Journey Return Recognition ref")?;
        }
        if returned.run_refs.is_empty() {
            return Err(JourneyError::ReturnWithoutRun(returned.return_ref));
        }
        for run in &returned.run_refs {
            if !self.runs.iter().any(|link| &link.run_ref == run) {
                return Err(JourneyError::UnknownRun(run.to_string()));
            }
        }
        if self
            .returns
            .iter()
            .any(|existing| existing.return_ref == returned.return_ref)
        {
            return Err(JourneyError::DuplicateReturn(returned.return_ref));
        }
        self.returns.push(returned);
        self.bump_revision()
    }

    pub fn recognize(
        &mut self,
        recognition_ref: impl Into<String>,
        subject_ref: impl Into<String>,
        basis_refs: Vec<String>,
    ) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        let recognition_ref = recognition_ref.into();
        let subject_ref = subject_ref.into();
        validate_ref_text(&recognition_ref, "Journey Recognition ref")?;
        validate_ref_text(&subject_ref, "Journey Recognition subject")?;
        validate_refs(&basis_refs, "Journey Recognition basis_refs")?;
        if self
            .recognitions
            .iter()
            .any(|link| link.recognition_ref == recognition_ref)
        {
            return Err(JourneyError::DuplicateRecognition(recognition_ref));
        }
        self.recognitions.push(JourneyRecognitionLink {
            recognition_ref,
            subject_ref,
            basis_refs,
        });
        self.bump_revision()
    }

    pub fn set_frontier(&mut self, frontier: impl Into<String>) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        let frontier = frontier.into();
        validate_text(&frontier, "Journey frontier")?;
        self.frontier = frontier;
        self.bump_revision()
    }

    pub fn complete(
        &mut self,
        completed_at: impl Into<String>,
        final_return_ref: &str,
        recognition_ref: &str,
    ) -> Result<(), JourneyError> {
        self.ensure_mutable()?;
        if self.runs.is_empty() {
            return Err(JourneyError::CompletionWithoutRun);
        }
        let returned = self
            .returns
            .iter()
            .find(|returned| returned.return_ref == final_return_ref)
            .ok_or_else(|| JourneyError::MissingFinalReturn(final_return_ref.to_owned()))?;
        let recognised = returned.recognition_ref.as_deref() == Some(recognition_ref)
            || self.recognitions.iter().any(|link| {
                link.recognition_ref == recognition_ref && link.subject_ref == final_return_ref
            });
        if !recognised {
            return Err(JourneyError::UnrecognisedFinalReturn {
                return_ref: final_return_ref.to_owned(),
                recognition_ref: recognition_ref.to_owned(),
            });
        }
        let completed_at = completed_at.into();
        validate_timestamp(&completed_at, "Journey completed_at")?;
        self.status = JourneyStatus::Completed;
        self.completed_at = Some(completed_at);
        self.frontier = format!("returned:{final_return_ref}");
        self.bump_revision()
    }

    pub fn validate(&self) -> Result<(), JourneyError> {
        if self.schema != JOURNEY_SCHEMA {
            return Err(JourneyError::Schema(self.schema.clone()));
        }
        validate_text(&self.commission.purpose, "Journey commission purpose")?;
        validate_text(&self.frontier, "Journey frontier")?;
        validate_timestamp(&self.started_at, "Journey started_at")?;
        if let Some(completed_at) = self.completed_at.as_deref() {
            validate_timestamp(completed_at, "Journey completed_at")?;
        }
        if self.status == JourneyStatus::Completed && self.completed_at.is_none() {
            return Err(JourneyError::CompletedWithoutTimestamp);
        }
        if self.status != JourneyStatus::Completed && self.completed_at.is_some() {
            return Err(JourneyError::TimestampOnOpenJourney);
        }
        validate_refs(&self.agent_session_refs, "Journey AgentSession refs")?;
        validate_refs(&self.activity_refs, "Journey Activity refs")?;
        validate_refs(&self.material_context_refs, "Journey material context refs")?;
        Ok(())
    }

    fn ensure_mutable(&self) -> Result<(), JourneyError> {
        if matches!(
            self.status,
            JourneyStatus::Completed | JourneyStatus::Abandoned
        ) {
            return Err(JourneyError::Terminal(self.status));
        }
        Ok(())
    }

    fn bump_revision(&mut self) -> Result<(), JourneyError> {
        self.revision = self.revision.next().ok_or(JourneyError::RevisionOverflow)?;
        Ok(())
    }
}

fn validate_text(value: &str, name: &str) -> Result<(), JourneyError> {
    if value.trim().is_empty() {
        return Err(JourneyError::InvalidText(name.to_owned()));
    }
    Ok(())
}

fn validate_ref_text(value: &str, name: &str) -> Result<(), JourneyError> {
    if value.trim().is_empty() || value.chars().any(char::is_whitespace) {
        return Err(JourneyError::InvalidRefText(name.to_owned()));
    }
    Ok(())
}

fn validate_refs(values: &[String], name: &str) -> Result<(), JourneyError> {
    for value in values {
        validate_ref_text(value, name)?;
    }
    Ok(())
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn upsert_by<T, K: Eq>(values: &mut Vec<T>, value: T, key: impl Fn(&T) -> K) {
    let value_key = key(&value);
    if let Some(index) = values
        .iter()
        .position(|existing| key(existing) == value_key)
    {
        values[index] = value;
    } else {
        values.push(value);
    }
}

fn validate_timestamp(value: &str, name: &str) -> Result<(), JourneyError> {
    // Factory deliberately does not add a time library for this portable core.
    // Enforce the ISO/RFC3339 structural floor used by the existing contracts.
    if value.len() < 20 || !value.contains('T') || !(value.ends_with('Z') || value.contains('+')) {
        return Err(JourneyError::InvalidTimestamp(name.to_owned()));
    }
    Ok(())
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum JourneyError {
    RefParse(RefParseError),
    WrongRefKind {
        expected: &'static str,
        actual: String,
    },
    Schema(String),
    InvalidText(String),
    InvalidRefText(String),
    InvalidTimestamp(String),
    DuplicateRun(String),
    UnknownRun(String),
    ReturnWithoutRun(String),
    DuplicateReturn(String),
    DuplicateRecognition(String),
    CompletionWithoutRun,
    MissingFinalReturn(String),
    UnrecognisedFinalReturn {
        return_ref: String,
        recognition_ref: String,
    },
    CompletedWithoutTimestamp,
    TimestampOnOpenJourney,
    Terminal(JourneyStatus),
    RevisionOverflow,
}

impl From<RefParseError> for JourneyError {
    fn from(error: RefParseError) -> Self {
        Self::RefParse(error)
    }
}

impl Display for JourneyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RefParse(error) => Display::fmt(error, formatter),
            Self::WrongRefKind { expected, actual } => {
                write!(formatter, "expected {expected} Ref, found {actual} Ref")
            }
            Self::Schema(schema) => write!(formatter, "unsupported Journey schema {schema}"),
            Self::InvalidText(name) => write!(formatter, "{name} must be non-empty"),
            Self::InvalidRefText(name) => {
                write!(formatter, "{name} must be a non-empty opaque ref")
            }
            Self::InvalidTimestamp(name) => {
                write!(formatter, "{name} must be an ISO/RFC3339 timestamp")
            }
            Self::DuplicateRun(reference) => {
                write!(formatter, "Journey already contains Run {reference}")
            }
            Self::UnknownRun(reference) => write!(
                formatter,
                "Journey Return references unknown Run {reference}"
            ),
            Self::ReturnWithoutRun(reference) => write!(
                formatter,
                "Journey Return {reference} must cite at least one Run"
            ),
            Self::DuplicateReturn(reference) => {
                write!(formatter, "Journey already contains Return {reference}")
            }
            Self::DuplicateRecognition(reference) => write!(
                formatter,
                "Journey already contains Recognition {reference}"
            ),
            Self::CompletionWithoutRun => {
                write!(formatter, "Journey cannot complete without a bounded Run")
            }
            Self::MissingFinalReturn(reference) => {
                write!(formatter, "Journey has no final Return {reference}")
            }
            Self::UnrecognisedFinalReturn {
                return_ref,
                recognition_ref,
            } => write!(
                formatter,
                "Journey Return {return_ref} has not been accepted by Recognition {recognition_ref}"
            ),
            Self::CompletedWithoutTimestamp => {
                write!(formatter, "completed Journey requires completed_at")
            }
            Self::TimestampOnOpenJourney => write!(
                formatter,
                "non-completed Journey must not declare completed_at"
            ),
            Self::Terminal(status) => {
                write!(formatter, "terminal Journey {status:?} cannot be mutated")
            }
            Self::RevisionOverflow => write!(formatter, "Journey revision overflow"),
        }
    }
}

impl Error for JourneyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use ulid::Ulid;

    fn internal_ref(kind: &str) -> Ref {
        Ref::new(kind, Ulid::new()).unwrap()
    }

    fn journey() -> Journey {
        Journey::new(
            JourneyRef::new(internal_ref("journey")).unwrap(),
            ProjectRef::try_from(internal_ref("project")).unwrap(),
            JourneyCommission {
                purpose: "Bring O:I inhabitation to native-main readiness".into(),
                commission_ref: Some("wayfinder:oi-155".into()),
                why_refs: vec!["position:world-situated-agency".into()],
            },
            "recursive-worlds",
            "2026-08-31T10:00:00Z",
        )
        .unwrap()
    }

    #[test]
    fn journey_spans_bounded_runs_and_sessions_without_collapsing_them() {
        let mut journey = journey();
        let run_one = RunRef::try_from(internal_ref("run")).unwrap();
        let run_two = RunRef::try_from(internal_ref("run")).unwrap();
        let journey_ref = journey.journey_ref.clone();

        journey
            .add_run(
                run_one.clone(),
                vec!["basis:main-a".into()],
                vec!["agent-session:alpha".into()],
            )
            .unwrap();
        journey
            .add_run(
                run_two.clone(),
                vec!["basis:main-b".into()],
                vec!["agent-session:beta".into()],
            )
            .unwrap();

        assert_eq!(journey.journey_ref, journey_ref);
        assert_eq!(journey.runs.len(), 2);
        assert_ne!(journey.runs[0].run_ref, journey.runs[1].run_ref);
        assert_eq!(
            journey.agent_session_refs,
            vec!["agent-session:alpha", "agent-session:beta"]
        );
    }

    #[test]
    fn relocation_changes_material_correlation_not_journey_identity() {
        let mut journey = journey();
        let identity = journey.journey_ref.clone();
        journey
            .correlate_material_context("workcell:local")
            .unwrap();
        journey
            .correlate_material_context("workcell:vm-lan-gpu")
            .unwrap();
        assert_eq!(journey.journey_ref, identity);
        assert_eq!(
            journey.material_context_refs,
            vec!["workcell:local", "workcell:vm-lan-gpu"]
        );
    }

    #[test]
    fn return_and_recognition_preserve_past_run_evidence_and_gate_completion() {
        let mut journey = journey();
        let run = RunRef::try_from(internal_ref("run")).unwrap();
        journey
            .add_run(
                run.clone(),
                vec!["basis:accepted-main".into()],
                vec!["agent-session:development".into()],
            )
            .unwrap();
        journey
            .record_return(JourneyReturn {
                return_ref: "return:journey-final".into(),
                run_refs: vec![run.clone()],
                basis_refs: vec!["basis:accepted-main".into()],
                evidence_refs: vec!["evidence:w11".into()],
                recognition_ref: None,
                summary: "The developmental undertaking has returned with exact evidence.".into(),
            })
            .unwrap();

        assert!(matches!(
            journey.complete(
                "2026-08-31T11:00:00Z",
                "return:journey-final",
                "recognition:final"
            ),
            Err(JourneyError::UnrecognisedFinalReturn { .. })
        ));
        assert_eq!(journey.runs[0].basis_refs, vec!["basis:accepted-main"]);

        journey
            .recognize(
                "recognition:final",
                "return:journey-final",
                vec!["basis:accepted-main".into(), "evidence:w11".into()],
            )
            .unwrap();
        journey
            .complete(
                "2026-08-31T11:00:00Z",
                "return:journey-final",
                "recognition:final",
            )
            .unwrap();
        assert_eq!(journey.status, JourneyStatus::Completed);
        assert_eq!(journey.runs[0].run_ref, run);
        assert!(journey.validate().is_ok());
    }

    #[test]
    fn activity_and_participants_are_correlations_not_execution_authority() {
        let mut journey = journey();
        journey
            .add_participant(JourneyParticipant {
                participant_ref: "agent-set:development".into(),
                role: Some("developer".into()),
            })
            .unwrap();
        journey.correlate_activity("activity:build-1").unwrap();
        let json = serde_json::to_value(&journey).unwrap();
        assert_eq!(
            json["participants"][0]["participant_ref"],
            "agent-set:development"
        );
        assert!(json.get("invocation_authority").is_none());
        assert_eq!(json["activity_refs"][0], "activity:build-1");
    }
}
