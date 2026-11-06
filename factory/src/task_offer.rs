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
/// What a person presents to make, revoke or accept an offer.
pub const OFFER_CAPABILITY_REF: &str = "capability/factory/offer-task";
/// What a person presents to take one up.
pub const TAKE_CAPABILITY_REF: &str = "capability/factory/take-task";

pub const VERBS: [&str; 9] = [
    "offers",
    "offer-make",
    "offer-revoke",
    "offer-receive",
    "offer-accept",
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
    "A task offered at a star (the offerer's side, then the taker's; each request a document, `-` for stdin):\n  factory development offers <state> [--json]\n  factory development offer-make|offer-revoke|offer-receive|offer-accept <state> [request-file|-] [--json]\n  factory development offer-take|offer-check|offer-revoked|offer-return <state> [request-file|-] [--json]".into()
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
    let reads = refs(grant.get("reads"), "reads", true)?;
    let touches = refs(grant.get("touches"), "touches", false)?;
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
        let name = lock_name(&path).map_err(|error| {
            io_refusal(
                &format!("could not name the lock of {}", path.display()),
                error,
            )
        })?;
        let lock = NativeFileTransaction::acquire(&path, &name, true)
            .map_err(|error| io_refusal(&format!("could not open {}", path.display()), error))?;
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
                    return Err(OfferRefusal::new(
                        "publication_uncertain",
                        format!("publication is uncertain after the replacement; read the offers back before trying again: {uncertain}"),
                    ))
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

    /// `offer-receive {return}`: what came back, as the field carried it,
    /// kept apart from the work until the person accepts it.
    pub fn receive(&self, request: &Value) -> Result<Value, OfferRefusal> {
        let object = object(request, "what came back")?;
        only_keys(object, &["return"], "offer-receive")?;
        let back = object.get("return").unwrap_or(&Value::Null);
        let offer_ref = offer_ref_of(self::object(back, "what came back")?)?;
        self.transact(Side::Made, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Made))?;
            let back = checked_return(back, &record["offer"])?;
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
    if let Some(option) = args
        .iter()
        .skip(1)
        .find(|argument| argument.starts_with("--"))
    {
        return Err(usage(format!("unknown option {option}")));
    }
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
        "offer-receive" => store.receive(&request),
        "offer-accept" => store.accept(&request, now),
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
        let request = json!({"run_ref": two.run_ref, "workflow_unit_ref": two.unit_ref, "star_ref": "star:river-maps",
            "grant": {"reads": ["project:river/docs"], "touches": ["project:river/maps"], "lapses_at": lapses_at}});
        two.offerer
            .make(&with(request, OFFER_CAPABILITY_REF), NOW)
            .unwrap()["offer"]
            .clone()
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
        let received = two.offerer.receive(&json!({"return": back})).unwrap();
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
        let refused = two.offerer.receive(&json!({"return": back})).unwrap_err();
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
        let received = two.offerer.receive(&json!({"return": back})).unwrap();
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
        assert_eq!(
            two.offerer
                .receive(&json!({"return": late_back}))
                .unwrap_err()
                .code,
            "lapsed"
        );
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
