//! Factory Return -> Central reviewed receiving. This calls the real `ctrl`
//! Action surface. It never edits, reviews, includes or recognises a document.
//! The only external write is the owner's idempotent producer-key submission.

use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
use crate::attempt_native_store::{FileAttemptStore, NativeReceivingObservationAdmission, ReceivingIntentClaim};
use crate::attempt_runtime::{
    FactoryAttemptActionRequest, FactoryAttemptOperation, FactoryAttemptReading,
    FactoryAttemptRecord, OwnerOperationPhase, OwnerOperationReceipt, FACTORY_ATTEMPT_ACTION,
};
use crate::cli::CliError;
use crate::core::run::RunRef;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use sha2::{Digest, Sha256};

pub const CENTRAL_CONTRACT_REVISION: &str = "e7e8479f1502732821bd3e7d3d5ceda38fd4279f";
pub const RECEIVING_ACTION: &str = "factory.attempt-receiving-action/v1";
pub const RECEIVING_RECEIPT: &str = "factory.attempt-receiving-receipt/v1";
const CALL_CONTRACT: &str = "factory.attempt-receiving-call/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CentralReceivingEndpoint {
    pub binary: PathBuf,
    pub root: PathBuf,
    pub contract_revision: String,
    pub project: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceivingTarget {
    pub source_ref: String,
    pub document_id: String,
    pub source_revision: String,
    pub expected_authority_revision: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceivingRequest {
    pub contract: String,
    pub request_ref: String,
    pub projection_ref: String,
    pub caller: FactoryActionCaller,
    pub run_ref: RunRef,
    pub expected_revision: u64,
    pub authority: ProjectedFactoryActionAuthority,
    pub attempt_ref: String,
    pub central: CentralReceivingEndpoint,
    /// Recovery-only binary replacement; original root/project/input remain immutable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup_endpoint: Option<CentralReceivingEndpoint>,
    pub target: ReceivingTarget,
    pub occurred_at_unix_seconds: Option<u64>,
    /// Explicitly reconcile the original intent through guarded native lookup.
    /// Missing/unavailable publication is unknown, never an implicit resend.
    /// Ordinary exact replay returns retained, unverified history without IPC.
    #[serde(default)]
    pub recover: bool,
}
fn record<'a>(
    reading: &'a FactoryAttemptReading,
    reference: &str,
) -> Result<&'a FactoryAttemptRecord, String> {
    reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == reference)
        .ok_or_else(|| "attempt not retained in this Run".into())
}
fn action(
    request: &ReceivingRequest,
    revision: u64,
    suffix: &str,
    operation: FactoryAttemptOperation,
) -> FactoryAttemptActionRequest {
    FactoryAttemptActionRequest {
        contract: FACTORY_ATTEMPT_ACTION.into(),
        projection_ref: format!(
            "{}:{}:{suffix}",
            request.projection_ref, request.request_ref
        ),
        caller: request.caller.clone(),
        run_ref: request.run_ref.clone(),
        expected_revision: revision,
        authority: request.authority.clone(),
        operation,
    }
}
fn retain(
    store: &mut FileAttemptStore,
    request: &ReceivingRequest,
    suffix: &str,
    operation: FactoryAttemptOperation,
) -> Result<(), CliError> {
    for _ in 0..8 {
        let reading = store.reading().map_err(CliError::from_native)?;
        let attempt = record(&reading, &request.attempt_ref)?;
        if let FactoryAttemptOperation::RecordObservation { receipt, .. } = &operation {
            if let Some(existing) = attempt
                .observations
                .iter()
                .find(|existing| existing.receipt_ref == receipt.receipt_ref)
            {
                return if existing == receipt {
                    Ok(())
                } else {
                    Err("receiving receipt identity conflicts".into())
                };
            }
        }
        let retention = action(request, reading.revision, suffix, operation.clone());
        let applied = if matches!(&operation, FactoryAttemptOperation::RecordObservation { .. }) {
            let admission = NativeReceivingObservationAdmission::new(&retention, None)
                .map_err(CliError::from_native)?;
            store.apply_native_receiving_observation(retention, admission).map(|result| result.receipt.expect("retention Action receipt"))
        } else { store.apply(retention) };
        match applied {
            Ok(_) => return Ok(()),
            Err(error) => {
                if crate::native_publication_uncertainty(&error).is_some() {
                    return Err(CliError::from_native(error));
                }
                if store.reading().map_err(CliError::from_native)?.revision == reading.revision {
                    return Err(CliError::from_native(error));
                }
            }
        }
    }
    Err("receiving retention remained contended; explicitly recover this request".into())
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn return_proposal(run_ref: &RunRef, attempt: &FactoryAttemptRecord) -> Result<Value, String> {
    let returned = attempt
        .readable_return
        .as_ref()
        .ok_or("a native readable Return is required before receiving")?;
    let identity = blake3::hash(
        format!(
            "{}\n{}\n{}",
            run_ref, attempt.attempt_ref, returned.return_ref
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    let refs = |values: &BTreeSet<String>| {
        values
            .iter()
            .map(|value| escape(value))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let html=format!("<p>{}</p><p>Factory Return: {}. Task: {}. Run: {}. Attempt: {}.</p><p>Artifacts: {}</p><p>Evidence: {}</p>",
        escape(&returned.summary),escape(&returned.return_ref),escape(&attempt.task_ref),escape(&run_ref.to_string()),escape(&attempt.attempt_ref),refs(&returned.artifact_refs),refs(&returned.evidence_refs));
    Ok(
        json!({"operation":"entry.add","entry_id":format!("factory-return:{identity}"),
        "contribution_id":format!("factory-return:{identity}:body"),"html":html}),
    )
}
fn submission(request: &ReceivingRequest, attempt: &FactoryAttemptRecord) -> Result<Value, String> {
    let proposal = return_proposal(&request.run_ref, attempt)?;
    let producer_key = proposal["entry_id"].clone();
    let mut input = json!({"producer_key":producer_key,"source_ref":request.target.source_ref,
        "document_id":request.target.document_id,"expected_source_revision":request.target.source_revision,
        "task_ref":attempt.task_ref,"run_ref":request.run_ref.to_string(),"session_ref":attempt.disposition.body.agent_session_ref,
        "proposal":proposal});
    if let Some(project) = &request.central.project {
        input["project"] = json!(project);
    }
    if let Some(revision) = &request.target.expected_authority_revision {
        input["expected_authority_revision"] = json!(revision);
    }
    // Read-only attempts can acquire their native NOW after initial admission.
    // Preserve that actual owner correlation without inventing a placement grant.
    let now_ref = attempt
        .disposition
        .placement
        .as_ref()
        .map(|placement| placement.now_ref.as_str())
        .or_else(|| {
            attempt
                .tracking
                .iter()
                .rev()
                .find(|fact| fact.owner_ref == "central" && fact.kind == "now")
                .map(|fact| fact.subject_ref.as_str())
        });
    if let Some(now_ref) = now_ref {
        input["now_ref"] = json!(now_ref);
    }
    if let Some(day) = attempt
        .tracking
        .iter()
        .rev()
        .find(|fact| fact.owner_ref == "central" && fact.kind == "day")
    {
        input["day_ref"] = json!(day.subject_ref);
    }
    if let Some(time) = request.occurred_at_unix_seconds {
        input["occurred_at_unix_seconds"] = json!(time);
    }
    if serde_json::to_vec(&input)
        .map_err(|error| error.to_string())?
        .len()
        > 512 * 1024
    {
        return Err("Return proposal exceeds Central's native receiving bound".into());
    }
    Ok(input)
}
pub(crate) fn lookup_input(endpoint: &CentralReceivingEndpoint, original: &Value) -> Result<Value, String> {
    let key = original["producer_key"].as_str().filter(|key| !key.trim().is_empty() && key.len() <= 4096)
        .ok_or("retained receiving input omitted its bounded native producer key")?;
    if !original.is_object() || original["project"].as_str() != endpoint.project.as_deref() {
        return Err("retained receiving input changed its exact owner scope".into());
    }
    let mut input = json!({"producer_key":key,"original_request":original});
    if let Some(project) = &endpoint.project { input["project"] = json!(project); }
    // The original expected_authority_revision stays inside original_request.
    // Current authentication belongs to the owner and protected host channel.
    Ok(input)
}

pub(crate) fn validate_original_request(response: &Value, input: &Value) -> Result<(), String> {
    let expected = format!("{:x}", Sha256::digest(serde_json::to_string(input).map_err(|error| error.to_string())?.as_bytes()));
    if response["data"]["record"]["request_digest"] != expected {
        return Err("native receiving does not retain this exact original request digest".into());
    }
    Ok(())
}
pub(crate) fn validate_lookup(response: &Value) -> Result<(), String> {
    if response["action"] != "central.receiving.read"
        || response["data"]["lookup"]["selector"] != "authenticated_producer_key"
        || response["data"]["lookup"]["original_request_verified"] != true
    {
        return Err("native receiving did not authenticate and verify the exact original producer input; outcome remains unknown".into());
    }
    Ok(())
}

/// Private invocation evidence. Broad diagnostics never print captured bytes.
/// An observation is transport evidence, not a readable owner reply or admission.
pub struct NativeCallFailure {
    pub(crate) message: String,
    pub(crate) observation: Value,
    cause: Option<Box<dyn std::error::Error + Send + Sync>>,
    secondary_causes: Vec<Box<dyn std::error::Error + Send + Sync>>,
}
impl NativeCallFailure {
    fn validation(message: String, observation: Value) -> Self {
        Self { message, observation, cause: None, secondary_causes: Vec::new() }
    }
    fn caused(message: String, observation: Value, cause: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self { message, observation, cause: Some(Box::new(cause)), secondary_causes: Vec::new() }
    }
    /// Available only through an explicit caller evidence read. Never Debug/Display.
    pub fn observation(&self) -> &Value { &self.observation }
    pub fn secondary_causes(&self) -> impl Iterator<Item = &(dyn std::error::Error + Send + Sync + 'static)> {
        self.secondary_causes.iter().map(|cause| cause.as_ref())
    }
}
impl std::fmt::Debug for NativeCallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeCallFailure").field("message", &self.message)
            .field("has_cause", &self.cause.is_some()).field("secondary_cause_count", &self.secondary_causes.len())
            .field("private_observation", &"withheld; explicit observation accessor only").finish()
    }
}
impl std::fmt::Display for NativeCallFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.message) }
}
impl std::error::Error for NativeCallFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|cause| cause.as_ref() as &(dyn std::error::Error + 'static))
    }
}
impl From<String> for NativeCallFailure {
    fn from(message: String) -> Self {
        Self::validation(message, json!({"stage":"receivingValidation","capture":null}))
    }
}
impl From<&str> for NativeCallFailure {
    fn from(message: &str) -> Self { message.to_owned().into() }
}
/// Two actual failures: the original invocation and its later retention. The
/// source chain keeps the native publisher discoverable; the original owned
/// transport error remains available without reconstructing it from text.
pub struct NativeReceivingRetentionFailure {
    invocation: NativeCallFailure,
    retention: CliError,
}
impl NativeReceivingRetentionFailure {
    pub fn invocation(&self) -> &NativeCallFailure { &self.invocation }
    pub fn retention(&self) -> &CliError { &self.retention }
    pub(crate) fn into_cli(invocation: NativeCallFailure, retention: CliError, result: Value) -> CliError {
        CliError::from_native(Self { invocation, retention }).with_native_result(result)
    }
}
impl std::fmt::Debug for NativeReceivingRetentionFailure {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeReceivingRetentionFailure").field("invocation",&self.invocation)
            .field("retention",&self.retention).finish()
    }
}
impl std::fmt::Display for NativeReceivingRetentionFailure {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native receiving outcome is unknown and observation retention failed; both original causes retained")
    }
}
impl std::error::Error for NativeReceivingRetentionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error+'static)> { Some(&self.retention) }
}
fn call_observed(endpoint: &CentralReceivingEndpoint, operation: &str, input: &Value) -> Result<(Value, Value), NativeCallFailure> {
    call_observed_bounded(endpoint, operation, input, Duration::from_secs(30))
}
pub(crate) fn call_observed_bounded(endpoint: &CentralReceivingEndpoint, operation: &str, input: &Value, timeout: Duration) -> Result<(Value, Value), NativeCallFailure> {
    let mut command = Command::new(&endpoint.binary);
    let body = serde_json::to_string(input).map_err(|error| NativeCallFailure::caused(
        "Central request serialization failed before invocation".into(),
        json!({"stage":"serialization","processStarted":false}), error))?;
    command.arg("--json").arg("--root").arg(&endpoint.root).args(["action","run",operation]).arg(body);
    let output = crate::native_process::output(&mut command, timeout).map_err(|error| {
        let observation = crate::native_owner::native_capture_observation(&error);
        NativeCallFailure::caused("Central transport outcome is unknown; original capture cause retained".into(), observation, error)
    })?;
    // Only complete Output reaches parsing. A failure prefix is never a Reply.
    let parsed = serde_json::from_slice::<Value>(&output.stdout);
    let observation = json!({"stage":"nativeReply","processStarted":true,"exitCode":output.status.code(),"exitStatus":output.status.to_string(),
        "stdoutEof":true,"stderrEof":true,"clientReaped":true,"stdoutTruncated":false,"stderrTruncated":false,
        "stdout":crate::native_owner::native_capture_prefix(&output.stdout),"stderr":crate::native_owner::native_capture_prefix(&output.stderr),"centralResponse":parsed.as_ref().ok()});
    let value = parsed.map_err(|error| NativeCallFailure::caused("Central did not return a readable complete ActionResult".into(), observation.clone(), error))?;
    if !output.status.success() || value["ok"] != true || value["status"] != "success" || value["action"] != operation {
        // Preserve actual action/code/message/details in the private observation;
        // arbitrary native error payloads must not become a broad diagnostic.
        return Err(NativeCallFailure::validation(format!("Central did not confirm {operation}; actual response retained, outcome requires owner reconciliation"), observation));
    }
    Ok((value, observation))
}
/// Finite native filesystem observations. These holds do not claim fd-exec or
/// atomically lock an external namespace against a different OS writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReceivingRootIdentity { device: u64, inode: u64 }
#[derive(Debug)]
pub(crate) struct HeldReceivingRoot {
    locator: PathBuf,
    canonical: PathBuf,
    held: std::fs::File,
    identity: ReceivingRootIdentity,
}
impl HeldReceivingRoot {
    pub(crate) fn open(locator: &Path) -> Result<Self, String> {
        if !locator.is_absolute() { return Err("receiving root must be the original absolute locator".into()); }
        #[cfg(unix)] {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            let canonical = locator.canonicalize().map_err(|error| format!("native receiving root unavailable: {error}"))?;
            let held = std::fs::OpenOptions::new().read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&canonical).map_err(|error| format!("native receiving root hold unavailable: {error}"))?;
            let metadata = held.metadata().map_err(|error| error.to_string())?;
            if !metadata.is_dir() { return Err("native receiving root is not an actual directory".into()); }
            let identity = ReceivingRootIdentity { device: metadata.dev(), inode: metadata.ino() };
            let result = Self { locator: locator.to_owned(), canonical, held, identity };
            result.recheck()?;
            Ok(result)
        }
        #[cfg(not(unix))] { Err("native receiving physical root qualification is unavailable on this platform".into()) }
    }
    pub(crate) fn canonical(&self) -> &Path { &self.canonical }
    pub(crate) fn identity(&self) -> &ReceivingRootIdentity { &self.identity }
    pub(crate) fn recheck(&self) -> Result<(), String> {
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            let held = self.held.metadata().map_err(|error| error.to_string())?;
            let current = self.locator.canonicalize().map_err(|error| format!("original receiving root unavailable: {error}"))?;
            let named = std::fs::symlink_metadata(&self.canonical).map_err(|error| error.to_string())?;
            let located = std::fs::metadata(&self.locator).map_err(|error| error.to_string())?;
            if current != self.canonical || !held.is_dir() || !named.is_dir() || !located.is_dir()
                || [&held, &named, &located].iter().any(|value| value.dev() != self.identity.device || value.ino() != self.identity.inode) {
                return Err("original receiving root physical identity changed; outcome remains unknown, never rebind or resend".into());
            }
            Ok(())
        }
        #[cfg(not(unix))] { Err("native receiving physical root qualification is unavailable on this platform".into()) }
    }
    pub(crate) fn require_claim(&self, claim: &Value) -> Result<(), String> {
        let original = claim["canonicalRoot"].as_str().ok_or("retained original canonical receiving root is unavailable")?;
        let identity: ReceivingRootIdentity = serde_json::from_value(claim["rootIdentity"].clone())
            .map_err(|_| "retained original physical receiving root identity is unavailable; never infer or backfill it")?;
        if Path::new(original) != self.canonical || identity != self.identity {
            return Err("original receiving root physical identity changed; outcome remains unknown, never rebind or resend".into());
        }
        self.recheck()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReceivingClientIdentity {
    device: u64, inode: u64, bytes: u64, mode: u32, uid: u32, gid: u32,
    modified: (i64, i64), changed: (i64, i64), sha256: String,
}
/// One actual client and owner hold survives descriptor/read and admission.
#[derive(Debug)]
pub(crate) struct QualifiedReceivingClient {
    endpoint: CentralReceivingEndpoint,
    locator: PathBuf,
    held: std::fs::File,
    identity: ReceivingClientIdentity,
    root: HeldReceivingRoot,
}
impl QualifiedReceivingClient {
    fn measure(file: &std::fs::File) -> Result<ReceivingClientIdentity, String> {
        #[cfg(unix)] {
            use std::io::{Read, Seek, SeekFrom};
            use std::os::unix::fs::MetadataExt;
            const MAX_CLIENT: u64 = 128 * 1024 * 1024;
            let before = file.metadata().map_err(|error| error.to_string())?;
            if !before.is_file() || before.nlink() == 0 || before.len() > MAX_CLIENT || before.mode() & 0o111 == 0 {
                return Err("qualified receiving client must be a bounded regular executable".into());
            }
            let shape = |value: &std::fs::Metadata| (value.dev(),value.ino(),value.len(),value.mode(),value.uid(),value.gid(),value.mtime(),value.mtime_nsec(),value.ctime(),value.ctime_nsec());
            let mut reader = file.try_clone().map_err(|error| error.to_string())?;
            reader.seek(SeekFrom::Start(0)).map_err(|error| error.to_string())?;
            let mut hasher = Sha256::new(); let mut length = 0_u64; let mut buffer = [0_u8; 64 * 1024];
            loop { let read = reader.read(&mut buffer).map_err(|error| error.to_string())?; if read == 0 { break; }
                length += read as u64; if length > MAX_CLIENT { return Err("receiving client grew beyond its native bound".into()); }
                hasher.update(&buffer[..read]); }
            let after = file.metadata().map_err(|error| error.to_string())?;
            if length != before.len() || shape(&before) != shape(&after) { return Err("receiving client changed while its native bytes were observed".into()); }
            Ok(ReceivingClientIdentity { device:before.dev(),inode:before.ino(),bytes:length,mode:before.mode(),uid:before.uid(),gid:before.gid(),
                modified:(before.mtime(),before.mtime_nsec()),changed:(before.ctime(),before.ctime_nsec()),sha256:format!("{:x}",hasher.finalize()) })
        }
        #[cfg(not(unix))] { let _ = file; Err("native receiving physical client qualification is unavailable on this platform".into()) }
    }
    pub(crate) fn new(endpoint: &CentralReceivingEndpoint, owner_claim: &Value) -> Result<Self, NativeCallFailure> {
        let failure = |message: String| NativeCallFailure::validation(message.clone(),json!({"stage":"physicalQualification","failure":message,"processStarted":false}));
        if !endpoint.binary.is_absolute() { return Err(failure("receiving client must be an absolute declared path".into())); }
        let root = HeldReceivingRoot::open(&endpoint.root).map_err(failure)?;
        root.require_claim(owner_claim).map_err(failure)?;
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            let canonical = endpoint.binary.canonicalize().map_err(|error| NativeCallFailure::caused("native receiving client canonical path is unavailable".into(),json!({"stage":"physicalQualification","errorKind":format!("{:?}",error.kind()),"rawOsError":error.raw_os_error(),"processStarted":false,"failure":error.to_string()}),error))?;
            let held = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&canonical).map_err(|error| NativeCallFailure::caused("native receiving client hold is unavailable".into(),json!({"stage":"physicalQualification","errorKind":format!("{:?}",error.kind()),"rawOsError":error.raw_os_error(),"processStarted":false,"failure":error.to_string()}),error))?;
            let identity = Self::measure(&held).map_err(failure)?;
            let mut physical = endpoint.clone(); physical.binary = canonical; physical.root = root.canonical().to_owned();
            let result = Self { endpoint:physical,locator:endpoint.binary.clone(),held,identity,root };
            result.recheck().map_err(failure)?; Ok(result)
        }
        #[cfg(not(unix))] { Err(failure("native receiving client qualification is unavailable on this platform".into())) }
    }
    pub(crate) fn recheck(&self) -> Result<(), String> {
        self.root.recheck()?;
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            let named = std::fs::symlink_metadata(&self.endpoint.binary).map_err(|error| error.to_string())?;
            let current = self.locator.canonicalize().map_err(|error| error.to_string())?;
            if current != self.endpoint.binary || !named.is_file() || named.dev() != self.identity.device || named.ino() != self.identity.inode
                || Self::measure(&self.held)? != self.identity {
                return Err("qualified receiving client identity or bytes changed; no lookup, resend or admission".into());
            }
            Ok(())
        }
        #[cfg(not(unix))] { Err("native receiving client qualification is unavailable on this platform".into()) }
    }
    pub(crate) fn evidence(&self) -> Value {
        json!({"rootLocator":self.root.locator,"canonicalRoot":self.root.canonical,"rootIdentity":self.root.identity,
            "clientLocator":self.locator,"canonicalClient":self.endpoint.binary,"clientDevice":self.identity.device,
            "clientInode":self.identity.inode,"clientByteLength":self.identity.bytes,"clientSha256":self.identity.sha256,
            "standing":"held objects with finite pre/post checks; not atomic executable selection or semantic owner authentication"})
    }
    pub(crate) fn call(&self, operation: &str, input: &Value) -> Result<Value, NativeCallFailure> {
        self.recheck().map_err(|message| NativeCallFailure::validation(message.clone(),json!({"stage":"beforeNativeCall","failure":message,"processStarted":false})))?;
        let result = call_observed(&self.endpoint, operation, input);
        match (result,self.recheck()) {
            (Ok((value,_observed)),Ok(())) => Ok(value),
            (Ok((_value,mut observed)),Err(message)) => {
                observed["stage"]=json!("afterNativeCall");observed["qualificationFailure"]=json!(message);observed["physicalQualification"]=self.evidence();
                Err(NativeCallFailure::validation(message,observed))
            },
            (Err(mut failure),Ok(())) => {failure.observation["physicalQualification"]=self.evidence();Err(failure)},
            (Err(mut failure),Err(secondary)) => { failure.observation["physicalQualification"]=self.evidence();failure.observation["secondaryQualificationFailure"]=json!(secondary);
                failure.secondary_causes.push(Box::new(NativeCallFailure::from(secondary))); Err(failure) }
        }
    }
}

