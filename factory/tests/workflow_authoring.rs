//! Controlled authoring/source-history/attempt harness. Every mutation runs
//! the native Factory attempt store in-process against a fresh World; the
//! ACP legs additionally run the real native AIKit/Actuation binaries and a
//! deterministic ACP process, never a live model. Owner receipts are
//! explicitly test-only.

#[path = "workflow_authoring/acp.rs"]
mod acp;

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_native_store::FileAttemptStore;
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::ReturnedArtifact;
use epilogos_factory::workflow::CompiledWorkflow;
use serde_json::{json, Value};
#[allow(unused_imports)]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
#[allow(unused_imports)]
use std::process::Child;
use std::process::{Command, Output, Stdio};

const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const SOURCE_DIGEST: &str = "5324b0a45168fa515804b58b7c3cc88546080ef31c21a8b91a2cecb992ba145e";

fn set<const N: usize>(keys: [&str; N]) -> BTreeSet<String> {
    keys.iter().map(|key| key.to_string()).collect()
}

/// One bounded unit: the exact controlled source read, nothing else.
fn definition() -> Value {
    unit_workflow("controlled-source-inspection", &[])
}

/// A fork of two independent source reads joined by an independent review
/// that must select the exact returned predecessor artifacts.
fn chain_definition() -> Value {
    unit_workflow(
        "controlled-plural-inspection",
        &[
            ("read-left", Vec::new(), vec!["read-right"]),
            ("read-right", Vec::new(), vec!["read-left"]),
            ("verify", vec!["read-left", "read-right"], Vec::new()),
        ],
    )
}

fn unit_workflow(workflow_key: &str, units: &[(&str, Vec<&str>, Vec<&str>)]) -> Value {
    let units: Vec<Value> = if units.is_empty() {
        vec![unit_source("inspect-source", &[], Vec::new())]
    } else {
        units
            .iter()
            .map(|(key, predecessors, independence)| {
                unit_source(key, predecessors, independence.clone())
            })
            .collect()
    };
    let barriers = if units.len() > 1 {
        json!([{
            "key": "both-readings",
            "waitsFor": ["read-left", "read-right"],
            "releases": ["verify"]
        }])
    } else {
        json!([])
    };
    json!({
        "schemaVersion": "factory.agent-workflow-source/v1",
        "coordinationContract": "factory.bounded-coordination/v1",
        "source": {
            "ref": "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FC1",
            "revision": "authoring-source-v1",
            "digest": SOURCE_DIGEST,
            "temporalRef": "flow-time:2026-09-20",
            "flowRef": "flow:factory-authoring"
        },
        "workflowKey": workflow_key,
        "units": units,
        "barriers": barriers
    })
}

fn unit_source(key: &str, predecessors: &[&str], independence: Vec<&str>) -> Value {
    let mut unit = json!({
        "key": key,
        "developmentalConcern": "Read the bounded controlled source exactly once",
        "requiredDifference": "Exact controlled source bytes are returned with attributable native tool evidence",
        "returnContract": "Return the exact source digest and the native evidence that produced it",
        "subjectRef": "source:controlled-acp-input",
        "basisRevision": "fixture-source-v1",
        "agentRequirements": {
            "agentRefs": [format!("agent:{key}")],
            "agentSetRefs": [],
            "agencyRefs": [format!("agency:{key}")]
        },
        "praxisRefs": [format!("skill/{key}")],
        "capabilityRefs": ["capability/source-read"],
        "dependencies": predecessors,
        "independenceFrom": independence,
        "permittedEffects": ["read exact controlled source"],
        "verificationObligations": ["exact source bytes and attributable native tool result"],
        "returnAddress": format!("return:controlled-{key}"),
        "stopConditions": "Stop when the controlled source is unavailable",
        "escalationConditions": "Escalate any attempted source mutation or fabricated completion"
    });
    if !predecessors.is_empty() {
        unit["inputs"] = json!(predecessors
            .iter()
            .map(|predecessor| json!({
                "predecessor": predecessor,
                "receivingContextRef": format!("context:controlled-{key}-from-{predecessor}")
            }))
            .collect::<Vec<_>>());
    }
    unit
}

