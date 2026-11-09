//! A task offered at a star, taken up on another person's machine (DLW §6,
//! §17.2; story SF13).
//!
//! "Light a star for a programme and you can offer some of its tasks there.
//! Another person's agent takes one up on their own machine, under a grant you
//! set for what it may read and touch and for how long, and what it makes
//! comes back to you under their name until you accept it."
//!
//! Factory keeps both sides, each in its own state's folder: the offers a
//! person made (`<state>/task-offers/made/`) and the offers a person took
//! (`<state>/task-offers/taken/`), one file each, published whole through
//! `NativeFileTransaction`. The field only carries the documents between the
//! two machines; what an offer asks, what its grant allows and whether what
//! came back enters the work are Factory's.
//!
//! The offerer's side: `offer-make` turns one unit of a Run into an offer at a
//! star with its grant (what the task may read, what it may touch, when the
//! grant lapses); `offer-revoke` withdraws it at any moment; `offer-receive`
//! takes in what came back, apart from the work, refusing a return to an offer
//! revoked or lapsed or one that touched past its grant; `offer-accept` brings
//! it into the Run under its maker's name. Nothing enters the work before.
//!
//! The taker's side: `offer-take` records the offer taken by one world;
//! `offer-check` answers whether the task may read or touch one thing now,
//! under the grant; `offer-revoked` records the offerer's revocation as the
//! field told it, after which nothing is allowed; `offer-return` answers what
//! the task made, under the taker's name, within the grant.
//!
//! The offerer's Factory also serves what a grant lets the task read:
//! `offer-taken` keeps the world the field verified took the offer, and
//! `offer-read` answers that world, and no other, the bytes of one granted
//! ref of its own project, checked against its own record of the grant,
//! never the taker's copy. Who acts on the offerer's record is never read
//! from a request: the relay names the world the field verified beside it,
//! `--by` for `offer-taken` and `offer-receive` (only the world that took the
//! offer returns to it), `--asker` for `offer-read`.
//!
//! A person makes, revokes, accepts and takes (a desktop-human caller on
//! Factory's authority, as a rack's macros are turned); what an agent checks
//! and returns is the agent's, within the grant. Every document says
//! `automatic_agent_or_model_invocation: false`: nothing here runs an agent.

use crate::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use crate::build::{CandidateRecord, FACTORY_NATIVE_OWNER};
use crate::core::run::{RunRef, WorkflowUnitRef};
use crate::native_file_transaction::{
    lock_name, read_native, NativeFileTransaction, PublicationError,
};
use crate::project_development_store::{read_developmental_state, transact_developmental_state};
use crate::shown_work::names_the_unshown;
use serde_json::{json, Map, Value};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const TASK_OFFER: &str = "factory.task-offer/v1";
pub const TASK_OFFER_RECEIPT: &str = "factory.task-offer-receipt/v1";
pub const TASK_OFFER_RETURN: &str = "factory.task-offer-return/v1";
pub const TASK_OFFER_CHECK: &str = "factory.task-offer-check/v1";
pub const TASK_OFFERS_READING: &str = "factory.task-offers-reading/v1";
pub const TASK_OFFER_REFUSAL: &str = "factory.task-offer-refusal/v1";
pub const TASK_OFFER_READ: &str = "factory.task-offer-read/v1";
/// The most one `offer-read` carries: a file larger is refused, never cut.
/// Its bytes go base64 in one document, so the field carries at most about
/// 1.4 MB of text for one read.
pub const MAX_READ_BYTES: u64 = 1024 * 1024;
/// The most entries one `offer-read` of a folder names.
const MAX_READ_ENTRIES: usize = 512;
/// What a person presents to make, revoke or accept an offer.
pub const OFFER_CAPABILITY_REF: &str = "capability/factory/offer-task";
/// What a person presents to take one up.
pub const TAKE_CAPABILITY_REF: &str = "capability/factory/take-task";

pub const VERBS: [&str; 11] = [
    "offers",
    "offer-make",
    "offer-revoke",
    "offer-receive",
    "offer-accept",
    "offer-read",
    "offer-taken",
    "offer-take",
    "offer-check",
    "offer-revoked",
    "offer-return",
];
const FOLDER: &str = "task-offers";
const MAX_REFS: usize = 32;
const MAX_REF_CHARS: usize = 512;
const MAX_TEXT_CHARS: usize = 2000;
const MAX_NAME_CHARS: usize = 200;
/// The longest a grant may stand: thirty days.
const MAX_GRANT_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub fn help() -> String {
    "A task offered at a star (the offerer's side, then the taker's; each request a document, `-` for stdin):\n  factory development offers <state> [--json]\n  factory development offer-make|offer-revoke|offer-accept <state> [request-file|-] [--json]\n  factory development offer-taken|offer-receive <state> --by <world_ref> [request-file|-] [--json]\n  factory development offer-read <state> --asker <world_ref> [request-file|-] [--json]\n  factory development offer-take|offer-check|offer-revoked|offer-return <state> [request-file|-] [--json]".into()
}

// ---- refusals ------------------------------------------------------------------

/// A refusal with a code a caller can act on and the words a person can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferRefusal {
    pub code: &'static str,
    pub message: String,
}

impl OfferRefusal {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }

    pub fn document(&self) -> Value {
        let effect = if self.code == "publication_uncertain" {
            "The change may have been published; read the offers back before trying again."
        } else {
            "Nothing was changed."
        };
        json!({"schema":TASK_OFFER_REFUSAL,"code":self.code,"message":self.message,"effect":effect})
    }
}

impl std::fmt::Display for OfferRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for OfferRefusal {}

fn io_refusal(what: &str, error: io::Error) -> OfferRefusal {
    let code = if error.kind() == io::ErrorKind::TimedOut {
        "busy"
    } else {
        "io"
    };
    OfferRefusal::new(code, format!("{what}: {error}"))
}

// ---- reading a request -----------------------------------------------------------

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, OfferRefusal> {
    value
        .as_object()
        .ok_or_else(|| OfferRefusal::invalid(format!("{what} must be an object")))
}

fn only_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    what: &str,
) -> Result<(), OfferRefusal> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(OfferRefusal::invalid(format!("{what} takes no `{key}`"))),
        None => Ok(()),
    }
}

/// Text a person can read, bounded, never naming a file, key or machine.
fn text<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    limit: usize,
) -> Result<&'a str, OfferRefusal> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OfferRefusal::invalid(format!("`{field}` is required text")))?;
    if value.chars().count() > limit {
        return Err(OfferRefusal::invalid(format!(
            "`{field}` is longer than {limit} characters"
        )));
    }
    if names_the_unshown(value) {
        return Err(OfferRefusal::new(
            "unshown_material",
            format!("`{field}` names a file, a key, a session or a machine; an offer and what comes back carry none"),
        ));
    }
    Ok(value)
}

/// A ref: a scheme, a colon and a body with no space; bounded; never naming
/// a file, key or machine.
fn reference(value: &Value, field: &str) -> Result<String, OfferRefusal> {
    let text = value
        .as_str()
        .ok_or_else(|| OfferRefusal::invalid(format!("`{field}` holds refs")))?;
    let well_formed = text.len() <= MAX_REF_CHARS
        && text.split_once(':').is_some_and(|(scheme, body)| {
            !scheme.is_empty()
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-_".contains(c))
                && !body.is_empty()
                && !body.chars().any(char::is_whitespace)
        });
    if !well_formed {
        return Err(OfferRefusal::invalid(format!(
            "`{field}` holds refs (a scheme, a colon and a name), not {text}"
        )));
    }
    if names_the_unshown(text) {
        return Err(OfferRefusal::new(
            "unshown_material",
            format!("`{field}` names a file, a key, a session or a machine ({text}); a grant names what it covers by ref"),
        ));
    }
    Ok(text.to_owned())
}

fn refs(
    value: Option<&Value>,
    field: &str,
    at_least_one: bool,
) -> Result<Vec<String>, OfferRefusal> {
    let list = match value {
        None | Some(Value::Null) if !at_least_one => return Ok(Vec::new()),
        Some(Value::Array(list)) => list,
        _ => {
            return Err(OfferRefusal::invalid(format!(
                "`{field}` is a list of refs"
            )))
        }
    };
    if (at_least_one && list.is_empty()) || list.len() > MAX_REFS {
        return Err(OfferRefusal::invalid(format!(
            "`{field}` names {} to {MAX_REFS} refs",
            usize::from(at_least_one)
        )));
    }
    let mut out = Vec::with_capacity(list.len());
    for value in list {
        let reference = reference(value, field)?;
        if !out.contains(&reference) {
            out.push(reference);
        }
    }
    Ok(out)
}

fn millis(object: &Map<String, Value>, field: &str) -> Result<i64, OfferRefusal> {
    object
        .get(field)
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(|| OfferRefusal::invalid(format!("`{field}` is a time in Unix milliseconds")))
}

/// A person at the desktop, on Factory's authority with `capability`.
fn require_person(
    object: &Map<String, Value>,
    capability: &str,
    act: &str,
) -> Result<(String, String), OfferRefusal> {
    let not_person = |fact: String| {
        OfferRefusal::new(
            "not_human",
            format!("{act} is a person's act at the desktop: {fact}"),
        )
    };
    let caller: FactoryActionCaller = match object.get("caller") {
        Some(value) if !value.is_null() => {
            serde_json::from_value(value.clone()).map_err(|error| {
                not_person(format!(
                    "the caller is not a Factory action caller ({error})"
                ))
            })?
        }
        _ => return Err(not_person("the request names no caller".into())),
    };
    if caller.projection_kind != FactoryActionProjectionKind::DesktopHuman {
        return Err(not_person(format!(
            "the caller {} is not a desktop-human",
            caller.caller_ref
        )));
    }
    if caller.caller_ref.trim().is_empty()
        || caller.lineage.last().map(String::as_str) != Some(caller.caller_ref.as_str())
    {
        return Err(not_person(
            "the caller's lineage must end at the caller".into(),
        ));
    }
    let not_authorised = |fact: String| {
        OfferRefusal::new(
            "not_authorised",
            format!("{act} needs Factory's authority: {fact}"),
        )
    };
    let authority: ProjectedFactoryActionAuthority = match object.get("authority") {
        Some(value) if !value.is_null() => {
            serde_json::from_value(value.clone()).map_err(|error| {
                not_authorised(format!(
                    "the authority is not a Factory authority ({error})"
                ))
            })?
        }
        _ => return Err(not_authorised("the request names no authority".into())),
    };
    if authority.native_owner != FACTORY_NATIVE_OWNER
        || authority.capability_ref.as_deref() != Some(capability)
        || !authority.capability_granted
        || !authority.action_authorised
        || authority.authority_ref.trim().is_empty()
    {
        return Err(not_authorised(format!(
            "the capability {capability} must be granted by {FACTORY_NATIVE_OWNER}, the act authorised, and the authority named"
        )));
    }
    Ok((caller.caller_ref, authority.authority_ref))
}

// ---- the grant and the offer ---------------------------------------------------------

