//! `factory.inhabitation-reading/v1`: per Run, the Positions in custody and the
//! occupant relations Factory already holds for each attempt.
//!
//! This is a projection over the developmental state, not a registry. Every
//! foreign ref (Central Position, AIKit/Actuation Agent, Agency, AgentSession,
//! SessionSpace, Workcell, NOW) is carried verbatim from the attempt record that
//! holds it, as a facet that is either `present` with its value or `absent`
//! with the reason. Sensing work may carry a source-qualified child NOW before
//! an Attempt exists; that relation is distinct from an Attempt's placement
//! NOW. Factory's own world facts are only its `project_ref` and the Central
//! project ref it was explicitly linked to.

use crate::attempt_runtime::FactoryAttemptRecord;
use crate::core::run::{RunLifecycle, RunRef};
use crate::developmental_read::FactoryDevelopmentalState;
use crate::orchestration::LegStatus;
use crate::work_custody::{FactoryWorkCustody, Refusal, NOTHING_READ};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FACTORY_INHABITATION_READING: &str = "factory.inhabitation-reading/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FacetState {
    Present,
    Absent,
    Ambiguous,
    Unavailable,
    NotAttempted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facet {
    pub state: FacetState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub source: String,
}

impl Facet {
    fn present(value: &str, source: &str) -> Self {
        Self {
            state: FacetState::Present,
            value: Some(value.to_owned()),
            reason: None,
            source: source.to_owned(),
        }
    }

