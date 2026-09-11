//! Concrete, revision-pinned native owner adapters. These execute real owner
//! binaries; a receipt is not verification, task Return or human Recognition.
//! The pinned PR contracts remain dependencies, not accepted-main claims.

use crate::attempt_runtime::{OwnerOperationPhase, OwnerOperationReceipt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

pub const AIKIT_CAW_CONTRACT_REVISION: &str = "3d23d1eefbb999b0a5058ed0b4ca98dd2575b632";
pub const WORKCELL_CAW_CONTRACT_REVISION: &str = "f3a5be9fc751ee94b78aff11411e0cde65a46e4c";
pub const AIKIT_DELIVERY_CONTRACT: &str = "aikit.encounter-delivery/v1";
pub const WORKCELL_WRITE_BOUNDARY_CONTRACT: &str = "workcell.write-boundary-result/v1";
pub const WORKCELL_CONTROL_CONTRACT: &str = "workcell.control/v1";
pub const DEFAULT_OWNER_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum NativeOwnerInvocation {
    AikitEncounter {
        binary: PathBuf,
        cwd: PathBuf,
        contract_revision: String,
        request: Value,
    },
    WorkcellWriteBoundaryInspect {
        binary: PathBuf,
        requirements: PathBuf,
        current_policy_revision: String,
        contract_revision: String,
    },
    WorkcellWriteBoundaryRun {
        binary: PathBuf,
        requirements: PathBuf,
        current_policy_revision: String,
        timeout_ms: u64,
        program: PathBuf,
        #[serde(default)]
        args: Vec<String>,
        contract_revision: String,
    },
    WorkcellWorld {
        binary: PathBuf,
        receipt: PathBuf,
        world_operation: WorkcellWorldOperation,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authorization: Option<String>,
        contract_revision: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkcellWorldOperation {
    Inspect,
    Observe,
    Recover,
    Expose,
    Collect,
    Release,
}

impl WorkcellWorldOperation {
    fn command(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Observe => "observe",
            Self::Recover => "recover",
            Self::Expose => "expose",
            Self::Collect => "collect",
            Self::Release => "release",
        }
    }

    fn success_phase(self) -> OwnerOperationPhase {
        match self {
            Self::Recover => OwnerOperationPhase::Recovered,
            Self::Release => OwnerOperationPhase::Released,
            _ => OwnerOperationPhase::Observed,
        }
    }
}

pub fn invoke_native_owner(
    invocation: &NativeOwnerInvocation,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    let timeout_ms = match invocation {
        NativeOwnerInvocation::WorkcellWriteBoundaryRun { timeout_ms, .. } => {
            timeout_ms.saturating_add(DEFAULT_OWNER_TIMEOUT_MS)
        }
        _ => DEFAULT_OWNER_TIMEOUT_MS,
    };
    invoke_native_owner_bounded(invocation, timeout_ms)
}

/// This deadline bounds client transport, not the lifetime of a remote worker.
/// Timeout/invalid output must therefore be retained as an uncertain outcome.
pub fn invoke_native_owner_bounded(
    invocation: &NativeOwnerInvocation,
    timeout_ms: u64,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    if timeout_ms == 0 {
        return Err(NativeOwnerError::InvalidInvocation(
            "native owner transport requires a positive timeout".into(),
        ));
    }
    let timeout = Duration::from_millis(timeout_ms);
    match invocation {
        NativeOwnerInvocation::AikitEncounter {
            binary,
            cwd,
            contract_revision,
            request,
        } => invoke_aikit(binary, cwd, contract_revision, request, timeout),
        NativeOwnerInvocation::WorkcellWriteBoundaryInspect {
            binary,
            requirements,
            current_policy_revision,
            contract_revision,
        } => invoke_workcell_boundary(
            binary,
            requirements,
            current_policy_revision,
            contract_revision,
            None,
            timeout,
        ),
        NativeOwnerInvocation::WorkcellWriteBoundaryRun {
            binary,
            requirements,
            current_policy_revision,
            timeout_ms,
            program,
            args,
            contract_revision,
        } => invoke_workcell_boundary(
            binary,
            requirements,
            current_policy_revision,
            contract_revision,
            Some((*timeout_ms, program.as_path(), args.as_slice())),
            timeout,
        ),
        NativeOwnerInvocation::WorkcellWorld {
            binary,
            receipt,
            world_operation,
            endpoint,
            authorization,
            contract_revision,
        } => invoke_workcell_world(
            binary,
            receipt,
            *world_operation,
            (endpoint.as_deref(), authorization.as_deref()),
            contract_revision,
            timeout,
        ),
    }
}

fn invoke_aikit(
    binary: &Path,
    cwd: &Path,
    contract_revision: &str,
    request: &Value,
    timeout: Duration,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    if contract_revision != crate::attempt_owner_dispatch::AIKIT_TASK_CONTRACT_REVISION {
        require_revision("AIKit", contract_revision, AIKIT_CAW_CONTRACT_REVISION)?;
    }
    if !cwd.is_absolute() {
        return Err(NativeOwnerError::InvalidInvocation(
            "AIKit encounter cwd must be an absolute Project path".into(),
        ));
    }
    let action = request.get("action").and_then(Value::as_str).unwrap_or("");
    if !matches!(action, "send" | "delivery" | "cancel" | "status" | "read") {
        return Err(NativeOwnerError::InvalidInvocation(format!(
            "AIKit encounter action `{action}` is outside Factory's CAW adapter"
        )));
    }
    let session = request
        .get("agent_session")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| NativeOwnerError::InvalidInvocation("missing agent_session".into()))?;
    let delivery_ref = match action {
        "send" => request
            .pointer("/turn/delivery_ref")
            .and_then(Value::as_str),
        "delivery" => request.get("delivery_ref").and_then(Value::as_str),
        _ => None,
    };
    if matches!(action, "send" | "delivery")
        && delivery_ref.is_none_or(|value| value.trim().is_empty())
    {
        return Err(NativeOwnerError::InvalidInvocation(
            "AIKit send/delivery requires a stable delivery_ref".into(),
        ));
    }
    let mut command = Command::new(binary);
    command
        .arg("-C")
        .arg(cwd)
        .arg("encounter")
        .arg("--request-json")
        .arg(serde_json::to_string(request)?);
    let output = owner_output("AIKit", binary, &mut command, timeout)?;
    let payload = parse_json_output("AIKit", &output)?;
    if payload.get("ok") == Some(&Value::Bool(false)) {
        return Err(command_failure("AIKit", output, Some(payload)));
    }
    let delivery = locate_delivery(&payload);
    if let Some(expected) = delivery_ref {
        if delivery
            .and_then(|value| value.get("delivery_ref"))
            .and_then(Value::as_str)
            != Some(expected)
            || delivery
                .and_then(|value| value.get("agent_session"))
                .and_then(Value::as_str)
                != Some(session)
        {
            return Err(NativeOwnerError::InvalidResponse(
                "AIKit response omitted or changed the addressed delivery/session identity".into(),
            ));
        }
    }
    if let Some(expected) = request.pointer("/turn/expected_task") {
        if contract_revision != crate::attempt_owner_dispatch::AIKIT_TASK_CONTRACT_REVISION
            || delivery.and_then(|v| v.pointer("/request/submission/turn/expected_task"))
                != Some(expected)
        {
            return Err(NativeOwnerError::InvalidResponse(
                "Native delivery did not retain the exact locked task admission".into(),
            ));
        }
    }
    let phase = match delivery
        .and_then(|value| value.get("phase"))
        .and_then(Value::as_str)
    {
        Some(phase) => parse_delivery_phase(phase)?,
        None if delivery_ref.is_some() => {
            return Err(NativeOwnerError::InvalidResponse(
                "AIKit delivery omitted phase".into(),
            ));
        }
        None => OwnerOperationPhase::Observed,
    };
    if !output.status.success()
        && !matches!(
            phase,
            OwnerOperationPhase::Failed
                | OwnerOperationPhase::Cancelled
                | OwnerOperationPhase::Uncertain
        )
    {
        return Err(command_failure("AIKit", output, Some(payload)));
    }
    let mut evidence_refs = BTreeSet::new();
    if let Some(cursor) = delivery
        .and_then(|value| value.get("terminal_cursor"))
        .and_then(Value::as_u64)
    {
        evidence_refs.insert(format!("aikit-encounter-cursor:{session}:{cursor}"));
    }
    Ok(OwnerOperationReceipt {
        owner_ref: "aikit/session-space".into(),
        contract: AIKIT_DELIVERY_CONTRACT.into(),
        operation_ref: aikit_operation_identity(action, session, delivery_ref),
        receipt_ref: stable_receipt_ref("aikit-delivery-observation", &payload),
        source_revision: contract_revision.into(),
        phase,
        evidence_refs,
        partial_effect_refs: BTreeSet::new(),
        payload,
    })
}

fn invoke_workcell_boundary(
    binary: &Path,
    requirements: &Path,
    current_policy_revision: &str,
    contract_revision: &str,
    run: Option<(u64, &Path, &[String])>,
    timeout: Duration,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    require_revision(
        "Workcell",
        contract_revision,
        WORKCELL_CAW_CONTRACT_REVISION,
    )?;
    if current_policy_revision.trim().is_empty() || !requirements.is_file() {
        return Err(NativeOwnerError::InvalidInvocation(
            "Workcell requires an available requirements file and current policy revision".into(),
        ));
    }
    let mut command = Command::new(binary);
    let operation = if let Some((timeout_ms, program, args)) = run {
        if timeout_ms == 0 {
            return Err(NativeOwnerError::InvalidInvocation(
                "Workcell timeout must be positive".into(),
            ));
        }
        command
            .arg("run")
            .arg(requirements)
            .arg(current_policy_revision)
            .arg(timeout_ms.to_string())
            .arg("--")
            .arg(program)
            .args(args);
        "run"
    } else {
        command
            .arg("inspect")
            .arg(requirements)
            .arg(current_policy_revision);
        "inspect"
    };
    let output = owner_output("Workcell", binary, &mut command, timeout)?;
    let payload = parse_json_output("Workcell write-boundary", &output)?;
    let phase = if output.status.success() {
        if operation == "run" {
            OwnerOperationPhase::Returned
        } else {
            OwnerOperationPhase::Observed
        }
    } else if uncertain_payload(&payload) {
        OwnerOperationPhase::Uncertain
    } else {
        OwnerOperationPhase::Failed
    };
    let mut evidence_refs = BTreeSet::new();
    if let Some(digest) = payload.get("requirements_digest").and_then(Value::as_str) {
        evidence_refs.insert(format!("workcell-requirements:{digest}"));
    }
    if let Some(policy) = payload.get("policy_revision").and_then(Value::as_str) {
        evidence_refs.insert(format!("workcell-policy-revision:{policy}"));
    }
    let mut partial_effect_refs = BTreeSet::new();
    if operation == "run"
        && payload.get("executed").and_then(Value::as_bool) == Some(true)
        && !output.status.success()
    {
        partial_effect_refs.insert(stable_receipt_ref("workcell-effect", &payload));
    }
    Ok(OwnerOperationReceipt {
        owner_ref: "workcell".into(),
        contract: WORKCELL_WRITE_BOUNDARY_CONTRACT.into(),
        operation_ref: format!(
            "workcell-write-boundary:{operation}:{}",
            requirements.display()
        ),
        receipt_ref: stable_receipt_ref("workcell-write-boundary", &payload),
        source_revision: contract_revision.into(),
        phase,
        evidence_refs,
        partial_effect_refs,
        payload,
    })
}

fn invoke_workcell_world(
    binary: &Path,
    receipt: &Path,
    operation: WorkcellWorldOperation,
    remote: (Option<&str>, Option<&str>),
    contract_revision: &str,
    timeout: Duration,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    require_revision(
        "Workcell",
        contract_revision,
        WORKCELL_CAW_CONTRACT_REVISION,
    )?;
    if !receipt.is_file() {
        return Err(NativeOwnerError::InvalidInvocation(format!(
            "Workcell material-world receipt is unavailable at {}",
            receipt.display()
        )));
    }
    let mut command = Command::new(binary);
    if let Some(endpoint) = remote.0 {
        if endpoint.trim().is_empty() {
            return Err(NativeOwnerError::InvalidInvocation(
                "empty Workcell endpoint".into(),
            ));
        }
        command.arg("--endpoint").arg(endpoint);
    }
    configure_workcell_authorization(&mut command, remote.1);
    command
        .arg("--receipt")
        .arg(receipt)
        .arg("--json")
        .arg(operation.command());
    let output = owner_output("Workcell", binary, &mut command, timeout)?;
    let payload = parse_json_output("Workcell", &output)?;
    let phase = if output.status.success() {
        operation.success_phase()
    } else if uncertain_payload(&payload) {
        OwnerOperationPhase::Uncertain
    } else {
        OwnerOperationPhase::Failed
    };
    Ok(OwnerOperationReceipt {
        owner_ref: "workcell".into(),
        contract: WORKCELL_CONTROL_CONTRACT.into(),
        operation_ref: format!(
            "workcell-world:{}:{}",
            operation.command(),
            receipt.display()
        ),
        receipt_ref: stable_receipt_ref("workcell-world", &payload),
        source_revision: contract_revision.into(),
        phase,
        evidence_refs: extract_reference_evidence(&payload),
        partial_effect_refs: BTreeSet::new(),
        payload,
    })
}

fn owner_output(
    owner: &'static str,
    binary: &Path,
    command: &mut Command,
    timeout: Duration,
) -> Result<Output, NativeOwnerError> {
    crate::native_process::output(command, timeout).map_err(|error| NativeOwnerError::Spawn {
        owner,
        binary: binary.to_path_buf(),
        error,
    })
}

fn parse_json_output(owner: &str, output: &Output) -> Result<Value, NativeOwnerError> {
    serde_json::from_slice(&output.stdout).map_err(|error| {
        NativeOwnerError::InvalidResponse(format!(
            "{owner} returned non-JSON stdout ({error}); stderr: {}",
            bounded_text(&output.stderr)
        ))
    })
}

pub(crate) fn locate_delivery(payload: &Value) -> Option<&Value> {
    // The native IPC envelope puts send's receipt in data.delivery and a
    // delivery read directly in data. Do not search unrelated nested JSON.
    if let Some(data) = payload.get("data") {
        if payload.get("ok") != Some(&Value::Bool(true)) {
            return None;
        }
        return data
            .get("delivery")
            .or_else(|| data.get("delivery_ref").map(|_| data));
    }
    // Retain the existing explicitly supported owner adapter envelopes.
    payload
        .get("delivery")
        .or_else(|| payload.pointer("/result/delivery"))
        .or_else(|| payload.pointer("/value/delivery"))
}

fn parse_delivery_phase(phase: &str) -> Result<OwnerOperationPhase, NativeOwnerError> {
    match phase {
        "dispatching" => Ok(OwnerOperationPhase::Dispatching),
        "submitted" => Ok(OwnerOperationPhase::Submitted),
        "returned" => Ok(OwnerOperationPhase::Returned),
        "failed" => Ok(OwnerOperationPhase::Failed),
        "cancelled" => Ok(OwnerOperationPhase::Cancelled),
        "uncertain" => Ok(OwnerOperationPhase::Uncertain),
        "reconciled-no-replay" => Ok(OwnerOperationPhase::ReconciledNoReplay),
        other => Err(NativeOwnerError::InvalidResponse(format!(
            "unsupported delivery phase `{other}`"
        ))),
    }
}

fn uncertain_payload(payload: &Value) -> bool {
    payload
        .get("effect_state")
        .and_then(Value::as_str)
        .is_some_and(|state| {
            let state = state.to_ascii_lowercase();
            state.contains("unknown") || state.contains("uncertain") || state.contains("unverified")
        })
        || payload.get("status").and_then(Value::as_str) == Some("uncertain")
}

fn extract_reference_evidence(payload: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    for key in ["world_ref", "receipt_ref", "observation_ref", "usage_ref"] {
        collect_named_ref(payload, key, &mut refs);
    }
    refs
}

fn collect_named_ref(value: &Value, key: &str, refs: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(reference) = map.get(key).and_then(Value::as_str) {
                refs.insert(reference.into());
            }
            for nested in map.values() {
                collect_named_ref(nested, key, refs);
            }
        }
        Value::Array(values) => {
            for nested in values {
                collect_named_ref(nested, key, refs);
            }
        }
        _ => {}
    }
}

