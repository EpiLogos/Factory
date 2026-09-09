//! W12-style three-way witness for composed agent capability intake (#188).
//!
//! Deterministic, recorded-fixture proof (no live calls) that the same stable
//! refs resolve to the same authored evidence (Central shape) and the same
//! effective evidence (AIKit shape) independently on every side, and that a
//! Factory Run can be commissioned whose agent capability comes entirely from
//! those refs — with retirement and withholding visible at intake.
//!
//! Fixture provenance: the envelopes under `contracts/factory/fixtures/intake/`
//! were captured by executing the exact revisions pinned in
//! `conformance-pins.json` (mirroring O:I `suite/mainline.json` at
//! observed_at 2026-09-09) in an isolated temp capture crate.

use epilogos_factory::agent_capability_intake::{
    AgentCapabilityIntake, AgentCapabilityRef, AuthoredAgentProfileRef, AuthoredProfileEvidence,
    EffectiveCompositionEvidence, EffectiveSkillSetRef, SkillSetMemberStanding,
    AGENT_CAPABILITY_INTAKE_SCHEMA,
};
use epilogos_factory::core::run::{Project, ProjectRef, Run, RunRef};
use epilogos_factory::journey::{Journey, JourneyCommission, JourneyRef};
use epilogos_factory::journey_commission::{JourneyAccountableSubject, JourneyCommissionState};
use serde_json::{json, Value};

const AUTHORED_FIXTURE: &str =
    include_str!("../../contracts/factory/fixtures/intake/central-authored-envelope.json");
const EFFECTIVE_FIXTURE: &str =
    include_str!("../../contracts/factory/fixtures/intake/aikit-effective-envelope.json");
const CONFORMANCE_PINS: &str =
    include_str!("../../contracts/factory/fixtures/intake/conformance-pins.json");

fn fixture(raw: &str) -> Value {
    serde_json::from_str(raw).expect("intake fixture must be JSON")
}

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FB0";

fn run_ref() -> RunRef {
    RUN.parse().unwrap()
}

// ---------------------------------------------------------------------------
// Pin binding: this lane answers to the pinned revisions, never live heads
// ---------------------------------------------------------------------------

#[test]
fn recorded_envelopes_cite_exactly_the_pinned_owner_revisions() {
    let pins = fixture(CONFORMANCE_PINS);
    let pinned_central = pins["pins"]["central"]["revision"].as_str().unwrap();
    let pinned_ai_kit = pins["pins"]["ai-kit"]["revision"].as_str().unwrap();

    let authored = fixture(AUTHORED_FIXTURE);
    assert_eq!(
        authored["schema"],
        json!("factory.agent-capability-intake-fixture/v1")
    );
    assert_eq!(authored["captured_from"]["revision"], json!(pinned_central));
    assert_eq!(authored["captured_from"]["product"], json!("central"));

    let effective = fixture(EFFECTIVE_FIXTURE);
    assert_eq!(
        effective["schema"],
        json!("factory.agent-capability-intake-fixture/v1")
    );
    assert_eq!(effective["captured_from"]["revision"], json!(pinned_ai_kit));
    assert_eq!(effective["captured_from"]["product"], json!("ai-kit"));

    assert_eq!(
        pins["source_of_truth"]["path"].as_str().unwrap(),
        "suite/mainline.json",
        "the pin authority is O:I suite/mainline.json"
    );
}

// ---------------------------------------------------------------------------
// Authored side: the same ref resolves to the same authored evidence,
// read independently through two paths (typed struct and raw JSON pointers)
// ---------------------------------------------------------------------------

fn authored_evidence() -> AuthoredProfileEvidence {
    let envelope = fixture(AUTHORED_FIXTURE);
    AuthoredProfileEvidence::from_profile_body(&envelope["envelope"]["profile"])
        .expect("recorded authored evidence parses through the typed path")
}

fn authored_raw_identity() -> (String, String, String, String) {
    let envelope = fixture(AUTHORED_FIXTURE);
    let profile = &envelope["envelope"]["profile"];
    (
        profile["ref"].as_str().unwrap().to_string(),
        profile["revision"].as_str().unwrap().to_string(),
        profile["agent_ref"].as_str().unwrap().to_string(),
        profile["scope"].as_str().unwrap().to_string(),
    )
}

#[test]
fn authored_ref_resolves_to_the_same_authored_evidence_on_independent_paths() {
    let typed = authored_evidence();
    let (profile_ref, revision, agent_ref, scope) = authored_raw_identity();

    assert_eq!(typed.profile_ref, profile_ref);
    assert_eq!(typed.revision, revision);
    assert_eq!(typed.agent_ref, agent_ref);
    assert_eq!(typed.scope.as_str(), scope.as_str());
    assert_eq!(typed.schema, "central.agent-profile/v1");
    assert_eq!(
        typed.skill_set_refs,
        vec!["skill-set:factory-intake".to_string()],
        "the authored source assigns the SkillSet the effective side answers for"
    );

    // The ref grammar round-trips from the evidence identity alone.
    let reference = AuthoredAgentProfileRef::new(
        typed.scope,
        typed.profile_ref.clone(),
        typed.revision.clone(),
    )
    .unwrap();
    let text = reference.to_string();
    assert_eq!(
        text,
        "central.agent-profile/personal/agent-profile:factory-researcher@profile-rev-1"
    );
    assert_eq!(text.parse::<AuthoredAgentProfileRef>().unwrap(), reference);
}