    fn optional(value: Option<&str>, absent_reason: &str, source: &str) -> Self {
        match value {
            Some(value) => Self::present(value, source),
            None => Self {
                state: FacetState::Absent,
                value: None,
                reason: Some(absent_reason.to_owned()),
                source: source.to_owned(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantFacets {
    pub position_ref: Facet,
    pub agent_ref: Facet,
    pub agency_ref: Facet,
    pub profile_ref: Facet,
    pub world_binding_ref: Facet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyFacets {
    pub agent_session_ref: Facet,
    pub session_space_ref: Facet,
    pub workcell_ref: Facet,
    pub material_world_ref: Facet,
    pub model_ref: Facet,
    pub provider_ref: Facet,
    pub harness_ref: Facet,
    pub harness_composition_ref: Facet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementFacets {
    pub now_ref: Facet,
}

/// What Factory holds about who occupied one attempt, and in what body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OccupantRelation {
    pub attempt_ref: String,
    pub task_ref: String,
    pub workflow_unit_ref: String,
    pub execution_ref: String,
    /// True when this attempt is the one its WorkflowUnit's leg currently runs.
    pub current_attempt: bool,
    /// The leg status for this attempt's execution, when the leg retains it.
    pub leg_status: Option<LegStatus>,
    pub participant: ParticipantFacets,
    pub body: BodyFacets,
    pub placement: PlacementFacets,
    pub return_address: Facet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionCustodySummary {
    pub custody_ref: String,
    pub state: String,
    pub work_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_unit_ref: Option<String>,
    /// The child NOW for this exact native custody/work relation.
    pub child_now_ref: Facet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionInRun {
    pub position_ref: String,
    /// True while any custody for this Position in the Run is open.
    pub in_custody: bool,
    pub custody: Vec<PositionCustodySummary>,
    pub attempt_refs: Vec<String>,
    pub current_attempt_refs: Vec<String>,
    /// The signal work's child NOW, joined to this exact Run/custody/Position.
    /// An Attempt's separate placement NOW remains under `occupants`.
    pub child_now_ref: Facet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunInhabitation {
    pub run_ref: String,
    pub lifecycle: RunLifecycle,
    pub journey_refs: Vec<String>,
    pub positions: Vec<PositionInRun>,
    pub custody: Vec<FactoryWorkCustody>,
    pub occupants: Vec<OccupantRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct InhabitationFilter {
    pub run_ref: Option<String>,
    pub position_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InhabitationReading {
    pub schema: String,
    pub project_ref: String,
    pub central_project_ref: Facet,
    pub filter: InhabitationFilter,
    pub runs: Vec<RunInhabitation>,
    /// Custody that names no Run; omitted from a Run-filtered reading.
    pub custody_outside_runs: Vec<FactoryWorkCustody>,
}

fn occupant(
    run_ref: &RunRef,
    attempt: &FactoryAttemptRecord,
    legs: &BTreeMap<crate::core::run::WorkflowUnitRef, crate::orchestration::LegRecord>,
) -> OccupantRelation {
    let source = format!("factory.attempt-state/v1:{run_ref}:{}", attempt.attempt_ref);
    let source = source.as_str();
    let execution = attempt
        .execution_ref
        .as_deref()
        .unwrap_or(&attempt.reserved_execution_ref);
    let leg = legs.get(&attempt.workflow_unit_ref);
    let current_attempt = leg.is_some_and(|leg| leg.execution_ref == execution);
    let leg_status = leg.and_then(|leg| {
        if current_attempt {
            Some(leg.status)
        } else {
            leg.attempts
                .iter()
                .find(|item| item.execution_ref == execution)
                .map(|item| item.status)
        }
    });
    let participant = &attempt.disposition.participant;
    let body = &attempt.disposition.body;
    OccupantRelation {
        attempt_ref: attempt.attempt_ref.clone(),
        task_ref: attempt.task_ref.clone(),
        workflow_unit_ref: attempt.workflow_unit_ref.to_string(),
        execution_ref: execution.to_owned(),
        current_attempt,
        leg_status,
        participant: ParticipantFacets {
            position_ref: Facet::optional(
                participant.position_ref.as_deref(),
                "the attempt's participant names no World Position",
                source,
            ),
            agent_ref: Facet::present(&participant.agent_ref, source),
            agency_ref: Facet::present(&participant.agency_ref, source),
            profile_ref: Facet::optional(
                participant.profile_ref.as_deref(),
                "the attempt's participant was admitted without an AgentProfile",
                source,
            ),
            world_binding_ref: Facet::present(&participant.world_binding_ref, source),
        },
        body: BodyFacets {
            agent_session_ref: Facet::present(&body.agent_session_ref, source),
            session_space_ref: Facet::present(&body.session_space_ref, source),
            workcell_ref: Facet::optional(
                body.workcell_ref.as_deref(),
                "the attempt's body records no Workcell",
                source,
            ),
            material_world_ref: Facet::optional(
                body.material_world_ref.as_deref(),
                "the attempt's body records no material world",
                source,
            ),
            model_ref: Facet::present(&body.model_ref, source),
            provider_ref: Facet::present(&body.provider_ref, source),
            harness_ref: Facet::present(&body.harness_ref, source),
            harness_composition_ref: Facet::present(&body.harness_composition_ref, source),
        },
        placement: PlacementFacets {
            now_ref: Facet::optional(
                attempt
                    .disposition
                    .placement
                    .as_ref()
                    .map(|placement| placement.now_ref.as_str()),
                "the attempt was admitted without a placement NOW",
                source,
            ),
        },
        return_address: Facet::present(&attempt.disposition.return_address, source),
    }
}

fn position_entry<'a>(
    positions: &'a mut BTreeMap<String, PositionInRun>,
    position: &str,
) -> &'a mut PositionInRun {
    positions
        .entry(position.to_owned())
        .or_insert_with(|| PositionInRun {
            position_ref: position.to_owned(),
            in_custody: false,
            custody: Vec::new(),
            attempt_refs: Vec::new(),
            current_attempt_refs: Vec::new(),
            child_now_ref: Facet::optional(
                None,
                "no source-qualified sensing work child NOW names this Run and Position",
                "factory.sensing-state/v1:signal.work",
            ),
        })
}

fn sensing_child_now(
    state: &FactoryDevelopmentalState,
    run_ref: &RunRef,
    position_ref: &str,
    custody: &[FactoryWorkCustody],
) -> Facet {
    let source = "factory.sensing-state/v1:signal.work";
    let mut candidates = Vec::<(String, String)>::new();
    let mut conflicts = Vec::new();
    let run = run_ref.to_string();
    for signal in state.sensing.signals.values() {
        let Some(work) = signal.work.as_ref().filter(|work| {
            work.run_ref.as_deref() == Some(run.as_str()) && work.position_ref == position_ref
        }) else {
            continue;
        };
        if custody.len() == 1 && custody[0].work_ref != work.work_ref {
            continue;
        }
        let identity_matches = state.sensing.project_world_ref.as_deref()
            == Some(signal.project_world_ref.as_str())
            && work.work_ref == signal.signal_ref;
        let custody_matches = custody.iter().any(|record| {
            record.custody_ref == work.custody_ref
                && record.work_ref == work.work_ref
                && record.position_ref == work.position_ref
                && record.run_ref.as_ref() == Some(run_ref)
        });
        if !identity_matches || !custody_matches {
            conflicts.push(signal.signal_ref.as_str());
            continue;
        }
        if let Some(now_ref) = work.now_ref.as_deref() {
            let prefix = format!("central:now:{}:", signal.project_world_ref);
            if !now_ref.starts_with(&prefix) || now_ref.len() == prefix.len() {
                conflicts.push(signal.signal_ref.as_str());
                continue;
            }
            candidates.push((signal.signal_ref.clone(), now_ref.to_owned()));
        }
    }
    if !conflicts.is_empty() {
        return Facet {
            state: FacetState::Unavailable,
            value: None,
            reason: Some(format!(
                "sensing work conflicts with native Project, Run, custody, work or Position for {}",
                conflicts.join(", ")
            )),
            source: source.into(),
        };
    }
    match candidates.len() {
        0 => Facet::optional(
            None,
            "no source-qualified sensing work child NOW names this Run and Position",
            source,
        ),
        1 => {
            let (signal_ref, now_ref) = candidates.into_iter().next().expect("one candidate");
            Facet::present(&now_ref, &format!("{source}:{signal_ref}:work"))
        }
        count => Facet {
            state: FacetState::Ambiguous,
            value: None,
            reason: Some(format!(
                "{count} source-qualified work relations name a child NOW for this Run and Position"
            )),
            source: source.into(),
        },
    }
}

fn summary(
    state: &FactoryDevelopmentalState,
    record: &FactoryWorkCustody,
    run_ref: &RunRef,
) -> PositionCustodySummary {
    PositionCustodySummary {
        custody_ref: record.custody_ref.clone(),
        state: record.state.as_str().into(),
        work_ref: record.work_ref.clone(),
        workflow_unit_ref: record.workflow_unit_ref.as_ref().map(ToString::to_string),
        child_now_ref: sensing_child_now(
            state,
            run_ref,
            &record.position_ref,
            std::slice::from_ref(record),
        ),
    }
}

pub fn inhabitation_reading(
    state: &FactoryDevelopmentalState,
    run_filter: Option<&RunRef>,
    position_filter: Option<&str>,
) -> Result<InhabitationReading, Refusal> {
    if let Some(run_ref) = run_filter {
        if state.build.run(run_ref).is_none() {
            return Err(Refusal::new(
                "factory.inhabitation.unknown_run",
                format!("Run {run_ref} is not in this Factory project's developmental state"),
                NOTHING_READ,
                "list the project's Runs with `factory development project <state> <project-ref>` and read one of them",
            ));
        }
    }
    let names_position = |value: &str| position_filter.is_none_or(|wanted| wanted == value);
    let mut custody = state
        .work_custody
        .iter()
        .filter(|record| names_position(&record.position_ref))
        .cloned()
        .collect::<Vec<_>>();
    custody.sort_by(|left, right| left.custody_ref.cmp(&right.custody_ref));

    let mut runs = Vec::new();
    for run_ref in state.build.run_refs() {
        if run_filter.is_some_and(|wanted| wanted != &run_ref) {
            continue;
        }
        let run = state.build.run(&run_ref).expect("listed Run exists");
        let run_custody = custody
            .iter()
            .filter(|record| record.run_ref.as_ref() == Some(&run_ref))
            .cloned()
            .collect::<Vec<_>>();
        let mut occupants = state
            .attempt_states
            .get(&run_ref)
            .map(|field| {
                field
                    .attempts()
                    .values()
                    .filter(|attempt| {
                        position_filter.is_none()
                            || attempt.disposition.participant.position_ref.as_deref()
                                == position_filter
                    })
                    .map(|attempt| occupant(&run_ref, attempt, field.snapshot().legs()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        occupants.sort_by(|left, right| left.attempt_ref.cmp(&right.attempt_ref));
        if position_filter.is_some() && run_custody.is_empty() && occupants.is_empty() {
            continue;
        }
        let mut positions: BTreeMap<String, PositionInRun> = BTreeMap::new();
        for record in &run_custody {
            let position = position_entry(&mut positions, &record.position_ref);
            position.in_custody |= !record.state.is_terminal();
            position.custody.push(summary(state, record, &run_ref));
        }
        for relation in &occupants {
            let Some(position_ref) = relation.participant.position_ref.value.as_deref() else {
                continue;
            };
            let position = position_entry(&mut positions, position_ref);
            position.attempt_refs.push(relation.attempt_ref.clone());
            if relation.current_attempt {
                position
                    .current_attempt_refs
                    .push(relation.attempt_ref.clone());
            }
        }
        for position in positions.values_mut() {
            position.child_now_ref =
                sensing_child_now(state, &run_ref, &position.position_ref, &run_custody);
        }
        let mut journey_refs = state
            .journeys
            .iter()
            .filter(|journey| journey.runs.iter().any(|link| link.run_ref == run_ref))
            .map(|journey| journey.journey_ref.to_string())
            .collect::<Vec<_>>();
        journey_refs.sort();
        runs.push(RunInhabitation {
            run_ref: run_ref.to_string(),
            lifecycle: run.lifecycle(),
            journey_refs,
            positions: positions.into_values().collect(),
            custody: run_custody,
            occupants,
        });
    }
    let custody_outside_runs = if run_filter.is_some() {
        Vec::new()
    } else {
        custody
            .into_iter()
            .filter(|record| record.run_ref.is_none())
            .collect()
    };
    Ok(InhabitationReading {
        schema: FACTORY_INHABITATION_READING.into(),
        project_ref: state.project_ref().to_string(),
        central_project_ref: Facet::optional(
            state
                .central_project_link()
                .map(|link| link.central_project_ref.as_str()),
            "this Factory project carries no Central project link",
            "factory.developmental-local-provider/v1:centralProjectLinks",
        ),
        filter: InhabitationFilter {
            run_ref: run_filter.map(ToString::to_string),
            position_ref: position_filter.map(str::to_owned),
        },
        runs,
        custody_outside_runs,
    })
}