fn stable_receipt_ref(prefix: &str, payload: &Value) -> String {
    let encoded = serde_json::to_vec(payload).expect("JSON values serialize");
    format!("{prefix}:blake3:{}", blake3::hash(&encoded).to_hex())
}

fn require_revision(
    owner: &'static str,
    actual: &str,
    expected: &'static str,
) -> Result<(), NativeOwnerError> {
    if actual == expected {
        Ok(())
    } else {
        Err(NativeOwnerError::ContractRevisionMismatch {
            owner,
            expected,
            actual: actual.into(),
        })
    }
}

fn bounded_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned()
}

fn command_failure(
    owner: &'static str,
    output: Output,
    payload: Option<Value>,
) -> NativeOwnerError {
    NativeOwnerError::CommandFailed {
        owner,
        code: output.status.code(),
        stdout: bounded_text(&output.stdout),
        stderr: bounded_text(&output.stderr),
        payload,
    }
}

pub fn execute_native_owner_cli(
    args: &[String],
    stdin_override: Option<&str>,
) -> Result<String, NativeOwnerError> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(owner_help()),
        Some(path) => {
            let body = read_input(path, stdin_override)?;
            let invocation: NativeOwnerInvocation = serde_json::from_str(&body)?;
            Ok(serde_json::to_string_pretty(&invoke_native_owner(
                &invocation,
            )?)?)
        }
    }
}

