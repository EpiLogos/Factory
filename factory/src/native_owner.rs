//! Concrete adapters from Factory orchestration to the native owner operations
//! published for CAW execution.
//!
//! These adapters deliberately execute owner binaries rather than simulating
//! worker success. Their output is returned as [`OwnerOperationReceipt`] evidence
//! for the durable Factory attempt application; it is never promoted directly to
//! verification, task completion, human inclusion, or Recognition.
//!
//! Contract bases used by this tranche are explicit and intentionally pinned to
//! the unmerged owner PR revisions. Updating either owner contract therefore
//! requires an explicit Factory change rather than silently accepting drift.

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

pub const AIKIT_CAW_CONTRACT_REVISION: &str = "3d23d1eefbb999b0a5058ed0b4ca98dd2575b632";
pub const WORKCELL_CAW_CONTRACT_REVISION: &str = "f3a5be9fc751ee94b78aff11411e0cde65a46e4c";
pub const AIKIT_DELIVERY_CONTRACT: &str = "aikit.encounter-delivery/v1";
pub const WORKCELL_WRITE_BOUNDARY_CONTRACT: &str = "workcell.write-boundary-result/v1";
pub const WORKCELL_CONTROL_CONTRACT: &str = "workcell.control/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum NativeOwnerInvocation {
    /// Invoke AIKit's addressed SessionSpace encounter operation. The request is
    /// the exact `EncounterRequest` JSON accepted by `aikit-session-space`.
    AikitEncounter {
        binary: PathBuf,
        cwd: PathBuf,
        contract_revision: String,
        request: Value,
    },
    /// Inspect the currently realisable Workcell material write boundary without
    /// performing the worker effect.
    WorkcellWriteBoundaryInspect {
        binary: PathBuf,
        requirements: PathBuf,
        current_policy_revision: String,
        contract_revision: String,
    },
    /// Execute one worker command under Workcell's native write boundary.
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
    /// Inspect/observe/recover/expose/collect/release a materialised execution
    /// World through Workcell's published CLI. Remote authorization is supplied
    /// through the process environment rather than copied into argv or receipts.
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
    match invocation {
        NativeOwnerInvocation::AikitEncounter {
            binary,
            cwd,
            contract_revision,
            request,
        } => invoke_aikit(binary, cwd, contract_revision, request),
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
            endpoint.as_deref(),
            authorization.as_deref(),
            contract_revision,
        ),
    }
}

