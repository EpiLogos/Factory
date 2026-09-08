use epilogos_factory::build::{ClaimRecord, EvidenceRecord, FactoryBuildState};
use epilogos_factory::core::run::{Project, ProjectRef, Run, RunRef};
use epilogos_factory::project_development::{
    CapabilityPraxisRow, DevelopmentObservation, DevelopmentObservationKind, OwnerReturnProposal,
    PraxisCondition, ProjectDevelopmentError, ProjectDevelopmentLedger,
    ProjectOrientationCondition, ReflectionAnchor,
};
use epilogos_factory::structural_ground::{
    verify_structural_ground, StructuralBinding, StructuralFidelityIssueKind,
    StructuralFidelityObservation, StructuralGround, StructuralSourceRef,
    STRUCTURAL_GROUND_VERSION,
};
use std::str::FromStr;

const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAA";
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAB";
const CENTRAL_CONTRACT: &str = "github:EpiLogos/Central:docs/PROJECTCENTRAL-CONTRACT.md";
const CENTRAL_MAIN: &str = "github:EpiLogos/Central:main";
const OI_GROUND: &str = "github:EpiLogos/O-I:docs/positions/FOUNDING-POSITIONS.md";
const AIKIT_KNOWLEDGE: &str =
    "github:EpiLogos/ai-kit:crates/aikit-core/src/knowledge_navigation.rs";
const AIKIT_PROJECT_MAP: &str = "github:EpiLogos/ai-kit:crates/aikit-core/src/project_map.rs";
const AIKIT_REFLECTION_HEAD: &str = "abc341dad55dfdb7c6792b3c344dc921825fe81c";
const EPI_REVISION: &str = "daa660cbc1b8c5da83828698665a753852cb0287";
const QL_HEAD: &str = "de7d50c9f7dcfec33cfa0fd5f8a8a1068b4fbe84";
const SEMANTIC: &str = "formal:sixfold-complement";
const MANIFEST: &str =
    "github:EpiLogos/QL-MEF:docs/integrations/epi-logos/EPI-HOLOGRAPHIC-KERNEL-MANIFEST.json";
const CODE: &str = "github:EpiLogos/QL-MEF:c/src/primitive.c#ql_position_invert";
const CLAIM: &str = "claim:01ARZ3NDEKTSV4RRFFQ69G5FAD";
const EVIDENCE: &str = "evidence:01ARZ3NDEKTSV4RRFFQ69G5FAE";

fn run_ref() -> RunRef {
    RunRef::from_str(RUN).unwrap()
}

fn epi_orientation() -> ProjectOrientationCondition {
    ProjectOrientationCondition {
        run_ref: run_ref(),
        condition_ref: "orientation/epi-project".into(),
        project_ref: "project/epi-logos".into(),
        projectcentral_contract_ref: Some(CENTRAL_CONTRACT.into()),
        projectcentral_revision: Some(CENTRAL_MAIN.into()),
        // No committed ProjectCentral instance is invented here. The authored whole-programme
        // Ground is retained while physical ProjectCentral acceptance remains explicit evidence work.
        human_ground_refs: vec![OI_GROUND.into()],
        semantic_wiki_refs: vec!["semantic-wiki/epi-logos".into()],
        structural_ground_ref: Some("structural-ground/epi-holographic-kernel".into()),
        knowledge_application_ref: Some(AIKIT_KNOWLEDGE.into()),
        project_map_ref: Some(AIKIT_PROJECT_MAP.into()),
        frontier_refs: vec!["github:EpiLogos/agent-system-design/issues/155".into()],
    }
}

fn epi_anchor() -> ReflectionAnchor {
    ReflectionAnchor {
        run_ref: run_ref(),
        anchor_ref: "reflection/epi-sixfold-complement".into(),
        semantic_ref: SEMANTIC.into(),
        local_source_ref: Some(MANIFEST.into()),
        code_refs: vec![CODE.into()],
        verification_refs: vec![
            "github:EpiLogos/ai-kit:crates/aikit-core/tests/epi_holographic_reflection.rs".into(),
        ],
        route_ref: Some("aikit-project-reflection/epi-sixfold-complement".into()),
        provider_ref: "github:EpiLogos/ai-kit".into(),
        provider_revision: AIKIT_REFLECTION_HEAD.into(),
        relation: "implemented-by".into(),
    }
}

