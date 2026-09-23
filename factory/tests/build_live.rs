use epilogos_factory::build::{
    CandidateRecord, ClaimRecord, EvidenceRecord, ExecutionRecord, FactoryActionAuthority,
    FactoryActionExecutor, FactoryActionInvocation, FactoryBuildError, FactoryBuildSelection,
    FactoryBuildState, FactoryBuildViewProvider, TrajectoryRecord, FACTORY_NATIVE_OWNER,
    REQUEST_MORE_EVIDENCE_ACTION_REF, REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
};
use epilogos_factory::core::run::{
    EdgeKind, NodeId, NodeKind, NodeState, Project, ProjectRef, Run, RunRef, RunTopologyCommand,
    TopologyEdge, TopologyMutation, TopologyNode,
};
use serde_json::json;
use std::str::FromStr;

const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAA";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAB";
const CANDIDATE: &str = "candidate:01ARZ3NDEKTSV4RRFFQ69G5FAC";
const CLAIM: &str = "claim:01ARZ3NDEKTSV4RRFFQ69G5FAD";
const EVIDENCE: &str = "evidence:01ARZ3NDEKTSV4RRFFQ69G5FAE";
const EXECUTION_RICH: &str = "execution:01ARZ3NDEKTSV4RRFFQ69G5FAF";
const EXECUTION_THIN: &str = "execution:01ARZ3NDEKTSV4RRFFQ69G5FAG";
const SESSION_SPACE: &str = "session-space/factory-build-live";