/// Replacement permits only the client. Original physical owner comes from
/// the retained canonical claim, never a fresh interpretation of its locator.
pub(crate) fn lookup_endpoint(original: &CentralReceivingEndpoint, supplied: Option<&CentralReceivingEndpoint>, owner_claim: &Value) -> Result<QualifiedReceivingClient, NativeCallFailure> {
    let selected = supplied.unwrap_or(original);
    if selected.contract_revision != original.contract_revision || selected.project != original.project
        || selected.root != original.root || owner_claim["project"].as_str() != original.project.as_deref() {
        let message = "lookup replacement may change only the actual binary, never original owner root/project/contract".to_owned();
        return Err(NativeCallFailure::validation(message.clone(),json!({"stage":"lookupRoute","failure":message,"processStarted":false})));
    }
    QualifiedReceivingClient::new(selected,owner_claim)
}
/// Host-only replacement for a retained question/closure owner. Public explicit
/// recovery retains its exact raw-root restriction in lookup_endpoint above.
/// This bridge checks the current host's physical owner, then carries ORIGINAL
/// locator/claim into the hold; argv still uses its qualified original canonical
/// root, never a fresh interpretation of either mutable locator.
pub(crate) fn host_client_for_original(
    original: &CentralReceivingEndpoint,
    current: &CentralReceivingEndpoint,
    owner_claim: &Value,
) -> Result<QualifiedReceivingClient, NativeCallFailure> {
    let failure = |message: String| NativeCallFailure::validation(message.clone(),json!({"stage":"hostOwnerBridge","failure":message,"processStarted":false}));
    if current.contract_revision != original.contract_revision
        || current.project != original.project
        || owner_claim["project"].as_str() != original.project.as_deref()
    {
        return Err(failure("host client replacement cannot change original owner project or contract".into()));
    }
    let current_root = HeldReceivingRoot::open(&current.root).map_err(failure)?;
    current_root.require_claim(owner_claim).map_err(failure)?;
    let mut selected = original.clone();
    selected.binary = current.binary.clone();
    // new() separately holds and verifies the ORIGINAL raw locator. A current
    // host at the old canonical path never excuses a retargeted original alias.
    let client = QualifiedReceivingClient::new(&selected, owner_claim)?;
    current_root.recheck().map_err(failure)?;
    Ok(client)
}
pub(crate) fn lookup_observed(client: &QualifiedReceivingClient, original: &Value) -> Result<Value, NativeCallFailure> {
    let described = client.call("action.describe",&json!({"action":"central.receiving.read"}))?;
    let data=&described["data"];
    let inputs=data["inputs"].as_array();
    let has = |name: &str, kind: &str| inputs.is_some_and(|fields| fields.iter().any(|field| field["name"]==name && field["type"]==kind && field["required"]==false));
    if data["id"]!="central.receiving.read" || data["mutation_class"]!="read-only" || data["availability"]["available"]!=true
        || data["output"]["type"]!="object" || !has("producer_key","string") || !has("original_request","object") || !has("return_ref","string") {
        return Err(NativeCallFailure::validation("actual Central descriptor does not support authenticated exact-original producer lookup".into(),json!({"stage":"lookupCapability","descriptorResponse":described,"outcome":"unavailable; no fallback or resend"})));
    }
    let input=lookup_input(&client.endpoint,original).map_err(|message|NativeCallFailure::validation(message.clone(),json!({"stage":"lookupInput","failure":message})))?;
    let response=client.call("central.receiving.read",&input).map_err(|mut failure| {failure.observation["descriptorResponse"]=described.clone();failure})?;
    validate_lookup(&response).map_err(|message|NativeCallFailure::validation(message.clone(),json!({"stage":"lookupGuard","descriptorResponse":described,"centralResponse":response,"failure":message})))?;
    Ok(response)
}
fn check_response(
    response: &Value,
    input: &Value,
    request: &ReceivingRequest,
    attempt: &FactoryAttemptRecord,
) -> Result<(), String> {
    check_native_response(response, input, &request.run_ref, attempt)
}
fn check_native_response(
    response: &Value,
    input: &Value,
    run_ref: &RunRef,
    attempt: &FactoryAttemptRecord,
) -> Result<(), String> {
    validate_original_request(response, input)?;
    let data = &response["data"];
    let received = &data["record"];
    if response["ok"] != true || response["status"] != "success"
        || !matches!(response["action"].as_str(), Some("central.receiving.submit" | "central.receiving.read"))
        || received["status"].as_str().is_none_or(|value| value.trim().is_empty())
        || data["included"] != json!(received["status"] == "included")
        || data["automatic_agent_or_model_invocation"] != false
        || data["schema"] != "central.receiving-reading/v1"
        || received["schema"] != "central.received-contribution/v1"
        || data["return_ref"]
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
        || data["revision"]
            .as_str()
            .is_none_or(|value| value.trim().is_empty())
        || received["return_ref"] != data["return_ref"]
        || received["source_ref"] != input["source_ref"]
        || received["document_id"] != input["document_id"]
        || received["proposed_source_revision"] != input["expected_source_revision"]
        || received["proposal"] != input["proposal"]
        || received["run_ref"] != json!(run_ref.to_string())
        || received["task_ref"] != json!(attempt.task_ref)
        || received["session_ref"] != json!(attempt.disposition.body.agent_session_ref)
        || received["author"]["principal_ref"] != json!(attempt.disposition.participant.agent_ref)
        || received["now_ref"] != input["now_ref"]
        || received["day_ref"] != input["day_ref"]
        || data["source_changed_by_arrival_or_review"] != false
    {
        return Err("Central receiving result omitted or changed native source/producer/task/session identity; retain as unresolved".into());
    }
    if attempt
        .readable_return
        .as_ref()
        .and_then(|returned| returned.receiving_ref.as_ref())
        .is_some_and(|reference| Some(reference.as_str()) != data["return_ref"].as_str())
    {
        return Err("a different receiving identity is already attached to this Return".into());
    }
    Ok(())
}

