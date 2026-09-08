//! Journey-scoped correlation of externally owned praxis/source objects.
//!
//! Factory owns why work exists developmentally and which returned reality carries
//! a Journey. Central continues to own AgentProfile source; AIKit continues to own
//! Skill/SkillSet/Method-classification, proof/Routine semantics and operative
//! resolution. This module therefore retains their public refs/contracts as
//! correlations only.

use crate::core::run::RunRef;
use crate::journey::{Journey, JourneyRef};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display};

pub const JOURNEY_PRAXIS_SCHEMA: &str = "factory.journey-praxis/v1";
pub const CENTRAL_AGENT_PROFILE_SCHEMA: &str = "central.agent-profile/v1";
/// Current AIKit Method classification contract. The classified object remains
/// an ordinary Skill; this name is retained for the Factory field/compatibility
/// surface and does not create a Method resource identity.
pub const AIKIT_SKILL_METHOD_METADATA_SCHEMA: &str = "aikit.skill-method-metadata/v1";
pub const AIKIT_METHOD_SCHEMA: &str = AIKIT_SKILL_METHOD_METADATA_SCHEMA;
pub const AIKIT_METHOD_PROOF_SCHEMA: &str = "aikit.method-proof/v1";
pub const AIKIT_ROUTINE_SCHEMA: &str = "aikit.routine/v1";
pub const AIKIT_ROUTINE_STALE_PROOF_STATE: &str = "stale-proof";

/// Selection of a Central-owned AgentProfile for this developmental relation.
/// The source body remains in Central; Factory retains only exact identity,
/// revision, semantic Agent and source World correlations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyAgentProfileSelection {
    pub contract: String,
    pub profile_ref: String,
    pub profile_revision: String,
    pub agent_ref: String,
    pub source_world_ref: String,
}

/// AIKit-owned proof relation correlated to one returned Factory Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyMethodProofCorrelation {
    pub contract: String,
    pub proof_ref: String,
    #[serde(default)]
    pub verification_refs: Vec<String>,
}

/// Consequential use of one exact AIKit Method-classified Skill revision by a
/// bounded Factory Run.
///
/// Context/body refs are retained as the actual operative/material condition that
/// carried the work; Factory does not resolve or reinterpret them. Activity,
/// Evidence and Return refs must already be present in the Journey's returned
/// reality before this correlation can be admitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyPraxisReturn {
    pub run_ref: RunRef,
    /// Compatibility field name in `factory.journey-praxis/v1`. Its value is
    /// the ordinary AIKit Skill ResourceRef, never a standalone MethodRef.
    pub method_contract: String,
    /// Compatibility field name retained for the v1 projection. The referenced
    /// identity is the Method-classified Skill named by AIKit.
    pub method_ref: String,
    pub method_revision: String,
    pub context_resolution_ref: String,
    #[serde(default)]
    pub body_condition_refs: Vec<String>,
    pub activity_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub return_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<JourneyMethodProofCorrelation>,
}

/// Observation of an AIKit Routine when that Routine carries this Journey through
/// time. Trigger/scheduler/authority semantics remain AIKit/provider-owned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyRoutineObservation {
    pub contract: String,
    pub routine_ref: String,
    /// Compatibility field name; this is the same Method-classified Skill ref
    /// carried by the Routine, not a second Method resource.
    pub method_ref: String,
    pub method_revision: String,
    pub proof_ref: String,
    /// Exact public `aikit.routine/v1` state value observed from AIKit.
    pub routine_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_profile_ref: Option<String>,
    #[serde(default)]
    pub activity_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub return_refs: Vec<String>,
}

impl JourneyRoutineObservation {
    /// A stale AIKit proof is not translated into a Factory scheduler state. It is
    /// surfaced only as explicit revalidation pressure for developmental reading.
    pub fn proof_revalidation_required(&self) -> bool {
        self.routine_state == AIKIT_ROUTINE_STALE_PROOF_STATE
    }
}

/// Factory-owned developmental correlation around one canonical Journey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyPraxisContext {
    pub schema: String,
    pub journey_ref: JourneyRef,
    #[serde(default)]
    pub agent_profiles: Vec<JourneyAgentProfileSelection>,
    #[serde(default)]
    pub praxis_returns: Vec<JourneyPraxisReturn>,
    #[serde(default)]
    pub routines: Vec<JourneyRoutineObservation>,
}