/// The store result or command output is a reading only when it actually
/// succeeded; failures stop the test with their real evidence attached.
fn success(value: impl IntoSuccess) -> Value {
    value.into_success()
}

trait IntoSuccess {
    fn into_success(self) -> Value;
}

impl IntoSuccess for Output {
    fn into_success(self) -> Value {
        assert!(
            self.status.success(),
            "native command failed: {}",
            String::from_utf8_lossy(&self.stderr)
        );
        serde_json::from_slice(&self.stdout).expect("native command returned JSON")
    }
}

impl IntoSuccess for Value {
    fn into_success(self) -> Value {
        self
    }
}

struct World {
    dir: tempfile::TempDir,
    run: RunRef,
    journey: String,
    workflow: CompiledWorkflow,
}

impl World {
    fn from_definition(source: Value) -> Self {
        // macOS exposes its temp root through a symlink; native agency
        // admission rightly refuses source paths that redirect through one.
        let base = std::env::temp_dir().canonicalize().unwrap();
        let dir = tempfile::TempDir::new_in(&base).unwrap();
        for name in ["NOW", "work", "Control"] {
            fs::create_dir(dir.path().join(name)).unwrap();
        }
        let mut source: epilogos_factory::workflow::WorkflowSource =
            serde_json::from_value(source).unwrap();
        // The declared digest is the computed semantic digest of the same
        // content, exactly as the authoring CLI admits it.
        source.source.digest = "0".repeat(64);
        source.source.digest = epilogos_factory::workflow::workflow_source_digest(&source).unwrap();
        let workflow = epilogos_factory::workflow::compile_workflow(source.clone()).unwrap();
        let run = Run::new(
            RUN.parse().unwrap(),
            PROJECT.parse().unwrap(),
            "controlled workflow authoring proof",
            "factory-test",
        )
        .unwrap();
        FileAttemptStore::initialize(
            dir.path().join("native.json"),
            FactoryAttemptSeed {
                run,
                workflow_source: source,
            },
        )
        .unwrap();
        Self {
            dir,
            run: RUN.parse().unwrap(),
            journey: "journey:controlled-authoring".into(),
            workflow,
        }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().join("native.json")
    }

    fn store(&self) -> FileAttemptStore {
        FileAttemptStore::open(self.path()).unwrap()
    }

    fn reading(&self) -> FactoryAttemptReading {
        self.store().reading().unwrap()
    }

    fn action(&self, operation: FactoryAttemptOperation) -> Value {
        let request = self.request(operation);
        match self.store().apply(request) {
            Ok(receipt) => serde_json::to_value(&receipt).unwrap(),
            Err(error) => panic!("native attempt action failed: {error}"),
        }
    }

    fn request(&self, operation: FactoryAttemptOperation) -> FactoryAttemptActionRequest {
        let revision = self.reading().revision;
        FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: format!("projection:controlled-{revision}"),
            caller: FactoryActionCaller {
                caller_ref: "caller:controlled-test".into(),
                projection_kind: FactoryActionProjectionKind::Headless,
                lineage: vec!["caller:controlled-test".into()],
            },
            run_ref: self.run.clone(),
            expected_revision: revision,
            authority: ProjectedFactoryActionAuthority {
                authority_ref: "authority:controlled-test".into(),
                native_owner: "factory".into(),
                capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
            operation,
        }
    }

