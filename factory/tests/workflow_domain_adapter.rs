//! VW1: a registered domain adapter (QL Vāk) through Factory's own authoring,
//! lowering, compilation and Commission. Nothing here launches a participant.
//!
//! Proves: the registry pins QL's vendored declarations and their values equal
//! the native binding contract; the three QL-authored workflows check and
//! lower every composition; C-prime enters semantic identity; invalid frames,
//! authority, actors and thread topologies are refused with locations;
//! unregistered domain imports and un-imported compositions stay refused;
//! Commission retains the lowered composition and inspection reports it.
use epilogos_factory::vak_orchestration::{
    AUTHORED_C_PRIME_FIELDS, C_PRIME_CONTENT, C_PRIME_DIRECTION, C_PRIME_FRAME,
    C_PRIME_PARTICIPATION, C_PRIME_POSITION, C_PRIME_SEQUENCE, C_PRIME_THREAD,
    QL_C_PRIME_PROFILE_CONTRACT,
};
use epilogos_factory::workflow::{compile_workflow, WorkflowError};
use epilogos_factory::workflow_authoring::{cli, domain, load_workflow, Diagnostic};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const QL: &str = "@epilogos/ql-vak";
const WORKFLOWS: [&str; 3] = [
    "expression-development.workflow.ts",
    "techne-constellation.workflow.ts",
    "development-and-knowledge.workflow.ts",
];

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ql-vak-workflows")
}

fn ql_declarations() -> &'static str {
    domain::adapter(QL)
        .unwrap()
        .expect("QL Vāk adapter is registered")
        .declarations_text()
}

/// The string-literal union a vendored `export type Name = ...;` declares.
fn union(name: &str) -> BTreeSet<String> {
    let line = ql_declarations()
        .lines()
        .find(|line| line.starts_with(&format!("export type {name} = ")))
        .unwrap_or_else(|| panic!("QL declarations lack {name}"));
    let body = line
        .trim_start_matches(&format!("export type {name} = "))
        .trim_end_matches(';');
    body.split(" | ")
        .map(|literal| serde_json::from_str::<String>(literal).unwrap())
        .collect()
}

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn registry_pins_the_vendored_ql_declarations() {
    let adapters = domain::adapters().expect("registry is valid");
    assert_eq!(adapters.len(), 1);
    let adapter = &adapters[0];
    assert_eq!(adapter.specifier, QL);
    assert_eq!(adapter.owner, "ql-mef");
    assert_eq!(adapter.types_contract, "ql.vak-workflow-types/v1");
    assert_eq!(adapter.unit_field, "composition");
    assert_eq!(
        adapter.lowering,
        epilogos_factory::vak_orchestration::C_PRIME_LOWERING
    );
    assert_eq!(adapter.upstream.repository, "EpiLogos/QL-MEF");
    assert_eq!(
        adapter.upstream.path,
        "adapters/factory-workflow/index.d.ts"
    );
    // The registry is data; the file a test reads is the file the binary embeds.
    let on_disk = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(&adapter.declarations),
    )
    .unwrap();
    assert_eq!(on_disk, adapter.declarations_text());
    assert!(adapter.exports_type("CPrime"));
    assert!(!adapter.exports_type("CPrimeExecutionBinding"));
    assert!(
        !ql_declarations().contains("export declare function"),
        "types only"
    );
}