impl JourneyPraxisContext {
    pub fn new(journey: &Journey) -> Self {
        Self {
            schema: JOURNEY_PRAXIS_SCHEMA.into(),
            journey_ref: journey.journey_ref.clone(),
            agent_profiles: Vec::new(),
            praxis_returns: Vec::new(),
            routines: Vec::new(),
        }
    }

    pub fn select_agent_profile(
        &mut self,
        journey: &Journey,
        selection: JourneyAgentProfileSelection,
    ) -> Result<(), JourneyPraxisError> {
        self.ensure_journey(journey)?;
        require_contract(
            &selection.contract,
            CENTRAL_AGENT_PROFILE_SCHEMA,
            "AgentProfile",
        )?;
        required(&selection.profile_ref, "AgentProfile ref")?;
        required(&selection.profile_revision, "AgentProfile revision")?;
        required(&selection.agent_ref, "Agent ref")?;
        required(&selection.source_world_ref, "AgentProfile source World ref")?;
        if let Some(existing) = self
            .agent_profiles
            .iter_mut()
            .find(|existing| existing.profile_ref == selection.profile_ref)
        {
            *existing = selection;
        } else {
            self.agent_profiles.push(selection);
        }
        Ok(())
    }

    pub fn record_praxis_return(
        &mut self,
        journey: &Journey,
        returned: JourneyPraxisReturn,
    ) -> Result<(), JourneyPraxisError> {
        self.ensure_journey(journey)?;
        require_contract(
            &returned.method_contract,
            AIKIT_METHOD_SCHEMA,
            "Method-classified Skill",
        )?;
        require_skill_ref(&returned.method_ref, "Method-classified Skill ref")?;
        required(&returned.method_revision, "Method revision")?;
        required(&returned.context_resolution_ref, "ContextResolution ref")?;
        validate_refs(&returned.body_condition_refs, "body condition refs")?;
        require_refs(&returned.activity_refs, "Activity refs")?;
        require_refs(&returned.evidence_refs, "Evidence refs")?;
        require_refs(&returned.return_refs, "Return refs")?;

        if !journey
            .runs
            .iter()
            .any(|link| link.run_ref == returned.run_ref)
        {
            return Err(JourneyPraxisError::RunOutsideJourney(
                returned.run_ref.to_string(),
            ));
        }
        for activity_ref in &returned.activity_refs {
            if !journey.activity_refs.contains(activity_ref) {
                return Err(JourneyPraxisError::UnknownActivity(activity_ref.clone()));
            }
        }
        for return_ref in &returned.return_refs {
            let journey_return = journey
                .returns
                .iter()
                .find(|candidate| candidate.return_ref == *return_ref)
                .ok_or_else(|| JourneyPraxisError::UnknownReturn(return_ref.clone()))?;
            if !journey_return
                .run_refs
                .iter()
                .any(|run| run == &returned.run_ref)
            {
                return Err(JourneyPraxisError::ReturnRunMismatch {
                    return_ref: return_ref.clone(),
                    run_ref: returned.run_ref.to_string(),
                });
            }
        }
        for evidence_ref in &returned.evidence_refs {
            let present = journey
                .returns
                .iter()
                .filter(|candidate| returned.return_refs.contains(&candidate.return_ref))
                .any(|candidate| candidate.evidence_refs.contains(evidence_ref));
            if !present {
                return Err(JourneyPraxisError::UnknownEvidence(evidence_ref.clone()));
            }
        }
        if let Some(proof) = &returned.proof {
            require_contract(&proof.contract, AIKIT_METHOD_PROOF_SCHEMA, "Method proof")?;
            required(&proof.proof_ref, "Method proof ref")?;
            require_refs(&proof.verification_refs, "verification refs")?;
        }

        if self.praxis_returns.iter().any(|existing| {
            existing.run_ref == returned.run_ref
                && existing.method_ref == returned.method_ref
                && existing.method_revision == returned.method_revision
        }) {
            return Err(JourneyPraxisError::DuplicatePraxisReturn {
                run_ref: returned.run_ref.to_string(),
                method_ref: returned.method_ref,
                method_revision: returned.method_revision,
            });
        }
        self.praxis_returns.push(returned);
        Ok(())
    }