pub fn native_owner_cli_main(args: &[String]) -> std::process::ExitCode {
    match execute_native_owner_cli(args, None) {
        Ok(output) => {
            println!("{output}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("factory owner: {error}");
            std::process::ExitCode::from(2)
        }
    }
}

fn owner_help() -> String {
    format!("Factory native owner adapters\n\nUsage:\n  factory owner <invocation-json|->\n\nAIKit contract revision: {AIKIT_CAW_CONTRACT_REVISION}\nWorkcell contract revision: {WORKCELL_CAW_CONTRACT_REVISION}\n\nBounded transport receipts are owner evidence, not worker quiescence, verification, Return or Recognition.")
}

fn read_input(path: &str, stdin_override: Option<&str>) -> Result<String, NativeOwnerError> {
    if path != "-" {
        return Ok(fs::read_to_string(path)?);
    }
    if let Some(input) = stdin_override {
        return Ok(input.into());
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

#[derive(Debug)]
pub enum NativeOwnerError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidInvocation(String),
    InvalidResponse(String),
    ContractRevisionMismatch {
        owner: &'static str,
        expected: &'static str,
        actual: String,
    },
    Spawn {
        owner: &'static str,
        binary: PathBuf,
        error: io::Error,
    },
    CommandFailed {
        owner: &'static str,
        code: Option<i32>,
        stdout: String,
        stderr: String,
        payload: Option<Value>,
    },
}

impl Display for NativeOwnerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O failure: {error}"),
            Self::Json(error) => write!(formatter, "JSON failure: {error}"),
            Self::InvalidInvocation(message) => write!(formatter, "invalid invocation: {message}"),
            Self::InvalidResponse(message) => write!(formatter, "invalid owner response: {message}"),
            Self::ContractRevisionMismatch { owner, expected, actual } => write!(formatter, "{owner} CAW contract revision changed: expected {expected}, got {actual}"),
            Self::Spawn { owner, binary, error } => write!(formatter, "{owner} owner client {}: {error}", binary.display()),
            Self::CommandFailed { owner, code, stdout, stderr, .. } => write!(formatter, "{owner} owner command failed with code {code:?}; stdout={stdout:?}; stderr={stderr:?}"),
        }
    }
}
impl Error for NativeOwnerError {}
impl From<io::Error> for NativeOwnerError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for NativeOwnerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn aikit_operation_identity(action: &str, session: &str, delivery: Option<&str>) -> String {
    match delivery {
        Some(delivery) => format!("aikit-encounter-delivery:{session}:{delivery}"),
        None => format!("aikit-encounter:{action}:{session}"),
    }
}