#[test]
fn vendored_ql_vocabulary_equals_the_native_binding_contract() {
    assert_eq!(union("Participation"), set(&C_PRIME_PARTICIPATION));
    assert_eq!(union("ContentType"), set(&C_PRIME_CONTENT));
    assert_eq!(union("ContentPosition"), set(&C_PRIME_POSITION));
    assert_eq!(union("ContextFrame"), set(&C_PRIME_FRAME));
    assert_eq!(union("ThreadForm"), set(&C_PRIME_THREAD));
    assert_eq!(union("ContextSequence"), set(&C_PRIME_SEQUENCE));
    assert_eq!(union("InquiryDirection"), set(&C_PRIME_DIRECTION));
    assert_eq!(
        union("ProfileContract"),
        set(&[QL_C_PRIME_PROFILE_CONTRACT])
    );
    // The authored CPrime fields Factory lowers are exactly QL's fields.
    let text = ql_declarations();
    let block = |start: &str| {
        let from = text.find(start).unwrap();
        let end = text[from..].find("};").unwrap();
        text[from..from + end].to_owned()
    };
    let mut declared = BTreeSet::new();
    for body in [
        block("export type CPrime = "),
        block("export type CPrimeParticipation ="),
    ] {
        for token in body.split("readonly ").skip(1) {
            let field = token.split([':', '?']).next().unwrap().trim().to_owned();
            declared.insert(field);
        }
    }
    assert_eq!(declared, set(&AUTHORED_C_PRIME_FIELDS));
    // The native schema/SDK carries the same values for the lowered binding.
    let schema: Value = serde_json::from_str(include_str!(
        "../../contracts/factory/agent-workflow-source.schema.json"
    ))
    .unwrap();
    let binding = &schema["$defs"]["cPrimeExecutionBinding"]["properties"];
    for (field, values) in [
        ("participation", &C_PRIME_PARTICIPATION[..]),
        ("content", &C_PRIME_CONTENT[..]),
        ("position", &C_PRIME_POSITION[..]),
        ("frame", &C_PRIME_FRAME[..]),
        ("thread", &C_PRIME_THREAD[..]),
        ("sequence", &C_PRIME_SEQUENCE[..]),
        ("direction", &C_PRIME_DIRECTION[..]),
    ] {
        let declared = binding[field]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(declared, set(values), "{field}");
    }
    // QL's wire spelling of the DayNow specialisation is ASCII.
    assert!(C_PRIME_CONTENT.contains(&"CT4b'"));
}

#[test]
fn ql_authored_workflows_check_and_lower_every_composition() {
    let mut frames = BTreeSet::new();
    let mut agents = BTreeSet::new();
    for name in WORKFLOWS {
        let loaded = load_workflow(&fixtures(), Path::new(name))
            .unwrap_or_else(|diagnostic| panic!("{name}: {diagnostic}"));
        let basis = loaded.source.source.authoring.as_ref().unwrap();
        assert_eq!(
            basis.adapters.keys().cloned().collect::<Vec<_>>(),
            vec![QL.to_string()]
        );
        assert_eq!(
            basis.adapters[QL].declarations_sha256,
            domain::adapter(QL).unwrap().unwrap().declarations_sha256
        );
        // Compilation is a pure function of the stamped native source,
        // including replay of the retained bytes and the adapter lowering.
        assert_eq!(
            compile_workflow(loaded.source.clone()).unwrap(),
            loaded.compiled
        );
        assert!(!loaded.compiled.units.is_empty());
        for unit in loaded.compiled.units.values() {
            let composition = unit
                .composition
                .as_ref()
                .unwrap_or_else(|| panic!("{name}:{} lacks C-prime", unit.key));
            assert_eq!(composition.contract, QL_C_PRIME_PROFILE_CONTRACT);
            assert_eq!(composition.subject_ref, unit.subject_ref.to_string());
            assert!(unit
                .agent_requirements
                .agent_refs
                .contains(&composition.actor_ref));
            for set_ref in &unit.agent_requirements.agent_set_refs {
                assert!(["agent-set/anima", "agent-set/aletheia"].contains(&set_ref.as_str()));
            }
            for praxis in &unit.praxis_refs {
                assert!(
                    praxis.starts_with("skill/ql/") || praxis.starts_with("skill/personal/"),
                    "{praxis}"
                );
            }
            frames.insert((name, composition.frame.clone()));
            agents.insert(composition.actor_ref.clone());
        }
    }
    // Expression development carries every constitutional voice CF1..CF7.
    for frame in C_PRIME_FRAME {
        assert!(
            frames.contains(&(WORKFLOWS[0], frame.to_string())),
            "{frame}"
        );
    }
    for agent in [
        "agent/anima",
        "agent/anima-nous",
        "agent/anima-logos",
        "agent/anima-eros",
        "agent/anima-mythos",
        "agent/anima-psyche",
        "agent/anima-sophia",
        "agent/anima-techne-helper",
        "agent/aletheia",
        "agent/aletheia-anansi",
        "agent/aletheia-janus",
        "agent/aletheia-moirai",
        "agent/aletheia-mercurius",
        "agent/aletheia-agora",
        "agent/aletheia-zeithoven",
    ] {
        assert!(agents.contains(agent), "{agent} performs no unit");
    }
}