#[test]
fn real_epi_anchor_is_bidirectionally_inspectable_without_a_factory_project_map() {
    let mut ledger = ProjectDevelopmentLedger::new(run_ref());
    ledger.set_orientation(epi_orientation()).unwrap();
    ledger.add_reflection_anchor(epi_anchor()).unwrap();

    let reverse = ledger.anchors_for_code(CODE);
    assert_eq!(reverse.len(), 1);
    assert_eq!(reverse[0].semantic_ref, SEMANTIC);
    assert_eq!(reverse[0].local_source_ref.as_deref(), Some(MANIFEST));
    assert_eq!(reverse[0].provider_ref, "github:EpiLogos/ai-kit");
    assert_eq!(reverse[0].provider_revision, AIKIT_REFLECTION_HEAD);

    let review = ledger.human_review();
    assert_eq!(review.intention_and_ground_refs, vec![OI_GROUND]);
    assert_eq!(review.semantic_refs, vec![SEMANTIC]);
    assert_eq!(review.local_source_refs, vec![MANIFEST]);
    assert_eq!(review.code_refs, vec![CODE]);
}

#[test]
fn stale_structural_binding_returns_discrepancy_and_owner_proposal_without_mutation() {
    let ground = StructuralGround {
        version: STRUCTURAL_GROUND_VERSION.into(),
        id: "structural-ground/epi-holographic-kernel".into(),
        source_refs: vec![StructuralSourceRef {
            reference: "github:EpiLogos/Epi-Logos-C-Experiments".into(),
            revision: Some(EPI_REVISION.into()),
        }],
        structural_manifest_refs: vec![MANIFEST.into()],
        relation_manifest_refs: Vec::new(),
        in_scope_identities: vec![SEMANTIC.into()],
        constitutive_relations: Vec::new(),
        existing_implementation_bindings: vec![StructuralBinding {
            identity: SEMANTIC.into(),
            implementation_ref: CODE.into(),
            relation: "implemented-by".into(),
            implementation_revision: Some(QL_HEAD.into()),
        }],
        unresolved_source_gaps: Vec::new(),
        return_or_mutation_law: Some(
            "Factory reports divergence; QL-MEF remains source/code mutation authority".into(),
        ),
    };
    let observed = StructuralFidelityObservation {
        source_refs: ground.source_refs.clone(),
        identities: vec![SEMANTIC.into()],
        implementation_bindings: vec![StructuralBinding {
            identity: SEMANTIC.into(),
            implementation_ref: CODE.into(),
            relation: "implemented-by".into(),
            implementation_revision: Some("stale-local-description-revision".into()),
        }],
        constitutive_relations: Vec::new(),
    };

    let fidelity = verify_structural_ground(run_ref(), &ground, &observed).unwrap();
    assert!(!fidelity.passed);
    assert!(fidelity
        .issues
        .iter()
        .any(|issue| issue.kind == StructuralFidelityIssueKind::StaleBinding));

    let mut ledger = ProjectDevelopmentLedger::new(run_ref());
    ledger.add_reflection_anchor(epi_anchor()).unwrap();
    ledger
        .add_observation(DevelopmentObservation {
            run_ref: run_ref(),
            observation_ref: "observation/stale-sixfold-binding".into(),
            kind: DevelopmentObservationKind::ProjectReflectionDiscrepancy,
            statement: "declared sixfold complement binding is stale against observed implementation revision"
                .into(),
            subject_refs: vec![SEMANTIC.into(), MANIFEST.into(), CODE.into()],
            evidence_refs: vec!["structural-fidelity/epi-sixfold-stale".into()],
            owner_return: Some(OwnerReturnProposal {
                owner_ref: "github:EpiLogos/QL-MEF".into(),
                source_ref: Some(MANIFEST.into()),
                proposal_ref: "proposal/refresh-sixfold-reflection".into(),
                recognition_required: true,
            }),
        })
        .unwrap();

    assert_eq!(ledger.reflection_anchors[0], epi_anchor());
    let review = ledger.human_review();
    assert_eq!(review.discrepancies.len(), 1);
    assert_eq!(
        review.recognition_returns[0].owner_ref,
        "github:EpiLogos/QL-MEF"
    );
}

