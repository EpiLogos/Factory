//! Authoritative current work for a World Position (`factory.current-work/v1`).
//!
//! Ported from OpenRig's `deriveCurrentWork` law, which refuses rather than
//! guesses because a wrong work node silently re-aims everything that reads it:
//!
//! - The input is every relation that could make work current for the Position:
//!   every `in-progress` custody record naming it, plus every attempt whose
//!   participant names it and which is the current attempt on a leg that is
//!   still running (`active` or `detached`, the leg states Factory already
//!   treats as able to return work). It is read straight from the whole
//!   developmental state: never a paged or capped display list, never
//!   most-recent-wins, never a first match.
//! - Each candidate is resolved to a work node before anything is counted. A
//!   WorkflowUnit inside a Run is the node `<run-ref>/<workflow-unit-ref>`;
//!   custody that names no WorkflowUnit is the node of its own `work_ref`.
//!   Candidates naming the same node collapse to one.
//! - A candidate that cannot be resolved is unknown, not irrelevant: the answer
//!   is refused as ambiguous with the reason, even when every other candidate
//!   resolved. More than one distinct node is ambiguous with every candidate
//!   listed. Exactly one node is the answer. None is an answer too, and its
//!   basis says that blocked custody is not current work.
//! - The result is independent of insertion order: candidates are sorted and
//!   every list in the reading is ordered by stable refs.