#[test]
fn check_and_compile_report_the_composition() {
    let path = fixtures().join(WORKFLOWS[0]);
    let checked = cli::execute(&[
        "workflow".into(),
        "check".into(),
        path.to_string_lossy().into_owned(),
        "--json".into(),
    ])
    .unwrap();
    assert_eq!(checked["valid"], true);
    assert_eq!(checked["execution"], "not-requested");
    assert!(checked["source"]["authoring"]["domainAdapters"][QL].is_object());
    let units = checked["units"].as_array().unwrap();
    assert_eq!(units.len(), 8);
    let anima = units.iter().find(|u| u["key"] == "anima-conduct").unwrap();
    assert_eq!(anima["composition"]["frame"], "CF5");
    assert_eq!(anima["composition"]["threadForm"], "melody");
    assert_eq!(anima["composition"]["actorRef"], "agent/anima");
    let eros = units.iter().find(|u| u["key"] == "eros-exchange").unwrap();
    assert_eq!(eros["composition"]["threadForm"], "chord");

    let compiled = cli::execute(&[
        "workflow".into(),
        "compile".into(),
        path.to_string_lossy().into_owned(),
    ])
    .unwrap();
    let lowered = &compiled["compiled"]["units"]["nous-open-ground"]["composition"];
    assert_eq!(lowered["contract"], QL_C_PRIME_PROFILE_CONTRACT);
    assert_eq!(lowered["aiKitScopeContract"], "aikit.operative-scope/v1");
    assert_eq!(lowered["subjectRef"], "project:O-I");
    assert!(lowered.get("undertakingAuthorityRef").is_none());

    let help = cli::execute(&["workflow".into(), "help".into()]).unwrap();
    assert_eq!(help["domainAdapters"][0]["specifier"], QL);
}

// ---------------------------------------------------------------------------
// Small authored sources for refusal and identity cases.
// ---------------------------------------------------------------------------

struct Unit<'a> {
    key: &'a str,
    actor: &'a str,
    thread: &'a str,
    frame: &'a str,
    extra: &'a str,
    dependencies: &'a [&'a str],
    independence: &'a [&'a str],
    inputs: &'a [&'a str],
}