#[test]
fn praxis_configuration_is_input_and_fitness_returns_as_ordinary_claim_evidence() {
    let project_ref = ProjectRef::from_str(PROJECT).unwrap();
    let run_ref = run_ref();
    let project = Project::new(project_ref.clone());
    let run = Run::new(
        run_ref.clone(),
        project_ref,
        "Develop through project meaning and return what reality disclosed",
        "factory-project-development-test",
    )
    .unwrap();
    let mut build = FactoryBuildState::new(project, run).unwrap();

    build
        .insert_evidence(EvidenceRecord {
            run_ref: run_ref.clone(),
            evidence_ref: EVIDENCE.into(),
            label: "Method was fit for semantic-to-code reflection work".into(),
            assessment: Some("supported".into()),
            native_ref: Some("aikit-history/praxis-resolution-155".into()),
            producing_execution_ref: Some("execution/project-development-155".into()),
        })
        .unwrap();
    build
        .insert_claim(ClaimRecord {
            run_ref: run_ref.clone(),
            claim_ref: CLAIM.into(),
            statement: "The selected project-reflection praxis was fit for this developmental act."
                .into(),
            status: "supported".into(),
            evidence_refs: vec![EVIDENCE.into()],
        })
        .unwrap();

    let mut ledger = ProjectDevelopmentLedger::new(run_ref.clone());
    ledger
        .set_praxis(PraxisCondition {
            run_ref: run_ref.clone(),
            condition_ref: "praxis/condition-155".into(),
            focus_ref: Some("focus/project-reflection".into()),
            method_ref: Some("skill/aikit/knowledge-navigation".into()),
            skill_refs: vec!["aikit:skill/knowledge-navigation".into()],
            skill_set_refs: vec!["aikit:skill-set/factory-development".into()],
            profile_ref: Some("aikit:profile/code".into()),
            usage_overlay_refs: vec!["aikit:usage-overlay/project-155".into()],
            context_source_refs: vec![CENTRAL_CONTRACT.into(), AIKIT_KNOWLEDGE.into()],
            action_refs: vec!["aikit:action/project-reflection".into()],
            model_ref: Some("model/current-harness-selection".into()),
            harness_ref: Some("harness/current-code".into()),
            harness_composition_ref: Some("aikit:harness-composition/resolved-155".into()),
            agency_ref: Some("actuation:agency/factory-developer".into()),
            material_condition_refs: Vec::new(),
            resolution_ref: Some("aikit:resolution/praxis-155".into()),
            provider_ref: "github:EpiLogos/ai-kit".into(),
            provider_revision: AIKIT_REFLECTION_HEAD.into(),
        })
        .unwrap();
    ledger
        .add_capability_row(CapabilityPraxisRow {
            run_ref: run_ref.clone(),
            row_ref: "capability-row/project-reflection".into(),
            capability_ref: "capability/project-reflection".into(),
            skill_ref: Some("aikit:skill/knowledge-navigation".into()),
            method_ref: Some("skill/aikit/knowledge-navigation".into()),
            project_target_refs: vec![SEMANTIC.into(), CODE.into()],
            ql_affinity: Some("descriptive-affinity-only".into()),
            use_type: "development".into(),
            context_source_refs: vec![CENTRAL_CONTRACT.into()],
            action_refs: vec!["aikit:action/project-reflection".into()],
            model_ref: Some("model/current-harness-selection".into()),
            harness_ref: Some("harness/current-code".into()),
            agency_ref: Some("actuation:agency/factory-developer".into()),
            material_condition_refs: Vec::new(),
            verification_expectation:
                "semantic/source/code relation remains attributable and inspectable".into(),
            observed_fitness_evidence_refs: vec![EVIDENCE.into()],
        })
        .unwrap();
    ledger
        .add_observation(DevelopmentObservation {
            run_ref,
            observation_ref: "observation/praxis-fitness".into(),
            kind: DevelopmentObservationKind::PraxisFitness,
            statement: "resolved praxis was fit for the consequential reflection act".into(),
            subject_refs: vec!["skill/aikit/knowledge-navigation".into()],
            evidence_refs: vec![EVIDENCE.into(), CLAIM.into()],
            owner_return: Some(OwnerReturnProposal {
                owner_ref: "github:EpiLogos/ai-kit".into(),
                source_ref: Some("skill/aikit/knowledge-navigation".into()),
                proposal_ref: "proposal/praxis-fitness-155".into(),
                recognition_required: true,
            }),
        })
        .unwrap();

    assert_eq!(
        ledger.praxis.as_ref().unwrap().method_ref.as_deref(),
        Some("skill/aikit/knowledge-navigation")
    );
    assert_eq!(
        ledger.capability_rows[0].observed_fitness_evidence_refs,
        vec![EVIDENCE]
    );
    let serialized = serde_json::to_string(&build).unwrap();
    assert!(serialized.contains(CLAIM));
    assert!(serialized.contains(EVIDENCE));
}