fn invoke_aikit(
    binary: &Path,
    cwd: &Path,
    contract_revision: &str,
    request: &Value,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    require_revision("AIKit", contract_revision, AIKIT_CAW_CONTRACT_REVISION)?;
    if !cwd.is_absolute() {
        return Err(NativeOwnerError::InvalidInvocation(
            "AIKit encounter cwd must be an absolute Project path".into(),
        ));
    }
    let action = request
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeOwnerError::InvalidInvocation(
                "AIKit EncounterRequest requires an explicit action".into(),
            )
        })?;
    if !matches!(action, "send" | "delivery" | "cancel" | "status" | "read") {
        return Err(NativeOwnerError::InvalidInvocation(format!(
            "AIKit encounter action `{action}` is outside Factory's CAW adapter"
        )));
    }
    let session = request
        .get("agent_session")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            NativeOwnerError::InvalidInvocation(
                "AIKit encounter action requires agent_session".into(),
            )
        })?;
    let delivery_ref = match action {
        "send" => request
            .pointer("/turn/delivery_ref")
            .and_then(Value::as_str),
        "delivery" => request.get("delivery_ref").and_then(Value::as_str),
        _ => None,
    };
    if matches!(action, "send" | "delivery") && delivery_ref.is_none() {
        return Err(NativeOwnerError::InvalidInvocation(
            "AIKit send/delivery operation requires a stable delivery_ref".into(),
        ));
    }

    let encoded = serde_json::to_string(request)?;
    let output = Command::new(binary)
        .arg("-C")
        .arg(cwd)
        .arg("encounter")
        .arg("--request-json")
        .arg(encoded)
        .output()
        .map_err(|error| NativeOwnerError::Spawn {
            owner: "AIKit",
            binary: binary.to_path_buf(),
            error,
        })?;
    let payload = parse_json_output("AIKit", &output)?;
    if !output.status.success() {
        return Err(command_failure("AIKit", output, Some(payload)));
    }

    let delivery = locate_delivery(&payload);
    if let Some(expected) = delivery_ref {
        let observed = delivery
            .and_then(|value| value.get("delivery_ref"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeOwnerError::InvalidResponse(
                    "AIKit send/delivery response omitted its durable delivery_ref".into(),
                )
            })?;
        if observed != expected {
            return Err(NativeOwnerError::InvalidResponse(format!(
                "AIKit returned delivery `{observed}` for addressed delivery `{expected}`"
            )));
        }
    }
    if let Some(observed) = delivery
        .and_then(|value| value.get("agent_session"))
        .and_then(Value::as_str)
    {
        if observed != session {
            return Err(NativeOwnerError::InvalidResponse(format!(
                "AIKit returned session `{observed}` for addressed session `{session}`"
            )));
        }
    }

    let phase = delivery
        .and_then(|value| value.get("phase"))
        .and_then(Value::as_str)
        .map(parse_delivery_phase)
        .transpose()?
        .unwrap_or(OwnerOperationPhase::Observed);
    let mut evidence_refs = BTreeSet::new();
    if let Some(cursor) = delivery
        .and_then(|value| value.get("terminal_cursor"))
        .and_then(Value::as_u64)
    {
        evidence_refs.insert(format!("aikit-encounter-cursor:{session}:{cursor}"));
    }
    let operation_ref = delivery_ref
        .map(|delivery| format!("aikit-encounter:{action}:{session}:{delivery}"))
        .unwrap_or_else(|| format!("aikit-encounter:{action}:{session}"));
    let receipt_ref = delivery_ref
        .map(|delivery| format!("aikit-delivery:{delivery}"))
        .unwrap_or_else(|| stable_receipt_ref("aikit", &payload));
    Ok(OwnerOperationReceipt {
        owner_ref: "aikit/session-space".into(),
        contract: AIKIT_DELIVERY_CONTRACT.into(),
        operation_ref,
        receipt_ref,
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
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    require_revision(
        "Workcell",
        contract_revision,
        WORKCELL_CAW_CONTRACT_REVISION,
    )?;
    if current_policy_revision.trim().is_empty() {
        return Err(NativeOwnerError::InvalidInvocation(
            "Workcell write-boundary operation requires the current policy revision".into(),
        ));
    }
    if !requirements.is_file() {
        return Err(NativeOwnerError::InvalidInvocation(format!(
            "Workcell write-boundary requirements are unavailable at {}",
            requirements.display()
        )));
    }

    let mut command = Command::new(binary);
    let operation = if let Some((timeout_ms, program, args)) = run {
        if timeout_ms == 0 {
            return Err(NativeOwnerError::InvalidInvocation(
                "Workcell write-boundary timeout must be greater than zero".into(),
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
    let output = command.output().map_err(|error| NativeOwnerError::Spawn {
        owner: "Workcell",
        binary: binary.to_path_buf(),
        error,
    })?;
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
    endpoint: Option<&str>,
    authorization: Option<&str>,
    contract_revision: &str,
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
    if let Some(endpoint) = endpoint {
        if endpoint.trim().is_empty() {
            return Err(NativeOwnerError::InvalidInvocation(
                "Workcell remote endpoint cannot be empty".into(),
            ));
        }
        command.arg("--endpoint").arg(endpoint);
    }
    if let Some(authorization) = authorization {
        command.arg("--authorization").arg(authorization);
    }
    command
        .arg("--receipt")
        .arg(receipt)
        .arg("--json")
        .arg(operation.command());
    let output = command.output().map_err(|error| NativeOwnerError::Spawn {
        owner: "Workcell",
        binary: binary.to_path_buf(),
        error,
    })?;
    let payload = parse_json_output("Workcell", &output)?;
    let phase = if output.status.success() {
        operation.success_phase()
    } else if uncertain_payload(&payload) {
        OwnerOperationPhase::Uncertain
    } else {
        OwnerOperationPhase::Failed
    };
    let evidence_refs = extract_reference_evidence(&payload);
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
        evidence_refs,
        partial_effect_refs: BTreeSet::new(),
        payload,
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

fn locate_delivery(payload: &Value) -> Option<&Value> {
    if payload.get("delivery").is_some() {
        return payload.get("delivery");
    }
    payload
        .get("result")
        .and_then(|result| result.get("delivery"))
        .or_else(|| payload.get("value").and_then(|value| value.get("delivery")))
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
            "AIKit returned unsupported delivery phase `{other}`"
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
        || payload
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status == "uncertain")
}

fn extract_reference_evidence(payload: &Value) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    collect_named_ref(payload, "world_ref", &mut refs);
    collect_named_ref(payload, "receipt_ref", &mut refs);
    collect_named_ref(payload, "observation_ref", &mut refs);
    collect_named_ref(payload, "usage_ref", &mut refs);
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
    let encoded = serde_json::to_vec(payload).unwrap_or_default();
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
    const LIMIT: usize = 4096;
    let slice = if bytes.len() > LIMIT {
        &bytes[..LIMIT]
    } else {
        bytes
    };
    String::from_utf8_lossy(slice).into_owned()
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
    format!(
        "Factory native owner adapters\n\nUsage:\n  factory owner <invocation-json|->\n\nAIKit contract revision: {AIKIT_CAW_CONTRACT_REVISION}\nWorkcell contract revision: {WORKCELL_CAW_CONTRACT_REVISION}\n\nReceipts are owner-operation evidence only. They do not imply Factory verification, Return, human inclusion or Recognition."
    )
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
            Self::ContractRevisionMismatch {
                owner,
                expected,
                actual,
            } => write!(
                formatter,
                "{owner} CAW contract revision changed: expected {expected}, got {actual}"
            ),
            Self::Spawn {
                owner,
                binary,
                error,
            } => write!(
                formatter,
                "could not execute {owner} owner binary {}: {error}",
                binary.display()
            ),
            Self::CommandFailed {
                owner,
                code,
                stdout,
                stderr,
                ..
            } => write!(
                formatter,
                "{owner} owner command failed with code {code:?}; stdout={stdout:?}; stderr={stderr:?}"
            ),
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
