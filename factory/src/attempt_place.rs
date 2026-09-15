//! Attempt-scoped Workcell place lifecycle: bounded request and semantic
//! release of a persistent room (Herdr first, tmux fallback under Workcell's
//! `auto` policy) over the pinned `workcell.place-grant/v1` contract.
//!
//! A place is a room, not a self. The grant is material provenance in the
//! attempt receipt; run identity lives in typed artifacts. A place outliving
//! the attempt is disclosed, never read as continuation; ending the place is a
//! semantic act, never inferred from run state.
//!
//! Like every native owner transport, this client is bounded: explicit argv,
//! no shell, one invocation per call. A refusal is surfaced verbatim as
//! observed evidence and is never implicitly sent again; canonical telemetry
//! comes from typed owner receipts, not terminal panes.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

/// The only place-grant revision Factory accepts. Pinned like every CAW
/// contract revision; a Workcell that answers with anything else is refused.
pub const PLACE_GRANT_CONTRACT_REVISION: &str = "workcell.place-grant/v1";
/// Workcell's census contract (`workcell places`). Not consumed by Factory's
/// code path; named here so the doc-facing surface states what it pins.
pub const PLACE_CENSUS_CONTRACT: &str = "workcell.place-census/v1";
const MAX_REFUSAL_TEXT: usize = 4096;

/// The provider that materialised a granted room. Opaque to Factory beyond
/// these two names; pane and process details stay provenance, never identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaceProvider {
    Herdr,
    Tmux,
}

impl PlaceProvider {
    fn as_str(self) -> &'static str {
        match self {
            Self::Herdr => "herdr",
            Self::Tmux => "tmux",
        }
    }

    fn place_ref_prefix(self) -> &'static str {
        match self {
            Self::Herdr => "workcell:place:herdr:",
            Self::Tmux => "workcell:place:tmux:",
        }
    }
}

/// The provider policy Factory may request. `Auto` is Workcell's own
/// Herdr-first, tmux-fallback ordering; Factory never re-implements it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaceProviderPolicy {
    #[default]
    Auto,
    Herdr,
    Tmux,
}

impl PlaceProviderPolicy {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Herdr => "herdr",
            Self::Tmux => "tmux",
        }
    }
}

/// A granted room, byte-faithful to `workcell.place-grant/v1`. The native
/// snake_case names are the contract's and are kept verbatim wherever this
/// grant travels, including inside a Factory attempt record's camelCase
/// envelope. Unknown fields refuse: the revision is pinned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct WorkcellPlaceGrant {
    pub schema: String,
    pub place_ref: String,
    pub provider: PlaceProvider,
    pub session_name: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub pane_pid: Option<u32>,
    #[serde(default)]
    pub process_start_marker: Option<String>,
    pub created_utc: String,
}

/// A request for a room. The timeout bounds client transport only; a Workcell
/// room may outlive this call exactly as any material arrangement may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceRequest<'a> {
    pub binary: &'a Path,
    pub provider: PlaceProviderPolicy,
    pub name: &'a str,
    pub timeout_ms: u64,
}

/// A semantic end of a room, made with the grant's own proof tokens. Factory
/// never invents a pid or start marker and never kills without both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceReleaseRequest<'a> {
    pub binary: &'a Path,
    pub grant: &'a WorkcellPlaceGrant,
    pub timeout_ms: u64,
}

/// Evidence of a completed release. The exit status carried it; the captured
/// text is not a second contract and Factory assigns it no further meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceReleaseReceipt {
    pub place_ref: String,
    pub stdout: String,
    pub stderr: String,
}