/// What a grant covers: the ref itself and what lies under it.
fn covers(granted: &[String], subject: &str) -> bool {
    granted.iter().any(|grant| {
        subject == grant
            || subject
                .strip_prefix(grant.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A grant's refs as a read reaches them: a project ref's empty steps
/// dropped (`project:river/docs//sub/` is `project:river/docs/sub`), so what
/// a grant covers is what a read of its entries is served (SF13.3c, F5).
/// Another scheme's ref stands as given.
fn grant_refs(refs: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(refs.len());
    for reference in refs {
        let reference = match reference.strip_prefix("project:") {
            Some(body) => format!(
                "project:{}",
                body.split('/')
                    .filter(|step| !step.is_empty())
                    .collect::<Vec<_>>()
                    .join("/")
            ),
            None => reference,
        };
        if !out.contains(&reference) {
            out.push(reference);
        }
    }
    out
}

/// An offer as Factory made it, or as the field carried it to the taker:
/// checked whole, so a taker's Factory holds what the offerer's made.
fn checked_offer(value: &Value) -> Result<Value, OfferRefusal> {
    let offer = object(value, "the offer")?;
    only_keys(
        offer,
        &[
            "schema",
            "offer_ref",
            "star_ref",
            "programme",
            "task",
            "grant",
            "offered_at",
            "automatic_agent_or_model_invocation",
        ],
        "an offer",
    )?;
    if offer.get("schema").and_then(Value::as_str) != Some(TASK_OFFER) {
        return Err(OfferRefusal::invalid(format!(
            "an offer is a {TASK_OFFER} document"
        )));
    }
    let offer_ref = reference(offer.get("offer_ref").unwrap_or(&Value::Null), "offer_ref")?;
    if !offer_ref.starts_with("offer:") {
        return Err(OfferRefusal::invalid("offer_ref is an offer: ref"));
    }
    let star_ref = reference(offer.get("star_ref").unwrap_or(&Value::Null), "star_ref")?;
    let programme = object(
        offer.get("programme").unwrap_or(&Value::Null),
        "the programme",
    )?;
    only_keys(
        programme,
        &["run_ref", "workflow_unit_ref"],
        "the programme",
    )?;
    let run_ref = reference(programme.get("run_ref").unwrap_or(&Value::Null), "run_ref")?;
    let unit_ref = reference(
        programme.get("workflow_unit_ref").unwrap_or(&Value::Null),
        "workflow_unit_ref",
    )?;
    let task = object(offer.get("task").unwrap_or(&Value::Null), "the task")?;
    only_keys(task, &["key", "asks", "returns"], "the task")?;
    let key = text(task, "key", MAX_NAME_CHARS)?;
    let asks = text(task, "asks", MAX_TEXT_CHARS)?;
    let returns = text(task, "returns", MAX_NAME_CHARS)?;
    let grant = object(offer.get("grant").unwrap_or(&Value::Null), "the grant")?;
    only_keys(grant, &["reads", "touches", "lapses_at"], "the grant")?;
    let reads = grant_refs(refs(grant.get("reads"), "reads", true)?);
    let touches = grant_refs(refs(grant.get("touches"), "touches", false)?);
    let lapses_at = millis(grant, "lapses_at")?;
    let offered_at = millis(offer, "offered_at")?;
    if lapses_at <= offered_at || lapses_at - offered_at > MAX_GRANT_MS {
        return Err(OfferRefusal::new(
            "grant_span",
            "A grant lapses after it is set, and within thirty days of it",
        ));
    }
    Ok(json!({
        "schema": TASK_OFFER,
        "offer_ref": offer_ref,
        "star_ref": star_ref,
        "programme": {"run_ref": run_ref, "workflow_unit_ref": unit_ref},
        "task": {"key": key, "asks": asks, "returns": returns},
        "grant": {"reads": reads, "touches": touches, "lapses_at": lapses_at},
        "offered_at": offered_at,
        "automatic_agent_or_model_invocation": false,
    }))
}

/// What came back, as the taker's Factory answered it: checked whole against
/// the offer it answers.
fn checked_return(value: &Value, offer: &Value) -> Result<Value, OfferRefusal> {
    let back = object(value, "what came back")?;
    only_keys(
        back,
        &[
            "schema",
            "offer_ref",
            "star_ref",
            "by",
            "made",
            "returned_at",
            "automatic_agent_or_model_invocation",
        ],
        "what came back",
    )?;
    if back.get("schema").and_then(Value::as_str) != Some(TASK_OFFER_RETURN) {
        return Err(OfferRefusal::invalid(format!(
            "what comes back is a {TASK_OFFER_RETURN} document"
        )));
    }
    if back.get("offer_ref") != offer.get("offer_ref")
        || back.get("star_ref") != offer.get("star_ref")
    {
        return Err(OfferRefusal::new(
            "not_this_offer",
            "What came back answers another offer",
        ));
    }
    let by = object(back.get("by").unwrap_or(&Value::Null), "who made it")?;
    only_keys(by, &["world_ref", "name"], "who made it")?;
    let world_ref = reference(by.get("world_ref").unwrap_or(&Value::Null), "world_ref")?;
    let name = text(by, "name", MAX_NAME_CHARS)?;
    let made = checked_made(back.get("made").unwrap_or(&Value::Null), &offer["grant"])?;
    let returned_at = millis(back, "returned_at")?;
    Ok(json!({
        "schema": TASK_OFFER_RETURN,
        "offer_ref": offer["offer_ref"],
        "star_ref": offer["star_ref"],
        "by": {"world_ref": world_ref, "name": name},
        "made": made,
        "returned_at": returned_at,
        "automatic_agent_or_model_invocation": false,
    }))
}

/// What a task made: its label, a summary and the refs it touched, every one
/// within the grant's touches.
fn checked_made(value: &Value, grant: &Value) -> Result<Value, OfferRefusal> {
    let made = object(value, "what was made")?;
    only_keys(made, &["label", "summary", "refs"], "what was made")?;
    let label = text(made, "label", MAX_NAME_CHARS)?;
    let summary = text(made, "summary", MAX_TEXT_CHARS)?;
    let refs = refs(made.get("refs"), "refs", false)?;
    let touches = strings(&grant["touches"]);
    if let Some(outside) = refs.iter().find(|made| !covers(&touches, made)) {
        return Err(OfferRefusal::new(
            "outside_grant",
            format!("What was made touches {outside}, which the grant does not let the task touch"),
        ));
    }
    Ok(json!({"label": label, "summary": summary, "refs": refs}))
}

// ---- storage ------------------------------------------------------------------

fn key_of(reference: &str) -> String {
    let tail: String = reference
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    let tail = tail.trim_matches('-');
    let hash = blake3::hash(reference.as_bytes()).to_hex();
    format!(
        "{}-{}",
        if tail.is_empty() { "offer" } else { tail },
        &hash.as_str()[..16]
    )
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, OfferRefusal> {
    match read_native(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_refusal(
            &format!("could not read {}", path.display()),
            error,
        )),
    }
}

/// Offers in `<state>/task-offers/`: a developmental state file as `<state>`
/// names the directory it lives in.
pub struct OfferStore {
    root: PathBuf,
    state_file: Option<PathBuf>,
}

/// Which side of an offer this machine stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Made,
    Taken,
}

impl Side {
    fn folder(self) -> &'static str {
        match self {
            Side::Made => "made",
            Side::Taken => "taken",
        }
    }
}

impl OfferStore {
    pub fn at(state: impl AsRef<Path>) -> Self {
        let path = state.as_ref();
        let (root, state_file) = if path.is_file() {
            let root = match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
                _ => PathBuf::from("."),
            };
            (root, Some(path.to_path_buf()))
        } else {
            (path.to_path_buf(), None)
        };
        Self { root, state_file }
    }

    fn path(&self, side: Side, offer_ref: &str) -> PathBuf {
        self.root
            .join(FOLDER)
            .join(side.folder())
            .join(format!("{}.json", key_of(offer_ref)))
    }

    fn records(&self, side: Side) -> Result<Vec<Value>, OfferRefusal> {
        let folder = self.root.join(FOLDER).join(side.folder());
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(io_refusal(
                    &format!("could not list {}", folder.display()),
                    error,
                ))
            }
        };
        let mut records = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| io_refusal("could not list the offers", error))?;
            let name = entry.file_name();
            if !name.to_string_lossy().ends_with(".json") {
                continue;
            }
            if let Some(bytes) = read_optional(&entry.path())? {
                records.push(decode(&bytes)?);
            }
        }
        records.sort_by(|left, right| {
            left["offer"]["offered_at"]
                .as_i64()
                .cmp(&right["offer"]["offered_at"].as_i64())
        });
        Ok(records)
    }

    /// Change one record under its lock: `change` sees what stands (None
    /// before the first write) and answers the record to publish and the
    /// answer to give.
    fn transact(
        &self,
        side: Side,
        offer_ref: &str,
        change: impl FnOnce(Option<Value>) -> Result<(Value, Value), OfferRefusal>,
    ) -> Result<Value, OfferRefusal> {
        let path = self.path(side, offer_ref);
        // The made side answers the relay, whose refusals go toward another
        // world: it names no local path (SF13.3c, F6).
        let record = match side {
            Side::Made => "the offer's record".to_owned(),
            Side::Taken => path.display().to_string(),
        };
        let name = lock_name(&path)
            .map_err(|error| io_refusal(&format!("could not name the lock of {record}"), error))?;
        let lock = NativeFileTransaction::acquire(&path, &name, true)
            .map_err(|error| io_refusal(&format!("could not open {record}"), error))?;
        let current = lock.current().map(decode).transpose()?;
        let (record, answer) = change(current)?;
        let bytes = encode(&record);
        if lock.current() != Some(bytes.as_slice()) {
            match lock.publish(&bytes, lock.current().is_none()) {
                Ok(()) => {}
                Err(PublicationError::Before(error)) => {
                    return Err(io_refusal("the change was not published", error))
                }
                Err(PublicationError::Uncertain(uncertain)) => {
                    let said = "publication is uncertain after the replacement; read the offers back before trying again";
                    return Err(OfferRefusal::new(
                        "publication_uncertain",
                        match side {
                            Side::Made => said.to_owned(),
                            Side::Taken => format!("{said}: {uncertain}"),
                        },
                    ));
                }
            }
        }
        Ok(answer)
    }

    fn state_path(&self) -> Result<&Path, OfferRefusal> {
        self.state_file.as_deref().ok_or_else(|| {
            OfferRefusal::invalid(
                "<state> names the developmental state file of the programme the offer comes from",
            )
        })
    }
}

/// Factory's state and each repository's own store: never the work a grant
/// names.
const MACHINERY: [&str; 5] = [".factory", ".git", ".hg", ".svn", ".jj"];

/// A name as a volume that folds case, normalises Unicode or ignores
/// zero-width code points may take it: every default-ignorable code point
/// dropped, then case folded. The machinery names are ASCII, and a name that
/// is ASCII under NFC or NFD holds only ASCII and the three singletons that
/// decompose to ASCII (KELVIN SIGN, GREEK QUESTION MARK, GREEK VARIA), which
/// are mapped here; so a name equal to a machinery name under either
/// normalisation, in any case, is equal to it here, on every volume.
fn folded(name: &str) -> String {
    name.chars()
        .filter(|c| !ignorable(*c))
        .map(|c| match c {
            '\u{212A}' => 'K',
            '\u{037E}' => ';',
            '\u{1FEF}' => '`',
            other => other,
        })
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
        .collect()
}

/// Unicode's default-ignorable code points: what HFS+ drops from a name,
/// and more.
fn ignorable(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFF8}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// Factory's state and the repository's own records, in any spelling:
/// never the work a grant names (DLW build ruling 2026-11-09, SF13.3b).
fn machinery(name: &str) -> bool {
    let name = folded(name);
    MACHINERY.contains(&name.as_str())
}

/// A step of a granted ref that names the work: not `.` or `..`, no
/// backslash or NUL, and no machinery.
fn plain_step(step: &str) -> bool {
    !step.is_empty()
        && step != "."
        && step != ".."
        && !step.contains(['\\', '\0'])
        && !machinery(step)
}

/// The root a placement names: refused when it names no absolute one,
/// never read as the process's own directory.
fn placed_root(located: &Value) -> Result<PathBuf, OfferRefusal> {
    located["projectRoot"]
        .as_str()
        .map(PathBuf::from)
        .filter(|root| root.is_absolute())
        .ok_or_else(|| OfferRefusal::new("not_placed", "the project's placement names no root"))
}

/// At most `limit` bytes from `source`, reading no further than one byte
/// past the bound: `None` when it holds more.
#[cfg_attr(not(unix), allow(dead_code))]
fn read_bounded(source: &mut impl Read, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    source.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

/// What a test runs at each step of a granted read (see `observe`).
#[cfg(test)]
type ReadObserver = Box<dyn FnMut(&str)>;

#[cfg(test)]
thread_local! {
    static READ_OBSERVER: std::cell::RefCell<Option<ReadObserver>> =
        const { std::cell::RefCell::new(None) };
}

/// A test's look at a granted read between its steps: `open <steps>` just
/// before a step is opened, `read` just before a file's bytes are read. It
/// runs code on the real tree and substitutes no result.
#[cfg(test)]
fn observe(stage: &str) {
    READ_OBSERVER.with(|observer| {
        if let Some(observer) = observer.borrow_mut().as_mut() {
            observer(stage);
        }
    });
}

#[cfg(not(test))]
fn observe(_: &str) {}

/// The bytes or entries of a granted ref, reached from the project's root
/// one step at a time through held handles, no link followed (SF13.3b).
#[cfg(unix)]
fn serve(
    mut answer: Value,
    subject: &str,
    root: &Path,
    base: &str,
    steps: &[String],
    after: Option<&str>,
) -> Result<Value, OfferRefusal> {
    use crate::native_file_transaction::{
        entry_names, entry_stat, open_directory, open_step, EntryKind,
    };
    use std::os::unix::fs::MetadataExt;
    let not_here = |why: String| OfferRefusal::new("not_here", why);
    let special = || {
        not_here(format!(
            "{subject} is a pipe, a socket or a device, not a file or folder of the project's work"
        ))
    };
    let mut held = open_directory(root)
        .map_err(|_| OfferRefusal::new("not_placed", "this project's root cannot be opened"))?;
    for (index, step) in steps.iter().enumerate() {
        let names = entry_names(&held)
            .map_err(|error| io_refusal(&format!("could not list {subject}"), error))?;
        if !holds(&names, step) {
            return Err(not_here(format!(
                "{subject} names nothing this project holds"
            )));
        }
        // What the step is, read without following it, before it is opened
        // (SF13.3c, F4): a pipe, a socket or a device is refused in its own
        // words and never opened. A link is left to the open, which refuses
        // it, as it refuses one swapped in after this reading.
        match entry_stat(&held, std::ffi::OsStr::new(step)) {
            Ok((EntryKind::Other, ..)) => return Err(special()),
            Ok(_) => {}
            Err(_) => {
                return Err(not_here(format!(
                    "{subject} names nothing this project holds"
                )))
            }
        }
        observe(&format!("open {}", steps[..=index].join("/")));
        held = open_step(&held, std::ffi::OsStr::new(step)).map_err(|error| {
            match error.raw_os_error() {
                Some(libc::ELOOP | libc::EMLINK) => not_here(format!(
                    "{subject} passes through a link; a grant serves only what stands in the project"
                )),
                _ => not_here(format!("{subject} names nothing this project holds")),
            }
        })?;
        let folder = held
            .metadata()
            .map_err(|error| io_refusal(&format!("could not read {subject}"), error))?
            .is_dir();
        if index + 1 < steps.len() && !folder {
            return Err(not_here(format!(
                "{subject} names no plain path of the project's work"
            )));
        }
    }
    let meta = held
        .metadata()
        .map_err(|error| io_refusal(&format!("could not read {subject}"), error))?;
    if meta.is_dir() {
        let (entries, more) = listing(&held, base, after)
            .map_err(|error| io_refusal(&format!("could not list {subject}"), error))?;
        answer["kind"] = json!("folder");
        answer["entries"] = json!(entries);
        answer["more"] = json!(more);
        return Ok(answer);
    }
    if after.is_some() {
        return Err(OfferRefusal::invalid(format!(
            "`after` pages a folder's entries, and {subject} is a file"
        )));
    }
    if !meta.is_file() {
        return Err(special());
    }
    if meta.nlink() > 1 {
        return Err(OfferRefusal::new(
            "other_names",
            format!("{subject}: this file has other names, outside what was granted"),
        ));
    }
    if meta.len() > MAX_READ_BYTES {
        return Err(OfferRefusal::new(
            "too_large",
            format!(
                "{subject} holds {} bytes; one read carries at most {MAX_READ_BYTES}",
                meta.len()
            ),
        ));
    }
    observe("read");
    let bytes = read_bounded(&mut held, MAX_READ_BYTES)
        .map_err(|error| io_refusal(&format!("could not read {subject}"), error))?
        .ok_or_else(|| {
            OfferRefusal::new(
                "too_large",
                format!("{subject} grew past {MAX_READ_BYTES} bytes while it was read"),
            )
        })?;
    use sha2::Digest;
    answer["kind"] = json!("file");
    answer["byte_len"] = json!(bytes.len());
    answer["sha256"] = json!(format!("{:x}", sha2::Sha256::digest(&bytes)));
    answer["content_base64"] = json!(base64(&bytes));
    Ok(answer)
}

/// Whether a folder's `names` hold `step` byte for byte (SF13.3b; lifted out
/// in SF13.3c, F3): a volume that folds case or normalises names opens no
/// other spelling of it.
#[cfg(unix)]
fn holds(names: &[std::ffi::OsString], step: &str) -> bool {
    use std::os::unix::ffi::OsStrExt;
    names.iter().any(|name| name.as_bytes() == step.as_bytes())
}

/// A held folder's entries in name order, after `after`, at most
/// `MAX_READ_ENTRIES`, and whether others follow. An entry is named only
/// when a read of its ref would not refuse it for what it is: its name is
/// text and a plain step, its ref is well formed and shows nothing unshown,
/// and it is a folder, or a file with one name within `MAX_READ_BYTES`
/// (SF13.3c, F5). Only the entries answered (and one more) are looked at.
#[cfg(unix)]
fn listing(dir: &std::fs::File, base: &str, after: Option<&str>) -> io::Result<(Vec<Value>, bool)> {
    use crate::native_file_transaction::{entry_names, entry_stat, EntryKind};
    let mut named: Vec<(String, std::ffi::OsString)> = entry_names(dir)?
        .into_iter()
        .filter_map(|name| {
            let text = name.to_str()?;
            let entry = format!("{base}/{text}");
            (plain_step(text)
                && reference(&json!(entry), "ref").is_ok()
                && after.is_none_or(|after| entry.as_str() > after))
            .then_some((entry, name))
        })
        .collect();
    named.sort();
    let mut entries = Vec::new();
    for (entry, name) in named {
        let Ok((kind, byte_len, links)) = entry_stat(dir, &name) else {
            continue;
        };
        let kind = match kind {
            EntryKind::Folder => "folder",
            EntryKind::File if links == 1 && byte_len <= MAX_READ_BYTES => "file",
            _ => continue,
        };
        if entries.len() == MAX_READ_ENTRIES {
            return Ok((entries, true));
        }
        entries.push(json!({"ref": entry, "kind": kind, "byte_len": byte_len}));
    }
    Ok((entries, false))
}

#[cfg(not(unix))]
fn serve(
    _: Value,
    subject: &str,
    _: &Path,
    _: &str,
    _: &[String],
    _: Option<&str>,
) -> Result<Value, OfferRefusal> {
    Err(OfferRefusal::new(
        "not_here",
        format!("{subject}: offer-read serves only on a Unix host, where each step is opened with no link followed"),
    ))
}

/// Standard base64, padded.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(ALPHABET[(n >> shift & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn decode(bytes: &[u8]) -> Result<Value, OfferRefusal> {
    serde_json::from_slice(bytes).map_err(|error| {
        OfferRefusal::new(
            "corrupt",
            format!("an offer record could not be read: {error}"),
        )
    })
}

