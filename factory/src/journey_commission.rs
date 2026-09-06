use crate::agent_capability_intake::{AgentCapabilityIntake, AgentCapabilityIntakeReading};
use crate::core::run::RunRef;
use crate::journey::{Journey, JourneyRef, JourneyStatus};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display};

pub const JOURNEY_COMMISSION_SCHEMA: &str = "factory.journey-commission/v1";

/// Developmental accountability is a relation to an existing semantic Agent,
/// AgentSet or Agency. Factory does not own these identities and this relation
/// does not grant Actuation/invocation authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum JourneyAccountableSubject {
    Agent { agent_ref: String },
    AgentSet { agent_set_ref: String },
    Agency { agency_ref: String },
}

impl JourneyAccountableSubject {
    pub fn reference(&self) -> &str {
        match self {
            Self::Agent { agent_ref } => agent_ref,
            Self::AgentSet { agent_set_ref } => agent_set_ref,
            Self::Agency { agency_ref } => agency_ref,
        }
    }
}

/// Developmental state survives process/session absence. It describes the
/// commission/Journey condition rather than daemon or AgentSession lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommissionState {
    Commissioned,
    Active,
    WaitingExternal,
    WaitingRecognition,
    Paused,
    Blocked,
    Returned,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyHandoff {
    pub from: JourneyAccountableSubject,
    pub to: JourneyAccountableSubject,
    /// Actuation-owned determination/delegation provenance retained opaquely.
    pub determination_ref: String,
    /// Returned work/provenance from the previous accountable Agency where present.
    #[serde(default)]
    pub return_ref: Option<String>,
    pub journey_revision: u64,
}

/// Additive application relation over `factory.journey/v1`.
///
/// Journey continues to own developmental identity/frontier/Run/Return relations.
/// This record makes "who currently carries the return?" explicit without
/// introducing a Worker/Bot/runtime/scheduler ontology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyCommissionState {
    pub schema: String,
    pub journey_ref: JourneyRef,
    pub commission_ref: String,
    pub commissioner_ref: String,
    pub accountable: JourneyAccountableSubject,
    pub intended_difference: String,
    #[serde(default)]
    pub closure_condition_refs: Vec<String>,
    #[serde(default)]
    pub bounds_refs: Vec<String>,
    #[serde(default)]
    pub escalation_refs: Vec<String>,
    pub state: CommissionState,
    /// Exact Journey revision against which this application relation was last
    /// reconciled. This is evidence, not a second Journey revision authority.
    pub journey_revision: u64,
    #[serde(default)]
    pub handoffs: Vec<JourneyHandoff>,
    /// External O:I/native Attention refs. Factory does not create an Attention store.
    #[serde(default)]
    pub attention_refs: Vec<String>,
    /// Composed agent capability admitted through stable refs (#188). Optional
    /// and additive: commissions without it are unchanged. Never a config blob.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_intake: Option<AgentCapabilityIntake>,
}

impl JourneyCommissionState {
    pub fn commission(
        journey: &Journey,
        commission_ref: impl Into<String>,
        commissioner_ref: impl Into<String>,
        accountable: JourneyAccountableSubject,
        intended_difference: impl Into<String>,
        closure_condition_refs: Vec<String>,
    ) -> Result<Self, JourneyCommissionError> {
        let commission_ref = required(commission_ref.into(), "commission_ref")?;
        let commissioner_ref = required(commissioner_ref.into(), "commissioner_ref")?;
        validate_subject(&accountable)?;
        let intended_difference = required(intended_difference.into(), "intended_difference")?;
        validate_refs(&closure_condition_refs, "closure_condition_refs")?;
        Ok(Self {
            schema: JOURNEY_COMMISSION_SCHEMA.into(),
            journey_ref: journey.journey_ref.clone(),
            commission_ref,
            commissioner_ref,
            accountable,
            intended_difference,
            closure_condition_refs,
            bounds_refs: Vec::new(),
            escalation_refs: Vec::new(),
            state: CommissionState::Commissioned,
            journey_revision: journey.revision.get(),
            handoffs: Vec::new(),
            attention_refs: Vec::new(),
            capability_intake: None,
        })
    }