    pub fn observe_routine(
        &mut self,
        journey: &Journey,
        observation: JourneyRoutineObservation,
    ) -> Result<(), JourneyPraxisError> {
        self.ensure_journey(journey)?;
        require_contract(&observation.contract, AIKIT_ROUTINE_SCHEMA, "Routine")?;
        required(&observation.routine_ref, "Routine ref")?;
        require_skill_ref(
            &observation.method_ref,
            "Routine Method-classified Skill ref",
        )?;
        required(&observation.method_revision, "Routine Method revision")?;
        required(&observation.proof_ref, "Routine proof ref")?;
        required(&observation.routine_state, "Routine state")?;
        validate_refs(&observation.activity_refs, "Routine Activity refs")?;
        validate_refs(&observation.evidence_refs, "Routine Evidence refs")?;
        validate_refs(&observation.return_refs, "Routine Return refs")?;
        if let Some(profile_ref) = observation.agent_profile_ref.as_deref() {
            required(profile_ref, "Routine AgentProfile ref")?;
            if !self
                .agent_profiles
                .iter()
                .any(|profile| profile.profile_ref == profile_ref)
            {
                return Err(JourneyPraxisError::UnknownAgentProfile(
                    profile_ref.to_owned(),
                ));
            }
        }

        let basis_exists = self.praxis_returns.iter().any(|returned| {
            returned.method_ref == observation.method_ref
                && returned.method_revision == observation.method_revision
                && returned
                    .proof
                    .as_ref()
                    .is_some_and(|proof| proof.proof_ref == observation.proof_ref)
        });
        if !basis_exists {
            return Err(JourneyPraxisError::RoutineWithoutJourneyProof {
                routine_ref: observation.routine_ref,
                proof_ref: observation.proof_ref,
            });
        }

        for activity_ref in &observation.activity_refs {
            if !journey.activity_refs.contains(activity_ref) {
                return Err(JourneyPraxisError::UnknownActivity(activity_ref.clone()));
            }
        }
        for return_ref in &observation.return_refs {
            if !journey
                .returns
                .iter()
                .any(|returned| returned.return_ref == *return_ref)
            {
                return Err(JourneyPraxisError::UnknownReturn(return_ref.clone()));
            }
        }
        for evidence_ref in &observation.evidence_refs {
            if !journey
                .returns
                .iter()
                .any(|returned| returned.evidence_refs.contains(evidence_ref))
            {
                return Err(JourneyPraxisError::UnknownEvidence(evidence_ref.clone()));
            }
        }

        if let Some(existing) = self
            .routines
            .iter_mut()
            .find(|existing| existing.routine_ref == observation.routine_ref)
        {
            *existing = observation;
        } else {
            self.routines.push(observation);
        }
        Ok(())
    }

    pub fn reading(&self, journey: &Journey) -> Result<JourneyPraxisReading, JourneyPraxisError> {
        self.ensure_journey(journey)?;
        Ok(JourneyPraxisReading {
            schema: JOURNEY_PRAXIS_SCHEMA.into(),
            journey_ref: self.journey_ref.clone(),
            project_ref: journey.project_ref.to_string(),
            agent_profiles: self.agent_profiles.clone(),
            praxis_returns: self.praxis_returns.clone(),
            routines: self.routines.clone(),
            revalidation_required_routine_refs: self
                .routines
                .iter()
                .filter(|routine| routine.proof_revalidation_required())
                .map(|routine| routine.routine_ref.clone())
                .collect(),
        })
    }

    fn ensure_journey(&self, journey: &Journey) -> Result<(), JourneyPraxisError> {
        if self.schema != JOURNEY_PRAXIS_SCHEMA {
            return Err(JourneyPraxisError::Schema(self.schema.clone()));
        }
        if self.journey_ref != journey.journey_ref {
            return Err(JourneyPraxisError::WrongJourney {
                expected: self.journey_ref.to_string(),
                actual: journey.journey_ref.to_string(),
            });
        }
        Ok(())
    }
}