fn encode(record: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(record).expect("an offer record encodes");
    bytes.push(b'\n');
    bytes
}

fn receipt(act: &str, record: &Value, by: Option<(&str, &str)>) -> Value {
    let mut answer = json!({
        "schema": TASK_OFFER_RECEIPT,
        "act": act,
        "offer": record["offer"],
        "state": record["state"],
        "automatic_agent_or_model_invocation": false,
    });
    for key in ["return", "accepted", "taken"] {
        if !record[key].is_null() {
            answer[key] = record[key].clone();
        }
    }
    if let Some((caller, authority)) = by {
        answer["by"] = json!({"caller_ref": caller, "authority_ref": authority});
    }
    answer
}

fn offer_ref_of(object: &Map<String, Value>) -> Result<String, OfferRefusal> {
    let offer_ref = reference(object.get("offer_ref").unwrap_or(&Value::Null), "offer_ref")?;
    if !offer_ref.starts_with("offer:") {
        return Err(OfferRefusal::invalid("offer_ref is an offer: ref"));
    }
    Ok(offer_ref)
}

fn unknown(offer_ref: &str, side: Side) -> OfferRefusal {
    OfferRefusal::new(
        "unknown_offer",
        match side {
            Side::Made => format!("This Factory made no offer {offer_ref}"),
            Side::Taken => format!("This Factory took no offer {offer_ref}"),
        },
    )
}

// ---- the offerer's side ------------------------------------------------------------

impl OfferStore {
    /// `offer-make {run_ref, workflow_unit_ref, star_ref, grant: {reads,
    /// touches, lapses_at}}`: one unit of a Run, offered at a star under its
    /// grant. What the task asks and returns is the unit's own.
    pub fn make(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the offer request")?;
        only_keys(
            object,
            &[
                "run_ref",
                "workflow_unit_ref",
                "star_ref",
                "grant",
                "caller",
                "authority",
            ],
            "offer-make",
        )?;
        let (caller, authority) = require_person(object, OFFER_CAPABILITY_REF, "Offering a task")?;
        let run_text = reference(object.get("run_ref").unwrap_or(&Value::Null), "run_ref")?;
        let unit_text = reference(
            object.get("workflow_unit_ref").unwrap_or(&Value::Null),
            "workflow_unit_ref",
        )?;
        let star_ref = reference(object.get("star_ref").unwrap_or(&Value::Null), "star_ref")?;
        let run_ref: RunRef = run_text
            .parse()
            .map_err(|error| OfferRefusal::invalid(format!("{run_text} is no run-ref: {error}")))?;
        let unit_ref: WorkflowUnitRef = unit_text.parse().map_err(|error| {
            OfferRefusal::invalid(format!("{unit_text} is no workflow-unit-ref: {error}"))
        })?;
        let state = read_developmental_state(self.state_path()?).map_err(|error| {
            OfferRefusal::new(
                "unreadable",
                format!("The Factory state could not be read: {error}"),
            )
        })?;
        let unit = state
            .workflow_unit_reading(&unit_ref, Some(&run_ref))
            .map_err(|error| {
                OfferRefusal::new(
                    "unknown_task",
                    format!("The Run holds no unit {unit_text} to offer: {error}"),
                )
            })?;
        let grant = object.get("grant").unwrap_or(&Value::Null);
        let offer = checked_offer(&json!({
            "schema": TASK_OFFER,
            "offer_ref": format!("offer:{}", ulid::Ulid::new().to_string().to_ascii_lowercase()),
            "star_ref": star_ref,
            "programme": {"run_ref": run_text, "workflow_unit_ref": unit_text},
            "task": {"key": unit.key, "asks": unit.required_difference, "returns": unit.required_return.contract},
            "grant": grant,
            "offered_at": now,
        }))?;
        let offer_ref = offer["offer_ref"].as_str().unwrap_or_default().to_owned();
        self.transact(Side::Made, &offer_ref, |current| {
            if current.is_some() {
                return Err(OfferRefusal::new("exists", format!("The offer {offer_ref} was made already")));
            }
            let record = json!({"offer": offer, "state": "open", "made_by": caller, "authority_ref": authority});
            let answer = receipt("make", &record, Some((&caller, &authority)));
            Ok((record, answer))
        })
    }