    pub fn reconcile(&mut self, journey: &Journey) -> Result<(), JourneyCommissionError> {
        self.ensure_journey(journey)?;
        self.journey_revision = journey.revision.get();
        if journey.status == JourneyStatus::Completed {
            self.state = CommissionState::Completed;
        }
        Ok(())
    }

    pub fn set_state(&mut self, state: CommissionState) -> Result<(), JourneyCommissionError> {
        if self.state == CommissionState::Completed && state != CommissionState::Completed {
            return Err(JourneyCommissionError::CompletedCannotReopen);
        }
        self.state = state;
        Ok(())
    }

    /// Accountability handoff requires explicit Actuation determination provenance.
    /// A participant/@ relation alone cannot call this successfully without that ref.
    pub fn handoff(
        &mut self,
        journey: &Journey,
        to: JourneyAccountableSubject,
        determination_ref: impl Into<String>,
        return_ref: Option<String>,
    ) -> Result<(), JourneyCommissionError> {
        self.ensure_journey(journey)?;
        validate_subject(&to)?;
        let determination_ref = required(determination_ref.into(), "determination_ref")?;
        if let Some(reference) = return_ref.as_deref() {
            required(reference.to_owned(), "return_ref")?;
        }
        let from = self.accountable.clone();
        self.accountable = to.clone();
        self.journey_revision = journey.revision.get();
        self.handoffs.push(JourneyHandoff {
            from,
            to,
            determination_ref,
            return_ref,
            journey_revision: self.journey_revision,
        });
        Ok(())
    }

    /// Entering this state requires an explicit external/native Attention ref. It
    /// does not infer human importance from failure text or routine completion.
    pub fn wait_for_recognition(
        &mut self,
        attention_ref: impl Into<String>,
    ) -> Result<(), JourneyCommissionError> {
        let attention_ref = required(attention_ref.into(), "attention_ref")?;
        if !self.attention_refs.contains(&attention_ref) {
            self.attention_refs.push(attention_ref);
        }
        self.state = CommissionState::WaitingRecognition;
        Ok(())
    }

    pub fn clear_attention(&mut self, attention_ref: &str) -> bool {
        let before = self.attention_refs.len();
        self.attention_refs.retain(|value| value != attention_ref);
        before != self.attention_refs.len()
    }

    /// Attach the composed agent capability intake for this commission. The
    /// intake's Run must already belong to the Journey: capability refs bind to
    /// developmental use, not to a floating intention. Replacing an existing
    /// intake for the same Run is an explicit refresh, not a duplication.
    pub fn set_capability_intake(
        &mut self,
        journey: &Journey,
        intake: AgentCapabilityIntake,
    ) -> Result<(), JourneyCommissionError> {
        self.ensure_journey(journey)?;
        if !journey
            .runs
            .iter()
            .any(|link| link.run_ref == intake.run_ref)
        {
            return Err(JourneyCommissionError::RunOutsideJourney(
                intake.run_ref.to_string(),
            ));
        }
        self.capability_intake = Some(intake);
        Ok(())
    }

    pub fn reading(
        &self,
        journey: &Journey,
    ) -> Result<JourneyCommissionReading, JourneyCommissionError> {
        self.ensure_journey(journey)?;
        Ok(JourneyCommissionReading {
            schema: JOURNEY_COMMISSION_SCHEMA.into(),
            journey_ref: self.journey_ref.clone(),
            commission_ref: self.commission_ref.clone(),
            accountable: self.accountable.clone(),
            intended_difference: self.intended_difference.clone(),
            state: if journey.status == JourneyStatus::Completed {
                CommissionState::Completed
            } else {
                self.state
            },
            frontier: journey.frontier.clone(),
            run_refs: journey
                .runs
                .iter()
                .map(|link| link.run_ref.clone())
                .collect(),
            agent_session_refs: journey.agent_session_refs.clone(),
            attention_refs: self.attention_refs.clone(),
            material_context_refs: journey.material_context_refs.clone(),
            capability_intake: self.capability_intake.as_ref().map(Into::into),
            journey_revision: journey.revision.get(),
        })
    }

    fn ensure_journey(&self, journey: &Journey) -> Result<(), JourneyCommissionError> {
        if self.schema != JOURNEY_COMMISSION_SCHEMA {
            return Err(JourneyCommissionError::Schema(self.schema.clone()));
        }
        if self.journey_ref != journey.journey_ref {
            return Err(JourneyCommissionError::WrongJourney {
                expected: self.journey_ref.to_string(),
                actual: journey.journey_ref.to_string(),
            });
        }
        Ok(())
    }
}