/// Workcell refused a place operation. The explanation is kept verbatim as
/// observed evidence; deciding what follows is an explicit act, never an
/// implicit retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceRefusal {
    pub operation: &'static str,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum PlaceError {
    InvalidInvocation(String),
    /// The grant carries no provable process binding, so releasing it would
    /// mean killing without proof. Nothing was sent to Workcell.
    UnprovableBinding {
        place_ref: String,
    },
    Refused(PlaceRefusal),
    InvalidResponse(String),
    Transport {
        binary: PathBuf,
        error: String,
    },
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInvocation(message) => {
                write!(formatter, "invalid place invocation: {message}")
            }
            Self::UnprovableBinding { place_ref } => write!(
                formatter,
                "place {place_ref} carries no pid and start marker; release would kill without proof and was not sent"
            ),
            Self::Refused(refusal) => write!(
                formatter,
                "workcell refused place {} with code {:?}; stderr={:?}",
                refusal.operation, refusal.exit_code, refusal.stderr
            ),
            Self::InvalidResponse(message) => {
                write!(formatter, "invalid workcell place response: {message}")
            }
            Self::Transport { binary, error } => {
                write!(formatter, "workcell place client {}: {error}", binary.display())
            }
        }
    }
}

impl std::error::Error for PlaceError {}

/// Ask Workcell for a room under the requested policy. One bounded call; a
/// refusal (for example `already-exists` or `provider-cannot-create`) is
/// returned verbatim and is never implicitly repeated with altered arguments.
pub fn request_place(request: &PlaceRequest<'_>) -> Result<WorkcellPlaceGrant, PlaceError> {
    validate_name(request.name)?;
    let mut command = place_request_command(request.binary, request.provider, request.name);
    let output = bounded_output(request.binary, &mut command, request.timeout_ms)?;
    if !output.status.success() {
        return Err(PlaceError::Refused(PlaceRefusal::capture(
            "request", output,
        )));
    }
    let grant: WorkcellPlaceGrant = serde_json::from_slice(&output.stdout).map_err(|error| {
        PlaceError::InvalidResponse(format!(
            "place request did not return pinned {PLACE_GRANT_CONTRACT_REVISION} JSON: {error}; stderr: {}",
            bounded_text(&output.stderr)
        ))
    })?;
    validate_grant(&grant).map_err(PlaceError::InvalidResponse)?;
    Ok(grant)
}

/// End a room as a semantic act, proving Workcell's pid and start marker
/// discipline with the grant's own tokens. A stale-binding refusal is
/// observed and surfaced verbatim; this function performs exactly one
/// invocation, so nothing is ever retried implicitly.
pub fn release_place(request: &PlaceReleaseRequest<'_>) -> Result<PlaceReleaseReceipt, PlaceError> {
    validate_grant(request.grant).map_err(|error| {
        PlaceError::InvalidInvocation(format!(
            "place release refuses a grant outside the pinned contract: {error}"
        ))
    })?;
    let (pid, start_marker) = match (
        request.grant.pane_pid,
        request.grant.process_start_marker.as_deref(),
    ) {
        (Some(pid), Some(start_marker)) => (pid, start_marker),
        _ => {
            return Err(PlaceError::UnprovableBinding {
                place_ref: request.grant.place_ref.clone(),
            })
        }
    };
    let mut command =
        place_release_command(request.binary, &request.grant.place_ref, pid, start_marker);
    let output = bounded_output(request.binary, &mut command, request.timeout_ms)?;
    if !output.status.success() {
        return Err(PlaceError::Refused(PlaceRefusal::capture(
            "release", output,
        )));
    }
    Ok(PlaceReleaseReceipt {
        place_ref: request.grant.place_ref.clone(),
        stdout: bounded_text(&output.stdout),
        stderr: bounded_text(&output.stderr),
    })
}

/// Validate a grant against the pinned contract's internal laws. Used when a
/// grant is first parsed and whenever an authored grant is admitted onto an
/// attempt record as provenance.
pub fn validate_grant(grant: &WorkcellPlaceGrant) -> Result<(), String> {
    if grant.schema != PLACE_GRANT_CONTRACT_REVISION {
        return Err(format!(
            "grant schema `{}` is not the pinned `{}`",
            grant.schema, PLACE_GRANT_CONTRACT_REVISION
        ));
    }
    if grant.session_name.trim().is_empty() {
        return Err("place grant omits session_name".into());
    }
    let prefix = grant.provider.place_ref_prefix();
    if grant.place_ref.len() <= prefix.len() || !grant.place_ref.starts_with(prefix) {
        return Err(format!(
            "place grant provider `{}` does not match place_ref `{}`",
            grant.provider.as_str(),
            grant.place_ref
        ));
    }
    if grant
        .pane_id
        .as_deref()
        .is_some_and(|pane| pane.trim().is_empty())
    {
        return Err("place grant pane_id is empty".into());
    }
    if grant.pane_pid.is_some_and(|pid| pid == 0) {
        return Err("place grant pane_pid is not a positive process id".into());
    }
    if grant
        .process_start_marker
        .as_deref()
        .is_some_and(|marker| marker.trim().is_empty())
    {
        return Err("place grant process_start_marker is empty".into());
    }
    chrono::DateTime::parse_from_rfc3339(&grant.created_utc)
        .map_err(|error| format!("place grant created_utc is not rfc3339: {error}"))?;
    Ok(())
}