// ---------------------------------------------------------------------------
// Effective side: the same ref resolves to the same effective evidence on
// independent paths, including the resolver's structured withheld reasons
// ---------------------------------------------------------------------------

fn effective_evidence() -> EffectiveCompositionEvidence {
    let envelope = fixture(EFFECTIVE_FIXTURE);
    EffectiveCompositionEvidence::from_envelope(&envelope["envelope"])
        .expect("recorded effective evidence parses through the typed path")
}

#[test]
fn effective_ref_resolves_to_the_same_effective_evidence_on_independent_paths() {
    let envelope = fixture(EFFECTIVE_FIXTURE)["envelope"].clone();
    let typed = effective_evidence();

    assert_eq!(typed.set_name, envelope["set"]["name"].as_str().unwrap());
    assert_eq!(
        typed.generation_id,
        envelope["generation"]["generation_id"].as_str().unwrap()
    );
    assert_eq!(
        typed.projected.len(),
        envelope["projected"].as_array().unwrap().len()
    );
    assert_eq!(
        typed.withheld.len(),
        envelope["withheld"].as_array().unwrap().len()
    );

    let reference =
        EffectiveSkillSetRef::new(typed.set_name.clone(), typed.generation_id.clone()).unwrap();
    let text = reference.to_string();
    assert_eq!(text, "aikit.skillset/factory-intake@gen_6c009b389c5f5dcc");
    assert_eq!(text.parse::<EffectiveSkillSetRef>().unwrap(), reference);
}

// ---------------------------------------------------------------------------
// Use side: a Run is commissioned whose capability comes entirely from the
// stable refs, and retirement/unresolved/withholding surface at intake
// ---------------------------------------------------------------------------