    /// `offer-revoke {offer_ref}`: the offer and its grant withdrawn, at any
    /// moment before what came back is accepted.
    pub fn revoke(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the revocation")?;
        only_keys(
            object,
            &["offer_ref", "caller", "authority"],
            "offer-revoke",
        )?;
        let (caller, authority) =
            require_person(object, OFFER_CAPABILITY_REF, "Revoking an offer")?;
        let offer_ref = offer_ref_of(object)?;
        self.transact(Side::Made, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Made))?;
            match record["state"].as_str() {
                Some("accepted") => {
                    return Err(OfferRefusal::new(
                        "accepted",
                        "What came back was accepted into the work already; the offer is closed",
                    ))
                }
                Some("revoked") => {}
                _ => {
                    record["state"] = json!("revoked");
                    record["revoked_at"] = json!(now);
                }
            }
            let answer = receipt("revoke", &record, Some((&caller, &authority)));
            Ok((record, answer))
        })
    }

    /// `offer-taken --by <world_ref> {offer_ref, by: {name}, taken_at}`: the
    /// take, as the field told it, kept on the offer this Factory made.
    /// `world_ref` is the world the field verified took it, which the relay
    /// names beside the request (`--by`); a request that names a world of
    /// its own is refused, and `by.name` is only the name it is shown by
    /// (SF13.3c, F1). Only that world reads under the grant (`offer-read`)
    /// and returns to it (`offer-receive`). The same take told again answers
    /// as before; another world's is refused.
    pub fn taken_by(&self, request: &Value, world_ref: &str) -> Result<Value, OfferRefusal> {
        let object = object(request, "the take")?;
        only_keys(object, &["offer_ref", "by", "taken_at"], "offer-taken")?;
        let offer_ref = offer_ref_of(object)?;
        let by = self::object(object.get("by").unwrap_or(&Value::Null), "who took it")?;
        if by.contains_key("world_ref") {
            return Err(OfferRefusal::invalid(
                "offer-taken names the world that took the offer only beside the request (--by), as the field verified it: this request names a world of its own",
            ));
        }
        only_keys(by, &["name"], "who took it")?;
        let world_ref = reference(&json!(world_ref), "by")?;
        let name = text(by, "name", MAX_NAME_CHARS)?.to_owned();
        let taken_at = millis(object, "taken_at")?;
        self.transact(Side::Made, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Made))?;
            match record["taken"]["world_ref"].as_str() {
                Some(taker) if taker == world_ref => {}
                Some(_) => {
                    return Err(OfferRefusal::new(
                        "taken",
                        "Another world took this offer already; one world takes it",
                    ))
                }
                None => {
                    record["taken"] =
                        json!({"world_ref": world_ref, "name": name, "taken_at": taken_at});
                }
            }
            let answer = receipt("taken", &record, None);
            Ok((record, answer))
        })
    }

    /// `offer-read {offer_ref, ref[, after]}`, asked by `asker`: the bytes of
    /// one ref the grant lets the task read (what it reads or touches),
    /// served by the offerer's Factory from its own record of the grant to
    /// the world that took the offer, and to no other (O-I #858 SF13;
    /// Factory#312). `asker` is the field's verified identity of who asked,
    /// passed by the relay beside the request (`--asker`), never read from
    /// the request.
    ///
    /// A ref `project:<id>/<path>` names `<path>` under this placed project's
    /// root, where `<id>` is the project's key or its Central project's name.
    /// It is reached from the root one step at a time: each step is a name
    /// the folder holds byte for byte, opened through the folder's handle
    /// with no link followed. A file is answered whole, base64, at most
    /// `MAX_READ_BYTES`, read through its handle within that bound; a file
    /// with other names is refused, and so is a pipe, socket or device. A
    /// folder answers at most `MAX_READ_ENTRIES` entries in name order, after
    /// `after` when it is given, with `more` when others follow, and names no
    /// entry a read would refuse. A project's `.factory`, `.git`, `.hg`,
    /// `.svn` and `.jj` are never served, in any spelling: a grant names the
    /// work, not its machinery. Other hidden files (`.env`, `.npmrc`) are the
    /// work's: a grant of a folder or of the whole project serves them. No
    /// person act is needed per read; a revoked, lapsed or spent grant serves
    /// nothing.
    pub fn read_granted(
        &self,
        request: &Value,
        asker: &str,
        now: i64,
    ) -> Result<Value, OfferRefusal> {
        let object = object(request, "the read")?;
        only_keys(object, &["offer_ref", "ref", "after"], "offer-read")?;
        let offer_ref = offer_ref_of(object)?;
        let subject = reference(
            object
                .get("ref")
                .ok_or_else(|| OfferRefusal::invalid("a read names one `ref`"))?,
            "ref",
        )?;
        let after = object
            .get("after")
            .map(|after| reference(after, "after"))
            .transpose()?;
        // Read without naming where the record lies: a refusal goes back
        // toward the taker.
        let record = match read_native(&self.path(Side::Made, &offer_ref)) {
            Ok(bytes) => decode(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(unknown(&offer_ref, Side::Made))
            }
            Err(error) => return Err(io_refusal("could not read the offer's record", error)),
        };
        if record["offer"]["offer_ref"].as_str() != Some(offer_ref.as_str()) {
            return Err(unknown(&offer_ref, Side::Made));
        }
        match record["taken"]["world_ref"].as_str() {
            Some(taker) if taker == asker => {}
            Some(_) => {
                return Err(OfferRefusal::new(
                    "not_the_taker",
                    format!("Only the world that took this offer reads under its grant, and {asker} did not take it"),
                ))
            }
            None => {
                return Err(OfferRefusal::new(
                    "not_the_taker",
                    "No world has taken this offer yet; its grant serves only the world that takes it",
                ))
            }
        }
        let grant = &record["offer"]["grant"];
        match record["state"].as_str() {
            Some("open") => {}
            Some("revoked") => {
                return Err(OfferRefusal::new(
                    "revoked",
                    "The offer was revoked; its grant serves nothing",
                ))
            }
            Some("returned" | "accepted") => {
                return Err(OfferRefusal::new(
                    "spent",
                    "What the task made has come back; the grant is spent",
                ))
            }
            _ => {
                return Err(OfferRefusal::new(
                    "corrupt",
                    "The offer's record stands in no state this Factory knows; its grant serves nothing",
                ))
            }
        }
        let Some(lapses) = grant["lapses_at"].as_i64() else {
            return Err(OfferRefusal::new(
                "corrupt",
                "The offer's record names no time its grant lapses; its grant serves nothing",
            ));
        };
        if lapses <= now {
            return Err(OfferRefusal::new("lapsed", "The grant has lapsed"));
        }
        let mut granted = strings(&grant["touches"]);
        granted.extend(strings(&grant["reads"]));
        if !covers(&granted, &subject) {
            return Err(OfferRefusal::new(
                "not_granted",
                format!("The grant does not let the task read {subject}"),
            ));
        }
        let (root, base, steps) = self.granted_steps(&subject)?;
        let answer = json!({
            "schema": TASK_OFFER_READ,
            "offer_ref": offer_ref,
            "ref": subject,
            "automatic_agent_or_model_invocation": false,
        });
        serve(answer, &subject, &root, &base, &steps, after.as_deref())
    }

    /// Where a granted ref stands: this placed project's root, the ref as its
    /// entries are named (`project:<id>/<steps>`), and its steps. Refused
    /// unless `<state>` is the project's own placed state, named by its
    /// absolute path, the ref names this project, and every step is a plain
    /// name of the work. No refusal carries a local path.
    fn granted_steps(&self, subject: &str) -> Result<(PathBuf, String, Vec<String>), OfferRefusal> {
        let not_placed = |why: &str| OfferRefusal::new("not_placed", why.to_owned());
        let not_here = |why: String| OfferRefusal::new("not_here", why);
        let state = self.state_path()?;
        if !state.is_absolute() {
            return Err(not_placed("<state> is a relative path; offer-read serves only from a placed project's state named by its absolute path"));
        }
        let root = state
            .parent()
            .filter(|dir| dir.file_name().is_some_and(|name| name == ".factory"))
            .and_then(Path::parent)
            .ok_or_else(|| not_placed("<state> is not a placed project's (.factory/development-state.json), so no granted ref stands here"))?;
        // `locate`'s own words can name a local path.
        let located = crate::project_setup::locate(root)
            .map_err(|_| not_placed("this project's placement cannot be read"))?;
        let placed = std::fs::canonicalize(state).ok();
        if located["statePath"].as_str().map(PathBuf::from) != placed {
            return Err(not_placed(
                "the project's placement names another developmental state",
            ));
        }
        let root = placed_root(&located)?;
        let names: Vec<String> = [
            located["projectKey"].as_str().map(str::to_owned),
            located["centralProjectRef"]
                .as_str()
                .and_then(crate::build::central_project_name),
        ]
        .into_iter()
        .flatten()
        .collect();
        let body = subject
            .strip_prefix("project:")
            .ok_or_else(|| not_here(format!("{subject} names no project")))?;
        let (id, rest) = body.split_once('/').unwrap_or((body, ""));
        if !names.iter().any(|name| name == id) {
            return Err(not_here(format!(
                "{subject} names another project than this one"
            )));
        }
        let steps: Vec<String> = rest
            .split('/')
            .filter(|step| !step.is_empty())
            .map(str::to_owned)
            .collect();
        if !steps.iter().all(|step| plain_step(step)) {
            return Err(not_here(format!(
                "{subject} names no plain path of the project's work"
            )));
        }
        let base = std::iter::once(format!("project:{id}"))
            .chain(steps.iter().cloned())
            .collect::<Vec<_>>()
            .join("/");
        Ok((root, base, steps))
    }

    /// `offer-receive --by <world_ref> {return}`: what came back, as the
    /// field carried it, kept apart from the work until the person accepts
    /// it. `world_ref` is the world the field verified brought it back,
    /// which the relay names beside the request; only the world this
    /// Factory keeps as the one that took the offer (`offer-taken`) returns
    /// to it, under its own name (SF13.3c, F2). Any other return is refused
    /// before it is read, and spends nothing.
    pub fn receive(&self, request: &Value, world_ref: &str) -> Result<Value, OfferRefusal> {
        let object = object(request, "what came back")?;
        only_keys(object, &["return"], "offer-receive")?;
        let back = object.get("return").unwrap_or(&Value::Null);
        let offer_ref = offer_ref_of(self::object(back, "what came back")?)?;
        let world_ref = reference(&json!(world_ref), "by")?;
        self.transact(Side::Made, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Made))?;
            match record["taken"]["world_ref"].as_str() {
                Some(taker) if taker == world_ref => {}
                Some(_) => {
                    return Err(OfferRefusal::new(
                        "not_the_taker",
                        format!("Only the world that took this offer returns to it, and {world_ref} did not take it"),
                    ))
                }
                None => {
                    return Err(OfferRefusal::new(
                        "not_the_taker",
                        "This Factory keeps no world as the one that took this offer, so nothing comes back to it yet",
                    ))
                }
            }
            let back = checked_return(back, &record["offer"])?;
            if back["by"]["world_ref"].as_str() != Some(world_ref.as_str()) {
                return Err(OfferRefusal::new(
                    "not_the_taker",
                    "What came back names another world than the one that brought it back",
                ));
            }
            match record["state"].as_str() {
                Some("revoked") => {
                    return Err(OfferRefusal::new(
                        "revoked",
                        "The offer was revoked; nothing that comes back to it enters the work",
                    ))
                }
                Some("returned" | "accepted") if record["return"] == back => {
                    let answer = receipt("receive", &record, None);
                    return Ok((record, answer));
                }
                Some("returned" | "accepted") => {
                    return Err(OfferRefusal::new(
                        "returned",
                        "Something else came back to this offer already",
                    ))
                }
                _ => {}
            }
            if back["returned_at"].as_i64() > record["offer"]["grant"]["lapses_at"].as_i64() {
                return Err(OfferRefusal::new(
                    "lapsed",
                    "It came back after the grant lapsed; nothing of it enters the work",
                ));
            }
            record["state"] = json!("returned");
            record["return"] = back;
            let answer = receipt("receive", &record, None);
            Ok((record, answer))
        })
    }

    /// `offer-accept {offer_ref}`: what came back enters the Run, under its
    /// maker's name, as a candidate of the work.
    pub fn accept(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the acceptance")?;
        only_keys(
            object,
            &["offer_ref", "caller", "authority"],
            "offer-accept",
        )?;
        let (caller, authority) =
            require_person(object, OFFER_CAPABILITY_REF, "Accepting what came back")?;
        let offer_ref = offer_ref_of(object)?;
        let state_path = self.state_path()?.to_path_buf();
        self.transact(Side::Made, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Made))?;
            match record["state"].as_str() {
                Some("returned") => {}
                Some("accepted") => {
                    let answer = receipt("accept", &record, Some((&caller, &authority)));
                    return Ok((record, answer));
                }
                Some("revoked") => {
                    return Err(OfferRefusal::new(
                        "revoked",
                        "The offer was revoked; nothing of it enters the work",
                    ))
                }
                _ => {
                    return Err(OfferRefusal::new(
                        "nothing_returned",
                        "Nothing has come back to this offer to accept",
                    ))
                }
            }
            let back = record["return"].clone();
            let run_ref: RunRef = record["offer"]["programme"]["run_ref"]
                .as_str()
                .unwrap_or_default()
                .parse()
                .map_err(|_| {
                    OfferRefusal::new("corrupt", "The offer's run-ref could not be read")
                })?;
            let candidate_ref = format!("candidate:{}", offer_ref.trim_start_matches("offer:"));
            let label = format!(
                "{} — by {}",
                back["made"]["label"].as_str().unwrap_or_default(),
                back["by"]["name"].as_str().unwrap_or_default()
            );
            let candidate = CandidateRecord {
                run_ref,
                candidate_ref: candidate_ref.clone(),
                revision: 1,
                label,
                status: "accepted".into(),
                producing_execution_refs: Vec::new(),
                claim_refs: Vec::new(),
                evidence_refs: Vec::new(),
                artifact_refs: strings(&back["made"]["refs"]),
                preview_ref: None,
                tradeoffs: Vec::new(),
            };
            transact_developmental_state(&state_path, |state| {
                state.build.insert_candidate(candidate).map_err(|error| {
                    crate::project_development_store::ProjectDevelopmentStoreError::Native(
                        error.to_string(),
                    )
                })
            })
            .map_err(|error| {
                OfferRefusal::new(
                    "not_accepted",
                    format!("What came back could not enter the Run: {error}"),
                )
            })?;
            record["state"] = json!("accepted");
            record["accepted"] =
                json!({"candidate_ref": candidate_ref, "accepted_at": now, "by": caller});
            let answer = receipt("accept", &record, Some((&caller, &authority)));
            Ok((record, answer))
        })
    }
}

// ---- the taker's side ------------------------------------------------------------

impl OfferStore {
    /// `offer-take {offer, by: {world_ref, name}}`: the offer as the field
    /// showed it, taken up here by one world, under its grant.
    pub fn take(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the take")?;
        only_keys(
            object,
            &["offer", "by", "caller", "authority"],
            "offer-take",
        )?;
        let (caller, authority) = require_person(object, TAKE_CAPABILITY_REF, "Taking up a task")?;
        let offer = checked_offer(object.get("offer").unwrap_or(&Value::Null))?;
        let by = self::object(object.get("by").unwrap_or(&Value::Null), "who takes it")?;
        only_keys(by, &["world_ref", "name"], "who takes it")?;
        let world_ref = reference(by.get("world_ref").unwrap_or(&Value::Null), "world_ref")?;
        let name = text(by, "name", MAX_NAME_CHARS)?.to_owned();
        if offer["grant"]["lapses_at"].as_i64() <= Some(now) {
            return Err(OfferRefusal::new(
                "lapsed",
                "The offer's grant has lapsed; it can no longer be taken",
            ));
        }
        let offer_ref = offer["offer_ref"].as_str().unwrap_or_default().to_owned();
        self.transact(Side::Taken, &offer_ref, |current| {
            if let Some(record) = current {
                if record["offer"] == offer && record["taken"]["world_ref"] == json!(world_ref) {
                    let answer = receipt("take", &record, Some((&caller, &authority)));
                    return Ok((record, answer));
                }
                return Err(OfferRefusal::new(
                    "taken",
                    "This offer was taken here already",
                ));
            }
            let record = json!({"offer": offer, "state": "taken",
                "taken": {"world_ref": world_ref, "name": name, "taken_at": now}});
            let answer = receipt("take", &record, Some((&caller, &authority)));
            Ok((record, answer))
        })
    }

    fn taken(&self, offer_ref: &str) -> Result<Value, OfferRefusal> {
        let bytes = read_optional(&self.path(Side::Taken, offer_ref))?
            .ok_or_else(|| unknown(offer_ref, Side::Taken))?;
        decode(&bytes)
    }

    /// `offer-check {offer_ref, read | touch}`: whether the task may read or
    /// touch one thing now, under the grant. A read of the record alone.
    pub fn check(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the check")?;
        only_keys(object, &["offer_ref", "read", "touch"], "offer-check")?;
        let offer_ref = offer_ref_of(object)?;
        let (act, subject) = match (object.get("read"), object.get("touch")) {
            (Some(subject), None) => ("read", reference(subject, "read")?),
            (None, Some(subject)) => ("touch", reference(subject, "touch")?),
            _ => {
                return Err(OfferRefusal::invalid(
                    "a check names one `read` or one `touch`",
                ))
            }
        };
        let record = self.taken(&offer_ref)?;
        let grant = &record["offer"]["grant"];
        let why = if record["state"] == "revoked" {
            Some("The offerer revoked the grant".to_owned())
        } else if grant["lapses_at"].as_i64() <= Some(now) {
            Some("The grant has lapsed".to_owned())
        } else if record["state"] == "returned" {
            Some("What the task made has gone back; the grant is spent".to_owned())
        } else {
            // What the task touches it may read; what it reads it may not touch.
            let mut allowed = strings(&grant["touches"]);
            if act == "read" {
                allowed.extend(strings(&grant["reads"]));
            }
            (!covers(&allowed, &subject))
                .then(|| format!("The grant does not let the task {act} {subject}"))
        };
        Ok(json!({
            "schema": TASK_OFFER_CHECK,
            "offer_ref": offer_ref,
            "act": act,
            "subject": subject,
            "allowed": why.is_none(),
            "why": why,
            "automatic_agent_or_model_invocation": false,
        }))
    }

    /// `offer-revoked {offer_ref, revoked_at}`: the offerer's revocation, as
    /// the field told it; nothing is allowed after it.
    pub fn revoked(&self, request: &Value) -> Result<Value, OfferRefusal> {
        let object = object(request, "the revocation")?;
        only_keys(object, &["offer_ref", "revoked_at"], "offer-revoked")?;
        let offer_ref = offer_ref_of(object)?;
        let revoked_at = millis(object, "revoked_at")?;
        self.transact(Side::Taken, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Taken))?;
            if record["state"] != "revoked" {
                record["state"] = json!("revoked");
                record["revoked_at"] = json!(revoked_at);
            }
            let answer = receipt("revoked", &record, None);
            Ok((record, answer))
        })
    }

    /// `offer-return {offer_ref, made: {label, summary, refs}}`: what the task
    /// made, under the taker's name, within the grant, before it lapses. The
    /// same made brought back again answers the return already given, as a
    /// take, a receipt, an acceptance and a revocation answer their repeats:
    /// what went back here and never reached the field goes on from here.
    pub fn give_back(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "what was made")?;
        only_keys(object, &["offer_ref", "made"], "offer-return")?;
        let offer_ref = offer_ref_of(object)?;
        let made = object.get("made").unwrap_or(&Value::Null);
        self.transact(Side::Taken, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Taken))?;
            match record["state"].as_str() {
                Some("revoked") => {
                    return Err(OfferRefusal::new(
                        "revoked",
                        "The offerer revoked the grant; nothing goes back",
                    ))
                }
                Some("returned") => {
                    let again = checked_made(made, &record["offer"]["grant"])?;
                    if record["return"]["made"] == again {
                        let back = record["return"].clone();
                        return Ok((record, back));
                    }
                    return Err(OfferRefusal::new(
                        "returned",
                        "What the task made has gone back already",
                    ));
                }
                _ => {}
            }
            if record["offer"]["grant"]["lapses_at"].as_i64() <= Some(now) {
                return Err(OfferRefusal::new(
                    "lapsed",
                    "The grant has lapsed; nothing goes back",
                ));
            }
            let made = checked_made(made, &record["offer"]["grant"])?;
            let back = json!({
                "schema": TASK_OFFER_RETURN,
                "offer_ref": record["offer"]["offer_ref"],
                "star_ref": record["offer"]["star_ref"],
                "by": {"world_ref": record["taken"]["world_ref"], "name": record["taken"]["name"]},
                "made": made,
                "returned_at": now,
                "automatic_agent_or_model_invocation": false,
            });
            record["state"] = json!("returned");
            record["return"] = back.clone();
            Ok((record, back))
        })
    }

    /// `offers`: the offers this Factory made and those it took, as each stands.
    pub fn reading(&self) -> Result<Value, OfferRefusal> {
        Ok(json!({
            "schema": TASK_OFFERS_READING,
            "made": self.records(Side::Made)?,
            "taken": self.records(Side::Taken)?,
            "automatic_agent_or_model_invocation": false,
        }))
    }
}