fn validate_name(name: &str) -> Result<(), PlaceError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || trimmed
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(PlaceError::InvalidInvocation(
            "place request requires a slug name without whitespace or control characters".into(),
        ));
    }
    Ok(())
}

fn place_request_command(binary: &Path, provider: PlaceProviderPolicy, name: &str) -> Command {
    let mut command = Command::new(binary);
    command
        .arg("place")
        .arg("request")
        .arg("--provider")
        .arg(provider.as_arg())
        .arg("--name")
        .arg(name);
    command
}

fn place_release_command(binary: &Path, place_ref: &str, pid: u32, start_marker: &str) -> Command {
    let mut command = Command::new(binary);
    command
        .arg("place")
        .arg("release")
        .arg("--place-ref")
        .arg(place_ref)
        .arg("--pid")
        .arg(pid.to_string())
        .arg("--start-marker")
        .arg(start_marker);
    command
}

fn bounded_output(
    binary: &Path,
    command: &mut Command,
    timeout_ms: u64,
) -> Result<Output, PlaceError> {
    if timeout_ms == 0 {
        return Err(PlaceError::InvalidInvocation(
            "place transport requires a positive timeout".into(),
        ));
    }
    crate::native_process::output(command, Duration::from_millis(timeout_ms)).map_err(|error| {
        PlaceError::Transport {
            binary: binary.to_path_buf(),
            error: error.to_string(),
        }
    })
}

impl PlaceRefusal {
    fn capture(operation: &'static str, output: Output) -> Self {
        Self {
            operation,
            exit_code: output.status.code(),
            stdout: bounded_text(&output.stdout),
            stderr: bounded_text(&output.stderr),
        }
    }
}

fn bounded_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_REFUSAL_TEXT)]).into_owned()
}

#[cfg(test)]
mod place_contract_laws {
    use super::*;
    use serde_json::json;

    fn granted() -> WorkcellPlaceGrant {
        WorkcellPlaceGrant {
            schema: PLACE_GRANT_CONTRACT_REVISION.into(),
            place_ref: "workcell:place:tmux:/tmp/socket:attempt-room".into(),
            provider: PlaceProvider::Tmux,
            session_name: "attempt-room".into(),
            pane_id: Some("%3".into()),
            pane_pid: Some(4242),
            process_start_marker: Some("1747344000.123456-3".into()),
            created_utc: "2026-09-15T09:30:00Z".into(),
        }
    }

    #[test]
    fn grants_keep_their_native_contract_names_inside_any_envelope() {
        let value = serde_json::to_value(granted()).expect("grant serializes");
        for key in [
            "schema",
            "place_ref",
            "provider",
            "session_name",
            "pane_id",
            "pane_pid",
            "process_start_marker",
            "created_utc",
        ] {
            assert!(value.get(key).is_some(), "native key `{key}` was renamed");
        }
        let parsed: WorkcellPlaceGrant =
            serde_json::from_value(value).expect("round trip preserves the pinned shape");
        assert_eq!(parsed, granted());
    }

    #[test]
    fn foreign_contract_shapes_are_refused_not_normalised() {
        let mut foreign = serde_json::to_value(granted()).expect("grant serializes");
        foreign["extra_field"] = json!("workcell grew a field");
        assert!(serde_json::from_value::<WorkcellPlaceGrant>(foreign).is_err());
        let mut renamed = serde_json::to_value(granted()).expect("grant serializes");
        renamed["placeRef"] = renamed["place_ref"].clone();
        assert!(serde_json::from_value::<WorkcellPlaceGrant>(renamed).is_err());
    }