/// Renderer/Agent-neutral application reading. Provider/session/material detail is
/// present only by native correlations already stored on Journey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyCommissionReading {
    pub schema: String,
    pub journey_ref: JourneyRef,
    pub commission_ref: String,
    pub accountable: JourneyAccountableSubject,
    pub intended_difference: String,
    pub state: CommissionState,
    pub frontier: String,
    pub run_refs: Vec<RunRef>,
    pub agent_session_refs: Vec<String>,
    pub attention_refs: Vec<String>,
    pub material_context_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_intake: Option<AgentCapabilityIntakeReading>,
    pub journey_revision: u64,
}

fn validate_subject(subject: &JourneyAccountableSubject) -> Result<(), JourneyCommissionError> {
    required(subject.reference().to_owned(), "accountable subject")?;
    Ok(())
}

fn validate_refs(values: &[String], field: &str) -> Result<(), JourneyCommissionError> {
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(JourneyCommissionError::InvalidText(field.into()));
    }
    Ok(())
}

fn required(value: String, field: &str) -> Result<String, JourneyCommissionError> {
    if value.trim().is_empty() {
        Err(JourneyCommissionError::InvalidText(field.into()))
    } else {
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JourneyCommissionError {
    InvalidText(String),
    Schema(String),
    WrongJourney { expected: String, actual: String },
    CompletedCannotReopen,
    RunOutsideJourney(String),
}

impl Display for JourneyCommissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidText(field) => write!(formatter, "{field} cannot be empty"),
            Self::Schema(schema) => {
                write!(formatter, "unsupported Journey Commission schema {schema}")
            }
            Self::WrongJourney { expected, actual } => {
                write!(
                    formatter,
                    "Journey Commission belongs to {expected}, got {actual}"
                )
            }
            Self::CompletedCannotReopen => {
                formatter.write_str("completed Journey Commission cannot reopen")
            }
            Self::RunOutsideJourney(run_ref) => {
                write!(formatter, "Run {run_ref} is not part of this Journey")
            }
        }
    }
}