// ---- the command ------------------------------------------------------------------

pub fn is_offer_command(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("development")
        && args
            .get(1)
            .is_some_and(|verb| VERBS.contains(&verb.as_str()))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// Process entry: `args` start at `development`.
pub fn main(args: &[String]) -> ExitCode {
    let json = args.iter().any(|argument| argument == "--json");
    let stripped: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|argument| argument.as_str() != "--json")
        .cloned()
        .collect();
    match execute(&stripped, None, now_ms()) {
        Ok(document) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&document).expect("an answer encodes")
            );
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&refusal.document())
                        .unwrap_or_else(|_| refusal.to_string())
                );
            } else {
                eprintln!("factory: {refusal}");
            }
            ExitCode::from(2)
        }
    }
}

/// `args` start at the verb, with `--json` removed; answers are documents.
pub fn execute(args: &[String], stdin: Option<&str>, now: i64) -> Result<Value, OfferRefusal> {
    let usage =
        |message: String| OfferRefusal::new("usage", format!("{message}; run: factory help"));
    let verb = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| usage("missing offer operation".into()))?;
    // Who acts on the offerer's record is named beside the request: the
    // field's verified identity of that world, passed by the relay, never
    // read from the request (`--asker` for `offer-read`; `--by` for
    // `offer-taken` and `offer-receive`, SF13.3c).
    let (flag, missing) = match verb {
        "offer-read" => ("--asker", "offer-read names who asks: --asker <world_ref>, the field's verified identity of who asked"),
        "offer-taken" => ("--by", "offer-taken names who took the offer: --by <world_ref>, the field's verified identity of the world that took it"),
        "offer-receive" => ("--by", "offer-receive names who brought it back: --by <world_ref>, the field's verified identity of the world that took the offer"),
        _ => ("", ""),
    };
    let mut args = args.to_vec();
    let who = match args.iter().position(|argument| argument == flag) {
        Some(at) if !flag.is_empty() => {
            let who = args
                .get(at + 1)
                .cloned()
                .ok_or_else(|| usage(format!("{flag} names a world_ref")))?;
            args.drain(at..=at + 1);
            Some(reference(&json!(who), &flag[2..])?)
        }
        _ => None,
    };
    if let Some(option) = args
        .iter()
        .skip(1)
        .find(|argument| argument.starts_with("--"))
    {
        return Err(usage(format!("unknown option {option}")));
    }
    if !flag.is_empty() && who.is_none() {
        return Err(usage(missing.into()));
    }
    let args = args.as_slice();
    let store_at = |state: &str| OfferStore::at(state);
    if verb == "offers" {
        let [_, state] = args else {
            return Err(usage("expected <state>".into()));
        };
        return store_at(state).reading();
    }
    let (state, source) = match args {
        [_, state] => (state, "-"),
        [_, state, source] => (state, source.as_str()),
        _ => return Err(usage("expected <state> and at most one request".into())),
    };
    let text = if source == "-" {
        match stdin {
            Some(text) => text.to_owned(),
            None => {
                let mut text = String::new();
                io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|error| io_refusal("could not read the request from stdin", error))?;
                text
            }
        }
    } else {
        std::fs::read_to_string(source).map_err(|error| {
            io_refusal(&format!("could not read the request file {source}"), error)
        })?
    };
    let request: Value = serde_json::from_str(&text)
        .map_err(|error| OfferRefusal::invalid(format!("the request is not JSON: {error}")))?;
    let store = store_at(state);
    match verb {
        "offer-make" => store.make(&request, now),
        "offer-revoke" => store.revoke(&request, now),
        "offer-receive" => store.receive(&request, who.as_deref().unwrap_or_default()),
        "offer-accept" => store.accept(&request, now),
        "offer-taken" => store.taken_by(&request, who.as_deref().unwrap_or_default()),
        "offer-read" => store.read_granted(&request, who.as_deref().unwrap_or_default(), now),
        "offer-take" => store.take(&request, now),
        "offer-check" => store.check(&request, now),
        "offer-revoked" => store.revoked(&request),
        "offer-return" => store.give_back(&request, now),
        other => Err(usage(format!("unknown offer operation `{other}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::create_developmental_conformance_state;

    const NOW: i64 = 1_793_000_000_000;
    const HOUR: i64 = 60 * 60 * 1000;

    fn person(capability: &str) -> Value {
        json!({
            "caller": {"callerRef": "oi:desk-offers", "projectionKind": "desktop-human", "lineage": ["oi:desk-offers"]},
            "authority": {"authorityRef": "central:native-action-authority#frank", "nativeOwner": "factory",
                "capabilityRef": capability, "capabilityGranted": true, "actionAuthorised": true},
        })
    }

    fn with(mut request: Value, capability: &str) -> Value {
        let person = person(capability);
        request["caller"] = person["caller"].clone();
        request["authority"] = person["authority"].clone();
        request
    }

    /// The offerer's programme (Factory's conformance Run and its unit) and an
    /// empty Factory for the taker, on another machine.
    struct Two {
        _dirs: (tempfile::TempDir, tempfile::TempDir),
        state: PathBuf,
        offerer: OfferStore,
        taker: OfferStore,
        run_ref: String,
        unit_ref: String,
    }

    fn two() -> Two {
        let offerer_dir = tempfile::tempdir().unwrap();
        let taker_dir = tempfile::tempdir().unwrap();
        let state = offerer_dir.path().join("state.json");
        let manifest = create_developmental_conformance_state(&state).unwrap();
        Two {
            offerer: OfferStore::at(&state),
            taker: OfferStore::at(taker_dir.path()),
            state,
            run_ref: manifest.run_ref.to_string(),
            unit_ref: manifest.workflow_unit_ref.to_string(),
            _dirs: (offerer_dir, taker_dir),
        }
    }

    fn make(two: &Two, lapses_at: i64) -> Value {
        make_granting(two, json!(["project:river/docs"]), lapses_at)
    }

    fn make_granting(two: &Two, reads: Value, lapses_at: i64) -> Value {
        let request = json!({"run_ref": two.run_ref, "workflow_unit_ref": two.unit_ref, "star_ref": "star:river-maps",
            "grant": {"reads": reads, "touches": ["project:river/maps"], "lapses_at": lapses_at}});
        two.offerer
            .make(&with(request, OFFER_CAPABILITY_REF), NOW)
            .unwrap()["offer"]
            .clone()
    }

    /// The offerer's project placed on its machine (`river`), holding what the
    /// grant names, with the offer made from it kept beside its state.
    fn served(two: &Two, offer: &Value) -> (tempfile::TempDir, OfferStore) {
        let dir = tempfile::tempdir().unwrap();
        crate::project_setup::setup(dir.path(), "river", None).unwrap();
        let root = dir.path();
        for (path, bytes) in [
            ("docs/brief.md", b"The brief.\n".to_vec()),
            ("docs/deep/note.md", b"A note.".to_vec()),
            ("docs/.git/config", b"[remote]".to_vec()),
            ("maps/delta", b"chart".to_vec()),
            ("README.md", b"Not granted.".to_vec()),
            ("docs/big.bin", vec![7; MAX_READ_BYTES as usize + 1]),
        ] {
            std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            std::fs::write(root.join(path), bytes).unwrap();
        }
        let store = OfferStore::at(root.join(".factory/development-state.json"));
        let offer_ref = offer["offer_ref"].as_str().unwrap();
        let kept = store.path(Side::Made, offer_ref);
        std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
        std::fs::copy(two.offerer.path(Side::Made, offer_ref), &kept).unwrap();
        told_taken(&store, offer_ref, "world:ada").unwrap();
        (dir, store)
    }

    /// The field's word that `world` took the offer, as the relay passes it.
    fn told_taken(store: &OfferStore, offer_ref: &str, world: &str) -> Result<Value, OfferRefusal> {
        store.taken_by(
            &json!({"offer_ref": offer_ref, "by": {"name": "Ada"}, "taken_at": NOW}),
            world,
        )
    }

    /// What came back, received by the offerer as the relay brings it: the
    /// take told first, then the return, each by the world the field
    /// verified (`world:ada`).
    fn received(two: &Two, back: &Value) -> Result<Value, OfferRefusal> {
        told_taken(
            &two.offerer,
            back["offer_ref"].as_str().unwrap(),
            "world:ada",
        )?;
        two.offerer.receive(&json!({"return": back}), "world:ada")
    }

    /// The kept record of the offer this Factory made, changed in place.
    fn change_kept(store: &OfferStore, offer_ref: &str, change: impl FnOnce(&mut Value)) {
        let kept = store.path(Side::Made, offer_ref);
        let mut record: Value = serde_json::from_slice(&std::fs::read(&kept).unwrap()).unwrap();
        change(&mut record);
        std::fs::write(&kept, encode(&record)).unwrap();
    }

    #[test]
    fn the_offerer_serves_the_bytes_of_a_granted_ref_from_its_own_record_and_nothing_else() {
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (dir, store) = served(&two, &offer);
        let read = |subject: &str, at: i64| {
            store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                at,
            )
        };
        let code = |subject: &str| read(subject, NOW).unwrap_err().code;

        let file = read("project:river/docs/brief.md", NOW).unwrap();
        assert_eq!(file["schema"], TASK_OFFER_READ);
        assert_eq!(file["kind"], "file");
        assert_eq!(file["byte_len"], 11);
        assert_eq!(file["content_base64"], "VGhlIGJyaWVmLgo=");
        use sha2::Digest;
        assert_eq!(
            file["sha256"],
            format!("{:x}", sha2::Sha256::digest(b"The brief.\n"))
        );
        assert_eq!(file["automatic_agent_or_model_invocation"], false);
        assert_eq!(
            read("project:river/maps/delta", NOW).unwrap()["content_base64"],
            "Y2hhcnQ=",
            "what the task touches it may read"
        );
        let folder = read("project:river/docs", NOW).unwrap();
        assert_eq!(folder["kind"], "folder");
        assert_eq!(
            folder["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["ref"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["project:river/docs/brief.md", "project:river/docs/deep"],
            "a folder answers its entries, never the repository's or Factory's own records, nor a file past the bound (F5)"
        );

        assert_eq!(code("project:river/README.md"), "not_granted");
        assert_eq!(code("project:river/docsets/x"), "not_granted");
        assert_eq!(code("project:river"), "not_granted");
        assert_eq!(code("project:river/docs/../README.md"), "not_here");
        assert_eq!(code("project:river/docs/.git/config"), "not_here");
        assert_eq!(code("project:river/docs/absent.md"), "not_here");
        assert_eq!(code("project:river/docs/big.bin"), "too_large");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("README.md"), dir.path().join("docs/link"))
                .unwrap();
            assert_eq!(
                code("project:river/docs/link"),
                "not_here",
                "a link is never followed out"
            );
        }
        assert_eq!(
            read("project:river/docs/brief.md", NOW + HOUR)
                .unwrap_err()
                .code,
            "lapsed"
        );

        // The taker's copy of the grant is never what serves: a wider grant
        // taken on this machine changes nothing the offerer's record allows.
        let mut wider = offer.clone();
        wider["grant"]["reads"] = json!(["project:river"]);
        store
            .take(
                &with(
                    json!({"offer": wider, "by": {"world_ref": "world:ada", "name": "Ada"}}),
                    TAKE_CAPABILITY_REF,
                ),
                NOW,
            )
            .unwrap();
        assert_eq!(code("project:river/README.md"), "not_granted");

        // Through the verb, as the field's relay asks it.
        let state = dir.path().join(".factory/development-state.json");
        let answer = execute(
            &[
                "offer-read".into(),
                state.display().to_string(),
                "--asker".into(),
                "world:ada".into(),
                "-".into(),
            ],
            Some(
                json!({"offer_ref": offer_ref, "ref": "project:river/docs/deep/note.md"})
                    .to_string()
                    .as_str(),
            ),
            NOW,
        )
        .unwrap();
        assert_eq!(answer["content_base64"], "QSBub3RlLg==");

        store
            .revoke(
                &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                NOW,
            )
            .unwrap();
        assert_eq!(code("project:river/docs/brief.md"), "revoked");
    }

    #[test]
    fn a_read_is_refused_where_the_state_is_no_placed_project_or_the_ref_names_another() {
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap();
        told_taken(&two.offerer, offer_ref, "world:ada").unwrap();
        let read = |store: &OfferStore, subject: &str| {
            store
                .read_granted(
                    &json!({"offer_ref": offer_ref, "ref": subject}),
                    "world:ada",
                    NOW,
                )
                .unwrap_err()
        };
        assert_eq!(
            read(&two.offerer, "project:river/docs").code,
            "not_placed",
            "a state that is no placed project's serves nothing"
        );
        let (dir, store) = served(&two, &offer);
        change_kept(&store, offer_ref, |record| {
            record["offer"]["grant"]["reads"] = json!(["project:delta/docs"]);
        });
        assert_eq!(
            read(&store, "project:delta/docs").code,
            "not_here",
            "a ref of another project is not this Factory's to serve"
        );
        change_kept(&store, offer_ref, |record| {
            record["offer"]["grant"]["reads"] = json!(["project:river/docs"]);
        });

        // A relative <state> is refused in its own words (SF13.3b).
        let relative = OfferStore {
            root: dir.path().join(".factory"),
            state_file: Some(PathBuf::from(".factory/development-state.json")),
        };
        let refused = read(&relative, "project:river/docs");
        assert_eq!(refused.code, "not_placed");
        assert!(refused.message.contains("relative path"), "{refused}");

        // A placement that cannot be read is refused without the local path
        // `locate` names (a refusal goes back toward the taker).
        let placement = dir.path().join(".factory/project.json");
        let copy = dir.path().join("placement-copy.json");
        std::fs::rename(&placement, &copy).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&copy, &placement).unwrap();
        let refused = read(&store, "project:river/docs");
        assert_eq!(refused.code, "not_placed");
        let local = dir.path().file_name().unwrap().to_str().unwrap();
        assert!(!refused.message.contains(local), "{refused}");
        assert!(!refused.message.contains("/private/"), "{refused}");

        // A placement that names no absolute root fails closed.
        for located in [
            json!({}),
            json!({"projectRoot": ""}),
            json!({"projectRoot": "relative/root"}),
        ] {
            assert_eq!(placed_root(&located).unwrap_err().code, "not_placed");
        }
        assert_eq!(
            placed_root(&json!({"projectRoot": "/a/root"})).unwrap(),
            PathBuf::from("/a/root")
        );
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd, 0xfc]), "//79/A==");
    }

    /// SF13.3b ruling 1: machinery in every spelling, by name, on any volume.
    #[test]
    fn machinery_is_known_in_every_spelling() {
        for name in [
            ".factory",
            ".git",
            ".hg",
            ".svn",
            ".jj",
            ".GIT",
            ".Git",
            ".FACTORY",
            ".Factory",
            ".HG",
            ".Svn",
            ".jJ",
            ".\u{17F}vn",
            ".git\u{200D}",
            ".g\u{200C}it",
            "\u{FEFF}.git",
            ".hg\u{FE0F}",
        ] {
            assert!(machinery(name), "{name:?} is machinery");
            assert!(!plain_step(name), "{name:?} is no plain step");
        }
        for name in [
            ".gitignore",
            ".github",
            "git",
            "factory",
            ".gi\u{0302}t",
            ".env",
            "docs",
        ] {
            assert!(!machinery(name), "{name:?} is the work's");
        }
        assert_eq!(folded("\u{212A}"), "k", "KELVIN SIGN decomposes to K");
    }

    /// SF13.3b rulings 1 and 4: under a grant of the whole project, its
    /// machinery is refused and never listed, in any spelling; a step names
    /// only what the folder holds, byte for byte, so a volume that folds
    /// case or normalises names opens no other spelling.
    #[test]
    fn a_whole_project_grant_never_serves_its_machinery_in_any_spelling() {
        let two = two();
        let offer = make_granting(&two, json!(["project:river"]), NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (dir, store) = served(&two, &offer);
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/config"), b"[remote]").unwrap();
        std::fs::write(dir.path().join("docs/caf\u{e9}.md"), b"composed").unwrap();
        let read = |subject: &str| {
            store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                NOW,
            )
        };
        for subject in [
            "project:river/.git/config",
            "project:river/.GIT/config",
            "project:river/.Git",
            "project:river/.factory/development-state.json",
            "project:river/.FACTORY/development-state.json",
            "project:river/.Factory/task-offers",
            "project:river/docs/.git/config",
            "project:river/DOCS/brief.md",
            "project:river/docs/cafe\u{301}.md",
        ] {
            assert_eq!(read(subject).unwrap_err().code, "not_here", "{subject}");
        }
        assert_eq!(
            read("project:river/docs/caf\u{e9}.md").unwrap()["content_base64"],
            "Y29tcG9zZWQ="
        );
        for subject in ["project:river", "project:river/", "project:river//"] {
            let listed: Vec<String> = read(subject).unwrap()["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["ref"].as_str().unwrap().to_owned())
                .collect();
            assert_eq!(
                listed,
                [
                    "project:river/README.md",
                    "project:river/docs",
                    "project:river/maps"
                ],
                "{subject}"
            );
        }
    }

    /// SF13.3b ruling 3: only the world the field says took the offer reads.
    #[test]
    fn only_the_world_that_took_the_offer_reads_under_its_grant() {
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (dir, store) = served(&two, &offer);
        let read = |asker: &str| {
            store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": "project:river/docs/brief.md"}),
                asker,
                NOW,
            )
        };
        assert!(read("world:ada").is_ok());
        let refused = read("world:bob").unwrap_err();
        assert_eq!(refused.code, "not_the_taker");
        assert!(
            refused.message.contains("world:bob did not take it"),
            "{refused}"
        );
        assert_eq!(
            told_taken(&store, &offer_ref, "world:bob")
                .unwrap_err()
                .code,
            "taken",
            "one world takes an offer"
        );
        assert_eq!(
            told_taken(&store, &offer_ref, "world:ada").unwrap()["taken"]["world_ref"],
            "world:ada",
            "the same take told again answers as before"
        );
        change_kept(&store, &offer_ref, |record| {
            record.as_object_mut().unwrap().remove("taken");
        });
        let refused = read("world:ada").unwrap_err();
        assert_eq!(refused.code, "not_the_taker");
        assert!(refused.message.contains("No world has taken"), "{refused}");
        told_taken(&store, &offer_ref, "world:ada").unwrap();

        // The asker is passed beside the request, never read from it.
        let state = dir.path().join(".factory/development-state.json");
        let through = |extra: &[&str], request: Value| {
            let mut args = vec!["offer-read".to_owned(), state.display().to_string()];
            args.extend(extra.iter().map(|word| (*word).to_owned()));
            args.push("-".into());
            execute(&args, Some(request.to_string().as_str()), NOW)
        };
        let request = json!({"offer_ref": offer_ref, "ref": "project:river/docs/brief.md"});
        assert_eq!(through(&[], request.clone()).unwrap_err().code, "usage");
        assert_eq!(
            through(&["--asker", "world:bob"], request.clone())
                .unwrap_err()
                .code,
            "not_the_taker"
        );
        let mut naming = request.clone();
        naming["asker"] = json!("world:ada");
        assert_eq!(
            through(&["--asker", "world:bob"], naming).unwrap_err().code,
            "invalid"
        );
        assert_eq!(
            through(&["--asker", "world:ada"], request).unwrap()["content_base64"],
            "VGhlIGJyaWVmLgo="
        );
    }

    /// SF13.3b ruling 4 (review 5): a spent grant, and a record in no state
    /// Factory knows, serve nothing.
    #[test]
    fn a_spent_grant_or_an_unknown_record_serves_nothing() {
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (_dir, store) = served(&two, &offer);
        let code = || {
            store
                .read_granted(
                    &json!({"offer_ref": offer_ref, "ref": "project:river/docs/brief.md"}),
                    "world:ada",
                    NOW,
                )
                .map_or_else(|refusal| refusal.code, |_| "served")
        };
        assert_eq!(code(), "served");
        for (state, expected) in [
            (json!("returned"), "spent"),
            (json!("accepted"), "spent"),
            (json!("taken"), "corrupt"),
            (Value::Null, "corrupt"),
            (json!("open"), "served"),
        ] {
            change_kept(&store, &offer_ref, |record| record["state"] = state.clone());
            assert_eq!(code(), expected, "{state}");
        }
        change_kept(&store, &offer_ref, |record| {
            record["offer"]["grant"]
                .as_object_mut()
                .unwrap()
                .remove("lapses_at");
        });
        assert_eq!(code(), "corrupt", "no lapse time");
    }

    /// SF13.3b ruling 2: each step opened with no link followed, a link
    /// swapped in between the check and the open included.
    #[cfg(unix)]
    #[test]
    fn a_read_follows_no_link_at_any_step_even_one_swapped_in() {
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (dir, store) = served(&two, &offer);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.md"), b"outside").unwrap();
        std::fs::write(outside.path().join("note.md"), b"outside").unwrap();
        let docs = dir.path().join("docs");
        std::os::unix::fs::symlink(outside.path(), docs.join("out")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("README.md"), docs.join("link")).unwrap();
        let read = |subject: &str| {
            store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                NOW,
            )
        };
        for subject in [
            "project:river/docs/out/secret.md",
            "project:river/docs/out",
            "project:river/docs/link",
        ] {
            let refused = read(subject).unwrap_err();
            assert_eq!(refused.code, "not_here", "{subject}");
            assert!(refused.message.contains("link"), "{refused}");
        }
        let listed: Vec<String> = read("project:river/docs").unwrap()["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["ref"].as_str().unwrap().to_owned())
            .collect();
        assert!(
            !listed
                .iter()
                .any(|entry| entry.ends_with("/out") || entry.ends_with("/link")),
            "{listed:?}"
        );

        // `docs/deep` is a folder when its name is checked, and a link to
        // the outside when it is opened.
        let swapped = outside.path().to_path_buf();
        let deep = docs.join("deep");
        READ_OBSERVER.with(|observer| {
            *observer.borrow_mut() = Some(Box::new(move |stage: &str| {
                if stage == "open docs/deep" {
                    std::fs::rename(&deep, deep.with_file_name("deep-moved")).unwrap();
                    std::os::unix::fs::symlink(&swapped, &deep).unwrap();
                }
            }));
        });
        let refused = read("project:river/docs/deep/note.md").unwrap_err();
        READ_OBSERVER.with(|observer| *observer.borrow_mut() = None);
        assert_eq!(refused.code, "not_here");
        assert!(refused.message.contains("link"), "{refused}");
    }

    /// F9 (review-factory-313b): a step that is a pipe when it is opened, as
    /// one swapped in after its stat would be, is never waited on. The open
    /// itself does not block: asked of a pipe with no writer, it answers at
    /// once, and a watchdog fails it red within its bound, never by hanging.
    #[cfg(unix)]
    #[test]
    fn a_step_opened_on_a_pipe_with_no_writer_answers_at_once() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let pipe = std::ffi::CString::new(dir.path().join("pipe").as_os_str().as_bytes()).unwrap();
        // SAFETY: a live NUL-terminated path; mkfifo only makes the node.
        assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o644) }, 0);
        let root = dir.path().to_owned();
        let (sent, answer) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let parent = crate::native_file_transaction::open_directory(&root).unwrap();
            let opened =
                crate::native_file_transaction::open_step(&parent, std::ffi::OsStr::new("pipe"));
            let _ = sent.send(opened.map(|_| ()).map_err(|error| error.to_string()));
        });
        answer
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("the open waited on a pipe with no writer")
            .expect("a pipe with no writer opens at once, to be refused by what it is");
    }

    /// SF13.3b ruling 2: only a regular file with one name, read through its
    /// handle within the bound; the size is checked before any byte is read.
    #[cfg(unix)]
    #[test]
    fn a_read_serves_only_a_regular_file_with_one_name_within_the_bound() {
        use std::os::unix::ffi::OsStrExt;
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (dir, store) = served(&two, &offer);
        let docs = dir.path().join("docs");
        let pipe = std::ffi::CString::new(docs.join("pipe").as_os_str().as_bytes()).unwrap();
        // SAFETY: a live NUL-terminated path; mkfifo only makes the node.
        assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o644) }, 0);
        let _socket = std::os::unix::net::UnixListener::bind(docs.join("sock")).unwrap();
        std::fs::hard_link(dir.path().join("README.md"), docs.join("alias.md")).unwrap();
        std::fs::write(docs.join("max.bin"), vec![1; MAX_READ_BYTES as usize]).unwrap();
        std::fs::write(docs.join("grows.bin"), b"short").unwrap();
        let read = |subject: &str| {
            store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                NOW,
            )
        };

        // A pipe or a socket is refused in its own words, and the read
        // never waits on it: a watchdog fails it red within its bound,
        // never by hanging (F4, F9).
        let state = dir.path().join(".factory/development-state.json");
        for special in ["project:river/docs/pipe", "project:river/docs/sock"] {
            let (sent, answer) = std::sync::mpsc::channel();
            let (state, offer_ref) = (state.clone(), offer_ref.clone());
            std::thread::spawn(move || {
                let _ = sent.send(OfferStore::at(&state).read_granted(
                    &json!({"offer_ref": offer_ref, "ref": special}),
                    "world:ada",
                    NOW,
                ));
            });
            let refused = answer
                .recv_timeout(std::time::Duration::from_secs(20))
                .unwrap_or_else(|_| panic!("a read of {special} waited on it"))
                .unwrap_err();
            assert_eq!(refused.code, "not_here", "{special}");
            assert!(
                refused.message.contains("a pipe, a socket or a device"),
                "{special}: {refused}"
            );
        }
        let refused = read("project:river/docs/alias.md").unwrap_err();
        assert_eq!(refused.code, "other_names");
        assert!(
            refused
                .message
                .contains("this file has other names, outside what was granted"),
            "{refused}"
        );
        assert_eq!(
            read("project:river/docs/max.bin").unwrap()["byte_len"],
            MAX_READ_BYTES,
            "exactly the bound is served"
        );
        let refused = read("project:river/docs/big.bin").unwrap_err();
        assert_eq!(refused.code, "too_large");
        assert!(
            refused
                .message
                .contains(&format!("holds {} bytes", MAX_READ_BYTES + 1)),
            "one byte past the bound is refused by its size, before any byte is read: {refused}"
        );
        let listed: Vec<String> = read("project:river/docs").unwrap()["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["ref"].as_str().unwrap().to_owned())
            .collect();
        assert!(
            !listed.iter().any(|entry| {
                entry.ends_with("/pipe")
                    || entry.ends_with("/sock")
                    || entry.ends_with("/alias.md")
                    || entry.ends_with("/big.bin")
            }),
            "{listed:?}"
        );
        assert!(
            listed.contains(&"project:river/docs/max.bin".to_owned()),
            "a file of exactly the bound is named: {listed:?}"
        );

        // The file grows past the bound after its size is read.
        let grows = docs.join("grows.bin");
        READ_OBSERVER.with(|observer| {
            *observer.borrow_mut() = Some(Box::new(move |stage: &str| {
                if stage == "read" {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .append(true)
                        .open(&grows)
                        .unwrap();
                    file.write_all(&vec![2; MAX_READ_BYTES as usize]).unwrap();
                }
            }));
        });
        let refused = read("project:river/docs/grows.bin").unwrap_err();
        READ_OBSERVER.with(|observer| *observer.borrow_mut() = None);
        assert_eq!(refused.code, "too_large");
        assert!(refused.message.contains("grew past"), "{refused}");

        // Never read to the end: one byte past the bound at most.
        let mut source = std::io::Cursor::new(vec![7u8; 4096]);
        assert_eq!(read_bounded(&mut source, 1024).unwrap(), None);
        assert!(source.position() <= 1025, "read {}", source.position());
        let mut exact = std::io::Cursor::new(vec![7u8; 1024]);
        assert_eq!(
            read_bounded(&mut exact, 1024)
                .unwrap()
                .map(|bytes| bytes.len()),
            Some(1024)
        );
    }

    /// SF13.3b rulings 4 and 5: a folder past its bound is paged with
    /// `after`; a listing names no ref a read of it would refuse.
    #[cfg(unix)]
    #[test]
    fn a_folder_is_paged_past_its_bound_and_names_no_ref_a_read_refuses() {
        use std::os::unix::ffi::OsStrExt;
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (dir, store) = served(&two, &offer);
        let many = dir.path().join("docs/many");
        std::fs::create_dir_all(&many).unwrap();
        for index in 0..600 {
            std::fs::write(many.join(format!("f{index:03}")), b"x").unwrap();
        }
        let odd = dir.path().join("docs/odd");
        std::fs::create_dir_all(odd.join(".GIT")).unwrap();
        for name in [
            "plain.md",
            "my notes.md",
            "localhost.md",
            "session-space.md",
            "back\\slash.md",
        ] {
            std::fs::write(odd.join(name), b"x").unwrap();
        }
        // A name that is no text, where the volume allows one.
        let _ = std::fs::write(
            odd.join(std::ffi::OsStr::from_bytes(b"bytes-\xff.md")),
            b"x",
        );
        let read = |request: Value| {
            let mut request = request;
            request["offer_ref"] = json!(offer_ref);
            store.read_granted(&request, "world:ada", NOW)
        };
        let refs = |answer: &Value| -> Vec<String> {
            answer["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["ref"].as_str().unwrap().to_owned())
                .collect()
        };

        let first = read(json!({"ref": "project:river/docs/many"})).unwrap();
        assert_eq!(refs(&first).len(), MAX_READ_ENTRIES);
        assert_eq!(first["more"], true);
        let last = refs(&first).last().unwrap().clone();
        assert_eq!(last, "project:river/docs/many/f511");
        let second = read(json!({"ref": "project:river/docs/many", "after": last})).unwrap();
        assert_eq!(refs(&second).len(), 600 - MAX_READ_ENTRIES);
        assert_eq!(second["more"], false);
        let all: Vec<String> = refs(&first).into_iter().chain(refs(&second)).collect();
        let expected: Vec<String> = (0..600)
            .map(|index| format!("project:river/docs/many/f{index:03}"))
            .collect();
        assert_eq!(all, expected, "every entry once, in name order");

        let odd_refs = refs(&read(json!({"ref": "project:river/docs/odd/"})).unwrap());
        assert_eq!(odd_refs, ["project:river/docs/odd/plain.md"]);
        assert_eq!(
            read(json!({"ref": "project:river/docs/brief.md", "after": "project:river/docs/a"}))
                .unwrap_err()
                .code,
            "invalid",
            "`after` pages a folder"
        );
    }

    fn take(two: &Two, offer: &Value, at: i64) -> Result<Value, OfferRefusal> {
        let request = json!({"offer": offer, "by": {"world_ref": "world:ada", "name": "Ada"}});
        two.taker.take(&with(request, TAKE_CAPABILITY_REF), at)
    }

    fn made(refs: Value) -> Value {
        json!({"label": "The delta chart", "summary": "Charted the delta's channels.", "refs": refs})
    }

    fn candidates(two: &Two) -> Vec<String> {
        read_developmental_state(&two.state)
            .unwrap()
            .run_reading(&two.run_ref.parse().unwrap())
            .unwrap()
            .candidates
            .into_iter()
            .map(|candidate| format!("{} [{}]", candidate.label, candidate.status))
            .collect()
    }

    fn allowed(two: &Two, offer_ref: &Value, act: &str, subject: &str, at: i64) -> (bool, Value) {
        let check = two
            .taker
            .check(&json!({"offer_ref": offer_ref, act: subject}), at)
            .unwrap();
        (check["allowed"] == true, check["why"].clone())
    }

    #[test]
    fn an_offer_is_taken_worked_under_its_grant_and_comes_back_under_the_takers_name_until_accepted(
    ) {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        assert_eq!(offer["schema"], TASK_OFFER);
        assert_eq!(offer["star_ref"], "star:river-maps");
        assert!(
            offer["task"]["asks"]
                .as_str()
                .is_some_and(|asks| !asks.is_empty()),
            "the task asks what its unit asks: {offer}"
        );
        assert_eq!(
            offer["grant"],
            json!({"reads": ["project:river/docs"], "touches": ["project:river/maps"], "lapses_at": NOW + 24 * HOUR})
        );
        let offer_ref = offer["offer_ref"].clone();
        // The offer travels as the field carries it: a document, whole.
        let carried: Value = serde_json::from_str(&offer.to_string()).unwrap();
        let taken = take(&two, &carried, NOW + HOUR).unwrap();
        assert_eq!(
            (taken["act"].as_str(), taken["state"].as_str()),
            (Some("take"), Some("taken"))
        );
        assert_eq!(taken["taken"]["name"], "Ada");

        // On the taker's machine the task reads and touches what the grant names, and nothing else.
        assert!(
            allowed(
                &two,
                &offer_ref,
                "read",
                "project:river/docs/brief.md",
                NOW + HOUR
            )
            .0
        );
        assert!(
            allowed(
                &two,
                &offer_ref,
                "touch",
                "project:river/maps/delta",
                NOW + HOUR
            )
            .0
        );
        assert!(
            allowed(&two, &offer_ref, "read", "project:river/maps", NOW + HOUR).0,
            "what it touches it reads"
        );
        let (touch_docs, why) = allowed(
            &two,
            &offer_ref,
            "touch",
            "project:river/docs/brief.md",
            NOW + HOUR,
        );
        assert!(!touch_docs, "what it may only read it may not touch");
        assert!(why
            .as_str()
            .unwrap()
            .contains("does not let the task touch"));
        assert!(
            !allowed(&two, &offer_ref, "read", "project:river", NOW + HOUR).0,
            "nothing above what is granted"
        );
        assert!(
            !allowed(
                &two,
                &offer_ref,
                "read",
                "project:river/docsets",
                NOW + HOUR
            )
            .0,
            "a name sharing a prefix is not under it"
        );

        let back = two
            .taker
            .give_back(
                &json!({"offer_ref": offer_ref, "made": made(json!(["project:river/maps/delta"]))}),
                NOW + 2 * HOUR,
            )
            .unwrap();
        assert_eq!(back["schema"], TASK_OFFER_RETURN);
        assert_eq!(
            back["by"],
            json!({"world_ref": "world:ada", "name": "Ada"}),
            "what it made comes back under the taker's name"
        );

        let before = candidates(&two);
        let received = received(&two, &back).unwrap();
        assert_eq!(received["state"], "returned");
        assert_eq!(
            candidates(&two),
            before,
            "nothing they made enters the work until it is accepted"
        );
        let accepted = two
            .offerer
            .accept(
                &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                NOW + 3 * HOUR,
            )
            .unwrap();
        assert_eq!(accepted["state"], "accepted");
        let after = candidates(&two);
        assert!(
            after.contains(&"The delta chart — by Ada [accepted]".to_owned()),
            "accepted, it enters the Run under its maker's name: {after:?}"
        );
        assert_eq!(after.len(), before.len() + 1);
        let again = two
            .offerer
            .accept(
                &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                NOW + 4 * HOUR,
            )
            .unwrap();
        assert_eq!(
            again["accepted"], accepted["accepted"],
            "accepting twice enters it once"
        );
        assert_eq!(candidates(&two).len(), after.len());
    }

    #[test]
    fn a_revoked_grant_allows_nothing_and_nothing_of_it_enters_the_work() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].clone();
        take(&two, &offer, NOW + HOUR).unwrap();
        // Made before the taker heard of the revocation (on a machine that was away).
        let away = OfferStore::at(two._dirs.1.path());
        let back = away
            .give_back(
                &json!({"offer_ref": offer_ref, "made": made(json!([]))}),
                NOW + 2 * HOUR,
            )
            .unwrap();
        let revoked = two
            .offerer
            .revoke(
                &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                NOW + 3 * HOUR,
            )
            .unwrap();
        assert_eq!(revoked["state"], "revoked");
        let refused = received(&two, &back).unwrap_err();
        assert_eq!(
            refused.code, "revoked",
            "what comes back to a revoked offer is refused: {refused}"
        );
        assert_eq!(
            two.offerer
                .accept(
                    &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                    NOW + 4 * HOUR
                )
                .unwrap_err()
                .code,
            "revoked"
        );
        assert!(!candidates(&two)
            .iter()
            .any(|candidate| candidate.contains("Ada")));

        // The taker's Factory, told by the field, allows nothing after.
        let (_third, other) = taken_elsewhere(&offer);
        other
            .revoked(&json!({"offer_ref": offer_ref, "revoked_at": NOW + 3 * HOUR}))
            .unwrap();
        let check = other
            .check(
                &json!({"offer_ref": offer_ref, "read": "project:river/docs"}),
                NOW + 4 * HOUR,
            )
            .unwrap();
        assert_eq!(
            (check["allowed"].as_bool(), check["why"].as_str()),
            (Some(false), Some("The offerer revoked the grant"))
        );
        assert_eq!(
            other
                .give_back(
                    &json!({"offer_ref": offer_ref, "made": made(json!([]))}),
                    NOW + 4 * HOUR
                )
                .unwrap_err()
                .code,
            "revoked"
        );
    }

    #[test]
    fn bringing_back_the_same_made_again_answers_the_return_already_given_and_another_made_is_refused(
    ) {
        // The taker's Factory gave it back, and the field did not hear: the person brings
        // back the same again, and the field must be given what this Factory gave.
        let two = two();
        let offer = make(&two, NOW + 2 * HOUR);
        let offer_ref = offer["offer_ref"].clone();
        take(&two, &offer, NOW + HOUR).unwrap();
        let request = json!({"offer_ref": offer_ref, "made": made(json!(["project:river/maps"]))});
        let back = two.taker.give_back(&request, NOW + HOUR).unwrap();
        assert_eq!(
            two.taker.give_back(&request, NOW + HOUR + 60_000).unwrap(),
            back,
            "a repeat answers the return already given, at the time it was given"
        );
        assert_eq!(
            two.taker.give_back(&request, NOW + 3 * HOUR).unwrap(),
            back,
            "it went back before the grant lapsed, so a repeat after still answers it"
        );
        let other = json!({"offer_ref": offer_ref, "made": {"label": "Another chart", "summary": "Something else.", "refs": []}});
        assert_eq!(
            two.taker.give_back(&other, NOW + HOUR).unwrap_err().code,
            "returned",
            "what was made went back already; another made is refused"
        );
        let theirs = two.taker.reading().unwrap();
        assert_eq!(
            theirs["taken"][0]["return"], back,
            "the record keeps the one return"
        );
        let received = received(&two, &back).unwrap();
        assert_eq!(received["state"], "returned");
    }

    /// The same offer taken on a third machine, which has heard nothing yet.
    fn taken_elsewhere(offer: &Value) -> (tempfile::TempDir, OfferStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = OfferStore::at(dir.path());
        store
            .take(
                &with(
                    json!({"offer": offer, "by": {"world_ref": "world:bo", "name": "Bo"}}),
                    TAKE_CAPABILITY_REF,
                ),
                NOW + HOUR,
            )
            .unwrap();
        (dir, store)
    }

    #[test]
    fn a_grant_lapses_when_it_said_it_would() {
        let two = two();
        let offer = make(&two, NOW + 2 * HOUR);
        let offer_ref = offer["offer_ref"].clone();
        take(&two, &offer, NOW + HOUR).unwrap();
        assert!(allowed(&two, &offer_ref, "read", "project:river/docs", NOW + HOUR).0);
        let (late, why) = allowed(
            &two,
            &offer_ref,
            "read",
            "project:river/docs",
            NOW + 2 * HOUR,
        );
        assert!(!late);
        assert_eq!(why, "The grant has lapsed");
        assert_eq!(
            two.taker
                .give_back(
                    &json!({"offer_ref": offer_ref, "made": made(json!([]))}),
                    NOW + 3 * HOUR
                )
                .unwrap_err()
                .code,
            "lapsed"
        );
        let late_dir = tempfile::tempdir().unwrap();
        let late_take = OfferStore::at(late_dir.path());
        assert_eq!(
            late_take
                .take(
                    &with(
                        json!({"offer": offer, "by": {"world_ref": "world:bo", "name": "Bo"}}),
                        TAKE_CAPABILITY_REF
                    ),
                    NOW + 2 * HOUR
                )
                .unwrap_err()
                .code,
            "lapsed"
        );
        // A return dated after the lapse is refused by the offerer whatever the taker's Factory said.
        let late_back = json!({"schema": TASK_OFFER_RETURN, "offer_ref": offer_ref, "star_ref": "star:river-maps",
            "by": {"world_ref": "world:ada", "name": "Ada"}, "made": made(json!([])), "returned_at": NOW + 3 * HOUR});
        assert_eq!(received(&two, &late_back).unwrap_err().code, "lapsed");
    }

    #[test]
    fn what_an_offer_carries_and_what_comes_back_stay_within_the_grant_and_name_no_machine() {
        let two = two();
        let offer = make(&two, NOW + 2 * HOUR);
        let offer_ref = offer["offer_ref"].clone();
        take(&two, &offer, NOW + HOUR).unwrap();
        let outside = two.taker.give_back(&json!({"offer_ref": offer_ref, "made": made(json!(["project:river/docs/brief.md"]))}), NOW + HOUR).unwrap_err();
        assert_eq!(outside.code, "outside_grant", "{outside}");
        let leaky = json!({"offer_ref": offer_ref, "made": {"label": "Notes at /Users/ada/notes.md", "summary": "x", "refs": []}});
        assert_eq!(
            two.taker.give_back(&leaky, NOW + HOUR).unwrap_err().code,
            "unshown_material"
        );
        for grant in [
            json!({"reads": ["workcell:ada-mac"], "touches": [], "lapses_at": NOW + HOUR}),
            json!({"reads": ["project:river/docs"], "touches": ["file:///Users/frank"], "lapses_at": NOW + HOUR}),
        ] {
            let request = json!({"run_ref": two.run_ref, "workflow_unit_ref": two.unit_ref, "star_ref": "star:river-maps", "grant": grant});
            assert_eq!(
                two.offerer
                    .make(&with(request, OFFER_CAPABILITY_REF), NOW)
                    .unwrap_err()
                    .code,
                "unshown_material"
            );
        }
        for (grant, code) in [
            (
                json!({"reads": [], "touches": [], "lapses_at": NOW + HOUR}),
                "invalid",
            ),
            (
                json!({"reads": ["project:river/docs"], "lapses_at": NOW - 1}),
                "grant_span",
            ),
            (
                json!({"reads": ["project:river/docs"], "lapses_at": NOW + 31 * 24 * HOUR}),
                "grant_span",
            ),
        ] {
            let request = json!({"run_ref": two.run_ref, "workflow_unit_ref": two.unit_ref, "star_ref": "star:river-maps", "grant": grant});
            assert_eq!(
                two.offerer
                    .make(&with(request, OFFER_CAPABILITY_REF), NOW)
                    .unwrap_err()
                    .code,
                code
            );
        }
        let unknown_unit = json!({"run_ref": two.run_ref, "workflow_unit_ref": "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FZZ", "star_ref": "star:river-maps",
            "grant": {"reads": ["project:river/docs"], "lapses_at": NOW + HOUR}});
        assert!(two
            .offerer
            .make(&with(unknown_unit, OFFER_CAPABILITY_REF), NOW)
            .is_err());
    }

    #[test]
    fn offering_taking_revoking_and_accepting_are_a_persons_acts() {
        let two = two();
        let request = json!({"run_ref": two.run_ref, "workflow_unit_ref": two.unit_ref, "star_ref": "star:river-maps",
            "grant": {"reads": ["project:river/docs"], "lapses_at": NOW + HOUR}});
        assert_eq!(
            two.offerer.make(&request, NOW).unwrap_err().code,
            "not_human"
        );
        assert_eq!(
            two.offerer
                .make(&with(request.clone(), TAKE_CAPABILITY_REF), NOW)
                .unwrap_err()
                .code,
            "not_authorised"
        );
        let mut agent = with(request, OFFER_CAPABILITY_REF);
        agent["caller"]["projectionKind"] = json!("agent");
        assert!(two.offerer.make(&agent, NOW).is_err());
        let offer = make(&two, NOW + HOUR);
        let take_as_offerer = with(
            json!({"offer": offer, "by": {"world_ref": "world:ada", "name": "Ada"}}),
            OFFER_CAPABILITY_REF,
        );
        assert_eq!(
            two.taker.take(&take_as_offerer, NOW).unwrap_err().code,
            "not_authorised"
        );
        assert_eq!(
            two.offerer
                .revoke(&json!({"offer_ref": offer["offer_ref"]}), NOW)
                .unwrap_err()
                .code,
            "not_human"
        );
        assert!(
            two.offerer.reading().unwrap()["made"][0]["state"] == "open",
            "nothing changed"
        );
    }

    /// SF13.3c F1: the take is kept under the world the relay names beside
    /// the request (`--by`), never one the request names.
    #[test]
    fn a_take_is_kept_under_the_world_named_beside_it_never_one_the_request_names() {
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let naming = json!({"offer_ref": offer_ref, "by": {"world_ref": "world:mallory", "name": "Ada"}, "taken_at": NOW});
        let refused = two.offerer.taken_by(&naming, "world:ada").unwrap_err();
        assert_eq!(refused.code, "invalid");
        assert!(
            refused.message.contains("only beside the request (--by)"),
            "{refused}"
        );
        assert_eq!(
            two.offerer.reading().unwrap()["made"][0]["taken"],
            Value::Null
        );

        let state = two.state.display().to_string();
        let through = |verb: &str, extra: &[&str], request: &Value| {
            let mut args = vec![verb.to_owned(), state.clone()];
            args.extend(extra.iter().map(|word| (*word).to_owned()));
            args.push("-".into());
            execute(&args, Some(request.to_string().as_str()), NOW)
        };
        let take = json!({"offer_ref": offer_ref, "by": {"name": "Ada"}, "taken_at": NOW});
        let usage = through("offer-taken", &[], &take).unwrap_err();
        assert_eq!(usage.code, "usage");
        assert!(usage.message.contains("--by <world_ref>"), "{usage}");
        assert!(
            through("offer-taken", &["--by"], &take).is_err(),
            "--by names one world"
        );
        assert_eq!(
            through("offer-taken", &["--asker", "world:ada"], &take)
                .unwrap_err()
                .code,
            "usage"
        );
        assert_eq!(
            through("offer-taken", &["--by", "-world:ada"], &take)
                .unwrap_err()
                .code,
            "invalid"
        );
        assert_eq!(
            through("offer-taken", &["--by", "world:mallory"], &naming)
                .unwrap_err()
                .code,
            "invalid"
        );
        let kept = through("offer-taken", &["--by", "world:ada"], &take).unwrap();
        assert_eq!(
            kept["taken"],
            json!({"world_ref": "world:ada", "name": "Ada", "taken_at": NOW})
        );
        let revoke = with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF);
        assert_eq!(
            through("offer-revoke", &["--by", "world:ada"], &revoke)
                .unwrap_err()
                .code,
            "usage",
            "only the relay's verbs name a world beside the request"
        );
    }

    /// SF13.3c F2: only the world kept as the one that took the offer
    /// returns to it; any other return is refused and spends nothing.
    #[test]
    fn only_the_world_that_took_the_offer_returns_to_it_and_another_spends_nothing() {
        let two = two();
        let offer = make(&two, NOW + 2 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let back = two
            .taker
            .give_back(
                &json!({"offer_ref": offer_ref, "made": made(json!(["project:river/maps/delta"]))}),
                NOW + HOUR,
            )
            .unwrap();
        let receive = |back: &Value, by: &str| two.offerer.receive(&json!({"return": back}), by);
        let refused = receive(&back, "world:ada").unwrap_err();
        assert_eq!(refused.code, "not_the_taker");
        assert!(refused.message.contains("keeps no world"), "{refused}");
        told_taken(&two.offerer, &offer_ref, "world:ada").unwrap();

        // A second world, returning under its own name or the taker's.
        let mut forged = back.clone();
        forged["by"] = json!({"world_ref": "world:bob", "name": "Bob"});
        let refused = receive(&forged, "world:bob").unwrap_err();
        assert_eq!(refused.code, "not_the_taker");
        assert!(
            refused.message.contains("world:bob did not take it"),
            "{refused}"
        );
        assert_eq!(
            receive(&back, "world:bob").unwrap_err().code,
            "not_the_taker"
        );
        let refused = receive(&forged, "world:ada").unwrap_err();
        assert_eq!(refused.code, "not_the_taker");
        assert!(refused.message.contains("names another world"), "{refused}");
        let mine = two.offerer.reading().unwrap();
        assert_eq!(
            (&mine["made"][0]["state"], &mine["made"][0]["return"]),
            (&json!("open"), &Value::Null),
            "a refused return spends nothing"
        );

        let state = two.state.display().to_string();
        let request = json!({"return": back}).to_string();
        let through = |extra: &[&str]| {
            let mut args = vec!["offer-receive".to_owned(), state.clone()];
            args.extend(extra.iter().map(|word| (*word).to_owned()));
            args.push("-".into());
            execute(&args, Some(request.as_str()), NOW)
        };
        assert_eq!(through(&[]).unwrap_err().code, "usage");
        assert_eq!(
            through(&["--by", "world:bob"]).unwrap_err().code,
            "not_the_taker"
        );
        assert_eq!(
            through(&["--by", "world:ada"]).unwrap()["state"],
            "returned"
        );
    }

    /// SF13.3c F3: a step is held only as the folder spells it, byte for
    /// byte, on any volume: the names are synthetic, so this bites where
    /// the volume folds nothing (Linux CI).
    #[cfg(unix)]
    #[test]
    fn a_step_is_held_only_as_the_folder_spells_it() {
        let names: Vec<std::ffi::OsString> = ["docs", "caf\u{e9}.md", ".git"]
            .iter()
            .map(|name| std::ffi::OsString::from(*name))
            .collect();
        assert!(holds(&names, "docs"));
        assert!(holds(&names, "caf\u{e9}.md"));
        assert!(!holds(&names, "DOCS"));
        assert!(!holds(&names, "cafe\u{301}.md"), "the NFD spelling");
        assert!(!holds(&names, ".GIT"));
        assert!(!holds(&names, "doc"));
    }

    /// SF13.3c F5 and F7: a grant's project refs are kept as a read reaches
    /// them, and a record answers only for its own offer.
    #[test]
    fn a_grant_covers_what_its_refs_name_however_spelled_and_a_record_answers_for_its_own_offer() {
        let two = two();
        let offer = make_granting(&two, json!(["project:river/docs//deep/"]), NOW + HOUR);
        assert_eq!(offer["grant"]["reads"], json!(["project:river/docs/deep"]));
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let (_dir, store) = served(&two, &offer);
        let read = |offer_ref: &str, subject: &str| {
            store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                NOW,
            )
        };
        assert_eq!(
            read(&offer_ref, "project:river/docs/deep/note.md").unwrap()["content_base64"],
            "QSBub3RlLg=="
        );
        let listed = read(&offer_ref, "project:river/docs/deep").unwrap();
        assert_eq!(
            listed["entries"][0]["ref"],
            "project:river/docs/deep/note.md"
        );

        let other = "offer:another";
        std::fs::copy(
            store.path(Side::Made, &offer_ref),
            store.path(Side::Made, other),
        )
        .unwrap();
        assert_eq!(
            read(other, "project:river/docs/deep/note.md")
                .unwrap_err()
                .code,
            "unknown_offer",
            "a record kept under another offer's name serves nothing"
        );
    }

    /// SF13.3c F6: the offerer's record answers the relay in words that name
    /// no local path.
    #[test]
    fn the_offerers_record_refuses_in_words_that_name_no_local_path() {
        let two = two();
        let offer = make(&two, NOW + HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        let root = two.state.parent().unwrap().to_path_buf();
        let made = root.join(FOLDER).join("made");
        std::fs::remove_dir_all(&made).unwrap();
        std::fs::write(&made, b"").unwrap();
        for refused in [
            told_taken(&two.offerer, &offer_ref, "world:ada").unwrap_err(),
            two.offerer
                .receive(&json!({"return": {"offer_ref": offer_ref}}), "world:ada")
                .unwrap_err(),
        ] {
            assert_eq!(refused.code, "io", "{refused}");
            assert!(refused.message.contains("the offer's record"), "{refused}");
            assert!(
                !refused.message.contains(root.to_str().unwrap()),
                "{refused}"
            );
        }
    }

    #[test]
    fn the_command_answers_documents_and_refuses_in_them() {
        let two = two();
        let state = two.state.to_string_lossy().into_owned();
        let taker = two._dirs.1.path().to_string_lossy().into_owned();
        let request = with(
            json!({"run_ref": two.run_ref, "workflow_unit_ref": two.unit_ref, "star_ref": "star:river-maps",
            "grant": {"reads": ["project:river/docs"], "touches": ["project:river/maps"], "lapses_at": NOW + HOUR}}),
            OFFER_CAPABILITY_REF,
        );
        let made = execute(
            &["offer-make".into(), state.clone(), "-".into()],
            Some(&request.to_string()),
            NOW,
        )
        .unwrap();
        assert_eq!(made["schema"], TASK_OFFER_RECEIPT);
        let took = with(
            json!({"offer": made["offer"], "by": {"world_ref": "world:ada", "name": "Ada"}}),
            TAKE_CAPABILITY_REF,
        );
        execute(
            &["offer-take".into(), taker.clone(), "-".into()],
            Some(&took.to_string()),
            NOW,
        )
        .unwrap();
        let offers = execute(&["offers".into(), state], None, NOW).unwrap();
        assert_eq!(
            (
                offers["schema"].as_str(),
                offers["made"].as_array().map(Vec::len),
                offers["taken"].as_array().map(Vec::len)
            ),
            (Some(TASK_OFFERS_READING), Some(1), Some(0))
        );
        let theirs = execute(&["offers".into(), taker.clone()], None, NOW).unwrap();
        assert_eq!(theirs["taken"][0]["state"], "taken");
        let refusal = execute(
            &["offer-check".into(), taker, "-".into()],
            Some("{\"offer_ref\":\"offer:none\",\"read\":\"project:x\"}"),
            NOW,
        )
        .unwrap_err();
        assert_eq!(refusal.document()["schema"], TASK_OFFER_REFUSAL);
        assert_eq!(refusal.code, "unknown_offer");
        assert!(is_offer_command(&[
            "development".into(),
            "offer-make".into()
        ]));
        assert!(!is_offer_command(&["development".into(), "show".into()]));
    }
}