    #[test]
    fn validate_enforces_schema_provider_and_proof_laws() {
        assert_eq!(validate_grant(&granted()), Ok(()));
        let mut herdr = granted();
        herdr.provider = PlaceProvider::Herdr;
        herdr.place_ref = "workcell:place:herdr:herdr-room-7".into();
        herdr.pane_id = None;
        herdr.pane_pid = None;
        herdr.process_start_marker = None;
        assert_eq!(validate_grant(&herdr), Ok(()));

        let mut schema = granted();
        schema.schema = "workcell.place-grant/v2".into();
        assert!(validate_grant(&schema).is_err());
        let mut provider = granted();
        provider.place_ref = "workcell:place:herdr:attempt-room".into();
        assert!(validate_grant(&provider).is_err());
        let mut short_ref = granted();
        short_ref.place_ref = "workcell:place:tmux:".into();
        assert!(validate_grant(&short_ref).is_err());
        let mut session = granted();
        session.session_name = "  ".into();
        assert!(validate_grant(&session).is_err());
        let mut pane = granted();
        pane.pane_id = Some(" ".into());
        assert!(validate_grant(&pane).is_err());
        let mut pid = granted();
        pid.pane_pid = Some(0);
        assert!(validate_grant(&pid).is_err());
        let mut marker = granted();
        marker.process_start_marker = Some(String::new());
        assert!(validate_grant(&marker).is_err());
        let mut time = granted();
        time.created_utc = "15/09/2026 09:30".into();
        assert!(validate_grant(&time).is_err());
    }

    #[test]
    fn argv_is_explicit_without_a_shell() {
        let request = place_request_command(
            Path::new("/bin/workcell"),
            PlaceProviderPolicy::Auto,
            "attempt-room",
        );
        assert_eq!(request.get_program(), "/bin/workcell");
        let arguments: Vec<&std::ffi::OsStr> = request.get_args().collect();
        assert_eq!(
            arguments,
            [
                "place",
                "request",
                "--provider",
                "auto",
                "--name",
                "attempt-room"
            ]
        );
        let tmux_request = place_request_command(
            Path::new("/bin/workcell"),
            PlaceProviderPolicy::Tmux,
            "room-2",
        );
        assert!(tmux_request
            .get_args()
            .any(|argument| argument == std::ffi::OsStr::new("tmux")));
        let release = place_release_command(
            Path::new("/bin/workcell"),
            "workcell:place:tmux:/tmp/socket:attempt-room",
            4242,
            "1747344000.123456-3",
        );
        let release_arguments: Vec<&std::ffi::OsStr> = release.get_args().collect();
        assert_eq!(
            release_arguments,
            [
                "place",
                "release",
                "--place-ref",
                "workcell:place:tmux:/tmp/socket:attempt-room",
                "--pid",
                "4242",
                "--start-marker",
                "1747344000.123456-3"
            ]
        );
    }

    #[test]
    fn provider_policy_defaults_to_workcell_auto_ordering() {
        assert_eq!(PlaceProviderPolicy::default(), PlaceProviderPolicy::Auto);
        assert_eq!(PlaceProviderPolicy::Auto.as_arg(), "auto");
        assert_eq!(PlaceProviderPolicy::Herdr.as_arg(), "herdr");
        assert_eq!(PlaceProviderPolicy::Tmux.as_arg(), "tmux");
    }

    #[test]
    fn slug_names_are_bounded_and_unambiguous() {
        assert!(validate_name("attempt-room").is_ok());
        assert!(validate_name("attempt room").is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name(" ").is_err());
        assert!(validate_name(&"a".repeat(129)).is_err());
    }

    #[test]
    fn zero_timeout_is_refused_before_any_process_starts() {
        let request = PlaceRequest {
            binary: Path::new("/bin/workcell"),
            provider: PlaceProviderPolicy::Auto,
            name: "attempt-room",
            timeout_ms: 0,
        };
        assert!(matches!(
            request_place(&request),
            Err(PlaceError::InvalidInvocation(_))
        ));
    }
}