impl<'a> Unit<'a> {
    fn new(key: &'a str) -> Self {
        Self {
            key,
            actor: "agent/anima-logos",
            thread: "CFP0",
            frame: "CF2",
            extra: "",
            dependencies: &[],
            independence: &[],
            inputs: &[],
        }
    }
    fn render(&self, composition: bool) -> String {
        let list = |items: &[&str]| {
            items
                .iter()
                .map(|item| format!("\"{item}\""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let inputs = self
            .inputs
            .iter()
            .map(|p| {
                format!(
                    "{{ predecessor: \"{p}\", receivingContextRef: \"context:test/{}-from-{p}\" }}",
                    self.key
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let composition = if composition {
            format!(
                r#"
      composition: {{
        CPF: "dialogical", CT: "CT1", CP: "4.1", CF: "{frame}", CFP: "{thread}", CS: "CS2", direction: "forward",
        actor: "{actor}", interpretation: cprime, whole: "central:source:project:quaternal-logic:ProjectCentral/now",
        resolvePath: "resolve-scoped-path:test/{key}", contextResolution: "context-resolution:test/{key}",
        sources: ["central:source:project:quaternal-logic:AGENTS.md"],{extra}
      }} satisfies CPrime,"#,
                frame = self.frame,
                thread = self.thread,
                actor = self.actor,
                key = self.key,
                extra = self.extra,
            )
        } else {
            String::new()
        };
        format!(
            r#"
    unit({{
      key: "{key}",
      developmentalConcern: "Adapter test concern {key}",
      requiredDifference: "Adapter test difference",
      returnContract: "Return the adapter test result",
      subjectRef: "project:quaternal-logic",
      basisRevision: "c624c52",
      agentRequirements: {{ agentRefs: ["agent/anima-logos", "agent/anima-eros"], agentSetRefs: ["agent-set/anima"] }},
      praxisRefs: ["skill/personal/writing-plans"],
      capabilityRefs: ["capability/source-read"],
      dependencies: [{dependencies}],
      independenceFrom: [{independence}],
      inputs: [{inputs}],
      permittedEffects: ["read source"],
      verificationObligations: ["cite sources"],
      returnAddress: "central:source:project:quaternal-logic:ProjectCentral/now",
      stopConditions: "Stop when blocked",
      escalationConditions: "Escalate owner decisions",{composition}
    }}),"#,
            key = self.key,
            dependencies = list(self.dependencies),
            independence = list(self.independence),
        )
    }
}

fn source(imports: &str, units: &[Unit], composition: bool, barriers: &str) -> String {
    format!(
        r#"import {{ defineWorkflow, unit, barrier }} from "@epilogos/factory-workflow";
{imports}
const cprime = {{ ref: "ql/interpretation/c-prime", revision: "09f7d29ad6262f85bc2858f7c468f22d0bd398f3" }};
export default defineWorkflow({{
  source: {{ ref: "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FC1", revision: "adapter-test-v1" }},
  workflowKey: "adapter-test",
  units: [{units}
  ],
  barriers: [{barriers}],
}});
"#,
        units = units
            .iter()
            .map(|unit| unit.render(composition))
            .collect::<String>(),
    )
}

const IMPORT: &str = r#"import type { CPrime } from "@epilogos/ql-vak";"#;

fn load(text: &str) -> Result<epilogos_factory::workflow_authoring::AuthoredWorkflow, Diagnostic> {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test.workflow.ts"), text).unwrap();
    load_workflow(dir.path(), Path::new("test.workflow.ts"))
}

fn refused(text: &str) -> Diagnostic {
    match load(text) {
        Ok(_) => panic!("source unexpectedly admitted:\n{text}"),
        Err(diagnostic) => diagnostic,
    }
}

#[test]
fn a_valid_c_prime_enters_semantic_identity_and_formatting_does_not() {
    let base = source(IMPORT, &[Unit::new("scope")], true, "");
    let first = load(&base).unwrap();
    let unit = &first.compiled.units["scope"];
    assert_eq!(unit.composition.as_ref().unwrap().frame, "CF2");

    // Formatting and comments: new byte revision, same semantic identity.
    let reformatted = load(&format!(
        "// commentary only\n{}",
        base.replace("  ", "   ")
    ))
    .unwrap();
    assert_ne!(
        reformatted
            .source
            .source
            .authoring
            .as_ref()
            .unwrap()
            .revision,
        first.source.source.authoring.as_ref().unwrap().revision
    );
    assert_eq!(reformatted.source.source.digest, first.source.source.digest);
    assert_eq!(
        reformatted.compiled.units["scope"].reference,
        unit.reference
    );

    // A changed C-prime is an execution-relevant change: new basis and unit.
    let mut changed = Unit::new("scope");
    changed.frame = "CF3";
    let changed = load(&source(IMPORT, &[changed], true, "")).unwrap();
    assert_ne!(changed.source.source.digest, first.source.source.digest);
    assert_ne!(changed.compiled.units["scope"].reference, unit.reference);

    // The same unit without C-prime is an ordinary generic unit again.
    let generic = load(&source("", &[Unit::new("scope")], false, "")).unwrap();
    assert!(generic.compiled.units["scope"].composition.is_none());
    assert!(generic
        .source
        .source
        .authoring
        .as_ref()
        .unwrap()
        .adapters
        .is_empty());
    assert_ne!(generic.source.source.digest, first.source.source.digest);
}

#[test]
fn an_invalid_frame_is_refused_at_its_unit_and_field() {
    let mut unit = Unit::new("scope");
    unit.frame = "CF9";
    let diagnostic = refused(&source(IMPORT, &[unit], true, ""));
    assert_eq!(diagnostic.code, "authoring.domain_lowering");
    assert_eq!(diagnostic.unit.as_deref(), Some("scope"));
    assert_eq!(diagnostic.field.as_deref(), Some("composition"));
    assert!(diagnostic.message.contains("InvalidCPrimeProfile"));
    let location = diagnostic.location.expect("source location");
    assert_eq!(location.file, "test.workflow.ts");
    assert!(location.line > 1);

    // U+2032 is not QL's wire spelling of CT4b'.
    let text = source(IMPORT, &[Unit::new("scope")], true, "")
        .replace("CT: \"CT1\"", "CT: \"CT4b\u{2032}\"");
    assert_eq!(refused(&text).code, "authoring.domain_lowering");
    assert!(load(&text.replace("CT4b\u{2032}", "CT4b'")).is_ok());
}

#[test]
fn authority_actor_and_unknown_fields_are_refused() {
    let undertaking = source(IMPORT, &[Unit::new("scope")], true, "")
        .replace("CPF: \"dialogical\"", "CPF: \"authorised-undertaking\"");
    let diagnostic = refused(&undertaking);
    assert_eq!(diagnostic.code, "authoring.domain_lowering");
    assert!(diagnostic.message.contains("undertakingAuthorityRef"));
    let mut authorised = Unit::new("scope");
    authorised.extra = r#" authority: "central:source:control:root:Control/user/placement.json","#;
    assert!(load(
        &source(IMPORT, &[authorised], true, "")
            .replace("CPF: \"dialogical\"", "CPF: \"authorised-undertaking\"")
    )
    .is_ok());

    let mut stranger = Unit::new("scope");
    stranger.actor = "agent/aletheia-janus";
    let diagnostic = refused(&source(IMPORT, &[stranger], true, ""));
    assert_eq!(diagnostic.code, "authoring.native_validation");
    assert_eq!(diagnostic.field.as_deref(), Some("composition"));
    assert!(diagnostic.message.contains("UndeclaredActor"));

    let mut decorated = Unit::new("scope");
    decorated.extra = r#" harmonicBasis: "fifths","#;
    let diagnostic = refused(&source(IMPORT, &[decorated], true, ""));
    assert_eq!(diagnostic.code, "authoring.domain_lowering");
    assert!(diagnostic.message.contains("harmonicBasis"));
}

#[test]
fn unregistered_domain_imports_and_undeclared_interpretations_are_refused() {
    let other = refused(&source(
        r#"import type { Composition } from "@epilogos/other-domain";"#,
        &[Unit::new("scope")],
        false,
        "",
    ));
    assert_eq!(other.code, "authoring.domain_adapter_unregistered");
    assert!(other.message.contains("@epilogos/other-domain"));

    let value = refused(&source(
        r#"import { CPrime } from "@epilogos/ql-vak";"#,
        &[Unit::new("scope")],
        false,
        "",
    ));
    assert_eq!(value.code, "authoring.domain_adapter");

    let unknown = refused(&source(
        r#"import type { CPrimeExecutionBinding } from "@epilogos/ql-vak";"#,
        &[Unit::new("scope")],
        false,
        "",
    ));
    assert_eq!(unknown.code, "authoring.domain_adapter");

    // A composition whose interpretation was never imported is not guessed.
    let undeclared =
        refused(&source("", &[Unit::new("scope")], true, "").replace(" satisfies CPrime", ""));
    assert_eq!(undeclared.code, "authoring.domain_adapter_required");
    assert_eq!(undeclared.field.as_deref(), Some("composition"));

    // Relative type-only modules remain outside this frontend.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("types.ts"), "export const x = 1;\n").unwrap();
    fs::write(
        dir.path().join("w.workflow.ts"),
        source(
            r#"import type { X } from "./types.ts";"#,
            &[Unit::new("scope")],
            false,
            "",
        ),
    )
    .unwrap();
    let relative = load_workflow(dir.path(), Path::new("w.workflow.ts")).unwrap_err();
    assert!(relative.message.contains("Domain type imports need"));
}

#[test]
fn thread_forms_must_match_the_compiled_topology() {
    // A chord needs an independent partner voice.
    let mut lone = Unit::new("left");
    lone.thread = "CFP1";
    let diagnostic = refused(&source(IMPORT, &[lone, Unit::new("other")], true, ""));
    assert_eq!(diagnostic.field.as_deref(), Some("composition"));
    assert!(diagnostic.message.contains("InvalidUnitShape"));

    let mut left = Unit::new("left");
    left.thread = "CFP1";
    let mut right = Unit::new("right");
    right.thread = "CFP1";
    right.actor = "agent/anima-eros";
    let diagnostic = refused(&source(IMPORT, &[left, right], true, ""));
    assert!(diagnostic.message.contains("NotIndependent"));
    let mut left = Unit::new("left");
    left.thread = "CFP1";
    left.independence = &["right"];
    let mut right = Unit::new("right");
    right.thread = "CFP1";
    assert!(load(&source(IMPORT, &[left, right], true, "")).is_ok());

    // A melody's successor consumes its predecessor's identified return.
    let mut first = Unit::new("first");
    first.thread = "CFP2";
    let mut second = Unit::new("second");
    second.thread = "CFP2";
    second.dependencies = &["first"];
    let diagnostic = refused(&source(IMPORT, &[first, second], true, ""));
    assert_eq!(diagnostic.unit.as_deref(), Some("second"));
    assert!(diagnostic.message.contains("MissingChainInput"));
    let mut first = Unit::new("first");
    first.thread = "CFP2";
    let mut second = Unit::new("second");
    second.thread = "CFP2";
    second.dependencies = &["first"];
    second.inputs = &["first"];
    assert!(load(&source(IMPORT, &[first, second], true, "")).is_ok());

    // A fusion's readings meet at a barrier.
    let fused = |independence: &'static [&'static str], key: &'static str| {
        let mut unit = Unit::new(key);
        unit.thread = "CFP3";
        unit.independence = independence;
        unit
    };
    let diagnostic = refused(&source(
        IMPORT,
        &[fused(&["b"], "a"), fused(&["a"], "b"), Unit::new("judge")],
        true,
        "",
    ));
    assert!(diagnostic.message.contains("MissingBarrier"));
    assert!(load(&source(
        IMPORT,
        &[fused(&["b"], "a"), fused(&["a"], "b"), Unit::new("judge")],
        true,
        r#"barrier({ key: "readings", waitsFor: ["a", "b"], releases: ["judge"] })"#,
    ))
    .is_ok());
}

#[test]
fn native_json_sources_are_validated_by_the_same_owner() {
    let authored = load(&source(IMPORT, &[Unit::new("scope")], true, "")).unwrap();
    let mut native = serde_json::to_value(&authored.source).unwrap();
    // Drop the authored basis: an ordinary native source may carry a lowered
    // binding directly, and the Vāk owner still validates it.
    native["source"]
        .as_object_mut()
        .unwrap()
        .remove("authoring");
    let mut source: epilogos_factory::workflow::WorkflowSource =
        serde_json::from_value(native).unwrap();
    source.source.digest = epilogos_factory::workflow::workflow_source_digest(&source).unwrap();
    assert!(compile_workflow(source.clone()).is_ok());
    source.units[0].composition.as_mut().unwrap().frame = "CF0".into();
    // An invalid C-prime cannot even canonicalize into a semantic digest.
    assert!(matches!(
        epilogos_factory::workflow::workflow_source_digest(&source),
        Err(WorkflowError::InvalidComposition { .. })
    ));
    source.source.digest = "0".repeat(64);
    assert!(matches!(
        compile_workflow(source),
        Err(WorkflowError::InvalidComposition { .. })
    ));
}

#[test]
fn commission_retains_the_lowered_composition_and_inspection_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state.json");
    let request = dir.path().join("request.json");
    fs::write(
        &request,
        serde_json::to_vec_pretty(&json!({
            "contract": "factory.commission-request/v1",
            "requestRef": "commission-request:expression-development-adapter-test",
            "projectKey": "expression-development-adapter-test",
            "purpose": "Admit the QL-authored expression-development source without executing it",
            "frontier": "Declared source; no execution has been requested",
            "runDestination": "expression-development/adapter-test",
            "writeOwner": "factory",
            "commissionedAt": "2026-09-24T00:00:00Z",
            "participantRequirements": [
                {"ref": "agent/anima", "description": "Anima conducts the constitutional voices",
                 "sourceOwner": "central", "sourceRef": "central:agent-set/anima", "sourceRevision": "specimen-r1"},
                {"ref": "agent-set/anima", "description": "Anima's constitutional voices",
                 "sourceOwner": "central", "sourceRef": "central:agent-set/anima", "sourceRevision": "specimen-r1"},
                {"ref": "agent-set/aletheia", "description": "Aletheia's disclosure and Return",
                 "sourceOwner": "central", "sourceRef": "central:agent-set/aletheia", "sourceRevision": "specimen-r1"}
            ],
            "rootAct": {"actRef": "act:expression-development-adapter-test", "agentRef": "agent/anima",
                "purpose": "Bound the Expression development source", "scopeRefs": ["project:O-I"],
                "standing": "commissioned-not-executed"}
        }))
        .unwrap(),
    )
    .unwrap();
    let path = fixtures().join(WORKFLOWS[0]);
    let receipt = cli::execute(&[
        "workflow".into(),
        "commission".into(),
        state.to_string_lossy().into_owned(),
        request.to_string_lossy().into_owned(),
        path.to_string_lossy().into_owned(),
    ])
    .unwrap();
    assert_eq!(receipt["execution"], "not-requested");
    let run = receipt["commission"]["commission"]["runRef"]
        .as_str()
        .unwrap()
        .to_owned();
    let reading = cli::execute(&[
        "workflow".into(),
        "inspect".into(),
        state.to_string_lossy().into_owned(),
        run.clone(),
        "--limit".into(),
        "100".into(),
    ])
    .unwrap();
    assert_eq!(reading["totalAttempts"], 0);
    assert!(reading["source"]["authoring"]["domainAdapters"][QL].is_object());
    let units = reading["units"].as_array().unwrap();
    assert_eq!(units.len(), 8);
    assert!(units
        .iter()
        .all(|unit| unit["composition"]["contract"] == QL_C_PRIME_PROFILE_CONTRACT));
    // The retained bytes replay through the same adapter lowering.
    let held = cli::execute(&[
        "workflow".into(),
        "source".into(),
        state.to_string_lossy().into_owned(),
        run,
        WORKFLOWS[0].into(),
    ])
    .unwrap();
    assert_eq!(
        held["module"]["content"],
        fs::read_to_string(&path).unwrap()
    );
}