pub(crate) fn validate_closure_read(
    response: &Value,
    input: &Value,
    run_ref: &RunRef,
    attempt: &FactoryAttemptRecord,
) -> Result<(), String> {
    check_native_response(response, input, run_ref, attempt)?;
    let received = &response["data"]["record"];
    if input["proposal"] != return_proposal(run_ref, attempt)?
        || matches!(received["status"].as_str(), Some("rejected" | "cancelled"))
    {
        return Err("live native receiving does not retain this exact final Return proposal and source basis".into());
    }
    Ok(())
}
fn stamp(
    mut receipt: OwnerOperationReceipt,
    phase: OwnerOperationPhase,
    detail: Value,
) -> OwnerOperationReceipt {
    receipt.phase = phase;
    receipt.payload["detail"] = detail;
    receipt.receipt_ref = format!(
        "factory-receiving-call:{}",
        blake3::hash(
            &serde_json::to_vec(&(&receipt.operation_ref, phase, &receipt.payload))
                .expect("JSON value")
        )
        .to_hex()
    );
    receipt
}
fn result(
    store: &FileAttemptStore,
    request: &ReceivingRequest,
    observation: OwnerOperationReceipt,
    response: Option<Value>,
    failure: Option<CliError>,
    replayed: bool,
) -> Value {
    let reading = store.reading();
    let publication_uncertainty = failure
        .as_ref()
        .and_then(|error| crate::native_publication_uncertainty(error))
        .map(|publication| publication.details());
    let mut result = json!({"contract":RECEIVING_RECEIPT,"requestRef":request.request_ref,"runRef":request.run_ref,"attemptRef":request.attempt_ref,
        "replayed":replayed,"needsReconciliation":failure.is_some()||reading.is_err()||matches!(observation.phase,OwnerOperationPhase::Dispatching|OwnerOperationPhase::Uncertain),
        "transportObservation":observation,"centralResponse":response,"retentionError":failure.as_ref().map(|error|error.to_string()).or_else(||reading.as_ref().err().map(|error|error.to_string())),
        "reading":reading.ok(),"humanRecognitionPerformed":false,"documentInclusionPerformed":false});
    if let Some(details) = publication_uncertainty {
        result["publicationUncertainty"] =
            serde_json::to_value(details).expect("native publication details serialize");
    }
    result
}

