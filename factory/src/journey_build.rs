//! Structured Build projection for developmental Journey carriage.
//!
//! This composes existing Factory-owned read models at read time. It does not add
//! Journey/Commission/Praxis state to `FactoryBuildState`, so desktop/renderers do
//! not gain a second semantic store merely by asking for one combined view.

use crate::build::{
    FactoryBuildError, FactoryBuildSelection, FactoryBuildSnapshot, FactoryBuildState,
    FactoryBuildViewProvider,
};
use crate::journey::Journey;
use crate::journey_commission::{
    JourneyCommissionError, JourneyCommissionReading, JourneyCommissionState,
};
use crate::journey_praxis::{JourneyPraxisContext, JourneyPraxisError, JourneyPraxisReading};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{self, Display};

pub const FACTORY_DEVELOPMENTAL_BUILD_VIEW: &str = "factory.developmental-build-view/v1";

/// One renderer-neutral whole for a selected Factory Run inside a Journey.
/// Every nested object is the native Factory reading over its canonical source;
/// Central/AIKit refs inside `praxis` remain opaque owner-native identities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryDevelopmentalBuildSnapshot {
    pub contract: String,
    pub build: FactoryBuildSnapshot,
    pub commission: JourneyCommissionReading,
    pub praxis: JourneyPraxisReading,
}

impl FactoryDevelopmentalBuildSnapshot {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Compose existing Run Build state with the current Commission and Journey-praxis
/// readings. The selected Run must actually belong to the selected Journey and
/// both must belong to the same Project; no renderer correlation is inferred.
pub fn developmental_build_snapshot(
    state: &FactoryBuildState,
    selection: &FactoryBuildSelection,
    journey: &Journey,
    commission: &JourneyCommissionState,
    praxis: &JourneyPraxisContext,
) -> Result<FactoryDevelopmentalBuildSnapshot, FactoryDevelopmentalBuildError> {
    if journey.project_ref != selection.project_ref {
        return Err(FactoryDevelopmentalBuildError::ProjectMismatch {
            journey: journey.project_ref.to_string(),
            build: selection.project_ref.to_string(),
        });
    }
    if !journey
        .runs
        .iter()
        .any(|link| link.run_ref == selection.run_ref)
    {
        return Err(FactoryDevelopmentalBuildError::RunOutsideJourney(
            selection.run_ref.to_string(),
        ));
    }

    let build = FactoryBuildViewProvider.snapshot(state, selection)?;
    let commission = commission.reading(journey)?;
    let praxis = praxis.reading(journey)?;

    Ok(FactoryDevelopmentalBuildSnapshot {
        contract: FACTORY_DEVELOPMENTAL_BUILD_VIEW.into(),
        build,
        commission,
        praxis,
    })
}

#[derive(Debug)]
pub enum FactoryDevelopmentalBuildError {
    Build(FactoryBuildError),
    Commission(JourneyCommissionError),
    Praxis(JourneyPraxisError),
    ProjectMismatch { journey: String, build: String },
    RunOutsideJourney(String),
}

impl From<FactoryBuildError> for FactoryDevelopmentalBuildError {
    fn from(error: FactoryBuildError) -> Self {
        Self::Build(error)
    }
}

impl From<JourneyCommissionError> for FactoryDevelopmentalBuildError {
    fn from(error: JourneyCommissionError) -> Self {
        Self::Commission(error)
    }
}

impl From<JourneyPraxisError> for FactoryDevelopmentalBuildError {
    fn from(error: JourneyPraxisError) -> Self {
        Self::Praxis(error)
    }
}

impl Display for FactoryDevelopmentalBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(error) => write!(formatter, "{error}"),
            Self::Commission(error) => write!(formatter, "{error}"),
            Self::Praxis(error) => write!(formatter, "{error}"),
            Self::ProjectMismatch { journey, build } => write!(
                formatter,
                "Journey Project {journey} does not match Build Project {build}"
            ),
            Self::RunOutsideJourney(run_ref) => {
                write!(
                    formatter,
                    "Build-selected Run {run_ref} is not in the Journey"
                )
            }
        }
    }
}

