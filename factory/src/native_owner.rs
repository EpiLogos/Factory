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
use std::time::{Duration, Instant};

pub const AIKIT_CAW_CONTRACT_REVISION: &str = "3d23d1eefbb999b0a5058ed0b4ca98dd2575b632";
pub const WORKCELL_CAW_CONTRACT_REVISION: &str = "f3a5be9fc751ee94b78aff11411e0cde65a46e4c";
pub const AIKIT_DELIVERY_CONTRACT: &str = "aikit.encounter-delivery/v1";
pub const WORKCELL_WRITE_BOUNDARY_CONTRACT: &str = "workcell.write-boundary-result/v1";
pub const WORKCELL_CONTROL_CONTRACT: &str = "workcell.control/v1";
pub const ACTUATION_GATEWAY_CONTRACT: &str = crate::native_gateway::GATEWAY_CONTRACT;
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
    /// A bounded tool round-trip on an Actuation Agency Gateway stream: hello,
    /// attach to the configured stream, announce the tool, execute it (or
    /// adopt an external executor's correlated tool-result), then land the
    /// tool-result, evidence and attributable return as durable stream events.
    /// The bearer token comes only from the environment.
    ActuationGateway {
        socket_path: PathBuf,
        subject: String,
        stream_ref: String,
        actuation_ref: String,
        agency_ref: String,
        agent_session_ref: String,
        return_ref: String,
        contract_revision: String,
        timeout_ms: u64,
        /// Bounded wait for an external executor's correlated tool-result
        /// before this attempt executes its own tool. 0 executes immediately.
        #[serde(default)]
        executor_wait_ms: u64,
        program: PathBuf,
        #[serde(default)]
        args: Vec<String>,
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
        NativeOwnerInvocation::WorkcellWriteBoundaryRun { timeout_ms, .. }
        | NativeOwnerInvocation::ActuationGateway { timeout_ms, .. } => {
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
        NativeOwnerInvocation::ActuationGateway {
            socket_path,
            subject,
            stream_ref,
            actuation_ref,
            agency_ref,
            agent_session_ref,
            return_ref,
            contract_revision,
            timeout_ms,
            executor_wait_ms,
            program,
            args,
        } => invoke_actuation_gateway(
            ActuationGatewayCall {
                socket_path,
                subject,
                stream_ref,
                actuation_ref,
                agency_ref,
                agent_session_ref,
                return_ref,
                contract_revision,
                timeout_ms: *timeout_ms,
                executor_wait_ms: *executor_wait_ms,
                program,
                args,
            },
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

/// The ActuationGateway invocation's validated, borrowed shape. Kept as one
/// struct so the adapter signature stays under the clippy argument bound.
struct ActuationGatewayCall<'a> {
    socket_path: &'a Path,
    subject: &'a str,
    stream_ref: &'a str,
    actuation_ref: &'a str,
    agency_ref: &'a str,
    agent_session_ref: &'a str,
    return_ref: &'a str,
    contract_revision: &'a str,
    timeout_ms: u64,
    executor_wait_ms: u64,
    program: &'a Path,
    args: &'a [String],
}

/// A gateway transport failure keeps its stage and its io kind; the caller
/// must be able to tell a connect refusal from a mid-round-trip deadline.
fn gateway_error(stage: &str, error: crate::native_gateway::GatewayError) -> NativeOwnerError {
    match error {
        crate::native_gateway::GatewayError::Io(io) => NativeOwnerError::Io(io::Error::new(
            io.kind(),
            format!("actuation gateway {stage}: {io}"),
        )),
        other => NativeOwnerError::InvalidResponse(format!("actuation gateway {stage}: {other}")),
    }
}

/// Execute one bounded tool round-trip as an attributed agent locus on an
/// Actuation Agency Gateway stream. The gateway stamps every landed event
/// with the granted locus, so the receipts are the attribution evidence; the
/// return event is correlated by the configured return_ref in both
/// directions. Every socket op shares one deadline; there are no retries.
fn invoke_actuation_gateway(
    call: ActuationGatewayCall<'_>,
    timeout: Duration,
) -> Result<OwnerOperationReceipt, NativeOwnerError> {
    use crate::native_gateway::{
        demand_ok, event_field, event_kind, event_ref, event_return_ref, event_sequence,
        receipt_event, GatewayConnection, GATEWAY_TOKEN_ENV,
    };
    require_revision(
        "Actuation",
        call.contract_revision,
        ACTUATION_GATEWAY_CONTRACT,
    )?;
    if call.timeout_ms == 0 {
        return Err(NativeOwnerError::InvalidInvocation(
            "Actuation gateway timeout must be positive".into(),
        ));
    }
    if !call.socket_path.is_absolute() {
        return Err(NativeOwnerError::InvalidInvocation(
            "Actuation gateway socket path must be absolute".into(),
        ));
    }
    for (value, name) in [
        (call.subject, "subject"),
        (call.stream_ref, "stream_ref"),
        (call.actuation_ref, "actuation_ref"),
        (call.agency_ref, "agency_ref"),
        (call.agent_session_ref, "agent_session_ref"),
        (call.return_ref, "return_ref"),
    ] {
        if value.trim().is_empty() {
            return Err(NativeOwnerError::InvalidInvocation(format!(
                "Actuation gateway {name} must not be empty"
            )));
        }
    }
    let token = std::env::var(GATEWAY_TOKEN_ENV)
        .ok()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| {
            NativeOwnerError::InvalidInvocation(format!(
                "environment variable {GATEWAY_TOKEN_ENV} must carry the gateway token; it is never embedded in configuration"
            ))
        })?;
    let deadline = Instant::now() + timeout;
    let mut connection = GatewayConnection::connect(call.socket_path, deadline)
        .map_err(|error| gateway_error("connect", error))?;

    let hello = connection
        .call(
            &serde_json::json!({
                "op": "hello",
                "protocol": ACTUATION_GATEWAY_CONTRACT,
                "token": token,
                "subject": call.subject,
            }),
            deadline,
        )
        .map_err(|error| gateway_error("hello", error))?;
    demand_ok(&hello, "hello").map_err(|error| gateway_error("hello", error))?;
    if hello.get("contract").and_then(Value::as_str) != Some(ACTUATION_GATEWAY_CONTRACT) {
        return Err(NativeOwnerError::InvalidResponse(format!(
            "gateway hello confirmed {} instead of {ACTUATION_GATEWAY_CONTRACT}",
            hello
                .get("contract")
                .and_then(Value::as_str)
                .unwrap_or("no contract")
        )));
    }
    let attach = connection
        .call(
            &serde_json::json!({
                "op": "attach",
                "stream_ref": call.stream_ref,
                "actuation_ref": call.actuation_ref,
                "agency_ref": call.agency_ref,
                "agent_session_ref": call.agent_session_ref,
            }),
            deadline,
        )
        .map_err(|error| gateway_error("attach", error))?;
    demand_ok(&attach, "attach").map_err(|error| gateway_error("attach", error))?;

    let tool_identity = format!(
        "tool:{}",
        call.program
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| call.program.display().to_string())
    );
    let tool_request = connection
        .call(
            &serde_json::json!({
                "op": "post",
                "kind": "tool-request",
                "content": format!(
                    "bounded tool execution for {}: `{} {}`",
                    call.return_ref,
                    call.program.display(),
                    call.args.join(" ")
                ),
                "return_ref": call.return_ref,
                "resource_refs": [tool_identity],
                "metadata": {"factory_subject": call.subject},
            }),
            deadline,
        )
        .map_err(|error| gateway_error("post tool-request", error))?;
    demand_ok(&tool_request, "post tool-request")
        .map_err(|error| gateway_error("post tool-request", error))?;

    // One bounded wait for an external executor's correlated tool-result; no
    // loop, and a quiet wait simply falls through to local execution.
    let mut external_content: Option<String> = None;
    let mut external_evidence: Vec<String> = Vec::new();
    if call.executor_wait_ms > 0 {
        let after = event_sequence(receipt_event(&tool_request)).unwrap_or(0);
        let wait_reply = connection
            .call(
                &serde_json::json!({
                    "op": "wait",
                    "after": after,
                    "timeout_ms": call.executor_wait_ms,
                }),
                deadline,
            )
            .map_err(|error| gateway_error("wait", error))?;
        demand_ok(&wait_reply, "wait").map_err(|error| gateway_error("wait", error))?;
        let events = wait_reply
            .get("events")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for event in events {
            if event_kind(event) == Some("tool-result")
                && event_return_ref(event) == Some(call.return_ref)
            {
                if let Some(content) = event_field(event, "content").and_then(Value::as_str) {
                    external_content = Some(content.to_owned());
                }
                if let Some(reference) = event_ref(event) {
                    external_evidence.push(reference.to_owned());
                }
                break;
            }
        }
    }
    let self_executed = external_content.is_none();

    // Local execution stays under the same deadline; a timeout or oversized
    // output is retained as an uncertain outcome, never as silent success.
    let execution = if self_executed {
        let mut command = Command::new(call.program);
        command.args(call.args);
        let budget = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_default();
        Some(crate::native_process::output(&mut command, budget))
    } else {
        None
    };
    let mut evidence_refs = BTreeSet::new();
    let (tool_content, exit_code, phase) = match execution {
        None => (
            external_content.unwrap_or_default(),
            None,
            OwnerOperationPhase::Returned,
        ),
        Some(Ok(output)) => {
            let exit = output.status.code();
            let digest_ref = format!(
                "factory-tool-output:blake3:{}",
                blake3::hash(&output.stdout).to_hex()
            );
            evidence_refs.insert(digest_ref);
            let mut text = format!("$ {} {}", call.program.display(), call.args.join(" "));
            if let Some(exit) = exit {
                text.push_str(&format!("  (exit {exit})"));
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                text.push('\n');
                text.push_str(&bounded_text(&output.stdout));
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                text.push_str(&format!("\n[stderr] {}", bounded_text(&output.stderr)));
            }
            let phase = if exit == Some(0) {
                OwnerOperationPhase::Returned
            } else {
                OwnerOperationPhase::Failed
            };
            (text, exit, phase)
        }
        Some(Err(error))
            if matches!(
                error.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::InvalidData
            ) =>
        {
            (
                format!("tool transport outcome uncertain: {error}"),
                None,
                OwnerOperationPhase::Uncertain,
            )
        }
        Some(Err(error)) => (
            format!("tool could not be started: {error}"),
            None,
            OwnerOperationPhase::Failed,
        ),
    };
    let tool_result = connection
        .call(
            &serde_json::json!({
                "op": "post",
                "kind": "tool-result",
                "content": tool_content,
                "return_ref": call.return_ref,
                "evidence_refs": external_evidence.iter().collect::<Vec<_>>(),
                "metadata": {
                    "factory_subject": call.subject,
                    "self_executed": self_executed,
                    "exit_code": exit_code,
                },
            }),
            deadline,
        )
        .map_err(|error| gateway_error("post tool-result", error))?;
    demand_ok(&tool_result, "post tool-result")
        .map_err(|error| gateway_error("post tool-result", error))?;
    if !self_executed {
        evidence_refs.extend(external_evidence.iter().cloned());
    }
    for receipt in [&tool_request, &tool_result] {
        if let Some(reference) = event_ref(receipt_event(receipt)) {
            evidence_refs.insert(reference.to_owned());
        }
        if let Some(sequence) = event_sequence(receipt_event(receipt)) {
            evidence_refs.insert(format!(
                "actuation-gateway-event:{}:{sequence}",
                call.stream_ref
            ));
        }
    }

    let evidence = connection
        .call(
            &serde_json::json!({
                "op": "post",
                "kind": "evidence",
                "content": format!(
                    "durable tool round-trip evidence for {}; tool-result event {}",
                    call.return_ref,
                    event_ref(receipt_event(&tool_result)).unwrap_or("unavailable")
                ),
                "return_ref": call.return_ref,
                "evidence_refs": evidence_refs.iter().collect::<Vec<_>>(),
                "metadata": {"factory_subject": call.subject},
            }),
            deadline,
        )
        .map_err(|error| gateway_error("post evidence", error))?;
    demand_ok(&evidence, "post evidence").map_err(|error| gateway_error("post evidence", error))?;

    let return_reply = connection
        .call(
            &serde_json::json!({
                "op": "post",
                "kind": "return",
                "content": format!(
                    "return for {}: bounded tool `{} {}` executed by {} on {}; outcome {}; evidence {}",
                    call.return_ref,
                    call.program.display(),
                    call.args.join(" "),
                    if self_executed { "this attempt" } else { "an external executor" },
                    call.subject,
                    phase_str(phase),
                    evidence_refs.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
                "return_ref": call.return_ref,
                "evidence_refs": evidence_refs.iter().collect::<Vec<_>>(),
                "metadata": {
                    "factory_subject": call.subject,
                    "self_executed": self_executed,
                    "exit_code": exit_code,
                },
            }),
            deadline,
        )
        .map_err(|error| gateway_error("post return", error))?;
    demand_ok(&return_reply, "post return").map_err(|error| gateway_error("post return", error))?;
    if event_kind(receipt_event(&return_reply)) != Some("return") {
        return Err(NativeOwnerError::InvalidResponse(
            "gateway receipt for the return did not retain a durable return event".into(),
        ));
    }
    for receipt in [&evidence, &return_reply] {
        if let Some(reference) = event_ref(receipt_event(receipt)) {
            evidence_refs.insert(reference.to_owned());
        }
        if let Some(sequence) = event_sequence(receipt_event(receipt)) {
            evidence_refs.insert(format!(
                "actuation-gateway-event:{}:{sequence}",
                call.stream_ref
            ));
        }
    }

    let payload = serde_json::json!({
        "hello": {
            "gateway": hello.get("gateway"),
            "contract": hello.get("contract"),
            "subject": hello.get("subject"),
        },
        "attach": attach,
        "tool_request_receipt": tool_request,
        "tool_result_receipt": tool_result,
        "evidence_receipt": evidence,
        "return_receipt": return_reply,
        "self_executed": self_executed,
        "exit_code": exit_code,
    });
    Ok(OwnerOperationReceipt {
        owner_ref: "actuation-gateway".into(),
        contract: ACTUATION_GATEWAY_CONTRACT.into(),
        operation_ref: format!("actuation-gateway:{}:{}", call.stream_ref, call.return_ref),
        receipt_ref: stable_receipt_ref("actuation-gateway-return", &payload),
        source_revision: call.contract_revision.into(),
        phase,
        evidence_refs,
        partial_effect_refs: BTreeSet::new(),
        payload,
    })
}

fn phase_str(phase: OwnerOperationPhase) -> &'static str {
    match phase {
        OwnerOperationPhase::Dispatching => "dispatching",
        OwnerOperationPhase::Submitted => "submitted",
        OwnerOperationPhase::Returned => "returned",
        OwnerOperationPhase::Failed => "failed",
        OwnerOperationPhase::Cancelled => "cancelled",
        OwnerOperationPhase::Uncertain => "uncertain",
        OwnerOperationPhase::ReconciledNoReplay => "reconciled-no-replay",
        OwnerOperationPhase::Observed => "observed",
        OwnerOperationPhase::Recovered => "recovered",
        OwnerOperationPhase::Released => "released",
    }
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
    format!("Factory native owner adapters\n\nUsage:\n  factory owner <invocation-json|->\n\nAIKit contract revision: {AIKIT_CAW_CONTRACT_REVISION}\nWorkcell contract revision: {WORKCELL_CAW_CONTRACT_REVISION}\nActuation gateway contract: {ACTUATION_GATEWAY_CONTRACT} (bearer token via the {token} environment variable)\n\nBounded transport receipts are owner evidence, not worker quiescence, verification, Return or Recognition.", token = crate::native_gateway::GATEWAY_TOKEN_ENV)
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

#[cfg(test)]
mod actuation_gateway_adapter_regressions {
    use super::*;
    use crate::native_gateway::{
        event_field, event_kind, event_ref, event_return_ref, event_sequence, read_frame,
        receipt_event, write_frame, GatewayConnection, GATEWAY_CONTRACT, GATEWAY_TOKEN_ENV,
    };
    use serde_json::json;
    use std::os::unix::net::UnixListener;
    use std::sync::Mutex;
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    /// The gateway token is a process-global; tests that touch its
    /// environment serialize on this lock.
    static GATEWAY_TOKEN_LOCK: Mutex<()> = Mutex::new(());

    fn with_gateway_token<T>(run: impl FnOnce() -> T) -> T {
        let guard = GATEWAY_TOKEN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(GATEWAY_TOKEN_ENV, "unit-test-only-not-a-credential");
        let result = run();
        std::env::remove_var(GATEWAY_TOKEN_ENV);
        drop(guard);
        result
    }

    fn gateway_invocation(
        socket: &Path,
        program: &str,
        executor_wait_ms: u64,
    ) -> NativeOwnerInvocation {
        NativeOwnerInvocation::ActuationGateway {
            socket_path: socket.to_path_buf(),
            subject: "agent:factory-unit".into(),
            stream_ref: "stream:factory-unit".into(),
            actuation_ref: "actuation:factory-unit".into(),
            agency_ref: "agency:factory-unit".into(),
            agent_session_ref: "session:factory-unit".into(),
            return_ref: "return:factory-unit".into(),
            contract_revision: ACTUATION_GATEWAY_CONTRACT.into(),
            timeout_ms: 10_000,
            executor_wait_ms,
            program: PathBuf::from(program),
            args: vec!["factory-gateway-round-trip".into()],
        }
    }

    fn scripted_receipt(posted: &Value) -> Value {
        let sequence = match posted.get("kind").and_then(Value::as_str) {
            Some("tool-request") => 1,
            Some("tool-result") => 2,
            Some("evidence") => 3,
            Some("return") => 4,
            _ => 0,
        };
        json!({
            "ok": true,
            "stream_ref": "stream:factory-unit",
            "cursor": {"last_sequence": sequence},
            "lifecycle": "open",
            "event": {
                "event_ref": format!("actuation:event:test-{sequence}"),
                "sequence": sequence,
                "kind": posted.get("kind"),
                "return_ref": posted.get("return_ref"),
                "content": posted.get("content"),
            },
        })
    }

    /// One-connection in-process gateway: scripted hello/attach replies, real
    /// post receipts, and an optional external tool-result surfaced on wait.
    /// Returns every frame the client sent, for token and ordering assertions.
    fn scripted_gateway(
        socket: &Path,
        hello_reply: Value,
        attach_reply: Value,
        external_tool_result: Option<Value>,
    ) -> JoinHandle<Vec<Value>> {
        let listener = UnixListener::bind(socket).expect("bind scripted gateway");
        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept scripted gateway");
            let mut writer = stream.try_clone().expect("clone scripted gateway stream");
            let mut reader = std::io::BufReader::new(stream);
            let mut seen = Vec::new();
            while let Ok(Some(frame)) = read_frame(&mut reader) {
                let reply = match frame.get("op").and_then(Value::as_str) {
                    Some("hello") => hello_reply.clone(),
                    Some("attach") => attach_reply.clone(),
                    Some("post") => scripted_receipt(&frame),
                    Some("wait") => match &external_tool_result {
                        Some(event) => json!({
                            "ok": true, "timed_out": false, "events": [event],
                            "last_sequence": event_field(event, "sequence").cloned().unwrap_or(json!(1)),
                        }),
                        None => {
                            json!({"ok": true, "timed_out": true, "events": [], "last_sequence": 1})
                        }
                    },
                    _ => break,
                };
                seen.push(frame);
                if write_frame(&mut writer, &reply).is_err() {
                    break;
                }
            }
            seen
        })
    }

    fn ok_hello() -> Value {
        json!({
            "ok": true, "gateway": "actuation-gateway", "contract": GATEWAY_CONTRACT,
            "subject": "agent:factory-unit", "store": "/controlled-test-store",
        })
    }

    fn ok_attach() -> Value {
        json!({
            "ok": true, "role": "agent", "stream_ref": "stream:factory-unit",
            "agency_granted": "agency:factory-unit", "agent_granted": "agent:factory-unit",
            "locus_granted": "locus:factory-unit", "cursor": {"last_sequence": 0},
        })
    }

    fn external_tool_result() -> Value {
        json!({
            "event_ref": "actuation:event:external", "sequence": 1,
            "kind": "tool-result", "return_ref": "return:factory-unit",
            "content": "external executor output 424242",
        })
    }

    fn posted_kinds(seen: &[Value]) -> Vec<String> {
        seen.iter()
            .filter(|frame| frame.get("op").and_then(Value::as_str) == Some("post"))
            .filter_map(|frame| frame.get("kind").and_then(Value::as_str))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn gateway_round_trip_self_executes_and_lands_the_attributable_return() {
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("gateway.sock");
        let server = scripted_gateway(&socket, ok_hello(), ok_attach(), None);
        let receipt = with_gateway_token(|| {
            invoke_native_owner(&gateway_invocation(&socket, "/bin/echo", 0)).unwrap()
        });
        let seen = server.join().unwrap();
        assert_eq!(receipt.owner_ref, "actuation-gateway");
        assert_eq!(receipt.contract, GATEWAY_CONTRACT);
        assert_eq!(
            receipt.operation_ref,
            "actuation-gateway:stream:factory-unit:return:factory-unit"
        );
        assert_eq!(receipt.phase, OwnerOperationPhase::Returned);
        assert_eq!(
            posted_kinds(&seen),
            ["tool-request", "tool-result", "evidence", "return"]
        );
        // The token travelled from the environment into the hello frame only.
        assert_eq!(seen[0]["token"], "unit-test-only-not-a-credential");
        assert!(seen[1..].iter().all(|frame| frame.get("token").is_none()));
        // Every posted frame carries the configured return correlation.
        for frame in seen.iter().skip(2) {
            assert_eq!(frame.get("return_ref"), Some(&json!("return:factory-unit")));
        }
        let attach = &receipt.payload["attach"];
        assert_eq!(attach["role"], "agent");
        assert_eq!(attach["locus_granted"], "locus:factory-unit");
        assert_eq!(receipt.payload["self_executed"], json!(true));
        assert_eq!(receipt.payload["exit_code"], json!(Some(0)));
        assert!(receipt.payload["tool_result_receipt"]["event"]["content"]
            .as_str()
            .unwrap()
            .contains("factory-gateway-round-trip"));
        for reference in [
            "actuation:event:test-1",
            "actuation:event:test-2",
            "actuation:event:test-3",
            "actuation:event:test-4",
            "actuation-gateway-event:stream:factory-unit:1",
            "actuation-gateway-event:stream:factory-unit:4",
        ] {
            assert!(
                receipt.evidence_refs.contains(reference),
                "missing {reference}"
            );
        }
        assert!(receipt
            .evidence_refs
            .iter()
            .any(|reference| reference.starts_with("factory-tool-output:blake3:")));
    }

    #[test]
    fn external_executor_result_is_adopted_without_running_the_local_tool() {
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("gateway.sock");
        let server = scripted_gateway(
            &socket,
            ok_hello(),
            ok_attach(),
            Some(external_tool_result()),
        );
        let receipt = with_gateway_token(|| {
            invoke_native_owner(&gateway_invocation(
                &socket,
                "/must-not-run/factory-tool",
                5_000,
            ))
            .unwrap()
        });
        let seen = server.join().unwrap();
        assert_eq!(receipt.phase, OwnerOperationPhase::Returned);
        assert_eq!(receipt.payload["self_executed"], json!(false));
        assert_eq!(receipt.payload["exit_code"], json!(None::<u64>));
        assert!(receipt.payload["tool_result_receipt"]["event"]["content"]
            .as_str()
            .unwrap()
            .contains("external executor output 424242"));
        assert!(receipt.evidence_refs.contains("actuation:event:external"));
        assert!(receipt
            .evidence_refs
            .iter()
            .all(|reference| !reference.starts_with("factory-tool-output:")));
        assert_eq!(
            posted_kinds(&seen),
            ["tool-request", "tool-result", "evidence", "return"]
        );
    }

    #[test]
    fn gateway_hello_refusal_surfaces_the_gateway_error_string() {
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("gateway.sock");
        let refused = json!({"ok": false, "denied": true, "error": "authentication failed"});
        let server = scripted_gateway(&socket, refused, ok_attach(), None);
        let failure = with_gateway_token(|| {
            invoke_native_owner(&gateway_invocation(&socket, "/bin/echo", 0)).unwrap_err()
        });
        server.join().unwrap();
        match failure {
            NativeOwnerError::InvalidResponse(message) => {
                assert!(
                    message.contains("hello refused: authentication failed"),
                    "{message}"
                );
            }
            other => panic!("expected a surfaced gateway refusal, got {other}"),
        }
    }

    #[test]
    fn attach_denial_names_the_missing_stream_grant() {
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("gateway.sock");
        let denied = json!({
            "ok": false, "denied": true,
            "error": "subject agent:factory-unit is not granted attach on stream:factory-unit",
        });
        let server = scripted_gateway(&socket, ok_hello(), denied, None);
        let failure = with_gateway_token(|| {
            invoke_native_owner(&gateway_invocation(&socket, "/bin/echo", 0)).unwrap_err()
        });
        server.join().unwrap();
        match failure {
            NativeOwnerError::InvalidResponse(message) => {
                assert!(message.contains("attach refused"), "{message}");
                assert!(
                    message.contains("not granted attach on stream:factory-unit"),
                    "{message}"
                );
            }
            other => panic!("expected a surfaced attach denial, got {other}"),
        }
    }

    #[test]
    fn missing_token_environment_is_refused_before_any_connection() {
        let guard = GATEWAY_TOKEN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::remove_var(GATEWAY_TOKEN_ENV);
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("must-never-be-approached.sock");
        let failure =
            invoke_native_owner(&gateway_invocation(&socket, "/bin/echo", 0)).unwrap_err();
        drop(guard);
        match failure {
            NativeOwnerError::InvalidInvocation(message) => {
                assert!(message.contains(GATEWAY_TOKEN_ENV), "{message}");
                assert!(message.contains("never embedded"), "{message}");
            }
            other => panic!("expected an invocation refusal, got {other}"),
        }
    }

    #[test]
    fn unaccepted_gateway_contract_revision_is_refused_without_connecting() {
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("unreachable.sock");
        let mut invocation = gateway_invocation(&socket, "/bin/echo", 0);
        if let NativeOwnerInvocation::ActuationGateway {
            contract_revision, ..
        } = &mut invocation
        {
            *contract_revision = "actuation.gateway/v9".into();
        }
        assert!(matches!(
            invoke_native_owner(&invocation),
            Err(NativeOwnerError::ContractRevisionMismatch {
                owner: "Actuation",
                ..
            })
        ));
    }

    #[test]
    fn empty_identity_fields_are_refused() {
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("gateway.sock");
        let mut invocation = gateway_invocation(&socket, "/bin/echo", 0);
        if let NativeOwnerInvocation::ActuationGateway { return_ref, .. } = &mut invocation {
            *return_ref = "  ".into();
        }
        let failure = invoke_native_owner(&invocation).unwrap_err();
        assert!(matches!(failure, NativeOwnerError::InvalidInvocation(_)));
    }

    #[test]
    fn exhausted_gateway_deadline_is_bounded_and_named() {
        let work = tempfile::tempdir().unwrap();
        let socket = work.path().join("gateway.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        // Held for the test's duration only; never joined, so the client's
        // deadline failure is not made to wait for the server's own clock.
        let _silent = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(30));
            drop(stream);
        });
        let started = Instant::now();
        let failure = with_gateway_token(|| {
            invoke_native_owner_bounded(&gateway_invocation(&socket, "/bin/echo", 0), 400)
                .unwrap_err()
        });
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "deadline not bounded: {elapsed:?}"
        );
        match failure {
            NativeOwnerError::Io(error) => {
                assert_eq!(error.kind(), io::ErrorKind::TimedOut, "{error}");
                assert!(
                    error.to_string().contains("actuation gateway hello"),
                    "{error}"
                );
            }
            other => panic!("expected a bounded transport timeout, got {other}"),
        }
    }

    /// Live round-trip against the real Actuation Agency Gateway service on
    /// the omarchy dev machine. The gateway policy carries a dedicated test
    /// grant (agent:factory-gateway-test on stream:factory-gateway-test), so
    /// this runs in its own isolated stream and never touches the resident
    /// carrier's locus. Run with:
    /// cargo test --workspace --all-targets --locked live_actuation_gateway -- --ignored --nocapture
    #[test]
    #[ignore = "requires the live smoke actuation-gateway and its smoke.env token"]
    fn live_actuation_gateway_round_trip_lands_the_attributable_return() {
        let home = std::env::var("HOME").expect("HOME");
        let env_file = std::env::var("FACTORY_ACTUATION_SMOKE_ENV")
            .unwrap_or_else(|_| format!("{home}/.config/actuation-gateway/smoke.env"));
        let env_text = fs::read_to_string(&env_file)
            .unwrap_or_else(|error| panic!("read {env_file}: {error}"));
        let token = env_text
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim() == GATEWAY_TOKEN_ENV)
            .map(|(_, value)| value.trim().trim_matches('"').to_owned())
            .filter(|value| !value.is_empty())
            .expect("smoke env file carries the gateway token");

        let return_ref = format!("return:factory-gateway-test-{}", ulid::Ulid::new());
        let invocation = NativeOwnerInvocation::ActuationGateway {
            socket_path: format!(
                "{home}/.local/state/workcell/smoke/actuation-gateway/gateway.sock"
            )
            .into(),
            subject: "agent:factory-gateway-test".into(),
            stream_ref: "stream:factory-gateway-test".into(),
            actuation_ref: "actuation:factory-gateway-test".into(),
            agency_ref: "agency:factory-gateway-test".into(),
            agent_session_ref: "session:factory-gateway-test".into(),
            return_ref: return_ref.clone(),
            contract_revision: ACTUATION_GATEWAY_CONTRACT.into(),
            timeout_ms: 20_000,
            executor_wait_ms: 1_500,
            program: "git".into(),
            args: vec![
                "-C".into(),
                format!("{home}/Central/Work/Factory"),
                "rev-parse".into(),
                "HEAD".into(),
            ],
        };
        let receipt = {
            let guard = GATEWAY_TOKEN_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::env::set_var(GATEWAY_TOKEN_ENV, &token);
            let receipt = invoke_native_owner(&invocation)
                .unwrap_or_else(|error| panic!("live gateway round-trip failed: {error}"));
            std::env::remove_var(GATEWAY_TOKEN_ENV);
            drop(guard);
            receipt
        };

        assert_eq!(receipt.phase, OwnerOperationPhase::Returned);
        assert_eq!(receipt.payload["self_executed"], json!(true));
        assert_eq!(receipt.payload["hello"]["contract"], GATEWAY_CONTRACT);
        assert_eq!(receipt.payload["attach"]["role"], "agent");
        assert_eq!(
            receipt.payload["attach"]["locus_granted"],
            "locus:factory-gateway-test"
        );
        let tool_output = receipt.payload["tool_result_receipt"]["event"]["content"]
            .as_str()
            .expect("tool-result content");
        let head = tool_output
            .split_whitespace()
            .find(|word| word.len() == 40 && word.chars().all(|c| c.is_ascii_hexdigit()))
            .expect("tool-result content carries the Factory rev-parse output")
            .to_owned();
        assert_eq!(receipt.payload["exit_code"], json!(Some(0)));
        assert_eq!(
            event_kind(receipt_event(&receipt.payload["return_receipt"])),
            Some("return")
        );
        println!("live receipt_ref: {}", receipt.receipt_ref);
        println!("live return_ref: {return_ref}");
        println!("live evidence_refs: {:?}", receipt.evidence_refs);
        println!("live factory head: {head}");

        // The attributable return, received back: replay the durable stream
        // through a fresh connection and confirm the gateway stamped the
        // granted locus on every event of this round-trip.
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut connection = GatewayConnection::connect(
            Path::new(&format!(
                "{home}/.local/state/workcell/smoke/actuation-gateway/gateway.sock"
            )),
            deadline,
        )
        .unwrap();
        let hello = connection
            .call(
                &json!({
                    "op": "hello", "protocol": GATEWAY_CONTRACT,
                    "token": token, "subject": "agent:factory-gateway-test",
                }),
                deadline,
            )
            .unwrap();
        assert_eq!(hello["ok"], json!(true));
        let attach = connection
            .call(
                &json!({
                    "op": "attach",
                    "stream_ref": "stream:factory-gateway-test",
                    "actuation_ref": "actuation:factory-gateway-test",
                    "agency_ref": "agency:factory-gateway-test",
                    "agent_session_ref": "session:factory-gateway-test",
                }),
                deadline,
            )
            .unwrap();
        assert_eq!(attach["ok"], json!(true));
        let replay = connection
            .call(&json!({"op": "replay", "after": 0}), deadline)
            .unwrap();
        assert_eq!(replay["ok"], json!(true));
        let events = replay["page"]["events"]
            .as_array()
            .expect("replay page events");
        let round_trip: Vec<&Value> = events
            .iter()
            .filter(|event| event_return_ref(event) == Some(return_ref.as_str()))
            .collect();
        let kinds: Vec<_> = round_trip
            .iter()
            .filter_map(|event| event_kind(event))
            .collect();
        assert_eq!(kinds, ["tool-request", "tool-result", "evidence", "return"]);
        let mut sequences: Vec<u64> = round_trip
            .iter()
            .filter_map(|event| event_sequence(event))
            .collect();
        assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
        sequences.dedup();
        let return_event = round_trip
            .iter()
            .find(|event| event_kind(event) == Some("return"))
            .expect("durable return event");
        let actor = &return_event["actor"];
        assert_eq!(actor["agency_ref"], "agency:factory-gateway-test");
        assert_eq!(actor["agent_ref"], "agent:factory-gateway-test");
        assert_eq!(actor["locus_ref"], "locus:factory-gateway-test");
        assert!(event_ref(return_event).is_some());
        println!(
            "live durable return event {} at sequence {}",
            event_ref(return_event).unwrap(),
            event_sequence(return_event).unwrap()
        );
    }
}