pub fn execute(path: &Path, request: ReceivingRequest) -> Result<Value, CliError> {
    if request.contract != RECEIVING_ACTION
        || request.request_ref.trim().is_empty()
        || request.projection_ref.trim().is_empty()
        || !request.central.root.is_absolute()
        || request.central.contract_revision != CENTRAL_CONTRACT_REVISION
        || request.target.source_ref.trim().is_empty()
        || request.target.document_id.trim().is_empty()
        || request.target.source_revision.trim().is_empty()
        || request
            .central
            .project
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Err("receiving requires exact Factory, Central PR #155 revision, absolute root and selected source identities".into());
    }
    let mut store =
        FileAttemptStore::open_run(path, request.run_ref.clone()).map_err(CliError::from_native)?;
    let reading = store.reading().map_err(CliError::from_native)?;
    let attempt = record(&reading, &request.attempt_ref)?;
    let mut identity = serde_json::to_value(&request).map_err(CliError::from_native)?;
    for key in ["recover", "lookupEndpoint", "expectedRevision", "projectionRef"] {
        identity
            .as_object_mut()
            .expect("request object")
            .remove(key);
    }
    let digest = blake3::hash(&serde_json::to_vec(&identity).map_err(CliError::from_native)?)
        .to_hex()
        .to_string();
    let operation_ref = format!("factory-attempt-receiving:{}", request.request_ref);
    let previous = attempt
        .observations
        .iter()
        .rev()
        .find(|receipt| {
            receipt.owner_ref == "factory"
                && receipt.contract == CALL_CONTRACT
                && receipt.operation_ref == operation_ref
        })
        .cloned();
    if previous
        .as_ref()
        .is_some_and(|receipt| receipt.payload["requestDigest"].as_str() != Some(digest.as_str()))
    {
        return Err("receiving Action identity reused with changed native request".into());
    }
    let input = if let Some(previous) = &previous {
        previous.payload["nativeRequest"].clone()
    } else {
        submission(&request, &store.receiving_attempt_basis(&request.attempt_ref).map_err(CliError::from_native)?)?
    };
    let intent = stamp(
        OwnerOperationReceipt {
            owner_ref: "factory".into(),
            contract: CALL_CONTRACT.into(),
            operation_ref,
            receipt_ref: String::new(),
            source_revision: format!("factory-state:{}", reading.revision),
            phase: OwnerOperationPhase::Dispatching,
            evidence_refs: BTreeSet::new(),
            partial_effect_refs: BTreeSet::new(),
            payload: json!({"requestDigest":digest,"nativeRequest":input,"centralContractRevision":CENTRAL_CONTRACT_REVISION,"attemptRevision":reading.revision}),
        },
        OwnerOperationPhase::Dispatching,
        json!({"meaning":"receiving call intent; not worker execution, inclusion or Recognition"}),
    );
    crate::attempt_runtime::validate_action_request(&action(
        &request,
        reading.revision,
        "admission",
        FactoryAttemptOperation::RecordObservation {
            attempt_ref: request.attempt_ref.clone(),
            receipt: intent.clone(),
        },
    ))
    .map_err(CliError::from_native)?;
    if let Some(previous) = &previous {
        if !request.recover {
            let response = previous
                .payload
                .pointer("/detail/centralResponse")
                .filter(|value| !value.is_null())
                .cloned();
            let mut retained = result(&store, &request, previous.clone(), None, None, true);
            retained["retainedUnverifiedResponse"] = json!(response);
            retained["needsReconciliation"] = json!(true);
            retained["standing"] = json!("retained receiving history; no current native delivery certification; explicit recover performs guarded owner lookup");
            return Ok(retained);
        }
    }
    let known_receipt = attempt.readable_return.as_ref().and_then(|returned| returned.receiving_ref.as_ref()).cloned();
    let retention = action(&request, request.expected_revision, "intent", FactoryAttemptOperation::RecordObservation {
        attempt_ref: request.attempt_ref.clone(), receipt: intent.clone(),
    });
    let claim = ReceivingIntentClaim::new(&request.central, &input, Some((
        intent.operation_ref.clone(), digest.clone(),
    ))).map_err(CliError::from_native)?;
    let claim = if request.recover { claim.lookup_only() } else { claim };
    let admission = NativeReceivingObservationAdmission::new(&retention, Some(claim)).map_err(CliError::from_native)?;
    let claimed = store.apply_native_receiving_observation(retention, admission).map_err(CliError::from_native)?;
    let inserted = claimed.inserted;
    let intent = claimed.receiving_intent.ok_or("canonical receiving claim omitted its original intent")?;
    let original = intent.payload["nativeRequest"].clone();
    if !original.is_object() { return Err("retained native receiving input is unavailable".into()); }
    let endpoint = match intent.payload.get("hostEndpoint") {
        Some(value) => serde_json::from_value::<CentralReceivingEndpoint>(value.clone()).map_err(CliError::from_native)?,
        None => request.central.clone(), // private claim checked exact legacy request digest; no state backfill
    };
    let (operation, call_input) = if inserted {
        ("central.receiving.submit", original.clone())
    } else {
        ("central.receiving.read", lookup_input(&endpoint, &original)?)
    };
    let input = original;
    // Hold the selected client and ORIGINAL root until all response admission.
    let (qualified, called) = match lookup_endpoint(&endpoint, if inserted { None } else { request.lookup_endpoint.as_ref() }, &intent.payload["ownerClaim"]) {
        Ok(client) => { let called = if inserted {client.call(operation,&call_input)} else {lookup_observed(&client,&input)}; (Ok(client),called) },
        Err(failure) => (Err(()),Err(failure)),
    };
    let response = match called {
        Ok(response) => response,
        Err(failure) => {
            let uncertain = stamp(
                intent,
                OwnerOperationPhase::Uncertain,
                json!({"failure":failure.message,"nativeCapture":failure.observation.clone(),"nativeAction":operation,"nativeInput":call_input,"recovery":"guarded native lookup of the original input; absence/error remains unknown; never resend"}),
            );
            let retention = retain(
                &mut store,
                &request,
                "uncertain",
                FactoryAttemptOperation::RecordObservation {
                    attempt_ref: request.attempt_ref.clone(),
                    receipt: uncertain.clone(),
                },
            )
            .err();
            let actual_response = failure.observation.get("centralResponse").filter(|value| !value.is_null()).cloned();
            if let Some(retention) = retention {
                let mut unresolved = result(&store,&request,uncertain,actual_response,None,false);
                unresolved["retentionError"] = json!(retention.to_string());
                if let Some(cause) = crate::native_publication_uncertainty(&retention) {
                    unresolved["publicationUncertainty"] = serde_json::to_value(cause.details()).map_err(CliError::from_native)?;
                }
                return Err(NativeReceivingRetentionFailure::into_cli(failure,retention,unresolved));
            }
            return Ok(result(&store, &request, uncertain, actual_response, None, false));
        }
    };
    let publication = (|| -> Result<(), CliError> {
        qualified.as_ref().map_err(|_| CliError::new("native receiving client qualification is unavailable"))?.recheck()?;
        let current = store.reading().map_err(CliError::from_native)?;
        let attempt = record(&current, &request.attempt_ref)?;
        if !inserted { validate_lookup(&response)?; }
        check_response(&response, &input, &request, attempt)?;
        if known_receipt.as_deref().is_some_and(|known| response["data"]["return_ref"].as_str() != Some(known)) {
            return Err("native lookup changed the already retained receiving ref".into());
        }
        let data = &response["data"];
        let receipt_ref = data["return_ref"]
            .as_str()
            .ok_or("missing Central receiving ref")?
            .to_owned();
        let revision = data["revision"]
            .as_str()
            .ok_or("missing Central receipt revision")?
            .to_owned();
        let evidence = BTreeSet::from([receipt_ref.clone(), request.target.source_ref.clone()]);
        let owner = OwnerOperationReceipt {
            owner_ref: "central".into(),
            contract: "central.receiving-reading/v1".into(),
            operation_ref: format!("central.receiving:{receipt_ref}"),
            receipt_ref: format!(
                "central-receiving-observation:{}",
                blake3::hash(&serde_json::to_vec(&response).map_err(CliError::from_native)?)
                    .to_hex()
            ),
            source_revision: revision.clone(),
            phase: OwnerOperationPhase::Observed,
            evidence_refs: evidence.clone(),
            partial_effect_refs: BTreeSet::new(),
            payload: response.clone(),
        };
        qualified.as_ref().map_err(|_| CliError::new("native receiving client qualification is unavailable"))?.recheck()?;
        retain(
            &mut store,
            &request,
            "owner-receipt",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: owner,
            },
        )?;
        let current = store.reading().map_err(CliError::from_native)?;
        let attached = record(&current, &request.attempt_ref)?
            .readable_return
            .as_ref()
            .and_then(|returned| returned.receiving_ref.as_ref())
            .is_some();
        // The original receiving basis stays immutable. Subsequent review or
        // inclusion observations are new owner receipts, not rewritten history.
        if !attached {
            qualified.as_ref().map_err(|_| CliError::new("native receiving client qualification is unavailable"))?.recheck()?;
            retain(
                &mut store,
                &request,
                "attach",
                FactoryAttemptOperation::AttachReceiving {
                    attempt_ref: request.attempt_ref.clone(),
                    receiving_ref: receipt_ref,
                    source_revision: revision,
                    evidence_refs: evidence,
                },
            )?;
        }
        qualified.as_ref().map_err(|_| CliError::new("native receiving client qualification is unavailable"))?.recheck()?;
        Ok(())
    })();
    let phase = if publication.is_ok() {
        OwnerOperationPhase::Observed
    } else {
        OwnerOperationPhase::Uncertain
    };
    let mut failure = publication.err();
    let primary_failure = failure.as_ref().map(|error|error.to_string());
    let mut secondary_failure = None;
    let settled = stamp(
        intent,
        phase,
        json!({"centralResponse":response,"physicalQualification":qualified.as_ref().ok().map(|client|client.evidence()),"nativeAction":operation,"nativeInput":call_input,"failure":failure.as_ref().map(|error|error.to_string()),"meaning":"owner receiving evidence; no document inclusion or human Recognition"}),
    );
    // A different revision does not justify another write after unknown
    // publication on this same attempt source.
    let publication_uncertain = failure
        .as_ref()
        .is_some_and(|error| crate::native_publication_uncertainty(error).is_some());
    if !publication_uncertain {
        if let Err(error) = retain(
            &mut store,
            &request,
            "settled",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: settled.clone(),
            },
        ) {
            secondary_failure = Some(error.to_string());
            if failure.is_none() || crate::native_publication_uncertainty(&error).is_some() {
                failure = Some(error);
            }
        }
    }
    let mut returned = result(&store,&request,settled,Some(response),failure,false);
    returned["failureCauses"] = json!({"primary":primary_failure,"retention":secondary_failure});
    Ok(returned)
}