/// Renderer-neutral Factory reading. External source/runtime objects remain the
/// same exact refs used by Central/AIKit and are not materialised into Factory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyPraxisReading {
    pub schema: String,
    pub journey_ref: JourneyRef,
    pub project_ref: String,
    pub agent_profiles: Vec<JourneyAgentProfileSelection>,
    pub praxis_returns: Vec<JourneyPraxisReturn>,
    pub routines: Vec<JourneyRoutineObservation>,
    pub revalidation_required_routine_refs: Vec<String>,
}

fn required(value: &str, field: &str) -> Result<(), JourneyPraxisError> {
    if value.trim().is_empty() {
        Err(JourneyPraxisError::InvalidText(field.into()))
    } else {
        Ok(())
    }
}

fn require_skill_ref(value: &str, field: &str) -> Result<(), JourneyPraxisError> {
    required(value, field)?;
    if value.starts_with("skill/") {
        Ok(())
    } else {
        Err(JourneyPraxisError::InvalidText(format!(
            "{field} must use the ordinary AIKit Skill ref grammar"
        )))
    }
}

fn validate_refs(values: &[String], field: &str) -> Result<(), JourneyPraxisError> {
    for value in values {
        required(value, field)?;
    }
    Ok(())
}

fn require_refs(values: &[String], field: &str) -> Result<(), JourneyPraxisError> {
    if values.is_empty() {
        return Err(JourneyPraxisError::MissingRefs(field.into()));
    }
    validate_refs(values, field)
}