fn canonical_state() -> (FactoryBuildState, FactoryBuildSelection) {
    let project_ref = ProjectRef::from_str(PROJECT).unwrap();
    let run_ref = RunRef::from_str(RUN).unwrap();
    let project = Project::new(project_ref.clone());
    let run = Run::new(
        run_ref.clone(),
        project_ref.clone(),
        "Host the live Factory Build surface",
        "factory-test-owner",
    )
    .unwrap();
    let mut state = FactoryBuildState::new(project, run).unwrap();

    // Change canonical Run/RunMap state, rather than introducing a GUI-local copy.
    let authority = state.run_mutation_authority(&run_ref).unwrap();
    let work_id = NodeId::new("live-build").unwrap();
    state
        .apply_run_topology_command(
            &run_ref,
            &authority,
            RunTopologyCommand {
                command_id: "build-live-frontier".into(),
                expected_revision: state.run(&run_ref).unwrap().revision(),
                mutation: TopologyMutation::Batch {
                    mutations: vec![
                        TopologyMutation::AddNode {
                            node: TopologyNode {
                                id: work_id.clone(),
                                kind: NodeKind::Work,
                                label: "Observe FactoryBuildView from canonical state".into(),
                                state: Some(NodeState::Active),
                                semantic_ref: None,
                            },
                        },
                        TopologyMutation::AddEdge {
                            edge: TopologyEdge {
                                from: NodeId::new("destination").unwrap(),
                                to: work_id,
                                relation: EdgeKind::BranchesTo,
                            },
                        },
                    ],
                },
            },
        )
        .unwrap();

    state
        .insert_evidence(EvidenceRecord {
            run_ref: run_ref.clone(),
            evidence_ref: EVIDENCE.into(),
            label: "Provider conformance evidence".into(),
            assessment: Some("standing".into()),
            native_ref: Some("dsh-session-event/42".into()),
            producing_execution_ref: Some(EXECUTION_RICH.into()),
        })
        .unwrap();
    state
        .insert_claim(ClaimRecord {
            run_ref: run_ref.clone(),
            claim_ref: CLAIM.into(),
            statement: "The live Build provider reflects canonical Factory state.".into(),
            status: "supported".into(),
            evidence_refs: vec![EVIDENCE.into()],
        })
        .unwrap();
    state
        .insert_candidate(CandidateRecord {
            run_ref: run_ref.clone(),
            candidate_ref: CANDIDATE.into(),
            revision: 3,
            label: "Native hosted Build Candidate".into(),
            status: "ready".into(),
            producing_execution_refs: vec![EXECUTION_RICH.into(), EXECUTION_THIN.into()],
            claim_refs: vec![CLAIM.into()],
            evidence_refs: vec![EVIDENCE.into()],
            artifact_refs: vec!["artifact/factory-build".into()],
            preview_ref: Some("surface/factory-build".into()),
            tradeoffs: vec!["thin Pi evidence remains thin".into()],
        })
        .unwrap();
    state
        .insert_execution(ExecutionRecord {
            run_ref: run_ref.clone(),
            execution_ref: EXECUTION_RICH.into(),
            status: "running".into(),
            agency_ref: Some("agency/parashakti-build".into()),
            agent_ref: Some("agent/parashakti".into()),
            harness_ref: Some("harness/deepseek".into()),
            harness_composition_ref: Some("harness-composition/dsh-maximal".into()),
            agent_session_ref: Some("agent-session/dsh-42".into()),
            session_space_ref: Some(SESSION_SPACE.into()),
            surface_refs: vec!["surface/factory-build".into()],
            workcell_binding_refs: vec!["binding/workcell-42".into()],
            native_trajectory_ref: Some("dsh-trajectory/42".into()),
        })
        .unwrap();
    state
        .insert_execution(ExecutionRecord {
            run_ref: run_ref.clone(),
            execution_ref: EXECUTION_THIN.into(),
            status: "success".into(),
            agency_ref: None,
            agent_ref: Some("agent/mahamaya".into()),
            harness_ref: Some("harness/pi".into()),
            harness_composition_ref: None,
            agent_session_ref: Some("agent-session/pi-7".into()),
            session_space_ref: None,
            surface_refs: Vec::new(),
            workcell_binding_refs: Vec::new(),
            native_trajectory_ref: Some("sssf-session/7".into()),
        })
        .unwrap();
    state
        .insert_trajectory(TrajectoryRecord {
            run_ref: run_ref.clone(),
            execution_ref: EXECUTION_RICH.into(),
            value: json!({
                "executionRef": EXECUTION_RICH,
                "nativeTrajectory": {
                    "kind": "deepseek-harness",
                    "ref": "dsh-trajectory/42",
                    "fingerprint": "composition-fingerprint-42"
                },
                "spans": [{"kind": "process", "nativeRef": "fiber/cordis-host"}]
            }),
        })
        .unwrap();
    state
        .insert_trajectory(TrajectoryRecord {
            run_ref: run_ref.clone(),
            execution_ref: EXECUTION_THIN.into(),
            value: json!({
                "executionRef": EXECUTION_THIN,
                "nativeTrajectory": {"kind": "sssf", "ref": "sssf-session/7"},
                "spans": []
            }),
        })
        .unwrap();

    (
        state,
        FactoryBuildSelection {
            project_ref,
            run_ref,
        },
    )
}