pub fn execute_cli(args: &[String], stdin: Option<&str>) -> Result<String, CliError> {
    let args = args
        .iter()
        .filter(|argument| argument.as_str() != "--json")
        .collect::<Vec<_>>();
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        return Ok(format!("factory attempt receiving <native-state> <request-json|-> [--json]\nAction: {RECEIVING_ACTION}\nCentral PR #155 source: {CENTRAL_CONTRACT_REVISION}\nThe host supplies CENTRAL_NATIVE_TOKEN, never JSON. Submit proposes a retained Return; recover explicitly reads the authenticated original producer key and exact original input; unknown outcomes never resend. This does not review/include a human document."));
    }
    if args.len() > 2 {
        return Err("unexpected receiving command arguments".into());
    }
    let input = args.get(1).map(|value| value.as_str()).unwrap_or("-");
    let body = if input != "-" {
        std::fs::read_to_string(input).map_err(CliError::from_native)?
    } else if let Some(body) = stdin {
        body.into()
    } else {
        let mut body = String::new();
        std::io::stdin()
            .read_to_string(&mut body)
            .map_err(CliError::from_native)?;
        body
    };
    let request = serde_json::from_str(&body).map_err(CliError::from_native)?;
    serde_json::to_string_pretty(&execute(Path::new(args[0]), request)?)
        .map_err(CliError::from_native)
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod publication_tests {
    use super::*;
    use crate::attempt_learning::publication_tests::{
        change_actual_published_privacy, native_observation, native_retention_source,
    };
    #[test]
    fn actual_receiving_retention_keeps_committed_uncertainty_without_revision_retry() {
        let (root, mut store, base) = native_retention_source();
        let before = store.reading().unwrap().revision;
        let request = ReceivingRequest {
            contract: RECEIVING_ACTION.into(),
            request_ref: base.request_ref,
            projection_ref: base.projection_ref,
            caller: base.caller,
            run_ref: base.run_ref,
            expected_revision: before,
            authority: base.authority,
            attempt_ref: base.attempt_ref,
            central: CentralReceivingEndpoint {
                binary: std::env::current_exe().unwrap(),
                root: root.path().to_path_buf(),
                contract_revision: CENTRAL_CONTRACT_REVISION.into(),
                project: None,
            },
            target: ReceivingTarget {
                source_ref: "factory:native-source".into(),
                document_id: "factory:native-source".into(),
                source_revision: format!("factory-state:{before}"),
                expected_authority_revision: None,
            },
            lookup_endpoint: None,
            occurred_at_unix_seconds: None,
            recover: false,
        };
        let observation = native_observation(&store);
        change_actual_published_privacy();
        let error = retain(
            &mut store,
            &request,
            "actual-retention",
            FactoryAttemptOperation::RecordObservation {
                attempt_ref: request.attempt_ref.clone(),
                receipt: observation.clone(),
            },
        )
        .unwrap_err();
        assert!(crate::native_publication_uncertainty(&error).is_some());
        assert_eq!(store.reading().unwrap().revision, before + 1);
        let returned = result(&store, &request, observation, None, Some(error), false);
        assert_eq!(returned["publicationUncertainty"]["published"], true);
        assert_eq!(returned["needsReconciliation"], true);
        assert!(
            returned["centralResponse"].is_null(),
            "no Central call or fabricated owner result"
        );
    }
}