fn require_contract(
    actual: &str,
    expected: &'static str,
    subject: &'static str,
) -> Result<(), JourneyPraxisError> {
    if actual == expected {
        Ok(())
    } else {
        Err(JourneyPraxisError::ExternalContract {
            subject,
            expected,
            actual: actual.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JourneyPraxisError {
    InvalidText(String),
    MissingRefs(String),
    Schema(String),
    WrongJourney {
        expected: String,
        actual: String,
    },
    ExternalContract {
        subject: &'static str,
        expected: &'static str,
        actual: String,
    },
    RunOutsideJourney(String),
    UnknownActivity(String),
    UnknownEvidence(String),
    UnknownReturn(String),
    ReturnRunMismatch {
        return_ref: String,
        run_ref: String,
    },
    UnknownAgentProfile(String),
    DuplicatePraxisReturn {
        run_ref: String,
        method_ref: String,
        method_revision: String,
    },
    RoutineWithoutJourneyProof {
        routine_ref: String,
        proof_ref: String,
    },
}

impl Display for JourneyPraxisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidText(field) => write!(formatter, "{field} cannot be empty"),
            Self::MissingRefs(field) => write!(formatter, "{field} must not be empty"),
            Self::Schema(schema) => write!(formatter, "unsupported Journey praxis schema {schema}"),
            Self::WrongJourney { expected, actual } => {
                write!(formatter, "Journey praxis belongs to {expected}, got {actual}")
            }
            Self::ExternalContract {
                subject,
                expected,
                actual,
            } => write!(
                formatter,
                "unsupported {subject} contract {actual}; expected {expected}"
            ),
            Self::RunOutsideJourney(run_ref) => {
                write!(formatter, "Run {run_ref} is not part of this Journey")
            }
            Self::UnknownActivity(reference) => {
                write!(formatter, "Journey has no Activity {reference}")
            }
            Self::UnknownEvidence(reference) => {
                write!(formatter, "Journey Returns do not contain Evidence {reference}")
            }
            Self::UnknownReturn(reference) => {
                write!(formatter, "Journey has no Return {reference}")
            }
            Self::ReturnRunMismatch {
                return_ref,
                run_ref,
            } => write!(
                formatter,
                "Journey Return {return_ref} does not include Run {run_ref}"
            ),
            Self::UnknownAgentProfile(reference) => write!(
                formatter,
                "Routine references AgentProfile {reference} which is not selected for this Journey"
            ),
            Self::DuplicatePraxisReturn {
                run_ref,
                method_ref,
                method_revision,
            } => write!(
                formatter,
                "Run {run_ref} already carries Method-classified Skill {method_ref}@{method_revision}"
            ),
            Self::RoutineWithoutJourneyProof {
                routine_ref,
                proof_ref,
            } => write!(
                formatter,
                "Routine {routine_ref} proof {proof_ref} is not grounded in this Journey's returned praxis"
            ),
        }
    }
}

impl Error for JourneyPraxisError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::run::ProjectRef;
    use crate::journey::{JourneyCommission, JourneyReturn};

    fn journey() -> (Journey, RunRef) {
        let run: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap();
        let mut journey = Journey::new(
            "journey:01ARZ3NDEKTSV4RRFFQ69G5FAD".parse().unwrap(),
            "project:01ARZ3NDEKTSV4RRFFQ69G5FAE"
                .parse::<ProjectRef>()
                .unwrap(),
            JourneyCommission {
                purpose: "Carry verified recurring developmental work.".into(),
                commission_ref: Some("commission:w12".into()),
                why_refs: vec!["source:vision:oi".into()],
            },
            "Verify the first Method return.",
            "2026-09-02T19:00:00Z",
        )
        .unwrap();
        journey
            .add_run(
                run.clone(),
                vec!["basis:accepted-main".into()],
                vec!["agent-session:first".into()],
            )
            .unwrap();
        journey.correlate_activity("activity:method:1").unwrap();
        journey
            .record_return(JourneyReturn {
                return_ref: "return:method:1".into(),
                run_refs: vec![run.clone()],
                basis_refs: vec!["skill/factory-native/factory-bounded-work".into()],
                evidence_refs: vec!["evidence:method:1".into()],
                recognition_ref: None,
                summary: "Verified Method return.".into(),
            })
            .unwrap();
        (journey, run)
    }

    fn profile() -> JourneyAgentProfileSelection {
        JourneyAgentProfileSelection {
            contract: CENTRAL_AGENT_PROFILE_SCHEMA.into(),
            profile_ref: "agent-profile:researcher".into(),
            profile_revision: "profile-rev-1".into(),
            agent_ref: "agent:researcher".into(),
            source_world_ref: "world:project".into(),
        }
    }

    fn praxis(run_ref: RunRef) -> JourneyPraxisReturn {
        JourneyPraxisReturn {
            run_ref,
            method_contract: AIKIT_METHOD_SCHEMA.into(),
            method_ref: "skill/factory-native/factory-bounded-work".into(),
            method_revision: "method-rev-1".into(),
            context_resolution_ref: "context-resolution:abc123".into(),
            body_condition_refs: vec!["harness-composition:research".into()],
            activity_refs: vec!["activity:method:1".into()],
            evidence_refs: vec!["evidence:method:1".into()],
            return_refs: vec!["return:method:1".into()],
            proof: Some(JourneyMethodProofCorrelation {
                contract: AIKIT_METHOD_PROOF_SCHEMA.into(),
                proof_ref: "proof:research:v1".into(),
                verification_refs: vec!["verification:research:1".into()],
            }),
        }
    }

    fn routine(state: &str) -> JourneyRoutineObservation {
        JourneyRoutineObservation {
            contract: AIKIT_ROUTINE_SCHEMA.into(),
            routine_ref: "routine:daily-research".into(),
            method_ref: "skill/factory-native/factory-bounded-work".into(),
            method_revision: "method-rev-1".into(),
            proof_ref: "proof:research:v1".into(),
            routine_state: state.into(),
            agent_profile_ref: Some("agent-profile:researcher".into()),
            activity_refs: vec!["activity:method:1".into()],
            evidence_refs: vec!["evidence:method:1".into()],
            return_refs: vec!["return:method:1".into()],
        }
    }

    #[test]
    fn saved_agent_profile_is_selected_without_becoming_factory_source() {
        let (journey, _) = journey();
        let mut context = JourneyPraxisContext::new(&journey);
        context.select_agent_profile(&journey, profile()).unwrap();
        let reading = context.reading(&journey).unwrap();
        assert_eq!(
            reading.agent_profiles[0].contract,
            CENTRAL_AGENT_PROFILE_SCHEMA
        );
        assert_eq!(
            reading.agent_profiles[0].profile_ref,
            "agent-profile:researcher"
        );
        assert_eq!(reading.agent_profiles[0].agent_ref, "agent:researcher");
    }

    #[test]
    fn consequential_run_retains_exact_method_context_body_and_returned_evidence() {
        let (journey, run) = journey();
        let mut context = JourneyPraxisContext::new(&journey);
        context.record_praxis_return(&journey, praxis(run)).unwrap();
        let reading = context.reading(&journey).unwrap();
        let returned = &reading.praxis_returns[0];
        assert_eq!(
            returned.method_ref,
            "skill/factory-native/factory-bounded-work"
        );
        assert_eq!(returned.method_revision, "method-rev-1");
        assert_eq!(returned.context_resolution_ref, "context-resolution:abc123");
        assert_eq!(
            returned.body_condition_refs,
            vec!["harness-composition:research"]
        );
        assert_eq!(returned.evidence_refs, vec!["evidence:method:1"]);
        assert_eq!(
            returned.proof.as_ref().unwrap().contract,
            AIKIT_METHOD_PROOF_SCHEMA
        );
    }

    #[test]
    fn stale_standalone_method_contract_is_rejected_at_record_boundary() {
        let (journey, run) = journey();
        let mut context = JourneyPraxisContext::new(&journey);
        let mut stale = praxis(run);
        stale.method_contract = "aikit.method/v1".into();
        assert!(matches!(
            context.record_praxis_return(&journey, stale),
            Err(JourneyPraxisError::ExternalContract {
                subject: "Method-classified Skill",
                expected: AIKIT_SKILL_METHOD_METADATA_SCHEMA,
                ..
            })
        ));
    }

    #[test]
    fn standalone_method_identity_is_rejected_for_new_praxis_records() {
        let (journey, run) = journey();
        let mut context = JourneyPraxisContext::new(&journey);
        let mut standalone = praxis(run);
        standalone.method_ref = "method/factory/bounded-work".into();
        assert!(matches!(
            context.record_praxis_return(&journey, standalone),
            Err(JourneyPraxisError::InvalidText(message))
                if message.contains("ordinary AIKit Skill ref grammar")
        ));
    }

    #[test]
    fn routine_requires_proof_already_grounded_in_journey_return() {
        let (journey, run) = journey();
        let mut context = JourneyPraxisContext::new(&journey);
        context.select_agent_profile(&journey, profile()).unwrap();
        let error = context
            .observe_routine(&journey, routine("enabled"))
            .unwrap_err();
        assert!(matches!(
            error,
            JourneyPraxisError::RoutineWithoutJourneyProof { .. }
        ));
        context.record_praxis_return(&journey, praxis(run)).unwrap();
        context
            .observe_routine(&journey, routine("enabled"))
            .unwrap();
        assert_eq!(context.routines.len(), 1);
    }

    #[test]
    fn stale_aikit_proof_is_explicit_revalidation_pressure_not_scheduler_state() {
        let (journey, run) = journey();
        let mut context = JourneyPraxisContext::new(&journey);
        context.select_agent_profile(&journey, profile()).unwrap();
        context.record_praxis_return(&journey, praxis(run)).unwrap();
        context
            .observe_routine(&journey, routine(AIKIT_ROUTINE_STALE_PROOF_STATE))
            .unwrap();
        let reading = context.reading(&journey).unwrap();
        assert_eq!(
            reading.revalidation_required_routine_refs,
            vec!["routine:daily-research"]
        );
        assert!(reading.routines[0].proof_revalidation_required());
    }

    #[test]
    fn routine_observation_reuses_same_agent_method_proof_activity_evidence_and_return_refs() {
        let (journey, run) = journey();
        let mut context = JourneyPraxisContext::new(&journey);
        context.select_agent_profile(&journey, profile()).unwrap();
        context.record_praxis_return(&journey, praxis(run)).unwrap();
        context
            .observe_routine(&journey, routine("enabled"))
            .unwrap();
        let reading = context.reading(&journey).unwrap();
        let observation = &reading.routines[0];
        assert_eq!(
            observation.agent_profile_ref.as_deref(),
            Some("agent-profile:researcher")
        );
        assert_eq!(observation.method_ref, reading.praxis_returns[0].method_ref);
        assert_eq!(
            observation.proof_ref,
            reading.praxis_returns[0].proof.as_ref().unwrap().proof_ref
        );
        assert_eq!(observation.activity_refs, vec!["activity:method:1"]);
        assert_eq!(observation.evidence_refs, vec!["evidence:method:1"]);
        assert_eq!(observation.return_refs, vec!["return:method:1"]);
    }
}