use crate::developmental_read::FactoryDevelopmentalState;
use crate::orchestration::LegStatus;
use crate::work_custody::{resolve_work_address, CustodyState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FACTORY_CURRENT_WORK: &str = "factory.current-work/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CurrentWorkOutcome {
    None,
    One,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateSource {
    Custody,
    Attempt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkNodeKind {
    WorkflowUnit,
    Work,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CandidateResolution {
    Resolved,
    Unresolved,
}

/// One relation that made work current for the Position, as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentWorkCandidate {
    pub source: CandidateSource,
    /// The custody ref or attempt ref that carries the relation.
    pub source_ref: String,
    pub resolution: CandidateResolution,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journey_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_unit_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_ref: Option<String>,
    /// Custody state or leg status at the time of reading.
    pub status: String,
}

/// The one work node the Position is currently carrying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkNode {
    pub node_ref: String,
    pub kind: WorkNodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_unit_ref: Option<String>,
    pub work_refs: Vec<String>,
    pub journey_refs: Vec<String>,
    pub custody_refs: Vec<String>,
    pub attempt_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentWorkReading {
    pub schema: String,
    pub position_ref: String,
    pub project_ref: String,
    pub outcome: CurrentWorkOutcome,
    /// Present only when the outcome is `one`; consumers read this, not `basis`.
    pub current: Option<WorkNode>,
    pub candidates: Vec<CurrentWorkCandidate>,
    /// Every relation naming the Position that was examined, of any state.
    pub considered: usize,
    pub basis: String,
}

fn unit_node_ref(run_ref: &str, workflow_unit_ref: &str) -> String {
    format!("{run_ref}/{workflow_unit_ref}")
}

/// Leg states in which the current attempt is still carrying work.
fn leg_is_running(status: LegStatus) -> bool {
    matches!(status, LegStatus::Active | LegStatus::Detached)
}

fn status_label(status: LegStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{status:?}"))
}

/// Gather every candidate for the Position from the whole state, uncapped.
/// Returns the candidates and how many relations naming the Position were seen.
pub(crate) fn gather_candidates(
    state: &FactoryDevelopmentalState,
    position_ref: &str,
) -> (Vec<CurrentWorkCandidate>, usize) {
    let mut considered = 0;
    let mut candidates = Vec::new();
    for record in &state.work_custody {
        if record.position_ref != position_ref {
            continue;
        }
        considered += 1;
        if record.state != CustodyState::InProgress {
            continue;
        }
        let run_ref = record.run_ref.as_ref().map(ToString::to_string);
        let unit_ref = record.workflow_unit_ref.as_ref().map(ToString::to_string);
        let resolution = resolve_work_address(
            state,
            record.run_ref.as_ref(),
            record.journey_ref.as_ref(),
            record.workflow_unit_ref.as_ref(),
        );
        let (resolution, node_ref, reason) = match resolution {
            Ok(()) => (
                CandidateResolution::Resolved,
                Some(match (&run_ref, &unit_ref) {
                    (Some(run), Some(unit)) => unit_node_ref(run, unit),
                    _ => record.work_ref.clone(),
                }),
                None,
            ),
            Err((_, fact)) => (CandidateResolution::Unresolved, None, Some(fact)),
        };
        candidates.push(CurrentWorkCandidate {
            source: CandidateSource::Custody,
            source_ref: record.custody_ref.clone(),
            resolution,
            node_ref,
            reason,
            work_ref: Some(record.work_ref.clone()),
            run_ref,
            journey_ref: record.journey_ref.as_ref().map(ToString::to_string),
            workflow_unit_ref: unit_ref,
            execution_ref: None,
            status: record.state.as_str().into(),
        });
    }
    for (run_ref, field) in &state.attempt_states {
        for attempt in field.attempts().values() {
            if attempt.disposition.participant.position_ref.as_deref() != Some(position_ref) {
                continue;
            }
            considered += 1;
            let execution = attempt
                .execution_ref
                .as_deref()
                .unwrap_or(&attempt.reserved_execution_ref);
            let run = run_ref.to_string();
            let unit = attempt.workflow_unit_ref.to_string();
            let candidate = |resolution, node_ref, reason, status: String| CurrentWorkCandidate {
                source: CandidateSource::Attempt,
                source_ref: attempt.attempt_ref.clone(),
                resolution,
                node_ref,
                reason,
                work_ref: None,
                run_ref: Some(run.clone()),
                journey_ref: None,
                workflow_unit_ref: Some(unit.clone()),
                execution_ref: Some(execution.to_owned()),
                status,
            };
            let Some(leg) = field.snapshot().legs().get(&attempt.workflow_unit_ref) else {
                // Whether this attempt is current cannot be told; that is
                // unknown work, so it blocks an answer instead of vanishing.
                candidates.push(candidate(
                    CandidateResolution::Unresolved,
                    None,
                    Some(format!(
                        "attempt {} has no native leg for WorkflowUnit {unit} in Run {run}",
                        attempt.attempt_ref
                    )),
                    "unavailable".into(),
                ));
                continue;
            };
            if leg.execution_ref != execution || !leg_is_running(leg.status) {
                continue;
            }
            let resolved =
                resolve_work_address(state, Some(run_ref), None, Some(&attempt.workflow_unit_ref));
            candidates.push(match resolved {
                Ok(()) => candidate(
                    CandidateResolution::Resolved,
                    Some(unit_node_ref(&run, &unit)),
                    None,
                    status_label(leg.status),
                ),
                Err((_, fact)) => candidate(
                    CandidateResolution::Unresolved,
                    None,
                    Some(fact),
                    status_label(leg.status),
                ),
            });
        }
    }
    (candidates, considered)
}

/// The decision over already-gathered candidates. Pure and order-independent.
pub fn decide(
    position_ref: &str,
    project_ref: &str,
    mut candidates: Vec<CurrentWorkCandidate>,
    considered: usize,
) -> CurrentWorkReading {
    candidates.sort_by(|left, right| {
        (&left.node_ref, left.source, &left.source_ref).cmp(&(
            &right.node_ref,
            right.source,
            &right.source_ref,
        ))
    });
    let reading = |outcome, current, basis: String, candidates| CurrentWorkReading {
        schema: FACTORY_CURRENT_WORK.into(),
        position_ref: position_ref.into(),
        project_ref: project_ref.into(),
        outcome,
        current,
        candidates,
        considered,
        basis,
    };
    if candidates.is_empty() {
        return reading(
            CurrentWorkOutcome::None,
            None,
            format!(
                "no in-progress work for this Position: only in-progress custody and attempts that are current on an active or detached leg count; blocked, released, completed or handed-off custody and historical or finished attempts are not current work ({considered} relation(s) naming the Position examined, uncapped)"
            ),
            candidates,
        );
    }
    let failures = candidates
        .iter()
        .filter(|candidate| candidate.resolution == CandidateResolution::Unresolved)
        .map(|candidate| {
            format!(
                "{} {} — {}",
                match candidate.source {
                    CandidateSource::Custody => "custody",
                    CandidateSource::Attempt => "attempt",
                },
                candidate.source_ref,
                candidate.reason.as_deref().unwrap_or("did not resolve")
            )
        })
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        return reading(
            CurrentWorkOutcome::Ambiguous,
            None,
            format!(
                "in-progress work did not resolve: {} — refusing to answer from the relations that did resolve",
                failures.join("; ")
            ),
            candidates,
        );
    }
    let mut nodes: BTreeMap<&str, Vec<&CurrentWorkCandidate>> = BTreeMap::new();
    for candidate in &candidates {
        if let Some(node_ref) = candidate.node_ref.as_deref() {
            nodes.entry(node_ref).or_default().push(candidate);
        }
    }
    if nodes.len() > 1 {
        let listed = nodes.keys().copied().collect::<Vec<_>>().join(", ");
        return reading(
            CurrentWorkOutcome::Ambiguous,
            None,
            format!(
                "{} distinct in-progress work nodes ({listed}) — refusing to guess",
                nodes.len()
            ),
            candidates,
        );
    }
    let (node_ref, group) = nodes
        .into_iter()
        .next()
        .expect("non-empty resolved candidates have one node");
    let unit_backed = group
        .iter()
        .find(|candidate| candidate.run_ref.is_some() && candidate.workflow_unit_ref.is_some());
    let collect = |pick: &dyn Fn(&CurrentWorkCandidate) -> Option<&String>| {
        group
            .iter()
            .filter_map(|candidate| pick(candidate).cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let by_source = |source: CandidateSource| {
        group
            .iter()
            .filter(|candidate| candidate.source == source)
            .map(|candidate| candidate.source_ref.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let node = WorkNode {
        node_ref: node_ref.to_owned(),
        kind: if unit_backed.is_some() {
            WorkNodeKind::WorkflowUnit
        } else {
            WorkNodeKind::Work
        },
        run_ref: unit_backed.and_then(|candidate| candidate.run_ref.clone()),
        workflow_unit_ref: unit_backed.and_then(|candidate| candidate.workflow_unit_ref.clone()),
        work_refs: collect(&|candidate| candidate.work_ref.as_ref()),
        journey_refs: collect(&|candidate| candidate.journey_ref.as_ref()),
        custody_refs: by_source(CandidateSource::Custody),
        attempt_refs: by_source(CandidateSource::Attempt),
    };
    let basis = format!(
        "one in-progress work node {} from {} relation(s): {} custody, {} running attempt(s)",
        node.node_ref,
        group.len(),
        node.custody_refs.len(),
        node.attempt_refs.len()
    );
    reading(CurrentWorkOutcome::One, Some(node), basis, candidates)
}

/// Derive the Position's current work from the whole developmental state.
pub fn derive_current_work(
    state: &FactoryDevelopmentalState,
    position_ref: &str,
) -> CurrentWorkReading {
    let (candidates, considered) = gather_candidates(state, position_ref);
    decide(
        position_ref,
        &state.project_ref().to_string(),
        candidates,
        considered,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FactoryBuildState;
    use crate::core::run::{Project, ProjectRef};
    use crate::work_custody::{assign_in, update_in, AssignRequest, UpdateRequest};

    const P: &str = "central:position:project:O-I:factory-guardian";
    const OTHER: &str = "central:position:project:O-I:reviewer";

    fn empty_state() -> FactoryDevelopmentalState {
        let project: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap();
        FactoryDevelopmentalState::new(FactoryBuildState::empty(Project::new(project)), vec![])
            .unwrap()
    }

    fn assign(state: &mut FactoryDevelopmentalState, position: &str, work: &str) -> String {
        assign_in(
            state,
            AssignRequest {
                position_ref: position.into(),
                work_ref: work.into(),
                reason: format!("assign {work}"),
                ..AssignRequest::default()
            },
            1,
        )
        .unwrap()
        .custody
        .custody_ref
    }

    fn set(state: &mut FactoryDevelopmentalState, custody_ref: &str, to: CustodyState) {
        update_in(
            state,
            UpdateRequest {
                custody_ref: custody_ref.into(),
                state: Some(to),
                reason: format!("move to {to}"),
                ..UpdateRequest::default()
            },
            2,
        )
        .unwrap();
    }

    #[test]
    fn nothing_held_is_none_with_a_scoped_basis() {
        let reading = derive_current_work(&empty_state(), P);
        assert_eq!(reading.outcome, CurrentWorkOutcome::None);
        assert!(reading.current.is_none());
        assert_eq!(reading.considered, 0);
        assert!(reading.basis.contains("blocked"));
    }

    #[test]
    fn blocked_custody_is_not_current_work() {
        let mut state = empty_state();
        let parked = assign(&mut state, P, "work:parked");
        set(&mut state, &parked, CustodyState::Blocked);
        let reading = derive_current_work(&state, P);
        assert_eq!(reading.outcome, CurrentWorkOutcome::None);
        assert_eq!(reading.considered, 1);
        assert!(reading.candidates.is_empty());
    }

    #[test]
    fn exactly_one_in_progress_relation_is_the_answer() {
        let mut state = empty_state();
        let held = assign(&mut state, P, "work:one");
        assign(&mut state, OTHER, "work:someone-else");
        let reading = derive_current_work(&state, P);
        assert_eq!(reading.outcome, CurrentWorkOutcome::One);
        let current = reading.current.unwrap();
        assert_eq!(current.node_ref, "work:one");
        assert_eq!(current.kind, WorkNodeKind::Work);
        assert_eq!(current.custody_refs, vec![held]);
        assert_eq!(reading.considered, 1);
    }

    #[test]
    fn two_distinct_nodes_are_ambiguous_with_every_candidate_listed() {
        let mut state = empty_state();
        assign(&mut state, P, "work:a");
        assign(&mut state, P, "work:b");
        let reading = derive_current_work(&state, P);
        assert_eq!(reading.outcome, CurrentWorkOutcome::Ambiguous);
        assert!(reading.current.is_none());
        assert_eq!(reading.candidates.len(), 2);
        assert!(reading.basis.contains("2 distinct"));
    }

    /// The presentation cap proof: the second in-progress relation sits far
    /// beyond any 100-row page in insertion order, and a capped list would
    /// have answered confidently with the first one.
    #[test]
    fn ambiguity_beyond_any_display_cap_is_still_refused() {
        let mut state = empty_state();
        let mut in_progress = Vec::new();
        for index in 0..152 {
            let work = format!("work:{index:03}");
            let reference = assign(&mut state, P, &work);
            match index {
                3 | 147 => in_progress.push(reference),
                _ if index % 2 == 0 => set(&mut state, &reference, CustodyState::Blocked),
                _ => set(&mut state, &reference, CustodyState::Completed),
            }
        }
        let capped = state.work_custody.iter().take(100).collect::<Vec<_>>();
        assert_eq!(
            capped
                .iter()
                .filter(|record| record.state == CustodyState::InProgress)
                .count(),
            1,
            "a 100-row page would see exactly one in-progress relation"
        );
        let reading = derive_current_work(&state, P);
        assert_eq!(reading.outcome, CurrentWorkOutcome::Ambiguous);
        assert_eq!(reading.considered, 152);
        let mut listed = reading
            .candidates
            .iter()
            .map(|candidate| candidate.source_ref.clone())
            .collect::<Vec<_>>();
        listed.sort();
        in_progress.sort();
        assert_eq!(listed, in_progress);

        // With the early one closed, the late one alone is found, not dropped.
        set(&mut state, &in_progress[0].clone(), CustodyState::Completed);
        let reading = derive_current_work(&state, P);
        assert_eq!(reading.outcome, CurrentWorkOutcome::One);
        assert_eq!(reading.current.unwrap().node_ref, "work:147");
    }

    #[test]
    fn the_answer_does_not_depend_on_insertion_order() {
        let mut state = empty_state();
        for work in ["work:x", "work:y", "work:z"] {
            assign(&mut state, P, work);
        }
        let parked = assign(&mut state, P, "work:parked");
        set(&mut state, &parked, CustodyState::Blocked);
        let baseline = serde_json::to_string(&derive_current_work(&state, P)).unwrap();
        let count = state.work_custody.len();
        for seed in 1..=12u64 {
            let mut shuffled = state.clone();
            // Deterministic Fisher-Yates with a small LCG: no RNG dependency.
            let mut value = seed;
            for index in (1..count).rev() {
                value = value
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let other = (value >> 33) as usize % (index + 1);
                shuffled.work_custody.swap(index, other);
            }
            assert_eq!(
                serde_json::to_string(&derive_current_work(&shuffled, P)).unwrap(),
                baseline,
                "seed {seed} changed the reading"
            );
        }
    }

    #[test]
    fn an_unresolvable_candidate_refuses_even_beside_a_resolved_one() {
        let mut state = empty_state();
        assign(&mut state, P, "work:fine");
        // Custody pointing at a Run the state does not hold (hand-edited drift).
        let mut broken = state.work_custody[0].clone();
        broken.custody_ref = "factory:custody:0190f5c2-0000-7000-8000-000000000001".into();
        broken.work_ref = "work:fine".into();
        broken.run_ref = Some("run:01ARZ3NDEKTSV4RRFFQ69G5FAA".parse().unwrap());
        state.work_custody.push(broken);
        let reading = derive_current_work(&state, P);
        assert_eq!(reading.outcome, CurrentWorkOutcome::Ambiguous);
        assert!(reading.current.is_none());
        assert!(reading.basis.contains("did not resolve"));
        assert!(reading
            .candidates
            .iter()
            .any(|candidate| candidate.resolution == CandidateResolution::Unresolved));
    }

    #[test]
    fn candidates_naming_one_node_collapse_to_one() {
        let candidate = |source, source_ref: &str| CurrentWorkCandidate {
            source,
            source_ref: source_ref.into(),
            resolution: CandidateResolution::Resolved,
            node_ref: Some("run:R/workflow-unit:U".into()),
            reason: None,
            work_ref: (source == CandidateSource::Custody).then(|| format!("work:{source_ref}")),
            run_ref: Some("run:R".into()),
            journey_ref: None,
            workflow_unit_ref: Some("workflow-unit:U".into()),
            execution_ref: None,
            status: "in-progress".into(),
        };
        let reading = decide(
            P,
            "project:X",
            vec![
                candidate(CandidateSource::Attempt, "attempt:1"),
                candidate(CandidateSource::Custody, "factory:custody:b"),
                candidate(CandidateSource::Custody, "factory:custody:a"),
            ],
            3,
        );
        assert_eq!(reading.outcome, CurrentWorkOutcome::One);
        let current = reading.current.unwrap();
        assert_eq!(current.kind, WorkNodeKind::WorkflowUnit);
        assert_eq!(
            current.custody_refs,
            vec!["factory:custody:a", "factory:custody:b"]
        );
        assert_eq!(current.attempt_refs, vec!["attempt:1"]);
        assert_eq!(current.run_ref.as_deref(), Some("run:R"));
    }
}