fn configure_workcell_authorization(command: &mut Command, authorization: Option<&str>) {
    if let Some(authorization) = authorization {
        command.env("WORKCELL_CONTROL_TOKEN", authorization);
    }
}

#[cfg(test)]
mod native_adapter_regressions {
    use super::*;
    use serde_json::json;

    #[test]
    fn delivery_observations_keep_operation_identity_without_collapsing_receipts() {
        let sent =
            aikit_operation_identity("send", "session:controlled", Some("delivery:controlled"));
        assert_eq!(
            sent,
            aikit_operation_identity(
                "delivery",
                "session:controlled",
                Some("delivery:controlled")
            )
        );
        assert_ne!(
            sent,
            aikit_operation_identity("delivery", "session:other", Some("delivery:controlled"))
        );
        let submitted =
            json!({"delivery":{"delivery_ref":"delivery:controlled","phase":"submitted"}});
        let returned =
            json!({"delivery":{"delivery_ref":"delivery:controlled","phase":"returned"}});
        assert_ne!(
            stable_receipt_ref("aikit-delivery-observation", &submitted),
            stable_receipt_ref("aikit-delivery-observation", &returned)
        );
        assert_eq!(
            parse_delivery_phase("submitted").unwrap(),
            OwnerOperationPhase::Submitted
        );
        assert!(parse_delivery_phase("made-up-success").is_err());
    }