    fn inspect(&self, args: &[&str]) -> Value {
        let mut attempt = None;
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            if *flag == "--attempt" {
                attempt = Some(args.next().expect("--attempt needs a value").to_string());
            }
        }
        epilogos_factory::workflow_authoring::inspect::inspect(
            &self.path(),
            &self.run,
            None,
            attempt,
            25,
            None,
        )
        .unwrap()
    }

    fn disposition(&self, key: &str) -> SituatedExecutionDisposition {
        let unit = self.workflow.unit(key).unwrap();
        let agent = unit
            .agent_requirements
            .agent_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agent:controlled-test".into());
        let agency = unit
            .agent_requirements
            .agency_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agency:controlled-test".into());
        let demand = ExecutionDemand {
            project_ref: PROJECT.into(),
            run_ref: RUN.into(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some(agency.clone()),
            profile_ref: None,
            use_type: "controlled-authoring-test".into(),
            required_capabilities: unit.capability_refs.clone(),
            required_modalities: BTreeSet::new(),
            required_actions: BTreeSet::new(),
            required_tools: BTreeSet::new(),
            context_characteristics: BTreeSet::new(),
            independence_from: BTreeSet::new(),
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            requires_local_materialisation: false,
        };
        let selection = accept_aikit_selection(
            demand,
            AikitModelRosterSelection {
                roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
                model_ref: "model:controlled-test".into(),
                provider_ref: "provider:controlled-test".into(),
                ranking_policy: "controlled-test".into(),
                ranking_explanation: json!({"testOnly":true}),
                provenance: vec!["selection:controlled-test".into()],
            },
            "2026-09-10T20:00:00Z",
        )
        .unwrap();
        SituatedExecutionDisposition {
            selected_inputs: Vec::new(),
            selection,
            participant: SituatedParticipant {
                agent_ref: agent,
                agency_ref: agency,
                world_binding_ref: "world-binding:controlled-test".into(),
                profile_ref: None,
                source_ref: "source:controlled-acp-input".into(),
                source_revision: "fixture-source-v1".into(),
                source_digest: format!(
                    "blake3:{}",
                    blake3::hash(b"controlled authoring test only").to_hex()
                ),
            },
            context_refs: set(["context:controlled-test"]),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            body: ExecutionBody {
                model_ref: "model:controlled-test".into(),
                provider_ref: "provider:controlled-test".into(),
                route_ref: "route:controlled-test".into(),
                harness_ref: "harness:controlled-test".into(),
                harness_composition_ref: "composition:controlled-test".into(),
                agent_session_ref: format!("agent-session/{key}"),
                session_space_ref: format!("session-space/{key}"),
                material_world_ref: Some("material-world:controlled-test".into()),
                workcell_ref: Some("workcell:controlled-test".into()),
            },
            // Read-only controlled attempts request no Workcell room and no
            // write protection; plain encounter delivery remains enforcement.
            placement: None,
            permitted_effects: unit.permitted_effects.clone(),
            verification_obligations: unit.verification_obligations.clone(),
            return_address: unit.return_address.clone(),
            stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(),
            budget: ExecutionBudget {
                cost_ceiling_usd: None,
                latency_preference_ms: None,
                wall_clock_timeout_ms: Some(10000),
                retry_grant_ref: None,
                maximum_attempts: Some(1),
            },
        }
    }

    fn start_op(&self, attempt_ref: &str, key: &str) -> FactoryAttemptOperation {
        FactoryAttemptOperation::StartSerial {
            attempt_ref: attempt_ref.into(),
            task_ref: format!("task:{key}"),
            parent_journey_ref: self.journey.clone(),
            workflow_unit_ref: self.workflow.unit(key).unwrap().reference.clone(),
            disposition: self.disposition(key),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        }
    }

    fn fork_start(&self, attempt_ref: &str, key: &str) -> AttemptStart {
        AttemptStart {
            attempt_ref: attempt_ref.into(),
            task_ref: format!("task:{key}"),
            workflow_unit_ref: self.workflow.unit(key).unwrap().reference.clone(),
            disposition: self.disposition(key),
            retry_grant: None,
            tracking: vec![],
            place_grant: None,
        }
    }
}

// Re-exported for the child module's `use super::*` and kept local so the
// compiler proves every shape the ACP legs depend on.
#[allow(unused_imports)]
use epilogos_factory::workflow_inputs::SelectedWorkflowInput;
