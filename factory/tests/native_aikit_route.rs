//! Read-only acceptance against actual installed owners. A JSON driver names
//! their exact current Task; no simulated owner or provider can satisfy this.
use epilogos_factory::native_aikit_route::{output, AikitOwnerTransport};
use epilogos_factory::native_owner::{
    invoke_native_owner_bounded, NativeOwnerError, NativeOwnerInvocation,
    AIKIT_CAW_CONTRACT_REVISION,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeReadingCase {
    binary: PathBuf,
    cwd: PathBuf,
    transport: AikitOwnerTransport,
    agent_session: String,
    task_ref: String,
    now_ref: String,
    task_revision: String,
    agency_ref: String,
    source_digest: String,
}

#[test]
fn legacy_invocation_round_trip_does_not_insert_a_transport_or_change_request_bytes() {
    let original = json!({"operation":"aikit-encounter","binary":"/native/aikit","cwd":"/native/source",
        "contract_revision":"retained-v1-contract","request":{"action":"delivery",
            "agent_session":"retained-session","delivery_ref":"retained-delivery"}});
    let invocation: NativeOwnerInvocation = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(invocation).unwrap(), original);
}

#[test]
fn absent_owner_executable_preserves_public_native_io_error_and_client_identity() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("absent-aikit");
    for transport in [
        None,
        Some(AikitOwnerTransport::Local {
            workcell_ref: "workcell:test".into(),
            environment: Default::default(),
        }),
    ] {
        let invocation = NativeOwnerInvocation::AikitEncounter {
            binary: binary.clone(),
            cwd: directory.path().to_path_buf(),
            transport,
            contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
            request: json!({"action":"status", "agent_session":"unopened-session"}),
        };
        match invoke_native_owner_bounded(&invocation, 1000).unwrap_err() {
            NativeOwnerError::Spawn {
                owner,
                binary: actual,
                error,
            } => {
                assert_eq!(owner, "AIKit");
                assert_eq!(actual, binary);
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("Actual owner client I/O was reclassified: {other:?}"),
        }
    }
}

#[test]
fn invalid_declared_route_refuses_before_spawn_and_retains_configuration_error() {
    let invocation = NativeOwnerInvocation::AikitEncounter {
        binary: "relative-aikit".into(),
        cwd: "/".into(),
        transport: Some(AikitOwnerTransport::Local {
            workcell_ref: "workcell:test".into(),
            environment: Default::default(),
        }),
        contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
        request: json!({"action":"status", "agent_session":"unopened-session"}),
    };
    assert!(matches!(
        invoke_native_owner_bounded(&invocation, 1000),
        Err(NativeOwnerError::InvalidInvocation(_))
    ));
}

#[test]
#[ignore = "requires an explicit current installed AIKit/Workcell/Task case; native acceptance invokes this gate"]
fn actual_installed_owner_retains_same_task_agency_and_prepared_source_on_declared_route() {
    let driver = std::env::var_os("FACTORY_NATIVE_AIKIT_ROUTE_CASE")
        .expect("name the exact reviewed current native owner case");
    let case: NativeReadingCase = serde_json::from_slice(&std::fs::read(driver).unwrap()).unwrap();
    let read = output(
        &case.binary,
        &case.cwd,
        Some(&case.transport),
        &[
            "session-space".into(),
            "encounter-task-read".into(),
            "--agent-session".into(),
            case.agent_session.clone(),
        ],
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        read.status.success(),
        "native owner refused: {}",
        String::from_utf8_lossy(&read.stderr)
    );
    let task: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(task["schema"], "aikit.encounter-task/v1");
    assert_eq!(task["ready"], true);
    assert_eq!(task["revision"], case.task_revision);
    assert_eq!(task["request"]["central"]["task_ref"], case.task_ref);
    assert_eq!(task["request"]["cwd"], json!(case.cwd));
    assert_eq!(task["allocation"]["allocation"]["now_ref"], case.now_ref);
    assert_eq!(
        task["prepared_run"]["scope"]["schema"],
        "workcell.prepared-run-scope/v1"
    );
    assert_eq!(
        task["prepared_run"]["scope"]["agency"]["agency_ref"],
        case.agency_ref
    );
    assert_eq!(
        task["prepared_run"]["scope"]["agency"]["source_digest"],
        case.source_digest
    );
    assert_eq!(
        task["prepared_run"]["scope"]["worktree_path"],
        json!(case.cwd)
    );
    assert_eq!(
        task["prepared_run"]["scope"]["prepared_write_boundary"]["requirements"],
        task["requirements"]
    );
    // A fresh owner process repeats the read without replacing native Task or
    // allocation, touching a model, or treating retained preparation as Return.
    let again = output(
        &case.binary,
        &case.cwd,
        Some(&case.transport),
        &[
            "session-space".into(),
            "encounter-task-read".into(),
            "--agent-session".into(),
            case.agent_session,
        ],
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(again.status.success());
    assert_eq!(again.stdout, read.stdout);
}