#[cfg(all(test,unix))]
mod capture_consumer_os_tests {
    use super::*;
    use std::error::Error;
    #[test]
    fn actual_missing_client_keeps_original_capture_io_and_safe_debug() {
        let root=tempfile::tempdir().unwrap();
        let endpoint=CentralReceivingEndpoint {binary:root.path().join("absent-native-client"),root:root.path().to_owned(),
            project:None,contract_revision:CENTRAL_CONTRACT_REVISION.into()};
        let failure=call_observed_bounded(&endpoint,"central.receiving.read",&json!({}),Duration::from_secs(2)).unwrap_err();
        let original=failure.source().unwrap().downcast_ref::<std::io::Error>().unwrap();
        let capture=crate::native_process::capture_failure(original).unwrap();
        assert_eq!(capture.cause().kind(),std::io::ErrorKind::NotFound);
        assert!(!capture.process_started());assert!(capture.status().is_none());
        assert_eq!(failure.observation()["processStarted"],false);assert!(failure.observation()["centralResponse"].is_null());
        let error=crate::attempt_runtime::FactoryAttemptError::NativeReceiving(Box::new(failure));
        assert!(error.source().unwrap().downcast_ref::<NativeCallFailure>().is_some());
        assert!(!format!("{error:?}").contains("retainedPrefix"));
    }
    #[test]
    fn actual_os_invalid_prefix_and_timeout_never_become_owner_reply_or_broad_debug() {
        use std::os::unix::fs::PermissionsExt;
        let root=tempfile::tempdir().unwrap();let binary=root.path().join("actual-stream-only-client");
        // This is an OS byte/deadline test, not a native Central response fixture.
        // No successful ActionResult is emitted or accepted.
        std::fs::write(&binary,b"#!/bin/sh\nprintf '\\377capture-private-os-marker'\nexec /bin/sleep 2\n").unwrap();
        std::fs::set_permissions(&binary,std::fs::Permissions::from_mode(0o700)).unwrap();
        let endpoint=CentralReceivingEndpoint {binary,root:root.path().to_owned(),project:None,contract_revision:CENTRAL_CONTRACT_REVISION.into()};
        let failure=call_observed_bounded(&endpoint,"central.receiving.read",&json!({}),Duration::from_millis(250)).unwrap_err();
        let original=failure.source().unwrap().downcast_ref::<std::io::Error>().unwrap();
        let capture=crate::native_process::capture_failure(original).unwrap();
        assert!(capture.process_started());assert!(capture.timed_out());assert!(capture.stdout().starts_with(b"\xffcapture-private-os-marker"));
        assert_eq!(failure.observation()["stdout"]["retainedPrefix"][0],255);
        assert!(failure.observation()["centralResponse"].is_null());
        assert!(!format!("{failure:?} {failure}").contains("capture-private-os-marker"));
        let receipt=OwnerOperationReceipt {owner_ref:"factory".into(),contract:CALL_CONTRACT.into(),operation_ref:"actual-os-capture".into(),
            receipt_ref:"actual-os-capture".into(),source_revision:"test-os-source".into(),phase:OwnerOperationPhase::Uncertain,
            evidence_refs:BTreeSet::new(),partial_effect_refs:BTreeSet::new(),payload:failure.observation.clone()};
        assert!(!format!("{receipt:?}").contains("capture-private-os-marker"));
        assert_eq!(serde_json::to_value(&receipt).unwrap()["payload"]["stdout"]["retainedPrefix"][0],255);
    }
}