#[test]
fn ordinary_project_needs_no_projectcentral_method_coordinates_or_headers() {
    let run_ref = run_ref();
    let mut ledger = ProjectDevelopmentLedger::new(run_ref.clone());
    ledger
        .set_orientation(ProjectOrientationCondition {
            run_ref: run_ref.clone(),
            condition_ref: "orientation/generic".into(),
            project_ref: "project/generic".into(),
            projectcentral_contract_ref: None,
            projectcentral_revision: None,
            human_ground_refs: Vec::new(),
            semantic_wiki_refs: Vec::new(),
            structural_ground_ref: None,
            knowledge_application_ref: None,
            project_map_ref: None,
            frontier_refs: vec!["focus/ordinary-change".into()],
        })
        .unwrap();
    ledger
        .set_praxis(PraxisCondition {
            run_ref: run_ref.clone(),
            condition_ref: "praxis/generic".into(),
            focus_ref: Some("focus/ordinary-change".into()),
            method_ref: None,
            skill_refs: Vec::new(),
            skill_set_refs: Vec::new(),
            profile_ref: None,
            usage_overlay_refs: Vec::new(),
            context_source_refs: vec!["context/source".into()],
            action_refs: vec!["action/edit".into()],
            model_ref: None,
            harness_ref: None,
            harness_composition_ref: None,
            agency_ref: None,
            material_condition_refs: Vec::new(),
            resolution_ref: None,
            provider_ref: "generic-context-provider".into(),
            provider_revision: "1".into(),
        })
        .unwrap();
    ledger
        .add_capability_row(CapabilityPraxisRow {
            run_ref,
            row_ref: "capability-row/generic".into(),
            capability_ref: "capability/edit".into(),
            skill_ref: None,
            method_ref: None,
            project_target_refs: vec!["source/file".into()],
            ql_affinity: None,
            use_type: "ordinary".into(),
            context_source_refs: vec!["context/source".into()],
            action_refs: vec!["action/edit".into()],
            model_ref: None,
            harness_ref: None,
            agency_ref: None,
            material_condition_refs: Vec::new(),
            verification_expectation: "ordinary test passes".into(),
            observed_fitness_evidence_refs: Vec::new(),
        })
        .unwrap();

    assert!(ledger.reflection_anchors.is_empty());
    assert!(ledger.praxis.as_ref().unwrap().method_ref.is_none());
    assert!(ledger.capability_rows[0].ql_affinity.is_none());
}

#[test]
fn development_records_cannot_leak_across_runs_or_duplicate_receipt_identity() {
    let run_ref = run_ref();
    let mut ledger = ProjectDevelopmentLedger::new(run_ref.clone());
    ledger.set_orientation(epi_orientation()).unwrap();
    let duplicate = ledger.set_praxis(PraxisCondition {
        run_ref: run_ref.clone(),
        condition_ref: "orientation/epi-project".into(),
        focus_ref: None,
        method_ref: None,
        skill_refs: Vec::new(),
        skill_set_refs: Vec::new(),
        profile_ref: None,
        usage_overlay_refs: Vec::new(),
        context_source_refs: Vec::new(),
        action_refs: Vec::new(),
        model_ref: None,
        harness_ref: None,
        harness_composition_ref: None,
        agency_ref: None,
        material_condition_refs: Vec::new(),
        resolution_ref: None,
        provider_ref: "provider".into(),
        provider_revision: "1".into(),
    });
    assert!(matches!(
        duplicate,
        Err(ProjectDevelopmentError::DuplicateRef(_))
    ));

    let other_run = RunRef::from_str("run:01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
    let mut wrong_anchor = epi_anchor();
    wrong_anchor.run_ref = other_run;
    assert!(matches!(
        ledger.add_reflection_anchor(wrong_anchor),
        Err(ProjectDevelopmentError::WrongRun { .. })
    ));
}