impl Error for JourneyCommissionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::run::ProjectRef;
    use crate::journey::{JourneyCommission, JourneyReturn};

    fn journey() -> Journey {
        Journey::new(
            "journey:01ARZ3NDEKTSV4RRFFQ69G5FAD".parse().unwrap(),
            "project:01ARZ3NDEKTSV4RRFFQ69G5FAE"
                .parse::<ProjectRef>()
                .unwrap(),
            JourneyCommission {
                purpose: "Converge deterministic O:I work before physical acceptance.".into(),
                commission_ref: Some("source:intent:commission-proof".into()),
                why_refs: vec!["source:vision:oi".into()],
            },
            "Resolve the next exact owner seam.",
            "2026-09-02T18:00:00Z",
        )
        .unwrap()
    }

    #[test]
    fn agent_set_accountability_survives_runs_sessions_and_material_relocation() {
        let mut journey = journey();
        let run1: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap();
        let run2: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FAB".parse().unwrap();
        journey
            .add_run(
                run1.clone(),
                vec!["basis:1".into()],
                vec!["agent-session:one".into()],
            )
            .unwrap();
        journey
            .correlate_material_context("workcell:first")
            .unwrap();

        let mut commission = JourneyCommissionState::commission(
            &journey,
            "commission:development",
            "human:owner",
            JourneyAccountableSubject::AgentSet {
                agent_set_ref: "agent-set:development".into(),
            },
            "Return a verified software difference.",
            vec!["success:tests-green".into()],
        )
        .unwrap();
        commission.set_state(CommissionState::Active).unwrap();

        journey
            .add_run(
                run2.clone(),
                vec!["basis:2".into()],
                vec!["agent-session:two".into()],
            )
            .unwrap();
        journey
            .correlate_material_context("workcell:replacement")
            .unwrap();
        commission.reconcile(&journey).unwrap();
        let reading = commission.reading(&journey).unwrap();

        assert_eq!(reading.accountable.reference(), "agent-set:development");
        assert_eq!(reading.run_refs, vec![run1, run2]);
        assert_eq!(
            reading.agent_session_refs,
            vec!["agent-session:one", "agent-session:two"]
        );
        assert_eq!(
            reading.material_context_refs,
            vec!["workcell:first", "workcell:replacement"]
        );
    }

    #[test]
    fn accountability_handoff_requires_explicit_determination_and_preserves_provenance() {
        let journey = journey();
        let mut commission = JourneyCommissionState::commission(
            &journey,
            "commission:handoff",
            "human:owner",
            JourneyAccountableSubject::Agency {
                agency_ref: "agency:a".into(),
            },
            "Research, implement and verify.",
            vec![],
        )
        .unwrap();

        assert!(commission
            .handoff(
                &journey,
                JourneyAccountableSubject::Agency {
                    agency_ref: "agency:b".into(),
                },
                "",
                None,
            )
            .is_err());
        commission
            .handoff(
                &journey,
                JourneyAccountableSubject::Agency {
                    agency_ref: "agency:b".into(),
                },
                "determination:a-to-b",
                Some("return:research".into()),
            )
            .unwrap();
        commission
            .handoff(
                &journey,
                JourneyAccountableSubject::Agent {
                    agent_ref: "agent:c".into(),
                },
                "determination:b-to-c",
                Some("return:design".into()),
            )
            .unwrap();

        assert_eq!(commission.accountable.reference(), "agent:c");
        assert_eq!(commission.handoffs.len(), 2);
        assert_eq!(commission.handoffs[0].from.reference(), "agency:a");
        assert_eq!(commission.handoffs[0].to.reference(), "agency:b");
        assert_eq!(commission.handoffs[1].to.reference(), "agent:c");
    }

    #[test]
    fn commissioned_work_remains_intelligible_without_live_agent_session() {
        let journey = journey();
        let mut commission = JourneyCommissionState::commission(
            &journey,
            "commission:background",
            "human:owner",
            JourneyAccountableSubject::Agent {
                agent_ref: "agent:research".into(),
            },
            "Investigate the dependency and return evidence.",
            vec![],
        )
        .unwrap();
        commission
            .set_state(CommissionState::WaitingExternal)
            .unwrap();
        let reading = commission.reading(&journey).unwrap();
        assert_eq!(reading.state, CommissionState::WaitingExternal);
        assert!(reading.agent_session_refs.is_empty());
        assert_eq!(reading.accountable.reference(), "agent:research");
    }

    #[test]
    fn recognition_wait_requires_explicit_attention_ref_and_does_not_infer_from_text() {
        let journey = journey();
        let mut commission = JourneyCommissionState::commission(
            &journey,
            "commission:recognition",
            "human:owner",
            JourneyAccountableSubject::Agent {
                agent_ref: "agent:developer".into(),
            },
            "Return only consequential decisions.",
            vec![],
        )
        .unwrap();
        assert!(commission.wait_for_recognition("").is_err());
        commission
            .wait_for_recognition("attention:recognition:1")
            .unwrap();
        assert_eq!(commission.state, CommissionState::WaitingRecognition);
        assert_eq!(commission.attention_refs, vec!["attention:recognition:1"]);
        assert!(commission.clear_attention("attention:recognition:1"));
        assert!(commission.attention_refs.is_empty());
    }

    #[test]
    fn journey_completion_closes_application_reading_but_does_not_recognise_every_return() {
        let mut journey = journey();
        let run: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FAC".parse().unwrap();
        journey
            .add_run(run.clone(), vec!["basis:final".into()], vec![])
            .unwrap();
        journey
            .record_return(JourneyReturn {
                return_ref: "return:final".into(),
                run_refs: vec![run],
                basis_refs: vec!["basis:final".into()],
                evidence_refs: vec!["evidence:final".into()],
                recognition_ref: Some("recognition:final".into()),
                summary: "Verified return.".into(),
            })
            .unwrap();
        journey
            .complete("2026-09-02T19:00:00Z", "return:final", "recognition:final")
            .unwrap();
        let commission = JourneyCommissionState::commission(
            &journey,
            "commission:complete",
            "human:owner",
            JourneyAccountableSubject::AgentSet {
                agent_set_ref: "agent-set:development".into(),
            },
            "Complete the current cut.",
            vec![],
        )
        .unwrap();
        assert_eq!(
            commission.reading(&journey).unwrap().state,
            CommissionState::Completed
        );
    }
}