/// FCI06 uses actual Central CLI/auth/document/Receiving code in its opt-in
/// cfg(test) child and the SAME Factory private transport consumer. It is not
/// an Original Run, model result, human-world review or installed-child claim.
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod paired_inclusion_native_tests {
    use super::*;
    use std::error::Error;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::time::{SystemTime, UNIX_EPOCH};

    // Public controlled test credentials are confined to freshly initialized
    // isolated roots. No personal credential is read or copied. The exact H
    // token must be supplied to the selected gate's process, not global-set by
    // a library test; all other native setup calls use per-Command env below.
    const HUMAN: &str = "factory-fci06-controlled-human-fixture-not-personal";
    const AGENT: &str = "factory-fci06-controlled-agent-fixture-not-personal";
    const HUMAN_REF: &str = "human:factory-fci06-controlled-reviewer";
    const AGENT_REF: &str = "agent:factory-fci06-controlled-producer";
    const BODY: &str = "<p>FCI06 actual isolated native document contribution.</p>";
    const PARENT_GATE: &str = "actual_central_post_document_and_secondary_receiving_failures_reach_same_factory_transport";

    struct PinnedNative {
        path: PathBuf,
        held: File,
        identity: (u64, u64),
        sha256: String,
    }
    impl PinnedNative {
        fn load(path_var: &str, sha_var: &str) -> Self {
            let locator = PathBuf::from(std::env::var_os(path_var).expect("selected FCI06 gate requires each actual native binary"));
            assert!(locator.is_absolute(), "native binary pin must be absolute");
            let path = locator.canonicalize().unwrap();
            let held = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&path).unwrap();
            let metadata = held.metadata().unwrap();
            assert!(metadata.is_file() && metadata.mode() & 0o111 != 0);
            assert!(metadata.len() <= 128 * 1024 * 1024);
            let sha256 = std::env::var(sha_var).expect("actual native SHA256 pin is mandatory; no fallback");
            assert!(sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
            let result = Self { path, held, identity:(metadata.dev(),metadata.ino()), sha256 };
            result.recheck();
            let mut header = [0_u8;4];
            let mut reader = result.held.try_clone().unwrap();
            reader.seek(SeekFrom::Start(0)).unwrap();
            reader.read_exact(&mut header).unwrap();
            assert!(header == *b"\x7fELF" || header == [0xcf,0xfa,0xed,0xfe] || header == [0xca,0xfe,0xba,0xbe],
                "requires a genuine native executable, no script/client double");
            result
        }
        fn recheck(&self) {
            let before = self.held.metadata().unwrap();
            let named = fs::symlink_metadata(&self.path).unwrap();
            assert!(!named.file_type().is_symlink() && named.is_file());
            assert_eq!((before.dev(),before.ino()),self.identity);
            assert_eq!((named.dev(),named.ino()),self.identity);
            assert!(before.len() <= 128 * 1024 * 1024 && before.mode() & 0o111 != 0);
            let mut reader = self.held.try_clone().unwrap();
            reader.seek(SeekFrom::Start(0)).unwrap();
            let mut buffer = [0_u8;64 * 1024];
            let mut digest = Sha256::new();
            let mut bytes = 0_u64;
            loop { let read = reader.read(&mut buffer).unwrap(); if read == 0 { break; }
                bytes += read as u64; assert!(bytes <= 128 * 1024 * 1024); digest.update(&buffer[..read]); }
            let after = self.held.metadata().unwrap();
            let named_after = fs::symlink_metadata(&self.path).unwrap();
            assert!(named_after.is_file() && !named_after.file_type().is_symlink());
            assert_eq!((named_after.dev(),named_after.ino()),self.identity);
            assert_eq!(bytes,before.len());
            assert_eq!((after.len(),after.mtime(),after.mtime_nsec(),after.ctime(),after.ctime_nsec()),
                (before.len(),before.mtime(),before.mtime_nsec(),before.ctime(),before.ctime_nsec()));
            assert_eq!((named_after.len(),named_after.mode(),named_after.uid(),named_after.gid()),
                (after.len(),after.mode(),after.uid(),after.gid()));
            assert_eq!(format!("{:x}",digest.finalize()),self.sha256,"native selected image changed");
        }
        fn facts(&self) -> Value {
            json!({"path":self.path,"sha256":self.sha256,"device":self.identity.0,"inode":self.identity.1})
        }
    }
    struct OwnedRoot {
        path: PathBuf,
        held: File,
        identity: (u64,u64),
    }
    impl OwnedRoot {
        fn create(parent: &Path, mode: &str) -> Self {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let path = parent.join(format!("fci06-{mode}-{}-{stamp}",std::process::id()));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path,fs::Permissions::from_mode(0o700)).unwrap();
            let held = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&path).unwrap();
            let metadata = held.metadata().unwrap();
            let root = Self { path, held, identity:(metadata.dev(),metadata.ino()) };
            root.recheck();root
        }
        fn recheck(&self) {
            let held = self.held.metadata().unwrap();
            let named = fs::symlink_metadata(&self.path).unwrap();
            assert!(held.is_dir() && named.is_dir() && !named.file_type().is_symlink());
            assert_eq!((held.dev(),held.ino()),self.identity);
            assert_eq!((named.dev(),named.ino()),self.identity);
            assert_eq!(named.uid(),unsafe {libc::geteuid()});
            assert_eq!(named.mode() & 0o777,0o700);
        }
        fn write_new(&self, name: &str, bytes: &[u8]) {
            self.recheck();
            let path = self.path.join(name);
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&path).unwrap();
            file.write_all(bytes).unwrap();file.sync_all().unwrap();self.held.sync_all().unwrap();
            assert_eq!(fs::read(path).unwrap(),bytes);self.recheck();
        }
    }
    // Fixtures are intentionally retained in the admitted evidence directory on
    // success and failure. No automatic recursive cleanup can delete a changed
    // locator/foreign object; child modes restore only held owned permissions.
    fn native_call(binary: &PinnedNative, root: &OwnedRoot, action: &str, input: &Value, token: Option<&str>, label: &str) -> Value {
        binary.recheck();root.recheck();
        let body = serde_json::to_string(input).unwrap();
        let mut command = Command::new(&binary.path);
        command.args(["--json","--root"]).arg(&root.path).args(["action","run",action]).arg(&body)
            .env_remove("CENTRAL_NATIVE_TOKEN").env_remove("CENTRAL_ROOT");
        if let Some(token) = token { command.env("CENTRAL_NATIVE_TOKEN",token); }
        let output = crate::native_process::output(&mut command,Duration::from_secs(20)).unwrap();
        binary.recheck();root.recheck();
        root.write_new(&format!("{label}.stdout"),&output.stdout);
        root.write_new(&format!("{label}.stderr"),&output.stderr);
        root.write_new(&format!("{label}.invocation.json"),&serde_json::to_vec_pretty(&json!({
            "argv":[binary.path.display().to_string(),"--json","--root",root.path.display().to_string(),
                "action","run",action,body],"exitCode":output.status.code(),
            "stdoutEof":true,"stderrEof":true,"clientReaped":true,"sha256":binary.sha256})).unwrap());
        assert!(output.status.success(),"actual prerequisite native action did not succeed; raw evidence retained");
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["ok"],true);assert_eq!(response["status"],"success");assert_eq!(response["action"],action);
        response["data"].clone()
    }
    fn write_fixture_authority(root: &OwnedRoot) {
        fs::create_dir_all(root.path.join("Control/relations")).unwrap();
        fs::create_dir_all(root.path.join("Work/fci06")).unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let actions = ["central.document.create","central.document.mutate","central.receiving.submit", "central.receiving.review","central.receiving.include"];
        let grants = [(HUMAN,HUMAN_REF,"human"),(AGENT,AGENT_REF,"agent")].into_iter()
            .map(|(token,actor,kind)| json!({"principal_ref":actor,"actor_kind":kind,
                "token_sha256":format!("{:x}",Sha256::digest(token.as_bytes())),"scope_refs":["control:root"],
                "actions":actions,"expires_at_unix_seconds":now+600})).collect::<Vec<_>>();
        let sources = [
            ("placement.json","work-placement-policy",json!({"schema":"central.work-placement-policy/v1","scope_ref":"control:root",
                "writable":[{"path":"Work/fci06","class":"repository"}],"enforcement":"native-actions","required_coverage":["file-content"],"lease_seconds":300})),
            ("time.json","civil-time-policy",json!({"schema":"central.civil-time-policy/v1","scope_ref":"control:root",
                "timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true})),
            ("authority.json","native-action-authority",json!({"schema":"central.native-action-authority/v1","scope_ref":"control:root","grants":grants}))];
        let relations = sources.into_iter().map(|(name,role,value)| {
            let member = format!("Control/user/{name}");
            let path = root.path.join(&member);
            assert!(!path.exists());
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).unwrap();
            file.write_all(serde_json::to_vec_pretty(&value).unwrap().as_slice()).unwrap();file.sync_all().unwrap();
            json!({"ref":format!("central:source:control:root:{member}"),"path":member,"roles":[role],
                "provenance":"human-adopted","standing":"architecture-contract","treatment":"projectcentral-user",
                "recognition":"controlled-native-fixture-not-personal-adoption","recorded_at_unix_seconds":now})
        }).collect::<Vec<_>>();
        let relations_path = root.path.join("Control/relations/source-relations.json");
        // Native init can have created the empty relation source. This fixture
        // declares isolated authored authority, never a copied personal World.
        fs::write(&relations_path,serde_json::to_vec_pretty(&json!({"schema":"central.control.ground-relations/v1",
            "project_id":"control:root","relations":relations})).unwrap()).unwrap();
        File::open(&relations_path).unwrap().sync_all().unwrap();root.held.sync_all().unwrap();root.recheck();
    }
    fn ledger_bytes(root: &OwnedRoot) -> Vec<(PathBuf,Vec<u8>)> {
        let mut result = fs::read_dir(root.path.join(".central/source-returns/contributions")).unwrap()
            .map(|entry| { let path = entry.unwrap().path(); let metadata = fs::symlink_metadata(&path).unwrap();
                assert!(metadata.is_file() && !metadata.file_type().is_symlink());(path.clone(),fs::read(path).unwrap()) })
            .collect::<Vec<_>>();
        result.sort_by(|left,right|left.0.cmp(&right.0));result
    }
    #[test]
    #[ignore = "requires exact qualified ordinary Ctrl and cfg(test) Central child plus private allocated evidence directory and controlled fixture token"]
    fn actual_central_post_document_and_secondary_receiving_failures_reach_same_factory_transport() {
        assert_ne!(unsafe {libc::geteuid()},0,"genuine native EACCES requires actual nonroot execution");
        assert!(std::env::var_os("CENTRAL_NATIVE_TOKEN").expect("explicit paired gate requires its controlled fixture token") == std::ffi::OsStr::new(HUMAN),
            "never forward a personal credential to the controlled World");
        let declared_source = std::env::var("FACTORY_FCI06_CENTRAL_SOURCE_MANIFEST_SHA256").expect("paired child compiler Source manifest must be explicitly pinned");
        assert!(declared_source.len()==64 && declared_source.bytes().all(|byte|byte.is_ascii_hexdigit()));
        let ordinary = PinnedNative::load("FACTORY_TEST_CTRL","FACTORY_TEST_CTRL_SHA256");
        let child = PinnedNative::load("FACTORY_FCI06_CHILD","FACTORY_FCI06_CHILD_SHA256");
        assert_ne!(ordinary.path,child.path,"normal Ctrl and cfg(test) child must stay distinct");
        assert_ne!(ordinary.identity,child.identity);
        let parent_locator = PathBuf::from(std::env::var_os("FACTORY_FCI06_ARTIFACT_DIR").expect("paired gate requires existing admitted evidence directory"));
        assert!(parent_locator.is_absolute());
        let parent = parent_locator.canonicalize().unwrap();assert_eq!(parent,parent_locator);
        let parent_hold = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&parent).unwrap();
        let parent_identity = parent_hold.metadata().unwrap();
        assert_eq!(parent_identity.uid(),unsafe {libc::geteuid()});
        assert_eq!(parent_identity.mode() & 0o077,0,"paired evidence directory must be private");
        let version_output = crate::native_process::output(Command::new(&ordinary.path).arg("--version"),Duration::from_secs(10)).unwrap();
        assert!(version_output.status.success());
        let version = String::from_utf8(version_output.stdout).unwrap();assert!(version.starts_with("ctrl "));
        for mode in ["committed-ledger-denied","readonly-document-ledger-denied"] {
            let root = OwnedRoot::create(&parent,mode);
            ordinary.recheck();
            let init = crate::native_process::output(Command::new(&ordinary.path).args(["--json","--root"])
                .arg(&root.path).arg("init").env_remove("CENTRAL_NATIVE_TOKEN"),Duration::from_secs(20)).unwrap();
            root.write_new("native-init.stdout",&init.stdout);root.write_new("native-init.stderr",&init.stderr);
            assert!(init.status.success());let initialized:Value=serde_json::from_slice(&init.stdout).unwrap();assert_eq!(initialized["ok"],true);
            write_fixture_authority(&root);
            let policy = native_call(&ordinary,&root,"central.work.policy",&json!({}),None,"01-policy");
            let document = native_call(&ordinary,&root,"central.document.create",&json!({"kind":"flow",
                "document_id":format!("doc:factory-fci06-{mode}"),"title":"FCI06 controlled native transport",
                "expected_policy_revision":policy["revision"],"fields":[]}),Some(HUMAN),"02-document-create");
            let original = json!({"producer_key":format!("producer:factory-fci06-{mode}"),
                "source_ref":document["source"]["ref"],"document_id":document["document_id"],
                "expected_source_revision":document["revision"]["revision"],"occurred_at_unix_seconds":42,
                "proposal":{"operation":"entry.add","entry_id":format!("entry:{mode}"),"contribution_id":format!("part:{mode}"),"html":BODY}});
            let submitted = native_call(&ordinary,&root,"central.receiving.submit",&original,Some(AGENT),"03-submit");
            let accepted = native_call(&ordinary,&root,"central.receiving.review",&json!({"return_ref":submitted["return_ref"],
                "expected_return_revision":submitted["revision"],"expected_source_revision":document["revision"]["revision"],
                "disposition":"accepted"}),Some(HUMAN),"04-review");
            let source_path = root.path.join(document["source"]["path"].as_str().unwrap());
            let source_before = fs::read(&source_path).unwrap();
            let source_metadata = fs::symlink_metadata(&source_path).unwrap();
            assert!(source_metadata.is_file() && !source_metadata.file_type().is_symlink());
            let ledger_path = root.path.join(".central/source-returns/contributions");
            let ledger_mode = fs::metadata(&ledger_path).unwrap().mode() & 0o7777;
            let cursor_before_include = fs::read(ledger_path.join("cursor.json")).unwrap();
            let include = json!({"return_ref":accepted["return_ref"],"expected_return_revision":accepted["revision"],
                "expected_source_revision":document["revision"]["revision"]});
            root.write_new(".fci06-native-admission.json",&serde_json::to_vec_pretty(&json!({"schema":"central.fci06-native-admission/v1",
                "mode":mode,"root_device":root.identity.0,"root_inode":root.identity.1,
                "source_device":source_metadata.dev(),"source_inode":source_metadata.ino(),
                "source_sha256":format!("{:x}",Sha256::digest(&source_before)),"source_ref":document["source"]["ref"],
                "return_ref":accepted["return_ref"],"include_request_sha256":format!("{:x}",Sha256::digest(serde_json::to_string(&include).unwrap().as_bytes()))})).unwrap());
            let endpoint = CentralReceivingEndpoint { binary:child.path.clone(),root:root.path.clone(),project:None,
                contract_revision:CENTRAL_CONTRACT_REVISION.into() };
            child.recheck();root.recheck();
            let failure = call_observed_bounded(&endpoint,"central.receiving.include",&include,Duration::from_secs(20)).unwrap_err();
            child.recheck();ordinary.recheck();root.recheck();
            let observation = failure.observation();
            root.write_new("factory-actual-transport-observation.json",&serde_json::to_vec_pretty(observation).unwrap());
            assert!(failure.source().is_none(),"actual complete native non-ok is not fabricated host IO");
            assert_eq!(failure.secondary_causes().count(),0);
            assert_eq!(observation["stage"],"nativeReply");
            assert_eq!(observation["processStarted"],true);assert_eq!(observation["clientReaped"],true);
            assert_eq!(observation["stdoutEof"],true);assert_eq!(observation["stderrEof"],true);
            assert_eq!(observation["stdoutTruncated"],false);assert_eq!(observation["stderrTruncated"],false);
            assert_eq!(observation["stdout"]["retainedPrefixComplete"],true);
            assert_eq!(observation["stderr"]["retainedPrefixComplete"],true);
            let actual_stdout = observation["stdout"]["retainedPrefix"].as_array().unwrap().iter()
                .map(|value|u8::try_from(value.as_u64().unwrap()).unwrap()).collect::<Vec<_>>();
            assert_eq!(observation["stdout"]["byteLength"],actual_stdout.len());
            assert_eq!(observation["stdout"]["sha256"],format!("{:x}",Sha256::digest(&actual_stdout)));
            assert!(observation["exitCode"].as_i64().is_some_and(|code|code != 0 && code != 78),"fixture setup/restoration failure must not pass as native partial completion");
            let response = &observation["centralResponse"];
            assert_eq!(&serde_json::from_slice::<Value>(&actual_stdout).unwrap(),response,
                "only the complete actual native output is the retained response");
            root.write_new("paired-native-include.stdout",&actual_stdout);
            assert_eq!(response["ok"],false);assert_eq!(response["status"],"partial_completion");
            assert_eq!(response["action"],"central.receiving.include");
            assert_eq!(response["error"]["code"],"central.receiving.inclusion_incomplete");
            let details = &response["error"]["details"];
            assert_eq!(details["automatic_retry"],false);assert_eq!(details["outcome"],"unknown");
            assert_eq!(details["receiving"]["return_ref"],submitted["return_ref"]);
            assert_eq!(details["receiving"]["prior_acknowledged"]["status"],"including");
            assert_eq!(details["receiving"]["persistence"],"unconfirmed");
            assert!(details["receiving"]["update_acknowledged"].is_null());
            assert_eq!(details["causes"]["receiving_write"]["kind"],"PermissionDenied");
            assert_eq!(details["causes"]["receiving_write"]["raw_os_error"],libc::EACCES);
            let physical:Value = serde_json::from_slice(&fs::read(root.path.join(".fci06-native-observation.json")).unwrap()).unwrap();
            assert_eq!(physical["mode"],mode);assert_eq!(physical["checkpoint_fired"],true);
            assert_eq!(physical["owned_permissions_restored"],true);
            assert_eq!(physical["creation_error_kind"],"PermissionDenied");
            assert_eq!(physical["creation_raw_os_error"],libc::EACCES);
            assert_eq!(physical["including_revision"],details["receiving"]["prior_acknowledged"]["revision"]);
            assert_eq!(fs::metadata(&ledger_path).unwrap().mode() & 0o7777,ledger_mode);
            assert_eq!(fs::metadata(&source_path).unwrap().mode() & 0o7777,source_metadata.mode() & 0o7777);
            let ledger_before_reads = ledger_bytes(&root);
            let by_ref = native_call(&ordinary,&root,"central.receiving.read",&json!({"return_ref":submitted["return_ref"]}),None,"05-independent-by-ref");
            let lookup = native_call(&ordinary,&root,"central.receiving.read",&json!({"producer_key":original["producer_key"],
                "original_request":original}),Some(AGENT),"06-independent-original-lookup");
            assert_eq!(by_ref["record"],lookup["record"]);assert_eq!(by_ref["revision"],lookup["revision"]);
            assert_eq!(lookup["lookup"]["original_request_verified"],true);
            assert_eq!(lookup["lookup"]["selector"],"authenticated_producer_key");
            assert_eq!(by_ref["record"]["status"],"including");
            assert_eq!(by_ref["included"],false);
            assert_eq!(by_ref["record"]["author"]["principal_ref"],AGENT_REF);
            assert_eq!(by_ref["revision"],physical["including_revision"]);
            assert!(by_ref["record"]["inclusion_request"].is_object());
            assert_eq!(by_ref["record"]["inclusion_request"]["source_ref"],document["source"]["ref"]);
            assert_eq!(by_ref["record"]["inclusion_request"]["document_id"],document["document_id"]);
            assert_eq!(by_ref["record"]["inclusion_request"]["expected_revision"],document["revision"]["revision"]);
            assert_eq!(by_ref["record"]["inclusion_request"]["request_id"],format!("include:{}",submitted["return_ref"].as_str().unwrap()));
            assert_eq!(fs::read(ledger_path.join("cursor.json")).unwrap(),cursor_before_include,
                "inclusion and native read cannot create a second arrival/cursor");
            assert_eq!(by_ref["record"]["request_digest"],format!("{:x}",Sha256::digest(serde_json::to_string(&original).unwrap().as_bytes())));
            assert_eq!(ledger_before_reads,ledger_bytes(&root),"actual native reads must not rewrite retained intent");
            let after = native_call(&ordinary,&root,"central.document.read",&json!({"source_ref":document["source"]["ref"],
                "document_id":document["document_id"]}),None,"07-independent-document-read");
            if mode == "committed-ledger-denied" {
                assert_eq!(details["document"]["outcome"],"committed");assert!(details["causes"]["document"].is_null());
                assert_eq!(details["receiving"]["attempted_status"],"included");
                assert_eq!(details["document"]["operation_receipt"]["revision"],after["revision"]["revision"]);
                assert_ne!(after["revision"],document["revision"]);
                assert_eq!(after["document"]["operations"].as_array().unwrap().len(),1);
                assert_eq!(after["document"]["contributions"][0]["author_ref"],AGENT_REF);
                assert!(fs::read(&source_path).unwrap() != source_before);
            } else {
                assert_eq!(details["document"]["outcome"],"unconfirmed");
                assert_eq!(details["causes"]["document"]["kind"],"PermissionDenied");
                assert!(details["causes"]["document"]["raw_os_error"].is_null(),"genuine native readonly preflight has no invented document errno");
                assert_eq!(details["receiving"]["attempted_status"],"uncertain");
                assert_eq!(after["revision"],document["revision"]);
                assert_eq!(after["document"]["operations"].as_array().unwrap().len(),0);
                assert_eq!(fs::read(&source_path).unwrap(),source_before);
                let metadata=fs::metadata(&source_path).unwrap();assert_eq!((metadata.dev(),metadata.ino()),(source_metadata.dev(),source_metadata.ino()));
            }
            assert_eq!(ledger_before_reads,ledger_bytes(&root));
            assert!(!format!("{failure:?} {failure}").contains(BODY));
            root.write_new("case-result.json",&serde_json::to_vec_pretty(&json!({"schema":"factory.fci06-paired-native-evidence/v1",
                "gate":PARENT_GATE,"mode":mode,"ordinary":ordinary.facts(),"child":child.facts(),"ordinaryReportedVersion":version,
                "childSourceManifestSha256":declared_source,"childSourceBinding":"declared pin; qualify against actual compiler Source receipt separately",
                "physical":physical,"nativeResponse":response,"nativeObservation":observation,"independentReceiving":by_ref,
                "independentOriginalLookup":lookup,"independentDocument":after,"automaticReplay":false,
                "originalCredit":false,"modelInvoked":false,"actualHumanWorldReply":false,"standing":"paired actual native controlled fixture"})).unwrap());
        }
        ordinary.recheck();child.recheck();
        let named=fs::symlink_metadata(&parent).unwrap();assert_eq!((named.dev(),named.ino()),(parent_identity.dev(),parent_identity.ino()));
        assert_eq!((parent_hold.metadata().unwrap().dev(),parent_hold.metadata().unwrap().ino()),(parent_identity.dev(),parent_identity.ino()));
    }
}