impl Error for FactoryDevelopmentalBuildError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{FactoryBuildSelection, FactoryBuildState};
    use crate::core::run::{Project, ProjectRef, Run, RunRef};
    use crate::journey::{JourneyCommission, JourneyReturn};
    use crate::journey_commission::{JourneyAccountableSubject, JourneyCommissionState};
    use crate::journey_praxis::{
        JourneyAgentProfileSelection, JourneyMethodProofCorrelation, JourneyPraxisReturn,
        JourneyRoutineObservation, AIKIT_METHOD_PROOF_SCHEMA, AIKIT_METHOD_SCHEMA,
        AIKIT_ROUTINE_SCHEMA, AIKIT_ROUTINE_STALE_PROOF_STATE, CENTRAL_AGENT_PROFILE_SCHEMA,
    };

    struct Fixture {
        state: FactoryBuildState,
        selection: FactoryBuildSelection,
        journey: Journey,
        commission: JourneyCommissionState,
        praxis: JourneyPraxisContext,
    }

    fn fixture(routine_state: &str) -> Fixture {
        let project_ref: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap();
        let run_ref: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap();
        let project = Project::new(project_ref.clone());
        let run = Run::new(
            run_ref.clone(),
            project_ref.clone(),
            "Return W12 developmental work",
            "factory",
        )
        .unwrap();
        let state = FactoryBuildState::new(project, run).unwrap();
        let selection = FactoryBuildSelection {
            project_ref: project_ref.clone(),
            run_ref: run_ref.clone(),
        };

        let mut journey = Journey::new(
            "journey:01ARZ3NDEKTSV4RRFFQ69G5FAD".parse().unwrap(),
            project_ref,
            JourneyCommission {
                purpose: "Carry one proven Method as repeated developmental responsibility.".into(),
                commission_ref: Some("commission:w12".into()),
                why_refs: vec!["source:vision:oi".into()],
            },
            "Return the first verified Method use.",
            "2026-09-02T19:00:00Z",
        )
        .unwrap();
        journey
            .add_run(
                run_ref.clone(),
                vec!["basis:accepted-main".into()],
                vec!["agent-session:first".into()],
            )
            .unwrap();
        journey.correlate_activity("activity:method:1").unwrap();
        journey
            .record_return(JourneyReturn {
                return_ref: "return:method:1".into(),
                run_refs: vec![run_ref.clone()],
                basis_refs: vec!["skill/factory-native/factory-bounded-work".into()],
                evidence_refs: vec!["evidence:method:1".into()],
                recognition_ref: None,
                summary: "Verified Method return.".into(),
            })
            .unwrap();

        let commission = JourneyCommissionState::commission(
            &journey,
            "commission:w12",
            "human:owner",
            JourneyAccountableSubject::Agent {
                agent_ref: "agent:researcher".into(),
            },
            "Return repeated verified research without losing developmental accountability.",
            vec!["closure:verified-return".into()],
        )
        .unwrap();

        let mut praxis = JourneyPraxisContext::new(&journey);
        praxis
            .select_agent_profile(
                &journey,
                JourneyAgentProfileSelection {
                    contract: CENTRAL_AGENT_PROFILE_SCHEMA.into(),
                    profile_ref: "agent-profile:researcher".into(),
                    profile_revision: "profile-rev-1".into(),
                    agent_ref: "agent:researcher".into(),
                    source_world_ref: "world:project".into(),
                },
            )
            .unwrap();
        praxis
            .record_praxis_return(
                &journey,
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
                },
            )
            .unwrap();
        praxis
            .observe_routine(
                &journey,
                JourneyRoutineObservation {
                    contract: AIKIT_ROUTINE_SCHEMA.into(),
                    routine_ref: "routine:daily-research".into(),
                    method_ref: "skill/factory-native/factory-bounded-work".into(),
                    method_revision: "method-rev-1".into(),
                    proof_ref: "proof:research:v1".into(),
                    routine_state: routine_state.into(),
                    agent_profile_ref: Some("agent-profile:researcher".into()),
                    activity_refs: vec!["activity:method:1".into()],
                    evidence_refs: vec!["evidence:method:1".into()],
                    return_refs: vec!["return:method:1".into()],
                },
            )
            .unwrap();

        Fixture {
            state,
            selection,
            journey,
            commission,
            praxis,
        }
    }

    #[test]
    fn structured_build_reuses_same_agent_profile_method_routine_journey_and_run_refs() {
        let fixture = fixture("enabled");
        let snapshot = developmental_build_snapshot(
            &fixture.state,
            &fixture.selection,
            &fixture.journey,
            &fixture.commission,
            &fixture.praxis,
        )
        .unwrap();

        assert_eq!(snapshot.contract, FACTORY_DEVELOPMENTAL_BUILD_VIEW);
        assert_eq!(
            snapshot.build.view.run.run_ref,
            fixture.selection.run_ref.to_string()
        );
        assert_eq!(snapshot.commission.journey_ref, fixture.journey.journey_ref);
        assert_eq!(snapshot.praxis.journey_ref, fixture.journey.journey_ref);
        assert_eq!(
            snapshot.praxis.agent_profiles[0].profile_ref,
            "agent-profile:researcher"
        );
        assert_eq!(
            snapshot.praxis.praxis_returns[0].method_ref,
            "skill/factory-native/factory-bounded-work"
        );
        assert_eq!(
            snapshot.praxis.routines[0].routine_ref,
            "routine:daily-research"
        );
        assert!(snapshot
            .to_json()
            .unwrap()
            .contains("routine:daily-research"));
    }

    #[test]
    fn stale_proof_is_visible_in_same_build_snapshot_as_revalidation_required() {
        let fixture = fixture(AIKIT_ROUTINE_STALE_PROOF_STATE);
        let snapshot = developmental_build_snapshot(
            &fixture.state,
            &fixture.selection,
            &fixture.journey,
            &fixture.commission,
            &fixture.praxis,
        )
        .unwrap();
        assert_eq!(
            snapshot.praxis.revalidation_required_routine_refs,
            vec!["routine:daily-research"]
        );
        assert_eq!(
            snapshot.praxis.routines[0].routine_state,
            AIKIT_ROUTINE_STALE_PROOF_STATE
        );
    }
}