    #[test]
    fn workcell_authorization_uses_environment_not_arguments() {
        let mut command = Command::new("not-executed-test-binary");
        configure_workcell_authorization(&mut command, Some("test-only-not-a-credential"));
        assert_eq!(command.get_args().count(), 0);
        let environment = command.get_envs().collect::<Vec<_>>();
        assert_eq!(environment.len(), 1);
        assert_eq!(environment[0].0, "WORKCELL_CONTROL_TOKEN");
        assert_eq!(environment[0].1.unwrap(), "test-only-not-a-credential");
        let mut inherited = Command::new("not-executed-test-binary");
        configure_workcell_authorization(&mut inherited, None);
        assert_eq!(inherited.get_envs().count(), 0);
    }

    #[test]
    fn unaccepted_owner_revision_is_not_silently_upgraded() {
        let invocation = NativeOwnerInvocation::AikitEncounter {
            binary: PathBuf::from("must-not-run"),
            cwd: PathBuf::from("/controlled-test"),
            contract_revision: "unverified-new-head".into(),
            request: json!({"action":"delivery","agent_session":"session:controlled","delivery_ref":"delivery:controlled"}),
        };
        assert!(matches!(
            invoke_native_owner(&invocation),
            Err(NativeOwnerError::ContractRevisionMismatch { .. })
        ));
    }

    #[test]
    fn native_send_and_read_envelopes_are_not_replaced_by_fixture_shapes() {
        let delivery = json!({"delivery_ref":"delivery/native", "agent_session":"agent-session/native", "phase":"returned"});
        let sent = json!({"ok":true,"data":{"delivery":delivery}});
        let read = json!({"ok":true,"data":delivery});
        assert_eq!(locate_delivery(&sent), Some(&delivery));
        assert_eq!(locate_delivery(&read), Some(&delivery));
        assert!(locate_delivery(&json!({"ok":false,"data":delivery})).is_none());
        assert!(locate_delivery(&json!({"ok":true,"data":{"unrelated":delivery}})).is_none());
    }
}