#[test]
fn canonical_factory_state_materialises_without_read_side_mutation() {
    let (state, selection) = canonical_state();
    let provider = FactoryBuildViewProvider;
    let before = serde_json::to_string(&state).unwrap();
    let snapshot = provider.snapshot(&state, &selection).unwrap();
    let after = serde_json::to_string(&state).unwrap();

    assert_eq!(
        before, after,
        "snapshot reads must not mutate canonical state"
    );
    assert_eq!(snapshot.revision, state.revision().get());
    assert_eq!(snapshot.provenance.owner, FACTORY_NATIVE_OWNER);
    assert_eq!(
        snapshot.provenance.factory_state_revision,
        snapshot.revision
    );
    assert_eq!(snapshot.view.project.project_ref, PROJECT);
    assert_eq!(snapshot.view.run.run_ref, RUN);
    assert_eq!(
        snapshot.view.frontier.title,
        "Observe FactoryBuildView from canonical state"
    );
    assert_eq!(snapshot.view.candidates[0].claim_refs, vec![CLAIM]);
    assert_eq!(snapshot.view.claims[0].evidence_refs, vec![EVIDENCE]);
    assert_eq!(
        snapshot.view.evidence[0].producing_execution_ref.as_deref(),
        Some(EXECUTION_RICH)
    );

    let rich = snapshot
        .view
        .executions
        .iter()
        .find(|execution| execution.execution_ref == EXECUTION_RICH)
        .unwrap();
    assert_eq!(rich.session_space_ref.as_deref(), Some(SESSION_SPACE));
    assert_eq!(
        rich.native_trajectory_ref.as_deref(),
        Some("dsh-trajectory/42")
    );
    let thin = snapshot
        .view
        .executions
        .iter()
        .find(|execution| execution.execution_ref == EXECUTION_THIN)
        .unwrap();
    assert!(thin.harness_composition_ref.is_none());
    assert!(thin.session_space_ref.is_none());

    // The published build-view contract accepts exactly this document.
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../contracts/factory/build-view.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::Validator::new(&schema).unwrap();
    let instance = serde_json::to_value(&snapshot).unwrap();
    let errors = validator
        .iter_errors(&instance)
        .map(|error| format!("{error} at {}", error.instance_path))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{errors:#?}");

    let serialised = snapshot.to_json().unwrap();
    assert!(serialised.contains(SESSION_SPACE));
    assert!(serialised.contains("composition-fingerprint-42"));
    assert!(serialised.contains("sssf-session/7"));
}

#[test]
fn authorised_factory_action_mutates_canonical_state_and_next_snapshot_revision() {
    let (mut state, selection) = canonical_state();
    let provider = FactoryBuildViewProvider;
    let executor = FactoryActionExecutor;
    let before = provider.snapshot(&state, &selection).unwrap();
    assert!(before.view.human_requests.is_empty());
    assert_eq!(
        before.view.actions[0].action_ref,
        REQUEST_MORE_EVIDENCE_ACTION_REF
    );

    let invocation = FactoryActionInvocation {
        action_ref: REQUEST_MORE_EVIDENCE_ACTION_REF.into(),
        subject_ref: CANDIDATE.into(),
        run_ref: selection.run_ref.clone(),
    };

    let missing_capability = executor
        .execute(
            &mut state,
            &invocation,
            &FactoryActionAuthority {
                authority_ref: "authority/oi-build".into(),
                native_owner: FACTORY_NATIVE_OWNER.into(),
                capability_ref: None,
                capability_granted: false,
                action_authorised: true,
            },
        )
        .unwrap_err();
    assert_eq!(
        missing_capability,
        FactoryBuildError::MissingCapabilityGrant
    );

    let missing_action_authority = executor
        .execute(
            &mut state,
            &invocation,
            &FactoryActionAuthority {
                authority_ref: "authority/oi-build".into(),
                native_owner: FACTORY_NATIVE_OWNER.into(),
                capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: false,
            },
        )
        .unwrap_err();
    assert_eq!(
        missing_action_authority,
        FactoryBuildError::MissingActionAuthority
    );

    let wrong_owner = executor
        .execute(
            &mut state,
            &invocation,
            &FactoryActionAuthority {
                authority_ref: "authority/oi-build".into(),
                native_owner: "oi".into(),
                capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
        )
        .unwrap_err();
    assert_eq!(
        wrong_owner,
        FactoryBuildError::WrongNativeOwner("oi".into())
    );

    // Failed authority checks have not mutated canonical Factory state.
    assert_eq!(
        provider.snapshot(&state, &selection).unwrap().revision,
        before.revision
    );

    let receipt = executor
        .execute(
            &mut state,
            &invocation,
            &FactoryActionAuthority {
                authority_ref: "authority/oi-build".into(),
                native_owner: FACTORY_NATIVE_OWNER.into(),
                capability_ref: Some(REQUEST_MORE_EVIDENCE_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
        )
        .unwrap();
    let after = provider.snapshot(&state, &selection).unwrap();

    assert!(after.revision > before.revision);
    assert_eq!(receipt.previous_revision, before.revision);
    assert_eq!(receipt.next_revision, after.revision);
    assert_eq!(after.view.human_requests.len(), 1);
    assert_eq!(after.view.human_requests[0].evidence_refs, vec![EVIDENCE]);
    assert!(after.view.human_requests[0].question.contains(CANDIDATE));
}