fn journey_with_run() -> Journey {
    let mut journey = Journey::new(
        "journey:01ARZ3NDEKTSV4RRFFQ69G5FC0"
            .parse::<JourneyRef>()
            .unwrap(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAE"
            .parse::<ProjectRef>()
            .unwrap(),
        JourneyCommission {
            purpose: "Carry composed agent capability through stable refs.".into(),
            commission_ref: Some("source:intake:oi-profile-intake".into()),
            why_refs: vec!["source:vision:oi".into()],
        },
        "Return one verified Run on composed capability.",
        "2026-09-06T09:00:00Z",
    )
    .unwrap();
    journey
        .add_run(run_ref(), vec!["basis:pinned-refs".into()], vec![])
        .unwrap();
    journey
}

#[test]
fn a_run_is_commissioned_entirely_from_stable_refs_with_honest_intake_facts() {
    let authored = authored_evidence();
    let effective = effective_evidence();

    let capability = AgentCapabilityRef::new(
        authored.agent_ref.clone(),
        AuthoredAgentProfileRef::new(
            authored.scope,
            authored.profile_ref.clone(),
            authored.revision.clone(),
        )
        .unwrap(),
        EffectiveSkillSetRef::new(effective.set_name.clone(), effective.generation_id.clone())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(capability.schema, AGENT_CAPABILITY_INTAKE_SCHEMA);

    // Developmental use: the Run exists first; the intake binds to it.
    let run = Run::new(
        run_ref(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap(),
        "Return verified work on composed capability.",
        "factory",
    )
    .unwrap();
    assert_eq!(run.reference().to_string(), RUN);

    let intake = AgentCapabilityIntake::admit(run_ref(), capability, &authored, &effective)
        .expect("refs answer to the recorded evidence: intake is admitted");

    // Failure honesty: every member the composition replied about is a fact.
    let envelope = fixture(EFFECTIVE_FIXTURE)["envelope"].clone();
    let expected_members = envelope["members"].as_u64().unwrap() as usize;
    assert_eq!(
        intake.members.len(),
        expected_members,
        "no member is silently dropped"
    );
    assert_eq!(
        intake.members.len(),
        intake.projected_members().len() + intake.withheld_members().len()
    );

    let retired = intake.retired_members();
    assert_eq!(retired.len(), 1, "a retired member surfaces at intake");
    assert_eq!(
        retired[0].capability_ref,
        "skill/factory/legacy-orientation"
    );
    assert_eq!(retired[0].standing, SkillSetMemberStanding::RetiredStanding);
    assert_eq!(
        retired[0].withheld_kind.as_deref(),
        Some("retired-standing")
    );
    assert!(
        retired[0].reason.as_deref().unwrap().contains(
            "Superseded by the composed AgentProfile orientation source; retired by owner."
        ),
        "the owner's retirement reason travels verbatim: {}",
        retired[0].reason.as_deref().unwrap_or_default()
    );

    let unresolved = intake.unresolved_members();
    assert_eq!(
        unresolved.len(),
        1,
        "an unresolved member surfaces at intake"
    );
    assert_eq!(
        unresolved[0].capability_ref,
        "skill/factory/unresolved-member"
    );
    assert_eq!(unresolved[0].standing, SkillSetMemberStanding::NotInCatalog);

    // Commission: the intake rides the commission, not a second config store.
    let journey = journey_with_run();
    let mut commission = JourneyCommissionState::commission(
        &journey,
        "commission:oi-profile-intake",
        "human:owner",
        JourneyAccountableSubject::Agent {
            agent_ref: authored.agent_ref.clone(),
        },
        "Return verified work with composed, evidence-backed capability.",
        vec!["closure:tests-green".into()],
    )
    .unwrap();
    commission
        .set_capability_intake(&journey, intake)
        .expect("the intake Run belongs to the Journey");

    let reading = commission.reading(&journey).unwrap();
    let intake_reading = reading.capability_intake.as_ref().unwrap();
    assert_eq!(intake_reading.schema, AGENT_CAPABILITY_INTAKE_SCHEMA);
    assert_eq!(intake_reading.run_ref, run_ref());
    assert_eq!(intake_reading.agent_ref, "agent:factory-researcher");
    assert_eq!(
        intake_reading.authored_source.to_string(),
        "central.agent-profile/personal/agent-profile:factory-researcher@profile-rev-1"
    );
    assert_eq!(
        intake_reading.effective_composition.to_string(),
        "aikit.skillset/factory-intake@gen_6c009b389c5f5dcc"
    );
    assert_eq!(
        intake_reading.retired_capability_refs,
        vec!["skill/factory/legacy-orientation".to_string()]
    );
    assert_eq!(
        intake_reading.unresolved_capability_refs,
        vec!["skill/factory/unresolved-member".to_string()]
    );
    assert_eq!(intake_reading.members.len(), expected_members);

    // The three-way identity: the same agent/set identity is readable on the
    // authored side, the effective side and the use side without conversion.
    assert_eq!(intake_reading.agent_ref, authored.agent_ref);
    assert!(
        authored
            .skill_set_refs
            .iter()
            .any(|r| r.strip_prefix("skill-set:")
                == Some(intake_reading.effective_composition.set_name.as_str())),
        "authored SkillSet assignment and effective set are the same identity"
    );

    // Commission state round-trips with the intake attached.
    let serialized = serde_json::to_string(&commission).unwrap();
    let restored: JourneyCommissionState = serde_json::from_str(&serialized).unwrap();
    assert_eq!(restored.capability_intake, commission.capability_intake);
}

#[test]
fn intake_cannot_bind_to_a_run_outside_the_commissioned_journey() {
    let authored = authored_evidence();
    let effective = effective_evidence();
    let intake = AgentCapabilityIntake::admit(
        run_ref(),
        AgentCapabilityRef::new(
            authored.agent_ref.clone(),
            AuthoredAgentProfileRef::new(
                authored.scope,
                authored.profile_ref.clone(),
                authored.revision.clone(),
            )
            .unwrap(),
            EffectiveSkillSetRef::new(effective.set_name.clone(), effective.generation_id.clone())
                .unwrap(),
        )
        .unwrap(),
        &authored,
        &effective,
    )
    .unwrap();

    let mut journey = journey_with_run();
    journey.runs.clear();
    let mut commission = JourneyCommissionState::commission(
        &journey,
        "commission:no-run",
        "human:owner",
        JourneyAccountableSubject::Agent {
            agent_ref: "agent:factory-researcher".into(),
        },
        "Should not accept a floating intake.",
        vec![],
    )
    .unwrap();
    assert!(commission.set_capability_intake(&journey, intake).is_err());
}

#[test]
fn commissions_without_an_intake_keep_working_unchanged() {
    // Additivity law: historical commission records have no capability_intake
    // field and must continue to deserialize and read.
    let journey = journey_with_run();
    let commission = JourneyCommissionState::commission(
        &journey,
        "commission:legacy",
        "human:owner",
        JourneyAccountableSubject::AgentSet {
            agent_set_ref: "agent-set:development".into(),
        },
        "Existing commissions are unchanged.",
        vec![],
    )
    .unwrap();
    let value = serde_json::to_value(&commission).unwrap();
    assert!(value.get("capability_intake").is_none());
    let restored: JourneyCommissionState = serde_json::from_value(json!({
        "schema": value["schema"].clone(),
        "journey_ref": value["journey_ref"].clone(),
        "commission_ref": value["commission_ref"].clone(),
        "commissioner_ref": value["commissioner_ref"].clone(),
        "accountable": value["accountable"].clone(),
        "intended_difference": value["intended_difference"].clone(),
        "state": value["state"].clone(),
        "journey_revision": value["journey_revision"].clone(),
    }))
    .unwrap();
    assert!(restored.capability_intake.is_none());
    assert!(restored
        .reading(&journey)
        .unwrap()
        .capability_intake
        .is_none());
    let project = Project::new("project:01ARZ3NDEKTSV4RRFFQ69G5FAE".parse().unwrap());
    assert_eq!(
        project.reference().to_string(),
        "project:01ARZ3NDEKTSV4RRFFQ69G5FAE"
    );
}
