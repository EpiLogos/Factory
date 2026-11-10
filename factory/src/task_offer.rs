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
//! The taker's side reads under the grant the same way (SF13.5): the task's
//! executor asks for one granted ref (`offer-want`), checked against the
//! taker's own record of the grant; the relay asks the field, the offerer's
//! Factory answers by `offer-read`, and the answer the field carried back is
//! placed here (`offer-read-received`): a file's bytes under
//! `<state>/task-offers/reads/<offer>/`, each named by its ref's key, never by
//! a path the ref spells; a folder's entries and a refusal in the record.
//! Only a ref this Factory wanted is placed, only while the grant stands, only
//! as whole as the answer names it, and only from the world that offered it.
//! A ref is read on both sides by one check (`plain_read`): no `.` or `..`,
//! no backslash or NUL, no machinery, so the taker wants nothing the offerer
//! would refuse.
//!
//! The reads folder is reached from the state folder one step at a time with
//! no link followed, and a link in its way is refused; a placed file is 0600,
//! in folders of 0700 (SF13.5b). A revocation, a return or a lapse drops what
//! was placed, under the record's lock and before the record says so; every
//! verb on a taken offer brings its reads up to the time first (`settle`):
//! `offer-check` and `offers` too, so a lapse found by any of them drops what
//! was read. A file that cannot be removed leaves its read `drop_pending`
//! with its path, the revocation says where its bytes still are, and the next
//! verb tries again. A want waits ten minutes, as long as the field keeps an
//! ask, and is then `expired`; `offer-unwant` stops waiting sooner; a placed
//! file found gone is `missing`; either is wanted again. A folder answered
//! with `more` is paged by a want naming `after`. The record keeps at most
//! `MAX_SETTLED` settled reads, by their digests and words alone.
//! No receipt or refusal carries what was read: each names the ref and its
//! size.
//!
//! Where the bytes travel: an `offer-read` answer is one JSON document, a
//! file's bytes as base64 within it, at most `MAX_READ_BYTES`. The offerer's
//! relay hands it to the field, which keeps it in the field's own database
//! until the taker's relay has carried it here, or ten minutes at most; while
//! it is there, whoever runs that SpacetimeDB can read it. It goes nowhere
//! else, and of the two Factories only the taker's keeps it, in its reads
//! folder, until a revocation, a return or a lapse drops it, as above. The
//! taker's relay names the world the answer came from (`offerer`) as the
//! field's sender identity, never as the document says, and this Factory
//! places it only when that is the world its take names as the offerer. A
//! request is read at most `MAX_REQUEST_BYTES` before it is parsed.
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
/// The most reads one taken offer's task waits on at once (`offer-want`).
const MAX_WANTED: usize = 32;
/// The most settled reads (refused, dropped, expired or missing) one taken
/// offer's record keeps; the oldest goes first (SF13.5b, F7).
const MAX_SETTLED: usize = 64;
/// How long a want waits for its answer: as long as the field keeps an ask
/// and its answer, ten minutes. Then it is `expired` (SF13.5b, F7).
const WANT_LIVES_MS: i64 = 10 * 60 * 1000;
/// Where the taker places what was read under a grant: one folder per offer.
const READS: &str = "reads";
/// The most one request document holds, read before it is parsed (SF13.5b,
/// F1): the largest answer a read brings back (a file's bytes as base64, or a
/// folder's entries with every byte of each ref escaped as six) and room for
/// the rest of the request.
pub const MAX_REQUEST_BYTES: u64 = {
    let file = MAX_READ_BYTES.div_ceil(3) * 4;
    let folder = MAX_READ_ENTRIES as u64 * (6 * MAX_REF_CHARS as u64 + 64);
    (if file > folder { file } else { folder }) + 64 * 1024
};
/// What a person presents to make, revoke or accept an offer.
pub const OFFER_CAPABILITY_REF: &str = "capability/factory/offer-task";
/// What a person presents to take one up.
pub const TAKE_CAPABILITY_REF: &str = "capability/factory/take-task";

pub const VERBS: [&str; 14] = [
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
    "offer-want",
    "offer-unwant",
    "offer-read-received",
];
const FOLDER: &str = "task-offers";
const MAX_REFS: usize = 32;
const MAX_REF_CHARS: usize = 512;
const MAX_TEXT_CHARS: usize = 2000;
const MAX_NAME_CHARS: usize = 200;
/// The longest a grant may stand: thirty days.
const MAX_GRANT_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub fn help() -> String {
    "A task offered at a star (the offerer's side, then the taker's; each request a document, `-` for stdin):\n  factory development offers <state> [--json]\n  factory development offer-make|offer-revoke|offer-accept <state> [request-file|-] [--json]\n  factory development offer-taken|offer-receive <state> --by <world_ref> [request-file|-] [--json]\n  factory development offer-read <state> --asker <world_ref> [request-file|-] [--json]\n  factory development offer-take|offer-check|offer-revoked|offer-return|offer-want|offer-unwant|offer-read-received <state> [request-file|-] [--json]".into()
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

/// The same, for the made side, whose refusals go toward another world: the
/// OS error in its own words alone (its message and its code, else its
/// kind), never the text a layer above it added, such as the stage path
/// a failed publication names (SF13.3d, F6 in full).
fn os_refusal(what: &str, error: &io::Error) -> OfferRefusal {
    let code = if error.kind() == io::ErrorKind::TimedOut {
        "busy"
    } else {
        "io"
    };
    OfferRefusal::new(code, format!("{what}: {}", os_words(error)))
}

/// The OS's own words for an error: the first OS error code in its chain,
/// as the OS says it, else only its kind. Nothing an annotation added is
/// repeated.
fn os_words(error: &io::Error) -> String {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = cause {
        if let Some(code) = current
            .downcast_ref::<io::Error>()
            .and_then(io::Error::raw_os_error)
        {
            return io::Error::from_raw_os_error(code).to_string();
        }
        cause = current.source();
    }
    error.kind().to_string()
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
        Some(key) => Err(OfferRefusal::invalid(format!(
            "{what} takes no `{}`",
            shown_key(key)
        ))),
        None => Ok(()),
    }
}

/// A key a document should not hold, as a refusal shows it: its first 64
/// characters, each escaped, and an ellipsis when there are more (SF13.5b,
/// F11). Whatever the key holds, the words stay short and plain.
fn shown_key(key: &str) -> String {
    let mut shown: String = key
        .chars()
        .take(64)
        .flat_map(char::escape_default)
        .collect();
    if key.chars().nth(64).is_some() {
        shown.push('…');
    }
    shown
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
        self.transact_with(side, offer_ref, |current| {
            change(current).map(|(record, answer)| (record, Ok(answer)))
        })
    }

    /// The same, where `change` may answer a refusal and still publish a
    /// record: what a refused verb found is kept (SF13.5b). A refusal of
    /// `change` itself publishes nothing.
    fn transact_with(
        &self,
        side: Side,
        offer_ref: &str,
        change: impl FnOnce(Option<Value>) -> Result<(Value, Result<Value, OfferRefusal>), OfferRefusal>,
    ) -> Result<Value, OfferRefusal> {
        let path = self.path(side, offer_ref);
        // The made side answers the relay, whose refusals go toward another
        // world: it names no local path, and says an OS error in the OS's
        // words alone (SF13.3c and SF13.3d, F6).
        let record = match side {
            Side::Made => "the offer's record".to_owned(),
            Side::Taken => path.display().to_string(),
        };
        let refuse = |what: &str, error: io::Error| match side {
            Side::Made => os_refusal(what, &error),
            Side::Taken => io_refusal(what, error),
        };
        let name = lock_name(&path)
            .map_err(|error| refuse(&format!("could not name the lock of {record}"), error))?;
        let lock = NativeFileTransaction::acquire(&path, &name, true)
            .map_err(|error| refuse(&format!("could not open {record}"), error))?;
        let current = lock.current().map(decode).transpose()?;
        let (record, answer) = change(current)?;
        let bytes = encode(&record);
        if lock.current() != Some(bytes.as_slice()) {
            match lock.publish(&bytes, lock.current().is_none()) {
                Ok(()) => {}
                Err(PublicationError::Before(error)) => {
                    return Err(refuse("the change was not published", error))
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
        answer
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

/// A ref a read reaches, on either side (SF13.5b, F5): `project:<id>/<path>`,
/// its id and every step of its path a plain step (an empty step is dropped,
/// as a grant's are). The id and the steps, or the refusal the offerer
/// answers: the taker wants nothing its offerer would refuse to read.
fn plain_read(subject: &str) -> Result<(&str, Vec<&str>), OfferRefusal> {
    let not_here = |why: String| OfferRefusal::new("not_here", why);
    let body = subject
        .strip_prefix("project:")
        .ok_or_else(|| not_here(format!("{subject} names no project")))?;
    let (id, rest) = body.split_once('/').unwrap_or((body, ""));
    let steps: Vec<&str> = rest.split('/').filter(|step| !step.is_empty()).collect();
    if !plain_step(id) || !steps.iter().all(|step| plain_step(step)) {
        return Err(not_here(format!(
            "{subject} names no plain path of the project's work"
        )));
    }
    Ok((id, steps))
}

/// The one step `entry` names under the folder `subject`: a plain step, or
/// None.
fn entry_step<'a>(subject: &str, entry: &'a str) -> Option<&'a str> {
    entry
        .strip_prefix(subject)
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|step| !step.contains('/') && plain_step(step))
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

/// Why nothing may be read under a taken offer's grant now, if anything: the
/// grant revoked, spent or lapsed (SF13.5).
fn read_barred(record: &Value, now: i64) -> Option<OfferRefusal> {
    if record["state"] == "revoked" {
        Some(OfferRefusal::new(
            "revoked",
            "The offerer revoked the grant; nothing is read under it",
        ))
    } else if record["state"] == "returned" {
        Some(OfferRefusal::new(
            "spent",
            "What the task made has gone back; the grant is spent",
        ))
    } else if record["offer"]["grant"]["lapses_at"].as_i64() <= Some(now) {
        Some(OfferRefusal::new("lapsed", "The grant has lapsed"))
    } else {
        None
    }
}

/// Whether the grant this Factory took lets the task read `subject`: what it
/// touches it may read, and what it reads.
fn read_granted_here(record: &Value, subject: &str) -> Result<(), OfferRefusal> {
    let grant = &record["offer"]["grant"];
    let mut allowed = strings(&grant["touches"]);
    allowed.extend(strings(&grant["reads"]));
    if covers(&allowed, subject) {
        Ok(())
    } else {
        Err(OfferRefusal::new(
            "not_granted",
            format!("The grant does not let the task read {subject}"),
        ))
    }
}

/// One read as a receipt names it: its ref and where it stands, never what it
/// held.
fn read_seen(subject: &str, read: &Value) -> Value {
    let mut seen = json!({"ref": subject});
    if let Some(read) = read.as_object() {
        for (key, value) in read {
            seen[key.as_str()] = value.clone();
        }
    }
    seen
}

/// An answer of the offerer's Factory to one read, checked whole.
enum Answered {
    File { bytes: Vec<u8>, sha256: String },
    Folder { entries: Vec<Value>, more: bool },
    Refused { code: String, message: String },
}

/// The offerer's answer to `subject`, as the field carried it: its
/// `factory.task-offer-read/v1` for this offer and this ref, a file whole
/// within the bound and as its size and digest name it, or a folder's entries
/// one plain step under it; or its refusal, in its words. Either may carry
/// the `offerer` the taker's relay names (`read_received` checks it). No
/// refusal here names what the answer held, and none shows more of a key it
/// should not hold than `shown_key`.
fn checked_read(answer: &Value, offer_ref: &str, subject: &str) -> Result<Answered, OfferRefusal> {
    let bad = |why: String| OfferRefusal::new("bad_answer", why);
    let object = answer
        .as_object()
        .ok_or_else(|| bad(format!("The answer for {subject} is no document")))?;
    let keys = |allowed: &[&str]| match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(bad(format!(
            "The answer for {subject} holds `{}`, which no answer holds",
            shown_key(key)
        ))),
        None => Ok(()),
    };
    match object.get("schema").and_then(Value::as_str) {
        Some(TASK_OFFER_REFUSAL) => {
            keys(&["schema", "code", "message", "effect", "offerer"])?;
            let code = object
                .get("code")
                .and_then(Value::as_str)
                .filter(|code| {
                    !code.is_empty()
                        && code.len() <= 64
                        && code.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                })
                .ok_or_else(|| bad(format!("The offerer's refusal of {subject} names no code")))?;
            let message = text(object, "message", MAX_TEXT_CHARS).map_err(|_| {
                bad(format!(
                    "The offerer's refusal of {subject} is not in words this Factory keeps"
                ))
            })?;
            Ok(Answered::Refused {
                code: code.to_owned(),
                message: message.to_owned(),
            })
        }
        Some(TASK_OFFER_READ) => {
            if object.get("offer_ref").and_then(Value::as_str) != Some(offer_ref)
                || object.get("ref").and_then(Value::as_str) != Some(subject)
            {
                return Err(bad(format!(
                    "The answer is not the one asked for {subject}"
                )));
            }
            if object.get("automatic_agent_or_model_invocation") != Some(&json!(false)) {
                return Err(bad(format!(
                    "The answer for {subject} would run something by itself"
                )));
            }
            let common = [
                "schema",
                "offer_ref",
                "ref",
                "kind",
                "automatic_agent_or_model_invocation",
                "offerer",
            ];
            match object.get("kind").and_then(Value::as_str) {
                Some("file") => {
                    keys(&[&common[..], &["byte_len", "sha256", "content_base64"][..]].concat())?;
                    let byte_len = object
                        .get("byte_len")
                        .and_then(Value::as_u64)
                        .filter(|len| *len <= MAX_READ_BYTES)
                        .ok_or_else(|| bad(format!("The answer for {subject} names no size within {MAX_READ_BYTES} bytes")))?;
                    let sha256 = object
                        .get("sha256")
                        .and_then(Value::as_str)
                        .filter(|sha| {
                            sha.len() == 64
                                && sha
                                    .chars()
                                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
                        })
                        .ok_or_else(|| {
                            bad(format!(
                                "The answer for {subject} names no digest of its {byte_len} bytes"
                            ))
                        })?;
                    let held = || {
                        bad(format!(
                            "The answer for {subject} does not hold the {byte_len} bytes it names"
                        ))
                    };
                    let content = object
                        .get("content_base64")
                        .and_then(Value::as_str)
                        .ok_or_else(held)?;
                    // Bounded before it is decoded: no more base64 than the
                    // bound's bytes take.
                    if content.len() as u64 > MAX_READ_BYTES.div_ceil(3) * 4 {
                        return Err(bad(format!(
                            "The answer for {subject} holds more base64 than {MAX_READ_BYTES} bytes take"
                        )));
                    }
                    let bytes = unbase64(content)
                        .filter(|bytes| bytes.len() as u64 == byte_len)
                        .ok_or_else(held)?;
                    use sha2::Digest;
                    if format!("{:x}", sha2::Sha256::digest(&bytes)) != sha256 {
                        return Err(bad(format!(
                            "The {byte_len} bytes for {subject} are not the ones the answer names"
                        )));
                    }
                    Ok(Answered::File {
                        bytes,
                        sha256: sha256.to_owned(),
                    })
                }
                Some("folder") => {
                    keys(&[&common[..], &["entries", "more"][..]].concat())?;
                    let more = object.get("more").and_then(Value::as_bool).ok_or_else(|| {
                        bad(format!(
                            "The answer for {subject} does not say whether more entries follow"
                        ))
                    })?;
                    let listed = object
                        .get("entries")
                        .and_then(Value::as_array)
                        .filter(|entries| entries.len() <= MAX_READ_ENTRIES)
                        .ok_or_else(|| bad(format!("The answer for {subject} names no entries within {MAX_READ_ENTRIES}")))?;
                    let mut entries = Vec::with_capacity(listed.len());
                    for entry in listed {
                        let entry = entry
                            .as_object()
                            .filter(|entry| {
                                entry
                                    .keys()
                                    .all(|key| ["ref", "kind", "byte_len"].contains(&key.as_str()))
                            })
                            .ok_or_else(|| {
                                bad(format!(
                                    "An entry the answer for {subject} names is no entry"
                                ))
                            })?;
                        let named = reference(entry.get("ref").unwrap_or(&Value::Null), "ref")
                            .map_err(|_| {
                                bad(format!("An entry the answer for {subject} names is no ref"))
                            })?;
                        // One plain step under the folder, as the offerer lists
                        // it (SF13.5b, F5).
                        let step = entry_step(subject, &named).ok_or_else(|| {
                            bad(format!("The answer for {subject} names an entry not in it"))
                        })?;
                        let kind = entry
                            .get("kind")
                            .and_then(Value::as_str)
                            .filter(|kind| ["file", "folder"].contains(kind));
                        let byte_len = entry.get("byte_len").and_then(Value::as_u64);
                        let (Some(kind), Some(byte_len)) = (kind, byte_len) else {
                            return Err(bad(format!(
                                "The entry {step} of {subject} is no file or folder with a size"
                            )));
                        };
                        if kind == "file" && byte_len > MAX_READ_BYTES {
                            return Err(bad(format!(
                                "The entry {step} of {subject} is larger than one read carries"
                            )));
                        }
                        entries.push(json!({"ref": named, "kind": kind, "byte_len": byte_len}));
                    }
                    Ok(Answered::Folder { entries, more })
                }
                _ => Err(bad(format!(
                    "The answer for {subject} is no file or folder"
                ))),
            }
        }
        _ => Err(bad(format!(
            "The answer for {subject} is no read or refusal of the offerer's Factory"
        ))),
    }
}

/// Standard base64, padded, as `base64` writes it; None for anything else.
fn unbase64(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| -> Option<u32> {
        let six = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        Some(u32::from(six))
    };
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        let pad = chunk.iter().rev().take_while(|c| **c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for (at, c) in chunk.iter().enumerate() {
            n <<= 6;
            if at < 4 - pad {
                n |= value(*c)?;
            } else if *c != b'=' {
                return None;
            }
        }
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&decoded[..3 - pad]);
        // the bits a padded group leaves unused are zero, as `base64` writes them
        if (pad == 1 && n & 0xff != 0) || (pad == 2 && n & 0xffff != 0) {
            return None;
        }
    }
    Some(out)
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
    for key in ["return", "accepted", "taken", "offerer"] {
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
    /// as before; another world's is refused. An offer already returned that
    /// keeps no taker (a record written before #314, or a take told only
    /// once its return came) is told it too, and its state stays as it is,
    /// so what came back can be received (SF13.3d, H1).
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
        // The taker's Factory reads a ref by the same check before it wants it.
        let (id, steps) = plain_read(subject)?;
        if !names.iter().any(|name| name == id) {
            return Err(not_here(format!(
                "{subject} names another project than this one"
            )));
        }
        let steps: Vec<String> = steps.into_iter().map(str::to_owned).collect();
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
    /// before it is read, and spends nothing. The same return received
    /// again, by the world it names, is answered as before ahead of that
    /// check: it changes nothing and spends nothing, so a return received
    /// before this Factory kept its taker answers again (SF13.3d, H1).
    pub fn receive(&self, request: &Value, world_ref: &str) -> Result<Value, OfferRefusal> {
        let object = object(request, "what came back")?;
        only_keys(object, &["return"], "offer-receive")?;
        let back = object.get("return").unwrap_or(&Value::Null);
        let offer_ref = offer_ref_of(self::object(back, "what came back")?)?;
        let world_ref = reference(&json!(world_ref), "by")?;
        self.transact(Side::Made, &offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(&offer_ref, Side::Made))?;
            let repeat = matches!(record["state"].as_str(), Some("returned" | "accepted"))
                && checked_return(back, &record["offer"]).is_ok_and(|same| {
                    same == record["return"]
                        && same["by"]["world_ref"].as_str() == Some(world_ref.as_str())
                });
            if repeat {
                let answer = receipt("receive", &record, None);
                return Ok((record, answer));
            }
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

/// The reads folder of one taken offer, `<state>/task-offers/reads/<key>/`,
/// reached from the state folder one step at a time through held handles,
/// no link followed (SF13.5b, F4).
#[cfg(unix)]
mod reads {
    use super::{FOLDER, READS};
    use crate::native_file_transaction::{
        create_new_at, entry_names, entry_stat, make_folder_at, open_directory, open_step,
        remove_at, rename_at, EntryKind,
    };
    use std::ffi::{OsStr, OsString};
    use std::fs::{File, Permissions};
    use std::io::{self, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn in_the_way(step: &str) -> io::Error {
        io::Error::other(format!(
            "a link, or something other than a folder, stands at {step}; nothing is placed or dropped through it"
        ))
    }

    /// The folder `step` of the held `dir`, opened with no link followed:
    /// None when nothing stands there; refused when a link or anything but a
    /// folder does.
    fn folder(dir: &File, step: &str) -> io::Result<Option<File>> {
        let name = OsStr::new(step);
        match entry_stat(dir, name) {
            Ok((EntryKind::Folder, ..)) => {}
            Ok(_) => return Err(in_the_way(step)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        }
        let held = open_step(dir, name).map_err(|error| match error.raw_os_error() {
            Some(libc::ELOOP) => in_the_way(step),
            _ => error,
        })?;
        if !held.metadata()?.is_dir() {
            return Err(in_the_way(step));
        }
        Ok(Some(held))
    }

    /// The reads folder of the offer keyed `offer` under the state folder
    /// `root`, or None when it is not there. When `make`, it is made where it
    /// is missing, and it and `reads` are made 0700 (SF13.5b, F11).
    pub(super) fn open(root: &Path, offer: &str, make: bool) -> io::Result<Option<File>> {
        let mut dir = open_directory(root)?;
        for (step, private) in [(FOLDER, false), (READS, true), (offer, true)] {
            if make {
                make_folder_at(&dir, OsStr::new(step), if private { 0o700 } else { 0o777 })?;
            }
            let Some(held) = folder(&dir, step)? else {
                return Ok(None);
            };
            if make && private && held.metadata()?.permissions().mode() & 0o777 != 0o700 {
                held.set_permissions(Permissions::from_mode(0o700))?;
            }
            dir = held;
        }
        Ok(Some(dir))
    }

    /// Whether the held reads folder holds the file `key`: a link, or
    /// anything else standing there, is not it.
    pub(super) fn holds(dir: &File, key: &str) -> io::Result<bool> {
        match entry_stat(dir, OsStr::new(key)) {
            Ok((kind, ..)) => Ok(kind == EntryKind::File),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// `bytes` placed whole as the file `key` of the held reads folder, 0600:
    /// written to a stage beside it, then renamed over whatever stands there
    /// (a link is replaced as a name, never followed).
    pub(super) fn place(dir: &File, key: &str, bytes: &[u8]) -> io::Result<()> {
        let stage = OsString::from(format!(".{key}.{}.tmp", ulid::Ulid::new()));
        let mut file = create_new_at(dir, &stage, 0o600)?;
        let written = file
            .set_permissions(Permissions::from_mode(0o600))
            .and_then(|()| file.write_all(bytes))
            .and_then(|()| file.sync_all())
            .and_then(|()| rename_at(dir, &stage, OsStr::new(key)));
        if let Err(error) = written {
            // The stage is this call's own, made new in a folder of 0700.
            let _ = remove_at(dir, &stage, false);
            return Err(error);
        }
        dir.sync_all()
    }

    /// Everything the held folder holds removed, no link followed: a folder
    /// within is emptied first.
    fn empty(dir: &File) -> io::Result<()> {
        for name in entry_names(dir)? {
            let (kind, ..) = entry_stat(dir, &name)?;
            if kind == EntryKind::Folder {
                empty(&open_step(dir, &name)?)?;
            }
            remove_at(dir, &name, kind == EntryKind::Folder)?;
        }
        Ok(())
    }

    /// The folder `step` of the held `dir`, opened with no link followed, or
    /// None when nothing stands there or when a link or anything but a
    /// folder did: that is removed itself, never followed.
    fn cleared(dir: &File, step: &str) -> io::Result<Option<File>> {
        match entry_stat(dir, OsStr::new(step)) {
            Ok((EntryKind::Folder, ..)) => folder(dir, step),
            Ok(_) => {
                remove_at(dir, OsStr::new(step), false)?;
                dir.sync_all()?;
                Ok(None)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The reads folder of the offer keyed `offer` removed whole. A link at
    /// `reads` or at the offer's folder is removed itself, never followed;
    /// one at `task-offers`, under which the records stand, is refused.
    pub(super) fn sweep(root: &Path, offer: &str) -> io::Result<()> {
        let root = open_directory(root)?;
        let Some(offers) = folder(&root, FOLDER)? else {
            return Ok(());
        };
        let Some(reads) = cleared(&offers, READS)? else {
            return Ok(());
        };
        let Some(held) = cleared(&reads, offer)? else {
            return Ok(());
        };
        empty(&held)?;
        remove_at(&reads, OsStr::new(offer), true)?;
        reads.sync_all()
    }
}

#[cfg(not(unix))]
mod reads {
    use std::fs::File;
    use std::io;
    use std::path::Path;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "what is read under a grant is placed only on a Unix host, where each step is opened with no link followed",
        )
    }
    pub(super) fn open(_: &Path, _: &str, _: bool) -> io::Result<Option<File>> {
        Err(unsupported())
    }
    pub(super) fn holds(_: &File, _: &str) -> io::Result<bool> {
        Err(unsupported())
    }
    pub(super) fn place(_: &File, _: &str, _: &[u8]) -> io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn sweep(_: &Path, _: &str) -> io::Result<()> {
        Err(unsupported())
    }
}

/// Whether a placed file is known gone from its reads folder, as `opened`
/// found the folder: gone when the folder is not there or does not hold it,
/// and not known gone when the folder could not be read.
fn gone(opened: &io::Result<Option<std::fs::File>>, key: &str) -> bool {
    match opened {
        Ok(None) => true,
        Ok(Some(dir)) => matches!(reads::holds(dir, key), Ok(false)),
        Err(_) => false,
    }
}

/// The states of a read that holds nothing on disk and waits on nothing.
const SETTLED: [&str; 4] = ["refused", "dropped", "expired", "missing"];

/// One read settled as `state` at `now`: it keeps its digest and its words,
/// never a listing, a file or a path (SF13.5b, F7 and F8); a paged want that
/// expired keeps where it would have continued.
fn settled_as(read: &mut Value, state: &str, now: i64) {
    if let Some(fields) = read.as_object_mut() {
        for key in ["entries", "more", "file", "path"] {
            fields.remove(key);
        }
        if state != "expired" {
            fields.remove("after");
        }
        fields.entry("settled_at").or_insert(json!(now));
        fields.insert("state".into(), json!(state));
    }
}

/// At most `MAX_SETTLED` settled reads kept, the oldest gone first (SF13.5b,
/// F7).
fn bounded(record: &mut Value) {
    let Some(reads) = record.get_mut("reads").and_then(Value::as_object_mut) else {
        return;
    };
    let mut settled: Vec<(i64, String)> = reads
        .iter()
        .filter(|(_, read)| {
            read["state"]
                .as_str()
                .is_some_and(|state| SETTLED.contains(&state))
        })
        .map(|(subject, read)| (read["settled_at"].as_i64().unwrap_or(0), subject.clone()))
        .collect();
    if settled.len() > MAX_SETTLED {
        settled.sort();
        for (_, subject) in &settled[..settled.len() - MAX_SETTLED] {
            reads.remove(subject);
        }
    }
}

/// A want older than the field keeps its ask, or one that names no time.
fn expired(read: &Value, now: i64) -> bool {
    read["state"] == "wanted"
        && read["wanted_at"]
            .as_i64()
            .is_none_or(|at| at + WANT_LIVES_MS <= now)
}

/// What a drop left on disk, as an answer says it (SF13.5b, F2): each read
/// whose bytes are still there and where, in words with the OS's reason;
/// None when nothing is left.
fn left_on_disk(record: &Value, why: Option<&str>) -> Option<Value> {
    let reads: Vec<Value> = record["reads"]
        .as_object()?
        .iter()
        .filter(|(_, read)| read["state"] == "drop_pending")
        .map(|(subject, read)| json!({"ref": subject, "path": read["path"]}))
        .collect();
    if reads.is_empty() {
        return None;
    }
    let paths: Vec<&str> = reads
        .iter()
        .filter_map(|read| read["path"].as_str())
        .collect();
    let why = why.map(|why| format!(" ({why})")).unwrap_or_default();
    Some(json!({
        "reads": reads,
        "message": format!(
            "What was read under the grant could not all be dropped{why}: its bytes are still on disk at {}; the next act on this offer drops them again",
            paths.join(", ")
        ),
    }))
}

impl OfferStore {
    /// `offer-take {offer, offerer, by: {world_ref, name}}`: the offer as the
    /// field showed it, taken up here by one world, under its grant.
    /// `offerer` is the world that offered it, which the taker's relay names
    /// as the field's sender identity of the offer, never as the offer says:
    /// only that world's answers to a read are placed here (SF13.5b, F6). A
    /// take told again with the offerer it lacked keeps it.
    pub fn take(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the take")?;
        only_keys(
            object,
            &["offer", "offerer", "by", "caller", "authority"],
            "offer-take",
        )?;
        let (caller, authority) = require_person(object, TAKE_CAPABILITY_REF, "Taking up a task")?;
        let offer = checked_offer(object.get("offer").unwrap_or(&Value::Null))?;
        let offerer = object
            .get("offerer")
            .map(|offerer| reference(offerer, "offerer"))
            .transpose()?;
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
            if let Some(mut record) = current {
                let same_offerer = match (record["offerer"].as_str(), offerer.as_deref()) {
                    (Some(kept), Some(told)) => kept == told,
                    _ => true,
                };
                if record["offer"] == offer
                    && record["taken"]["world_ref"] == json!(world_ref)
                    && same_offerer
                {
                    if let (true, Some(told)) = (record["offerer"].is_null(), &offerer) {
                        record["offerer"] = json!(told);
                    }
                    let answer = receipt("take", &record, Some((&caller, &authority)));
                    return Ok((record, answer));
                }
                return Err(OfferRefusal::new(
                    "taken",
                    "This offer was taken here already",
                ));
            }
            let mut record = json!({"offer": offer, "state": "taken",
                "taken": {"world_ref": world_ref, "name": name, "taken_at": now}});
            if let Some(offerer) = &offerer {
                record["offerer"] = json!(offerer);
            }
            let answer = receipt("take", &record, Some((&caller, &authority)));
            Ok((record, answer))
        })
    }

    fn taken(&self, offer_ref: &str) -> Result<Value, OfferRefusal> {
        let bytes = read_optional(&self.path(Side::Taken, offer_ref))?
            .ok_or_else(|| unknown(offer_ref, Side::Taken))?;
        decode(&bytes)
    }

    /// The folder where what was read under one taken offer's grant is placed.
    fn reads_folder(&self, offer_ref: &str) -> PathBuf {
        self.root.join(FOLDER).join(READS).join(key_of(offer_ref))
    }

    /// The reads of one taken offer brought up to `now`, under its record's
    /// lock (SF13.5b). A grant that stands no more keeps nothing read under
    /// it: the reads folder is removed, and only then does the record say
    /// the reads are dropped (F2, F3). A file that could not be removed
    /// leaves its read `drop_pending` with its path, and the OS's words are
    /// answered; whichever verb comes next tries again. While the grant
    /// stands, a want older than the field keeps its ask is `expired` and a
    /// placed file found gone is `missing` (F7, F9).
    fn settle(&self, offer_ref: &str, record: &mut Value, now: i64) -> Option<String> {
        let barred = read_barred(record, now).is_some();
        let offer = key_of(offer_ref);
        let folder = self.reads_folder(offer_ref);
        let reads = record.get_mut("reads").and_then(Value::as_object_mut)?;
        if barred {
            let held = |read: &Value| {
                matches!(
                    read["state"].as_str(),
                    Some("placed" | "wanted" | "drop_pending")
                )
            };
            if !reads.values().any(held) {
                return None;
            }
            let swept = reads::sweep(&self.root, &offer);
            let opened = match swept {
                Ok(()) => Ok(None),
                Err(_) => reads::open(&self.root, &offer, false),
            };
            for read in reads.values_mut().filter(|read| held(read)) {
                let left = swept.is_err()
                    && matches!(read["state"].as_str(), Some("placed" | "drop_pending"))
                    && read["file"].as_str().is_some_and(|key| !gone(&opened, key));
                if left {
                    let path = folder.join(read["file"].as_str().unwrap_or_default());
                    read["state"] = json!("drop_pending");
                    read["path"] = json!(path.display().to_string());
                } else {
                    settled_as(read, "dropped", now);
                }
            }
            return swept.err().map(|error| error.to_string());
        }
        let placed_file = |read: &Value| read["state"] == "placed" && read["kind"] == "file";
        let opened = if reads.values().any(placed_file) {
            reads::open(&self.root, &offer, false)
        } else {
            Ok(None)
        };
        for read in reads.values_mut() {
            if expired(read, now) {
                settled_as(read, "expired", now);
            } else if placed_file(read) && gone(&opened, read["file"].as_str().unwrap_or_default())
            {
                settled_as(read, "missing", now);
            }
        }
        None
    }

    /// Whether `settle` would change what one taken offer's record holds,
    /// read without its lock: what `offer-check` and `offers` ask before they
    /// settle.
    fn unsettled(&self, offer_ref: &str, record: &Value, now: i64) -> bool {
        let Some(reads) = record["reads"].as_object() else {
            return false;
        };
        let barred = read_barred(record, now).is_some();
        let mut opened = None;
        reads.values().any(|read| match read["state"].as_str() {
            Some("drop_pending") => true,
            Some("placed" | "wanted") if barred => true,
            Some("wanted") => expired(read, now),
            Some("placed") if read["kind"] == "file" => {
                let opened = opened
                    .get_or_insert_with(|| reads::open(&self.root, &key_of(offer_ref), false));
                gone(opened, read["file"].as_str().unwrap_or_default())
            }
            _ => false,
        })
    }

    /// One verb on a taken offer, under its record's lock: its reads settled
    /// first, then `act` on the record, handed the OS's words if a drop
    /// failed. A refusal publishes the record as settled, so what the verb
    /// found is kept whatever it answers; an answer publishes what `act` made
    /// of it. Either way the settled reads are bounded.
    fn on_taken(
        &self,
        offer_ref: &str,
        now: i64,
        act: impl FnOnce(&mut Value, Option<String>) -> Result<Value, OfferRefusal>,
    ) -> Result<Value, OfferRefusal> {
        self.transact_with(Side::Taken, offer_ref, |current| {
            let mut record = current.ok_or_else(|| unknown(offer_ref, Side::Taken))?;
            let why = self.settle(offer_ref, &mut record, now);
            bounded(&mut record);
            let settled = record.clone();
            Ok(match act(&mut record, why) {
                Ok(answer) => {
                    bounded(&mut record);
                    (record, Ok(answer))
                }
                Err(refusal) => (settled, Err(refusal)),
            })
        })
    }

    /// `offer-check {offer_ref, read | touch}`: whether the task may read or
    /// touch one thing now, under the grant. A read of the record, settled
    /// first when it must be (SF13.5b, F3): a lapse found here drops what was
    /// read, as any verb's does, and a drop that left bytes on disk says so.
    /// A project ref the offerer would refuse to read is never allowed read.
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
        let mut record = self.taken(&offer_ref)?;
        let mut left = None;
        if self.unsettled(&offer_ref, &record, now) {
            let settled = self.on_taken(&offer_ref, now, |record, why| {
                Ok(json!({"record": record.clone(), "why": why}))
            })?;
            left = left_on_disk(&settled["record"], settled["why"].as_str());
            record = settled["record"].clone();
        }
        let grant = &record["offer"]["grant"];
        let why = if record["state"] == "revoked" {
            Some("The offerer revoked the grant".to_owned())
        } else if grant["lapses_at"].as_i64() <= Some(now) {
            Some("The grant has lapsed".to_owned())
        } else if record["state"] == "returned" {
            Some("What the task made has gone back; the grant is spent".to_owned())
        } else if let Some(Err(refusal)) =
            (act == "read" && subject.starts_with("project:")).then(|| plain_read(&subject))
        {
            Some(refusal.message)
        } else {
            // What the task touches it may read; what it reads it may not touch.
            let mut allowed = strings(&grant["touches"]);
            if act == "read" {
                allowed.extend(strings(&grant["reads"]));
            }
            (!covers(&allowed, &subject))
                .then(|| format!("The grant does not let the task {act} {subject}"))
        };
        let mut answer = json!({
            "schema": TASK_OFFER_CHECK,
            "offer_ref": offer_ref,
            "act": act,
            "subject": subject,
            "allowed": why.is_none(),
            "why": why,
            "automatic_agent_or_model_invocation": false,
        });
        if let Some(left) = left {
            answer["left_on_disk"] = left;
        }
        Ok(answer)
    }

    /// `offer-revoked {offer_ref, revoked_at}`: the offerer's revocation, as
    /// the field told it; nothing is allowed after it. What was read under
    /// the grant is removed before the record says it is dropped; bytes that
    /// could not be removed are said, with where they are (`left_on_disk`).
    pub fn revoked(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the revocation")?;
        only_keys(object, &["offer_ref", "revoked_at"], "offer-revoked")?;
        let offer_ref = offer_ref_of(object)?;
        let revoked_at = millis(object, "revoked_at")?;
        self.on_taken(&offer_ref, now, |record, _| {
            if record["state"] != "revoked" {
                record["state"] = json!("revoked");
                record["revoked_at"] = json!(revoked_at);
            }
            // What was read under the grant goes with it (SF13.5, SF13.5b).
            let why = self.settle(&offer_ref, record, now);
            let mut answer = receipt("revoked", record, None);
            if let Some(left) = left_on_disk(record, why.as_deref()) {
                answer["left_on_disk"] = left;
            }
            Ok(answer)
        })
    }

    /// `offer-return {offer_ref, made: {label, summary, refs}}`: what the task
    /// made, under the taker's name, within the grant, before it lapses. The
    /// same made brought back again answers the return already given, as a
    /// take, a receipt, an acceptance and a revocation answer their repeats:
    /// what went back here and never reached the field goes on from here. A
    /// return refused as revoked or lapsed drops what was read, as a return
    /// given does.
    pub fn give_back(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "what was made")?;
        only_keys(object, &["offer_ref", "made"], "offer-return")?;
        let offer_ref = offer_ref_of(object)?;
        let made = object.get("made").unwrap_or(&Value::Null);
        self.on_taken(&offer_ref, now, |record, _| {
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
                        return Ok(record["return"].clone());
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
            // The grant is spent: what was read under it goes (SF13.5, SF13.5b).
            self.settle(&offer_ref, record, now);
            Ok(back)
        })
    }

    /// `offer-want {offer_ref, ref[, after]}`: the task's executor asks to
    /// read one ref under the grant (SF13.5). Checked against this Factory's
    /// own record of the grant it took and read as the offerer reads it
    /// (`plain_read`), and kept as wanted until the offerer's answer is
    /// placed (`offer-read-received`) or ten minutes pass (`expired`); the
    /// relay asks the field for what is wanted, with its `after`. Asked
    /// again, a want or a placed read stands as it is; one refused, dropped,
    /// expired or missing is asked again. `after` continues a folder whose
    /// answer said `more`: the next page is wanted, after that entry
    /// (SF13.5b, F10).
    pub fn want(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the want")?;
        only_keys(object, &["offer_ref", "ref", "after"], "offer-want")?;
        let offer_ref = offer_ref_of(object)?;
        let subject = reference(
            object
                .get("ref")
                .ok_or_else(|| OfferRefusal::invalid("a want names one `ref`"))?,
            "ref",
        )?;
        let after = object
            .get("after")
            .map(|after| reference(after, "after"))
            .transpose()?;
        self.on_taken(&offer_ref, now, |record, _| {
            if let Some(barred) = read_barred(record, now) {
                return Err(barred);
            }
            plain_read(&subject)?;
            read_granted_here(record, &subject)?;
            if !record["reads"].is_object() {
                record["reads"] = json!({});
            }
            let standing = record["reads"][subject.as_str()].clone();
            let continues = |after: &str| standing["after"].as_str() == Some(after);
            let wanted = match (after.as_deref(), standing["state"].as_str()) {
                (None, Some("wanted" | "placed")) => false,
                (None, _) => true,
                (Some(after), Some("wanted")) if continues(after) => false,
                (Some(after), Some("expired")) if continues(after) => true,
                (Some(after), Some("placed"))
                    if standing["kind"] == "folder"
                        && standing["more"] == true
                        && entry_step(&subject, after).is_some() =>
                {
                    true
                }
                (Some(_), _) => {
                    return Err(OfferRefusal::invalid(format!(
                        "`after` names an entry of a folder whose last answer said more follow, and {subject} is not one"
                    )))
                }
            };
            if wanted {
                let open = record["reads"].as_object().map_or(0, |reads| {
                    reads
                        .values()
                        .filter(|read| read["state"] == "wanted")
                        .count()
                });
                if open >= MAX_WANTED {
                    return Err(OfferRefusal::new(
                        "too_many",
                        format!("This task waits on {MAX_WANTED} reads already; ask again once one is placed"),
                    ));
                }
                let mut read = json!({"state": "wanted", "wanted_at": now});
                if let Some(after) = &after {
                    read["after"] = json!(after);
                }
                record["reads"][subject.as_str()] = read;
            }
            let mut answer = receipt("want", record, None);
            answer["read"] = read_seen(&subject, &record["reads"][subject.as_str()]);
            Ok(answer)
        })
    }

    /// `offer-unwant {offer_ref, ref}`: the task's executor stops waiting on
    /// a read it wanted (SF13.5b, F7): the want is dropped, and an answer
    /// brought after it is not placed. A read not waited on stands as it is.
    pub fn unwant(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "the unwant")?;
        only_keys(object, &["offer_ref", "ref"], "offer-unwant")?;
        let offer_ref = offer_ref_of(object)?;
        let subject = reference(
            object
                .get("ref")
                .ok_or_else(|| OfferRefusal::invalid("an unwant names one `ref`"))?,
            "ref",
        )?;
        self.on_taken(&offer_ref, now, |record, _| {
            let read = record
                .get_mut("reads")
                .and_then(|reads| reads.get_mut(subject.as_str()))
                .ok_or_else(|| {
                    OfferRefusal::new(
                        "not_wanted",
                        format!("This task is not waiting to read {subject}"),
                    )
                })?;
            if read["state"] == "wanted" {
                settled_as(read, "dropped", now);
            }
            let mut answer = receipt("unwant", record, None);
            answer["read"] = read_seen(&subject, &record["reads"][subject.as_str()]);
            Ok(answer)
        })
    }

    /// `offer-read-received {offer_ref, ref, answer}`: the offerer's answer to
    /// a read this task wanted, as the field carried it (SF13.5): a file's
    /// bytes, placed under `<state>/task-offers/reads/<offer>/` named by the
    /// ref's key, 0600; a folder's entries (or the page after the entry its
    /// want named), kept in the record; or the offerer's refusal, kept in its
    /// words. Placed only for a ref this Factory wanted, only while the grant
    /// stands, checked against its own record of the grant, only from the
    /// world its take names as the offerer (the answer's `offerer`, which
    /// the relay names as the field's sender identity; SF13.5b, F6), and only
    /// whole: the answer names the offer and the ref asked for, and holds the
    /// bytes, size and digest it names. The same answer brought again answers
    /// as before. No receipt or refusal carries what was read: each names the
    /// ref and its size.
    pub fn read_received(&self, request: &Value, now: i64) -> Result<Value, OfferRefusal> {
        let object = object(request, "what was read")?;
        only_keys(
            object,
            &["offer_ref", "ref", "answer"],
            "offer-read-received",
        )?;
        let offer_ref = offer_ref_of(object)?;
        let subject = reference(
            object
                .get("ref")
                .ok_or_else(|| OfferRefusal::invalid("what was read names one `ref`"))?,
            "ref",
        )?;
        let answer = object
            .get("answer")
            .ok_or_else(|| OfferRefusal::invalid("what was read carries the offerer's `answer`"))?;
        self.on_taken(&offer_ref, now, |record, _| {
            if let Some(barred) = read_barred(record, now) {
                return Err(barred);
            }
            plain_read(&subject)?;
            read_granted_here(record, &subject)?;
            match (
                record["offerer"].as_str(),
                answer.get("offerer").and_then(Value::as_str),
            ) {
                (Some(offerer), Some(from)) if offerer == from => {}
                (None, _) => {
                    return Err(OfferRefusal::new(
                        "no_offerer",
                        format!("This Factory keeps no world as the one that offered {offer_ref}, so no answer to a read under it is placed; the take names its offerer"),
                    ))
                }
                (Some(_), Some(_)) => {
                    return Err(OfferRefusal::new(
                        "not_the_offerer",
                        format!("The answer for {subject} comes from another world than the one that offered {offer_ref}"),
                    ))
                }
                (Some(_), None) => {
                    return Err(OfferRefusal::new(
                        "bad_answer",
                        format!("The answer for {subject} names no world it comes from"),
                    ))
                }
            }
            let read = checked_read(answer, &offer_ref, &subject)?;
            let standing = record["reads"][subject.as_str()].clone();
            let after = standing["after"].as_str().filter(|_| standing["state"] == "wanted");
            if let Some(after) = after {
                let page = match &read {
                    Answered::Folder { entries, .. } => entries
                        .iter()
                        .all(|entry| entry["ref"].as_str().is_some_and(|entry| entry > after)),
                    Answered::Refused { .. } => true,
                    Answered::File { .. } => false,
                };
                if !page {
                    return Err(OfferRefusal::new(
                        "bad_answer",
                        format!("The answer for {subject} is not the page asked for, after {after}"),
                    ));
                }
            }
            let mut kept = match &read {
                Answered::File { bytes, sha256 } => json!({"state": "placed", "kind": "file",
                    "byte_len": bytes.len(), "sha256": sha256, "file": key_of(&subject)}),
                Answered::Folder { entries, more } => json!({"state": "placed", "kind": "folder",
                    "entries": entries, "more": more}),
                Answered::Refused { code, message } => json!({"state": "refused",
                    "why": {"code": code, "message": message}, "settled_at": now}),
            };
            let same = |key: &str| standing[key] == kept[key];
            if standing["state"] != "wanted" {
                if standing["state"] == kept["state"]
                    && ["kind", "sha256", "entries", "more", "why"]
                        .iter()
                        .all(|key| same(key))
                {
                    let mut answer = receipt("read-received", record, None);
                    answer["read"] = read_seen(&subject, &standing);
                    return Ok(answer);
                }
                return Err(OfferRefusal::new(
                    "not_wanted",
                    format!("This task is not waiting to read {subject}"),
                ));
            }
            if let Answered::File { bytes, .. } = &read {
                let refuse = |error: io::Error| {
                    io_refusal(
                        &format!(
                            "could not place the {} bytes read for {subject}",
                            bytes.len()
                        ),
                        error,
                    )
                };
                let dir = reads::open(&self.root, &key_of(&offer_ref), true)
                    .map_err(refuse)?
                    .ok_or_else(|| refuse(io::ErrorKind::NotFound.into()))?;
                reads::place(&dir, &key_of(&subject), bytes).map_err(refuse)?;
            }
            kept["wanted_at"] = standing["wanted_at"].clone();
            kept["placed_at"] = json!(now);
            if let (Some(after), Answered::Folder { .. }) = (after, &read) {
                kept["after"] = json!(after);
            }
            record["reads"][subject.as_str()] = kept;
            let mut answer = receipt("read-received", record, None);
            answer["read"] = read_seen(&subject, &record["reads"][subject.as_str()]);
            Ok(answer)
        })
    }

    /// The offers this Factory made and those it took, as each stands, read
    /// alone.
    pub fn reading(&self) -> Result<Value, OfferRefusal> {
        Ok(json!({
            "schema": TASK_OFFERS_READING,
            "made": self.records(Side::Made)?,
            "taken": self.records(Side::Taken)?,
            "automatic_agent_or_model_invocation": false,
        }))
    }

    /// `offers`: the offers this Factory made and those it took, as each
    /// stands at `now`: a taken offer's reads are settled first where they
    /// must be (SF13.5b, F3), so a lapse found here drops what was read.
    pub fn offers(&self, now: i64) -> Result<Value, OfferRefusal> {
        for record in self.records(Side::Taken)? {
            let Some(offer_ref) = record["offer"]["offer_ref"].as_str() else {
                continue;
            };
            if self.unsettled(offer_ref, &record, now) {
                self.on_taken(offer_ref, now, |_, _| Ok(Value::Null))?;
            }
        }
        self.reading()
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

/// A request's bytes, read no further than one byte past `MAX_REQUEST_BYTES`:
/// one that holds more is refused before it is parsed, and nothing past the
/// bound is read (SF13.5b, F1).
fn request_bytes(source: &mut impl Read, from: &str) -> Result<Vec<u8>, OfferRefusal> {
    read_bounded(source, MAX_REQUEST_BYTES)
        .map_err(|error| io_refusal(&format!("could not read the request {from}"), error))?
        .ok_or_else(|| {
            OfferRefusal::new(
                "too_large",
                format!(
                    "The request holds more than {MAX_REQUEST_BYTES} bytes; it is refused unread"
                ),
            )
        })
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
        return store_at(state).offers(now);
    }
    let (state, source) = match args {
        [_, state] => (state, "-"),
        [_, state, source] => (state, source.as_str()),
        _ => return Err(usage("expected <state> and at most one request".into())),
    };
    let bytes = if source == "-" {
        match stdin {
            Some(text) => request_bytes(&mut text.as_bytes(), "from stdin"),
            None => request_bytes(&mut io::stdin().lock(), "from stdin"),
        }
    } else {
        let mut file = std::fs::File::open(source).map_err(|error| {
            io_refusal(&format!("could not read the request file {source}"), error)
        })?;
        request_bytes(&mut file, &format!("file {source}"))
    }?;
    let request: Value = serde_json::from_slice(&bytes)
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
        "offer-revoked" => store.revoked(&request, now),
        "offer-return" => store.give_back(&request, now),
        "offer-want" => store.want(&request, now),
        "offer-unwant" => store.unwant(&request, now),
        "offer-read-received" => store.read_received(&request, now),
        other => Err(usage(format!("unknown offer operation `{other}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::create_developmental_conformance_state;

    const NOW: i64 = 1_793_000_000_000;
    const HOUR: i64 = 60 * 60 * 1000;
    const MINUTE: i64 = 60 * 1000;

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
        served_in(two, offer, tempfile::tempdir().unwrap())
    }

    /// The same, placed in `dir`.
    fn served_in(
        two: &Two,
        offer: &Value,
        dir: tempfile::TempDir,
    ) -> (tempfile::TempDir, OfferStore) {
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

    /// A temporary folder whose paths leave room for a Unix socket's name
    /// (`sun_path` holds 104 bytes on macOS): the temporary folder when its
    /// path is short enough, else the user's own temporary folder on macOS
    /// (`/tmp` elsewhere), so a long `TMPDIR` never fails a bind (SF13.3d;
    /// the review's Info 5).
    #[cfg(unix)]
    fn short_tempdir() -> tempfile::TempDir {
        let fits = |base: &Path| base.as_os_str().len() + "/.tmpXXXXXX/docs/sock".len() < 100;
        let base = std::env::temp_dir();
        let base = if fits(&base) {
            base
        } else {
            user_temp_dir()
                .filter(|dir| fits(dir))
                .unwrap_or_else(|| PathBuf::from("/tmp"))
        };
        tempfile::Builder::new().tempdir_in(base).unwrap()
    }

    /// macOS's per-user temporary folder (`confstr(_CS_DARWIN_USER_TEMP_DIR)`),
    /// whatever `TMPDIR` says.
    #[cfg(target_vendor = "apple")]
    fn user_temp_dir() -> Option<PathBuf> {
        use std::os::unix::ffi::OsStringExt;
        let mut buf = vec![0u8; 1024];
        // SAFETY: `buf` is writable for `buf.len()` bytes; confstr writes at
        // most that many, NUL included, and returns the length it needed.
        let needed = unsafe {
            libc::confstr(
                libc::_CS_DARWIN_USER_TEMP_DIR,
                buf.as_mut_ptr().cast(),
                buf.len(),
            )
        };
        if needed == 0 || needed > buf.len() {
            return None;
        }
        buf.truncate(needed - 1);
        Some(PathBuf::from(std::ffi::OsString::from_vec(buf)))
    }

    #[cfg(all(unix, not(target_vendor = "apple")))]
    fn user_temp_dir() -> Option<PathBuf> {
        None
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
        // under a short folder, so the socket's name fits whatever TMPDIR is
        let (dir, store) = served_in(&two, &offer, short_tempdir());
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

    /// The world that offered the task, as the taker's relay names it from
    /// the field's sender identity (SF13.5b, F6).
    const OFFERER: &str = "world:frank";

    fn take(two: &Two, offer: &Value, at: i64) -> Result<Value, OfferRefusal> {
        let request = json!({"offer": offer, "offerer": OFFERER, "by": {"world_ref": "world:ada", "name": "Ada"}});
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
            .revoked(
                &json!({"offer_ref": offer_ref, "revoked_at": NOW + 3 * HOUR}),
                NOW + 3 * HOUR,
            )
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
        // what a task touches is kept as a read reaches it too, and served so (SF13.3d; the review's Info 5)
        let touching = two
            .offerer
            .make(
                &with(
                    json!({"run_ref": two.run_ref, "workflow_unit_ref": two.unit_ref, "star_ref": "star:river-maps",
                        "grant": {"reads": ["project:river/docs"], "touches": ["project:river//maps/"], "lapses_at": NOW + HOUR}}),
                    OFFER_CAPABILITY_REF,
                ),
                NOW,
            )
            .unwrap()["offer"]
            .clone();
        assert_eq!(touching["grant"]["touches"], json!(["project:river/maps"]));
        let (_touched, touched) = served(&two, &touching);
        assert_eq!(
            touched
                .read_granted(
                    &json!({"offer_ref": touching["offer_ref"], "ref": "project:river/maps/delta"}),
                    "world:ada",
                    NOW,
                )
                .unwrap()["content_base64"],
            "Y2hhcnQ="
        );
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

        // A failure before publication (the record's folder read-only, so no
        // stage name can be made there) is said in the OS's words alone,
        // never with the stage path (SF13.3d, F6 in full).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let fresh = self::two();
            let offer = make(&fresh, NOW + HOUR);
            let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
            let root = fresh.state.parent().unwrap().to_path_buf();
            let made = root.join(FOLDER).join("made");
            std::fs::set_permissions(&made, std::fs::Permissions::from_mode(0o555)).unwrap();
            let refused = told_taken(&fresh.offerer, &offer_ref, "world:ada");
            std::fs::set_permissions(&made, std::fs::Permissions::from_mode(0o755)).unwrap();
            let refused = refused.unwrap_err();
            assert_eq!(refused.code, "io", "{refused}");
            assert!(
                refused
                    .message
                    .starts_with("the change was not published: "),
                "{refused}"
            );
            assert!(refused.message.contains("os error"), "{refused}");
            assert!(
                !refused.message.contains(root.to_str().unwrap()),
                "{refused}"
            );
            assert!(!refused.message.contains(".factory-owner-"), "{refused}");
            let mine = fresh.offerer.reading().unwrap();
            assert!(mine["made"][0]["taken"].is_null(), "nothing was kept");
        }
    }

    /// SF13.3d H1: an offer returned and received before this Factory kept
    /// who took it (a record as #313 wrote it: returned, its return kept, no
    /// `taken`) is told its taker without its state moving back; the same
    /// return received again, by the world it names, answers as before
    /// ahead of the taker check, and any other return meets that check
    /// first.
    #[test]
    fn an_offer_returned_before_its_taker_was_kept_is_told_it_and_its_return_answers_again() {
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
        received(&two, &back).unwrap();
        change_kept(&two.offerer, &offer_ref, |record| {
            record.as_object_mut().unwrap().remove("taken");
        });
        let kept = || std::fs::read(two.offerer.path(Side::Made, &offer_ref)).unwrap();
        let before = kept();
        let receive = |back: &Value, by: &str| two.offerer.receive(&json!({"return": back}), by);

        // The same return again answers as before, though no taker is kept,
        // and changes nothing.
        let again = receive(&back, "world:ada").unwrap();
        assert_eq!(
            (&again["act"], &again["state"]),
            (&json!("receive"), &json!("returned"))
        );
        assert_eq!(
            kept(),
            before,
            "a repeat spends nothing and changes nothing"
        );
        // Any other return, or the same one brought by another world, meets
        // the taker check first.
        let mut other = back.clone();
        other["made"]["summary"] = json!("Charted other channels.");
        let refused = receive(&other, "world:ada").unwrap_err();
        assert_eq!(refused.code, "not_the_taker", "{refused}");
        assert!(refused.message.contains("keeps no world"), "{refused}");
        assert_eq!(
            receive(&back, "world:bob").unwrap_err().code,
            "not_the_taker"
        );
        assert_eq!(kept(), before);

        // The take is told on the returned record: kept, and its state stays.
        told_taken(&two.offerer, &offer_ref, "world:ada").unwrap();
        let mine = two.offerer.reading().unwrap();
        assert_eq!(mine["made"][0]["state"], "returned");
        assert_eq!(mine["made"][0]["taken"]["world_ref"], "world:ada");
        assert!(!mine["made"][0]["return"].is_null(), "what came back stays");
        // and what came back is received and accepted, as the window does
        receive(&back, "world:ada").unwrap();
        let accepted = two
            .offerer
            .accept(
                &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                NOW + HOUR,
            )
            .unwrap();
        assert_eq!(accepted["state"], "accepted");
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

    // ---- SF13.5: the taker's executor reads under the grant ----------------------------

    /// What the field carries back for one read, as the taker's relay hands
    /// it on: the offerer's answer or its refusal, as a document, whole, with
    /// the world the field says sent it (`offerer`, SF13.5b F6).
    fn carried(answer: Result<Value, OfferRefusal>) -> Value {
        let mut document = match answer {
            Ok(answer) => answer,
            Err(refusal) => refusal.document(),
        };
        document["offerer"] = json!(OFFERER);
        serde_json::from_str(&document.to_string()).unwrap()
    }

    fn want(two: &Two, offer_ref: &str, subject: &str, at: i64) -> Result<Value, OfferRefusal> {
        two.taker
            .want(&json!({"offer_ref": offer_ref, "ref": subject}), at)
    }

    fn arrived(
        two: &Two,
        offer_ref: &str,
        subject: &str,
        answer: &Value,
        at: i64,
    ) -> Result<Value, OfferRefusal> {
        two.taker.read_received(
            &json!({"offer_ref": offer_ref, "ref": subject, "answer": answer}),
            at,
        )
    }

    fn placed_bytes(two: &Two, offer_ref: &str, subject: &str) -> Option<Vec<u8>> {
        std::fs::read(two.taker.reads_folder(offer_ref).join(key_of(subject))).ok()
    }

    #[test]
    fn the_takers_executor_wants_a_granted_read_and_the_offerers_answer_is_placed_whole() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (_dir, store) = served(&two, &offer);
        let serve = |subject: &str| {
            carried(store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                NOW + HOUR,
            ))
        };

        // A file: wanted, served by the offerer, placed as it was served.
        let brief = "project:river/docs/brief.md";
        let wanted = want(&two, &offer_ref, brief, NOW + HOUR).unwrap();
        assert_eq!(
            (wanted["act"].as_str(), wanted["read"]["state"].as_str()),
            (Some("want"), Some("wanted"))
        );
        assert_eq!(
            placed_bytes(&two, &offer_ref, brief),
            None,
            "nothing is read before the offerer answers"
        );
        let answer = serve(brief);
        let placed = arrived(&two, &offer_ref, brief, &answer, NOW + HOUR).unwrap();
        assert_eq!(
            (
                placed["act"].as_str(),
                placed["read"]["state"].as_str(),
                placed["read"]["kind"].as_str(),
                placed["read"]["byte_len"].as_u64()
            ),
            (
                Some("read-received"),
                Some("placed"),
                Some("file"),
                Some(11)
            )
        );
        assert_eq!(
            placed_bytes(&two, &offer_ref, brief).as_deref(),
            Some(&b"The brief.\n"[..])
        );
        // Brought again, the same answer answers as before; wanted again, the read stands placed.
        assert_eq!(
            arrived(&two, &offer_ref, brief, &answer, NOW + HOUR).unwrap()["read"],
            placed["read"]
        );
        assert_eq!(
            want(&two, &offer_ref, brief, NOW + HOUR).unwrap()["read"]["state"],
            "placed"
        );

        // A folder: its entries, kept in the record.
        let docs = "project:river/docs";
        want(&two, &offer_ref, docs, NOW + HOUR).unwrap();
        let listed = arrived(&two, &offer_ref, docs, &serve(docs), NOW + HOUR).unwrap();
        assert_eq!(listed["read"]["kind"], "folder");
        let names: Vec<&str> = listed["read"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| entry["ref"].as_str())
            .collect();
        assert_eq!(
            names,
            ["project:river/docs/brief.md", "project:river/docs/deep"],
            "the folder as the offerer listed it"
        );

        // The offerer's refusal, kept in its words; wanted again, it is asked again.
        let big = "project:river/docs/big.bin";
        want(&two, &offer_ref, big, NOW + HOUR).unwrap();
        let refused = arrived(&two, &offer_ref, big, &serve(big), NOW + HOUR).unwrap();
        assert_eq!(
            (
                refused["read"]["state"].as_str(),
                refused["read"]["why"]["code"].as_str()
            ),
            (Some("refused"), Some("too_large"))
        );
        assert_eq!(placed_bytes(&two, &offer_ref, big), None);
        assert_eq!(
            want(&two, &offer_ref, big, NOW + HOUR).unwrap()["read"]["state"],
            "wanted"
        );

        // What is wanted stands in the taker's offers, as the relay reads it.
        let reading = two.taker.reading().unwrap();
        let reads = &reading["taken"][0]["reads"];
        assert_eq!(
            (reads[brief]["state"].as_str(), reads[big]["state"].as_str()),
            (Some("placed"), Some("wanted"))
        );
    }

    #[test]
    fn a_read_is_wanted_and_placed_only_under_the_grant_while_it_stands() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (_dir, store) = served(&two, &offer);
        let serve = |subject: &str| {
            carried(store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                NOW + HOUR,
            ))
        };
        let (brief, note) = (
            "project:river/docs/brief.md",
            "project:river/docs/deep/note.md",
        );

        // Outside the grant: never wanted, and never placed.
        assert_eq!(
            want(&two, &offer_ref, "project:river/README.md", NOW + HOUR)
                .unwrap_err()
                .code,
            "not_granted"
        );
        let outside = json!({"schema": TASK_OFFER_READ, "offer_ref": offer_ref, "ref": "project:river/README.md", "kind": "file",
            "byte_len": 3, "sha256": format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(b"abc")), "content_base64": "YWJj",
            "automatic_agent_or_model_invocation": false, "offerer": OFFERER});
        assert_eq!(
            arrived(
                &two,
                &offer_ref,
                "project:river/README.md",
                &outside,
                NOW + HOUR
            )
            .unwrap_err()
            .code,
            "not_granted"
        );
        // Granted but not wanted: not placed.
        assert_eq!(
            arrived(&two, &offer_ref, note, &serve(note), NOW + HOUR)
                .unwrap_err()
                .code,
            "not_wanted"
        );
        assert_eq!(placed_bytes(&two, &offer_ref, note), None);

        // Wanted: placed only as whole as the answer names it.
        want(&two, &offer_ref, note, NOW + HOUR).unwrap();
        let good = serve(note);
        assert_eq!(
            arrived(&two, &offer_ref, note, &serve(brief), NOW + HOUR)
                .unwrap_err()
                .code,
            "bad_answer",
            "an answer for another ref"
        );
        for (key, value) in [
            ("sha256", json!("0".repeat(64))),
            ("byte_len", json!(6)),
            ("content_base64", json!("QSBub3RlLg")),
            ("content_base64", json!("QSBub3RlLh==")),
            ("offer_ref", json!("offer:another")),
            ("automatic_agent_or_model_invocation", json!(true)),
            ("smuggled", json!("x")),
        ] {
            let mut bent = good.clone();
            bent[key] = value;
            assert_eq!(
                arrived(&two, &offer_ref, note, &bent, NOW + HOUR)
                    .unwrap_err()
                    .code,
                "bad_answer",
                "{key} bent"
            );
        }
        assert_eq!(
            placed_bytes(&two, &offer_ref, note),
            None,
            "nothing bent is placed"
        );
        assert_eq!(
            arrived(&two, &offer_ref, note, &good, NOW + HOUR).unwrap()["read"]["state"],
            "placed"
        );

        // At most 32 reads wait at once (pinned as a literal: SF13.5b, 13).
        for n in 0..32 {
            want(
                &two,
                &offer_ref,
                &format!("project:river/docs/w{n}"),
                NOW + HOUR,
            )
            .unwrap();
        }
        assert_eq!(
            want(&two, &offer_ref, "project:river/docs/one-more", NOW + HOUR)
                .unwrap_err()
                .code,
            "too_many"
        );

        // A revocation drops what was placed, and the wants it finds still
        // waiting, and nothing is read after it.
        assert!(placed_bytes(&two, &offer_ref, note).is_some());
        two.taker
            .revoked(
                &json!({"offer_ref": offer_ref, "revoked_at": NOW + HOUR + 5 * MINUTE}),
                NOW + HOUR + 5 * MINUTE,
            )
            .unwrap();
        assert!(
            !two.taker.reads_folder(&offer_ref).exists(),
            "what was read under the grant goes with it"
        );
        let record = &two.taker.reading().unwrap()["taken"][0];
        assert_eq!(
            (
                record["reads"][note]["state"].as_str(),
                record["reads"]["project:river/docs/w0"]["state"].as_str()
            ),
            (Some("dropped"), Some("dropped"))
        );
        assert_eq!(
            arrived(&two, &offer_ref, note, &good, NOW + 2 * HOUR)
                .unwrap_err()
                .code,
            "revoked"
        );
        assert_eq!(
            want(&two, &offer_ref, brief, NOW + 2 * HOUR)
                .unwrap_err()
                .code,
            "revoked"
        );
        assert!(!two.taker.reads_folder(&offer_ref).exists());
    }

    #[test]
    fn what_was_read_goes_when_the_grant_is_spent_or_lapses() {
        let brief = "project:river/docs/brief.md";
        let placed = |two: &Two, offer: &Value| -> (String, Value) {
            let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
            take(two, offer, NOW + HOUR).unwrap();
            let (_dir, store) = served(two, offer);
            want(two, &offer_ref, brief, NOW + HOUR).unwrap();
            let answer = carried(store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": brief}),
                "world:ada",
                NOW + HOUR,
            ));
            arrived(two, &offer_ref, brief, &answer, NOW + HOUR).unwrap();
            assert!(placed_bytes(two, &offer_ref, brief).is_some());
            (offer_ref, answer)
        };
        let state =
            |two: &Two| two.taker.reading().unwrap()["taken"][0]["reads"][brief]["state"].clone();

        // Returned: the grant is spent, and what was read goes with it.
        let returned = two();
        let (offer_ref, _) = placed(&returned, &make(&returned, NOW + 24 * HOUR));
        returned
            .taker
            .give_back(
                &json!({"offer_ref": offer_ref, "made": made(json!(["project:river/maps"]))}),
                NOW + 2 * HOUR,
            )
            .unwrap();
        assert!(
            !returned.taker.reads_folder(&offer_ref).exists(),
            "a return drops what was read"
        );
        assert_eq!(state(&returned), "dropped");

        // Lapsed: nothing is wanted or placed, and what was read goes as the
        // lapse is found, by a want or by a placing.
        for by_want in [true, false] {
            let lapsed = two();
            let (offer_ref, answer) = placed(&lapsed, &make(&lapsed, NOW + 2 * HOUR));
            let refused = if by_want {
                want(
                    &lapsed,
                    &offer_ref,
                    "project:river/docs/deep",
                    NOW + 3 * HOUR,
                )
            } else {
                arrived(&lapsed, &offer_ref, brief, &answer, NOW + 3 * HOUR)
            };
            assert_eq!(refused.unwrap_err().code, "lapsed");
            assert!(
                !lapsed.taker.reads_folder(&offer_ref).exists(),
                "a lapsed grant keeps nothing read under it (by_want {by_want})"
            );
            assert_eq!(state(&lapsed), "dropped");
        }
    }

    #[test]
    fn what_was_read_is_named_by_its_ref_and_size_never_by_what_it_holds() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (dir, store) = served(&two, &offer);
        const MARKER: &str = "MARKER-SF135-7f3c1e";
        std::fs::write(
            dir.path().join("docs/marked.md"),
            format!("Before {MARKER} after."),
        )
        .unwrap();
        let marked = "project:river/docs/marked.md";
        let answer = carried(store.read_granted(
            &json!({"offer_ref": offer_ref, "ref": marked}),
            "world:ada",
            NOW + HOUR,
        ));
        let encoded = answer["content_base64"].as_str().unwrap().to_owned();
        let mut said: Vec<String> = Vec::new();
        said.push(
            want(&two, &offer_ref, marked, NOW + HOUR)
                .unwrap()
                .to_string(),
        );
        // every way the answer is refused, then the one it is placed by
        for (key, value) in [
            ("sha256", json!("0".repeat(64))),
            ("byte_len", json!(1)),
            ("ref", json!("project:river/docs/brief.md")),
        ] {
            let mut bent = answer.clone();
            bent[key] = value;
            let refusal = arrived(&two, &offer_ref, marked, &bent, NOW + HOUR).unwrap_err();
            said.push(refusal.to_string());
            said.push(refusal.document().to_string());
        }
        said.push(
            arrived(&two, &offer_ref, marked, &answer, NOW + HOUR)
                .unwrap()
                .to_string(),
        );
        said.push(two.taker.reading().unwrap().to_string());
        let gone = two
            .taker
            .revoked(
                &json!({"offer_ref": offer_ref, "revoked_at": NOW + 2 * HOUR}),
                NOW + 2 * HOUR,
            )
            .unwrap();
        said.push(gone.to_string());
        said.push(
            arrived(&two, &offer_ref, marked, &answer, NOW + 2 * HOUR)
                .unwrap_err()
                .document()
                .to_string(),
        );
        for words in &said {
            assert!(
                !words.contains(MARKER) && !words.contains(&encoded),
                "what was read is named here: {words}"
            );
        }
        assert!(
            said.iter().any(|words| words.contains(marked)),
            "the ref is named"
        );
    }

    #[test]
    fn base64_comes_back_as_it_went_and_nothing_else_does() {
        for len in 0..12 {
            let bytes: Vec<u8> = (0..len).map(|n| (n * 37 + 11) as u8).collect();
            assert_eq!(unbase64(&base64(&bytes)), Some(bytes));
        }
        for bent in [
            "A", "AB=C", "A===", "AB==AB==", "AB==", "QR==", "QUJ=", "QU J", "QUJD\n",
        ] {
            assert_eq!(unbase64(bent), None, "{bent}");
        }
        assert_eq!(unbase64("QQ=="), Some(b"A".to_vec()));
    }

    #[test]
    fn the_command_wants_and_places_a_read() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (_dir, store) = served(&two, &offer);
        let taker = two.taker.root.display().to_string();
        let brief = "project:river/docs/brief.md";
        let wanted = execute(
            &["offer-want".into(), taker.clone(), "-".into()],
            Some(&json!({"offer_ref": offer_ref, "ref": brief}).to_string()),
            NOW + HOUR,
        )
        .unwrap();
        assert_eq!(wanted["read"]["state"], "wanted");
        let answer = carried(store.read_granted(
            &json!({"offer_ref": offer_ref, "ref": brief}),
            "world:ada",
            NOW + HOUR,
        ));
        let placed = execute(
            &["offer-read-received".into(), taker, "-".into()],
            Some(&json!({"offer_ref": offer_ref, "ref": brief, "answer": answer}).to_string()),
            NOW + HOUR,
        )
        .unwrap();
        assert_eq!(placed["read"]["state"], "placed");
        assert!(VERBS.contains(&"offer-want") && VERBS.contains(&"offer-read-received"));
    }

    #[test]
    fn the_offerer_answers_a_read_only_inside_the_grant_as_it_stands_when_it_answers() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (dir, store) = served(&two, &offer);
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.md"), b"outside the grant").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("docs/out")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.md"),
            dir.path().join("docs/secret.md"),
        )
        .unwrap();
        let serve = |subject: &str| {
            store.read_granted(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                "world:ada",
                NOW + HOUR,
            )
        };

        // A ref outside the grant, and a link out of it, answer nothing, in words.
        let outside_grant = serve("project:river/README.md").unwrap_err();
        assert_eq!(outside_grant.code, "not_granted");
        assert!(outside_grant.message.contains("project:river/README.md"));
        for linked in [
            "project:river/docs/out/secret.md",
            "project:river/docs/secret.md",
        ] {
            let refused = serve(linked).unwrap_err();
            assert_eq!(refused.code, "not_here", "{linked}");
            assert!(
                refused.message.contains("link") || refused.message.contains("names nothing"),
                "{linked}: {}",
                refused.message
            );
            assert!(!refused.to_string().contains("outside the grant"));
        }

        // Asked while the offer stood; revoked before the offerer answers: the answer is the refusal, in words, and the taker keeps it so.
        let brief = "project:river/docs/brief.md";
        want(&two, &offer_ref, brief, NOW + HOUR).unwrap();
        assert!(serve(brief).is_ok(), "served while the offer stands");
        store
            .revoke(
                &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                NOW + HOUR,
            )
            .unwrap();
        let revoked = serve(brief).unwrap_err();
        assert_eq!(
            (revoked.code, revoked.message.as_str()),
            ("revoked", "The offer was revoked; its grant serves nothing")
        );
        let kept = arrived(&two, &offer_ref, brief, &carried(Err(revoked)), NOW + HOUR).unwrap();
        assert_eq!(
            (
                kept["read"]["state"].as_str(),
                kept["read"]["why"]["code"].as_str()
            ),
            (Some("refused"), Some("revoked"))
        );
        assert_eq!(placed_bytes(&two, &offer_ref, brief), None);
    }

    // ---- SF13.5b: the follow-ups of the review of Factory #315 --------------------------

    /// The offerer's answer to one read as the taker's relay hands it on.
    fn served_read(
        store: &OfferStore,
        offer_ref: &str,
        subject: &str,
        after: Option<&str>,
        at: i64,
    ) -> Value {
        let mut request = json!({"offer_ref": offer_ref, "ref": subject});
        if let Some(after) = after {
            request["after"] = json!(after);
        }
        carried(store.read_granted(&request, "world:ada", at))
    }

    /// A file's answer built by hand, whole, as the relay hands it on.
    fn file_answer(offer_ref: &str, subject: &str, bytes: &[u8]) -> Value {
        use sha2::Digest;
        json!({"schema": TASK_OFFER_READ, "offer_ref": offer_ref, "ref": subject, "kind": "file",
            "byte_len": bytes.len(), "sha256": format!("{:x}", sha2::Sha256::digest(bytes)),
            "content_base64": base64(bytes), "automatic_agent_or_model_invocation": false, "offerer": OFFERER})
    }

    /// A folder's answer built by hand, as the relay hands it on.
    fn folder_answer(offer_ref: &str, subject: &str, entries: Value, more: bool) -> Value {
        json!({"schema": TASK_OFFER_READ, "offer_ref": offer_ref, "ref": subject, "kind": "folder",
            "entries": entries, "more": more, "automatic_agent_or_model_invocation": false, "offerer": OFFERER})
    }

    /// The offerer's refusal built by hand, as the relay hands it on.
    fn refusal_answer(code: &str, message: &str) -> Value {
        json!({"schema": TASK_OFFER_REFUSAL, "code": code, "message": message,
            "effect": "Nothing was changed.", "offerer": OFFERER})
    }

    /// An offer made and taken here (by world:ada, from OFFERER), lapsing at
    /// `lapses_at`.
    fn taken_offer(two: &Two, lapses_at: i64) -> String {
        let offer = make(two, lapses_at);
        take(two, &offer, NOW + HOUR).unwrap();
        offer["offer_ref"].as_str().unwrap().to_owned()
    }

    /// `bytes` wanted and placed as `subject`.
    fn placed(two: &Two, offer_ref: &str, subject: &str, bytes: &[u8]) {
        want(two, offer_ref, subject, NOW + HOUR).unwrap();
        let answer = file_answer(offer_ref, subject, bytes);
        let placed = arrived(two, offer_ref, subject, &answer, NOW + HOUR).unwrap();
        assert_eq!(placed["read"]["state"], "placed", "{subject}");
    }

    /// The taken offer's record, read alone.
    fn kept_record(two: &Two, offer_ref: &str) -> Value {
        two.taker.reading().unwrap()["taken"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["offer"]["offer_ref"] == offer_ref)
            .cloned()
            .unwrap()
    }

    fn read_state(two: &Two, offer_ref: &str, subject: &str) -> Value {
        kept_record(two, offer_ref)["reads"][subject]["state"].clone()
    }

    /// Root removes from a read-only folder, so a removal made to fail by its
    /// mode fails only for another user.
    fn root() -> bool {
        // SAFETY: geteuid reads the process's effective user and cannot fail.
        unsafe { libc::geteuid() == 0 }
    }

    #[test]
    fn a_request_past_its_bound_is_refused_unread_and_the_largest_answer_fits_within_it() {
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        let taker = two.taker.root.display().to_string();
        let through = |source: &str, stdin: Option<&str>| {
            execute(
                &["offer-read-received".into(), taker.clone(), source.into()],
                stdin,
                NOW + HOUR,
            )
        };
        // One byte over: refused before it is parsed (it is no JSON at all).
        let over = "x".repeat(MAX_REQUEST_BYTES as usize + 1);
        assert_eq!(through("-", Some(&over)).unwrap_err().code, "too_large");
        let file = two.taker.root.join("over.json");
        std::fs::write(&file, &over).unwrap();
        assert_eq!(
            through(file.to_str().unwrap(), None).unwrap_err().code,
            "too_large"
        );
        // At the bound: read and parsed, then refused for what it says.
        let at = format!("{{{}}}", " ".repeat(MAX_REQUEST_BYTES as usize - 2));
        let parsed = through("-", Some(&at)).unwrap_err();
        assert_eq!(
            (parsed.code, parsed.message.contains("offer_ref")),
            ("invalid", true)
        );

        // The largest file a read carries, as the relay brings it, fits and is placed.
        let largest = "project:river/docs/largest.bin";
        want(&two, &offer_ref, largest, NOW + HOUR).unwrap();
        let bytes = vec![0xA5; MAX_READ_BYTES as usize];
        let request = json!({"offer_ref": offer_ref, "ref": largest,
            "answer": file_answer(&offer_ref, largest, &bytes)})
        .to_string();
        assert!(request.len() as u64 <= MAX_REQUEST_BYTES);
        assert_eq!(
            through("-", Some(&request)).unwrap()["read"]["state"],
            "placed"
        );
        assert_eq!(
            placed_bytes(&two, &offer_ref, largest).map(|bytes| bytes.len()),
            Some(MAX_READ_BYTES as usize)
        );
        // So does the largest folder: every byte of every ref escaped as six, the
        // request's own refs too. It is read and parsed, then refused for what it says.
        let escaped = "\u{1}".repeat(MAX_REF_CHARS);
        let entries: Vec<Value> = (0..MAX_READ_ENTRIES)
            .map(|_| json!({"ref": escaped, "kind": "folder", "byte_len": u64::MAX}))
            .collect();
        let folder = json!({"offer_ref": escaped, "ref": escaped,
            "answer": folder_answer(&escaped, &escaped, json!(entries), true)})
        .to_string();
        let largest_answer = MAX_READ_BYTES.div_ceil(3) * 4;
        assert!(
            folder.len() as u64
                > largest_answer.max(MAX_READ_ENTRIES as u64 * (6 * MAX_REF_CHARS as u64 + 64))
                && folder.len() as u64 <= MAX_REQUEST_BYTES,
            "{} bytes",
            folder.len()
        );
        let parsed = through("-", Some(&folder)).unwrap_err();
        assert_ne!(parsed.code, "too_large", "{}", parsed.message);
    }

    #[test]
    fn a_removal_that_fails_leaves_its_read_drop_pending_says_where_and_the_next_verb_sweeps_it() {
        use std::os::unix::fs::PermissionsExt;
        if root() {
            eprintln!("skipped: root removes from a read-only folder");
            return;
        }
        let brief = "project:river/docs/brief.md";
        let mode = |path: &Path, bits: u32| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits)).unwrap()
        };

        // A revocation: the record says revoked, the read waits as drop_pending with its path, and the answer says where.
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        placed(&two, &offer_ref, brief, b"The brief.\n");
        let folder = two.taker.reads_folder(&offer_ref);
        let path = folder.join(key_of(brief)).display().to_string();
        mode(&folder, 0o555);
        let gone = two
            .taker
            .revoked(
                &json!({"offer_ref": offer_ref, "revoked_at": NOW + HOUR}),
                NOW + HOUR,
            )
            .unwrap();
        mode(&folder, 0o700);
        assert_eq!(gone["state"], "revoked");
        assert_eq!(
            gone["left_on_disk"]["reads"],
            json!([{"ref": brief, "path": path}])
        );
        let said = gone["left_on_disk"]["message"].as_str().unwrap();
        assert!(
            said.contains(&path) && said.contains("Permission denied"),
            "{said}"
        );
        assert_eq!(read_state(&two, &offer_ref, brief), "drop_pending");
        assert_eq!(
            kept_record(&two, &offer_ref)["reads"][brief]["path"],
            json!(path)
        );
        assert!(placed_bytes(&two, &offer_ref, brief).is_some());
        // The next verb, a check, sweeps it first.
        let check = two
            .taker
            .check(&json!({"offer_ref": offer_ref, "read": brief}), NOW + HOUR)
            .unwrap();
        assert_eq!(
            (check["allowed"].as_bool(), check["left_on_disk"].is_null()),
            (Some(false), true)
        );
        assert!(!folder.exists());
        assert_eq!(read_state(&two, &offer_ref, brief), "dropped");
        assert!(kept_record(&two, &offer_ref)["reads"][brief]["path"].is_null());

        // A check that fails to sweep says so too; a return: the same, and a reading sweeps it.
        let two = self::two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        placed(&two, &offer_ref, brief, b"The brief.\n");
        let folder = two.taker.reads_folder(&offer_ref);
        mode(&folder, 0o555);
        let back = two.taker.give_back(
            &json!({"offer_ref": offer_ref, "made": made(json!(["project:river/maps"]))}),
            NOW + HOUR,
        );
        assert_eq!(back.unwrap()["schema"], TASK_OFFER_RETURN);
        assert_eq!(read_state(&two, &offer_ref, brief), "drop_pending");
        let check = two
            .taker
            .check(&json!({"offer_ref": offer_ref, "read": brief}), NOW + HOUR)
            .unwrap();
        assert_eq!(
            check["left_on_disk"]["reads"][0]["ref"].as_str(),
            Some(brief)
        );
        mode(&folder, 0o700);
        two.taker.offers(NOW + HOUR).unwrap();
        assert!(!folder.exists());
        assert_eq!(read_state(&two, &offer_ref, brief), "dropped");
    }

    #[test]
    fn a_lapse_found_by_a_check_or_a_reading_drops_what_was_read() {
        let brief = "project:river/docs/brief.md";
        let lapses = NOW + 2 * HOUR;
        let two = two();
        let checked = taken_offer(&two, lapses);
        placed(&two, &checked, brief, b"The brief.\n");
        // Still standing a moment before.
        let before = two
            .taker
            .check(&json!({"offer_ref": checked, "read": brief}), lapses - 1)
            .unwrap();
        assert_eq!(before["allowed"], true);
        assert!(placed_bytes(&two, &checked, brief).is_some());
        // At the moment it lapses, a check alone leaves no file and no placed read.
        let at = two
            .taker
            .check(&json!({"offer_ref": checked, "read": brief}), lapses)
            .unwrap();
        assert_eq!(
            (at["allowed"].as_bool(), at["why"].as_str()),
            (Some(false), Some("The grant has lapsed"))
        );
        assert!(!two.taker.reads_folder(&checked).exists());
        assert_eq!(read_state(&two, &checked, brief), "dropped");
        // A want at that moment is refused as lapsed.
        assert_eq!(
            want(&two, &checked, "project:river/docs/deep", lapses)
                .unwrap_err()
                .code,
            "lapsed"
        );

        // Reading the offers after a lapse drops what was read.
        let two = self::two();
        let read = taken_offer(&two, lapses);
        placed(&two, &read, brief, b"The brief.\n");
        let reading = two.taker.offers(lapses + HOUR).unwrap();
        assert_eq!(reading["taken"][0]["reads"][brief]["state"], "dropped");
        assert!(!two.taker.reads_folder(&read).exists());

        // And `offers` on the command line is that same reading: it drops what was read too.
        let two = self::two();
        let read = taken_offer(&two, lapses);
        placed(&two, &read, brief, b"The brief.\n");
        let taker = two._dirs.1.path().to_string_lossy().into_owned();
        let reading = execute(&["offers".into(), taker], None, lapses + HOUR).unwrap();
        assert_eq!(reading["taken"][0]["reads"][brief]["state"], "dropped");
        assert!(!two.taker.reads_folder(&read).exists());
    }

    #[test]
    fn a_link_in_the_way_of_the_reads_folder_is_refused_and_removed_never_followed_on_a_drop() {
        let brief = "project:river/docs/brief.md";
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("kept"), b"not ours").unwrap();
        let only_kept = |dir: &Path| {
            let mut names: Vec<_> = std::fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names == [std::ffi::OsString::from("kept")]
        };
        for at_offer in [false, true] {
            let two = two();
            let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
            let reads = two.taker.root.join(FOLDER).join(READS);
            let link = if at_offer {
                std::fs::create_dir_all(&reads).unwrap();
                reads.join(key_of(&offer_ref))
            } else {
                reads.clone()
            };
            std::os::unix::fs::symlink(outside.path(), &link).unwrap();
            want(&two, &offer_ref, brief, NOW + HOUR).unwrap();
            let answer = file_answer(&offer_ref, brief, b"The brief.\n");
            let refused = arrived(&two, &offer_ref, brief, &answer, NOW + HOUR).unwrap_err();
            assert_eq!(refused.code, "io", "at_offer {at_offer}");
            assert!(refused.message.contains("a link"), "{}", refused.message);
            assert!(only_kept(outside.path()), "nothing placed through it");
            assert_eq!(read_state(&two, &offer_ref, brief), "wanted");
            // The drop removes the link itself; what it pointed at stays as it was.
            two.taker
                .revoked(
                    &json!({"offer_ref": offer_ref, "revoked_at": NOW + HOUR}),
                    NOW + HOUR,
                )
                .unwrap();
            assert!(
                std::fs::symlink_metadata(&link).is_err(),
                "at_offer {at_offer}"
            );
            assert!(only_kept(outside.path()));
            assert_eq!(read_state(&two, &offer_ref, brief), "dropped");
        }

        // A link at task-offers, under which the records stand, is refused for a drop: it is
        // neither removed nor followed, and what was read stays where it is, said as left.
        let two = self::two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        placed(&two, &offer_ref, brief, b"The brief.\n");
        let moved = tempfile::tempdir().unwrap();
        let offers = two.taker.root.join(FOLDER);
        std::fs::rename(&offers, moved.path().join(FOLDER)).unwrap();
        std::os::unix::fs::symlink(moved.path().join(FOLDER), &offers).unwrap();
        let placed_at = moved
            .path()
            .join(FOLDER)
            .join(READS)
            .join(key_of(&offer_ref))
            .join(key_of(brief));
        let _ = two.taker.revoked(
            &json!({"offer_ref": offer_ref, "revoked_at": NOW + HOUR}),
            NOW + HOUR,
        );
        assert!(std::fs::symlink_metadata(&offers)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read(&placed_at).unwrap(), b"The brief.\n");
        assert_eq!(read_state(&two, &offer_ref, brief), "drop_pending");

        // A link standing where the file goes is replaced as a name, never written through.
        let two = self::two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        want(&two, &offer_ref, brief, NOW + HOUR).unwrap();
        let folder = two.taker.reads_folder(&offer_ref);
        std::fs::create_dir_all(&folder).unwrap();
        std::os::unix::fs::symlink(outside.path().join("kept"), folder.join(key_of(brief)))
            .unwrap();
        let answer = file_answer(&offer_ref, brief, b"The brief.\n");
        arrived(&two, &offer_ref, brief, &answer, NOW + HOUR).unwrap();
        assert_eq!(
            std::fs::read(outside.path().join("kept")).unwrap(),
            b"not ours"
        );
        let leaf = std::fs::symlink_metadata(folder.join(key_of(brief))).unwrap();
        assert!(leaf.is_file());
        assert_eq!(
            placed_bytes(&two, &offer_ref, brief).as_deref(),
            Some(&b"The brief.\n"[..])
        );
    }

    #[test]
    fn the_taker_wants_no_ref_the_offerer_would_refuse_and_keeps_no_entry_it_would_not_list() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (_dir, store) = served(&two, &offer);
        for odd in [
            "project:river/docs/../../README.md",
            "project:river/docs/.git/config",
            "project:river/docs/.GIT/config",
            "project:river/docs/a\\b",
            "project:river/docs/./brief.md",
        ] {
            let offerer = store
                .read_granted(
                    &json!({"offer_ref": offer_ref, "ref": odd}),
                    "world:ada",
                    NOW + HOUR,
                )
                .unwrap_err();
            let taker = want(&two, &offer_ref, odd, NOW + HOUR).unwrap_err();
            assert_eq!(
                (taker.code, taker.message.as_str()),
                (offerer.code, offerer.message.as_str()),
                "{odd}"
            );
            assert_eq!(taker.code, "not_here", "{odd}");
            let check = two
                .taker
                .check(&json!({"offer_ref": offer_ref, "read": odd}), NOW + HOUR)
                .unwrap();
            assert_eq!(check["allowed"], false, "{odd}");
        }
        assert!(
            kept_record(&two, &offer_ref)["reads"].is_null(),
            "no want slot spent"
        );

        // A folder's entries are kept only as the offerer lists them: one plain step under it.
        let docs = "project:river/docs";
        want(&two, &offer_ref, docs, NOW + HOUR).unwrap();
        for entry in [
            "project:river/docs/..",
            "project:river/docs/.",
            "project:river/docs/.git",
            "project:river/docs/a\\b",
            "project:river/docs/deep/note.md",
            "project:river/maps/delta",
            "project:river/docs/",
        ] {
            let answer = folder_answer(
                &offer_ref,
                docs,
                json!([{"ref": entry, "kind": "file", "byte_len": 1}]),
                false,
            );
            assert_eq!(
                arrived(&two, &offer_ref, docs, &answer, NOW + HOUR)
                    .unwrap_err()
                    .code,
                "bad_answer",
                "{entry}"
            );
        }
        let entries: Vec<Value> = (0..=MAX_READ_ENTRIES)
            .map(|n| json!({"ref": format!("{docs}/f{n:04}"), "kind": "file", "byte_len": 1}))
            .collect();
        let too_many = folder_answer(&offer_ref, docs, json!(entries), false);
        assert_eq!(
            arrived(&two, &offer_ref, docs, &too_many, NOW + HOUR)
                .unwrap_err()
                .code,
            "bad_answer"
        );
        let large = folder_answer(
            &offer_ref,
            docs,
            json!([{"ref": format!("{docs}/big"), "kind": "file", "byte_len": MAX_READ_BYTES + 1}]),
            false,
        );
        let refused = arrived(&two, &offer_ref, docs, &large, NOW + HOUR).unwrap_err();
        assert!(
            refused.message.contains("larger than one read"),
            "{}",
            refused.message
        );
        let good = folder_answer(
            &offer_ref,
            docs,
            json!([{"ref": format!("{docs}/brief.md"), "kind": "file", "byte_len": 11}]),
            false,
        );
        assert_eq!(
            arrived(&two, &offer_ref, docs, &good, NOW + HOUR).unwrap()["read"]["state"],
            "placed"
        );
    }

    #[test]
    fn an_answer_is_placed_only_from_the_world_that_offered() {
        let brief = "project:river/docs/brief.md";
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        assert_eq!(kept_record(&two, &offer_ref)["offerer"], OFFERER);
        want(&two, &offer_ref, brief, NOW + HOUR).unwrap();
        let good = file_answer(&offer_ref, brief, b"The brief.\n");
        let mut other = good.clone();
        other["offerer"] = json!("world:mallory");
        assert_eq!(
            arrived(&two, &offer_ref, brief, &other, NOW + HOUR)
                .unwrap_err()
                .code,
            "not_the_offerer"
        );
        let mut unnamed = good.clone();
        unnamed.as_object_mut().unwrap().remove("offerer");
        assert_eq!(
            arrived(&two, &offer_ref, brief, &unnamed, NOW + HOUR)
                .unwrap_err()
                .code,
            "bad_answer"
        );
        let mut refusal =
            refusal_answer("revoked", "The offer was revoked; its grant serves nothing");
        refusal["offerer"] = json!("world:mallory");
        assert_eq!(
            arrived(&two, &offer_ref, brief, &refusal, NOW + HOUR)
                .unwrap_err()
                .code,
            "not_the_offerer",
            "a refusal too"
        );
        assert_eq!(placed_bytes(&two, &offer_ref, brief), None);
        assert_eq!(read_state(&two, &offer_ref, brief), "wanted");
        assert_eq!(
            arrived(&two, &offer_ref, brief, &good, NOW + HOUR).unwrap()["read"]["state"],
            "placed"
        );

        // A take that names no offerer places nothing, until it is told again with one.
        let dir = tempfile::tempdir().unwrap();
        let store = OfferStore::at(dir.path());
        let by = json!({"world_ref": "world:bo", "name": "Bo"});
        let bare = with(json!({"offer": offer, "by": by}), TAKE_CAPABILITY_REF);
        store.take(&bare, NOW + HOUR).unwrap();
        let wanted = |store: &OfferStore| {
            store.want(&json!({"offer_ref": offer_ref, "ref": brief}), NOW + HOUR)
        };
        let bring = |store: &OfferStore| {
            store.read_received(
                &json!({"offer_ref": offer_ref, "ref": brief, "answer": good}),
                NOW + HOUR,
            )
        };
        wanted(&store).unwrap();
        assert_eq!(bring(&store).unwrap_err().code, "no_offerer");
        let named = with(
            json!({"offer": offer, "offerer": OFFERER, "by": by}),
            TAKE_CAPABILITY_REF,
        );
        assert_eq!(store.take(&named, NOW + HOUR).unwrap()["offerer"], OFFERER);
        let another = with(
            json!({"offer": offer, "offerer": "world:mallory", "by": by}),
            TAKE_CAPABILITY_REF,
        );
        assert_eq!(store.take(&another, NOW + HOUR).unwrap_err().code, "taken");
        assert_eq!(bring(&store).unwrap()["read"]["state"], "placed");
    }

    #[test]
    fn the_record_keeps_at_most_64_settled_reads_by_their_digests_and_words() {
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        let message = "The offerer could not serve this read now. ".repeat(44);
        assert!(message.len() > 1800 && message.len() <= MAX_TEXT_CHARS);
        let subject = |n: usize| format!("project:river/docs/r{n:03}-{}", "a".repeat(275));
        for n in 0..400 {
            let at = NOW + HOUR + n as i64;
            two.taker
                .want(&json!({"offer_ref": offer_ref, "ref": subject(n)}), at)
                .unwrap();
            let refused = refusal_answer("too_large", &message);
            two.taker
                .read_received(
                    &json!({"offer_ref": offer_ref, "ref": subject(n), "answer": refused}),
                    at,
                )
                .unwrap();
        }
        let record = kept_record(&two, &offer_ref);
        let reads = record["reads"].as_object().unwrap();
        assert_eq!(reads.len(), 64);
        assert!(reads.contains_key(&subject(399)) && reads.contains_key(&subject(336)));
        assert!(!reads.contains_key(&subject(335)), "the oldest goes first");
        let size = std::fs::metadata(two.taker.path(Side::Taken, &offer_ref))
            .unwrap()
            .len();
        assert!(size < 200_000, "the record holds {size} bytes");

        // A verb refused keeps the bound too: the reads it found expired are published bounded.
        let later = NOW + 2 * HOUR;
        for n in 400..402 {
            two.taker
                .want(&json!({"offer_ref": offer_ref, "ref": subject(n)}), later)
                .unwrap();
        }
        assert!(two
            .taker
            .want(
                &json!({"offer_ref": offer_ref, "ref": "project:elsewhere/notes.md"}),
                later + 10 * MINUTE
            )
            .is_err());
        let reads = kept_record(&two, &offer_ref)["reads"].clone();
        let reads = reads.as_object().unwrap();
        assert_eq!(reads.len(), 64);
        assert_eq!(reads[&subject(401)]["state"], "expired");
    }

    #[test]
    fn a_record_an_earlier_factory_kept_is_read_as_this_one_reads_it() {
        // A want that names no time has expired, and a want of a ref this Factory would refuse is never placed.
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        let (brief, odd) = (
            "project:river/docs/brief.md",
            "project:river/docs/.git/config",
        );
        let path = two.taker.path(Side::Taken, &offer_ref);
        let mut record: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        record["reads"] =
            json!({brief: {"state": "wanted"}, odd: {"state": "wanted", "wanted_at": NOW + HOUR}});
        std::fs::write(&path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
        assert_eq!(
            two.taker.offers(NOW + HOUR).unwrap()["taken"][0]["reads"][brief]["state"],
            "expired"
        );
        let answer = file_answer(&offer_ref, odd, b"[core]\n");
        assert!(arrived(&two, &offer_ref, odd, &answer, NOW + HOUR).is_err());
        assert_eq!(placed_bytes(&two, &offer_ref, odd), None);
        assert!(!two.taker.reads_folder(&offer_ref).exists());
    }

    #[test]
    fn a_want_expires_with_the_fields_ask_and_one_can_be_unwanted_sooner() {
        let (brief, note) = (
            "project:river/docs/brief.md",
            "project:river/docs/deep/note.md",
        );
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        let answer = file_answer(&offer_ref, brief, b"The brief.\n");

        // Unanswered ten minutes, a want is expired; its answer is not placed; wanted again, it waits again.
        want(&two, &offer_ref, brief, NOW + HOUR).unwrap();
        let reading = two.taker.offers(NOW + HOUR + 10 * MINUTE - 1).unwrap();
        assert_eq!(reading["taken"][0]["reads"][brief]["state"], "wanted");
        let reading = two.taker.offers(NOW + HOUR + 10 * MINUTE).unwrap();
        assert_eq!(reading["taken"][0]["reads"][brief]["state"], "expired");
        assert_eq!(
            arrived(&two, &offer_ref, brief, &answer, NOW + HOUR + 10 * MINUTE)
                .unwrap_err()
                .code,
            "not_wanted"
        );
        assert_eq!(placed_bytes(&two, &offer_ref, brief), None);
        assert_eq!(
            want(&two, &offer_ref, brief, NOW + 2 * HOUR).unwrap()["read"]["state"],
            "wanted"
        );

        // An expired want frees its place among the 32.
        for n in 0..31 {
            want(
                &two,
                &offer_ref,
                &format!("project:river/docs/w{n}"),
                NOW + 2 * HOUR,
            )
            .unwrap();
        }
        assert_eq!(
            want(&two, &offer_ref, note, NOW + 2 * HOUR)
                .unwrap_err()
                .code,
            "too_many"
        );
        assert_eq!(
            want(&two, &offer_ref, note, NOW + 2 * HOUR + 10 * MINUTE).unwrap()["read"]["state"],
            "wanted"
        );

        // Unwanted, a want is dropped, and its answer is not placed.
        let unwant = |subject: &str| {
            two.taker.unwant(
                &json!({"offer_ref": offer_ref, "ref": subject}),
                NOW + 2 * HOUR + 10 * MINUTE,
            )
        };
        assert_eq!(unwant(note).unwrap()["read"]["state"], "dropped");
        let late = file_answer(&offer_ref, note, b"A note.");
        assert_eq!(
            arrived(&two, &offer_ref, note, &late, NOW + 2 * HOUR + 10 * MINUTE)
                .unwrap_err()
                .code,
            "not_wanted"
        );
        assert_eq!(
            unwant("project:river/docs/never").unwrap_err().code,
            "not_wanted"
        );
        // A placed read is not waited on: it stands as it is.
        want(&two, &offer_ref, note, NOW + 2 * HOUR + 10 * MINUTE).unwrap();
        arrived(&two, &offer_ref, note, &late, NOW + 2 * HOUR + 10 * MINUTE).unwrap();
        assert_eq!(unwant(note).unwrap()["read"]["state"], "placed");
        assert!(VERBS.contains(&"offer-unwant"));
    }

    #[test]
    fn a_dropped_read_keeps_no_listing_and_a_refused_one_keeps_its_words() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (_dir, store) = served(&two, &offer);
        let (docs, big) = ("project:river/docs", "project:river/docs/big.bin");
        for subject in [docs, big] {
            want(&two, &offer_ref, subject, NOW + HOUR).unwrap();
            let answer = served_read(&store, &offer_ref, subject, None, NOW + HOUR);
            arrived(&two, &offer_ref, subject, &answer, NOW + HOUR).unwrap();
        }
        assert!(kept_record(&two, &offer_ref)["reads"][docs]["entries"].is_array());
        two.taker
            .revoked(
                &json!({"offer_ref": offer_ref, "revoked_at": NOW + HOUR}),
                NOW + HOUR,
            )
            .unwrap();
        let record = kept_record(&two, &offer_ref);
        let dropped = record["reads"][docs].as_object().unwrap();
        assert_eq!(dropped["state"], "dropped");
        assert!(
            !dropped.contains_key("entries") && !dropped.contains_key("more"),
            "{dropped:?}"
        );
        assert_eq!(record["reads"][big]["state"], "refused");
        assert_eq!(record["reads"][big]["why"]["code"], "too_large");
    }

    #[test]
    fn a_placed_file_found_gone_is_missing_and_can_be_wanted_again() {
        let brief = "project:river/docs/brief.md";
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        placed(&two, &offer_ref, brief, b"The brief.\n");
        let record = kept_record(&two, &offer_ref);
        assert_eq!(
            (
                record["reads"][brief]["file"].as_str(),
                record["reads"][brief]["placed_at"].as_i64()
            ),
            (Some(key_of(brief).as_str()), Some(NOW + HOUR))
        );
        std::fs::remove_file(two.taker.reads_folder(&offer_ref).join(key_of(brief))).unwrap();
        let reading = two.taker.offers(NOW + HOUR).unwrap();
        assert_eq!(reading["taken"][0]["reads"][brief]["state"], "missing");
        assert_eq!(
            want(&two, &offer_ref, brief, NOW + HOUR).unwrap()["read"]["state"],
            "wanted"
        );
        let answer = file_answer(&offer_ref, brief, b"The brief.\n");
        arrived(&two, &offer_ref, brief, &answer, NOW + HOUR).unwrap();
        assert_eq!(
            placed_bytes(&two, &offer_ref, brief).as_deref(),
            Some(&b"The brief.\n"[..])
        );
        // Another file brought for a read placed is refused: the same answer alone answers again.
        let other = file_answer(&offer_ref, brief, b"Not the brief.");
        assert_eq!(
            arrived(&two, &offer_ref, brief, &other, NOW + HOUR)
                .unwrap_err()
                .code,
            "not_wanted"
        );
    }

    #[test]
    fn a_folder_is_paged_by_a_want_naming_after_and_no_page_comes_after_a_revoke() {
        let two = two();
        let offer = make(&two, NOW + 24 * HOUR);
        let offer_ref = offer["offer_ref"].as_str().unwrap().to_owned();
        take(&two, &offer, NOW + HOUR).unwrap();
        let (dir, store) = served(&two, &offer);
        std::fs::create_dir(dir.path().join("docs/many")).unwrap();
        for n in 0..600 {
            std::fs::write(dir.path().join(format!("docs/many/f{n:03}")), b"x").unwrap();
        }
        let many = "project:river/docs/many";
        want(&two, &offer_ref, many, NOW + HOUR).unwrap();
        let first = served_read(&store, &offer_ref, many, None, NOW + HOUR);
        let page = arrived(&two, &offer_ref, many, &first, NOW + HOUR).unwrap();
        assert_eq!(
            (
                page["read"]["entries"].as_array().map(Vec::len),
                page["read"]["more"].as_bool()
            ),
            (Some(MAX_READ_ENTRIES), Some(true))
        );
        let last = page["read"]["entries"][MAX_READ_ENTRIES - 1]["ref"]
            .as_str()
            .unwrap()
            .to_owned();
        let after = |at: i64, from: &str| {
            two.taker.want(
                &json!({"offer_ref": offer_ref, "ref": many, "after": from}),
                at,
            )
        };
        // `after` names an entry of the folder, or nothing is wanted.
        assert_eq!(
            after(NOW + HOUR, "project:river/docs/other")
                .unwrap_err()
                .code,
            "invalid"
        );
        let wanted = after(NOW + HOUR, &last).unwrap();
        assert_eq!(
            (
                wanted["read"]["state"].as_str(),
                wanted["read"]["after"].as_str()
            ),
            (Some("wanted"), Some(last.as_str()))
        );
        // Unanswered ten minutes, the paged want has expired and keeps where it continued: wanted again, it continues there.
        let later = NOW + HOUR + 10 * MINUTE;
        assert_eq!(
            two.taker.offers(later).unwrap()["taken"][0]["reads"][many]["state"],
            "expired"
        );
        let again = after(later, &last).unwrap();
        assert_eq!(
            (
                again["read"]["state"].as_str(),
                again["read"]["after"].as_str()
            ),
            (Some("wanted"), Some(last.as_str()))
        );
        // The first page brought again for the page after it is not the page asked for.
        assert_eq!(
            arrived(&two, &offer_ref, many, &first, NOW + HOUR)
                .unwrap_err()
                .code,
            "bad_answer"
        );
        let second = served_read(&store, &offer_ref, many, Some(&last), NOW + HOUR);
        // A page that repeats `after` itself is not the page after it.
        let mut repeated = second.clone();
        repeated["entries"]
            .as_array_mut()
            .unwrap()
            .insert(0, first["entries"][MAX_READ_ENTRIES - 1].clone());
        assert_eq!(
            arrived(&two, &offer_ref, many, &repeated, NOW + HOUR)
                .unwrap_err()
                .code,
            "bad_answer"
        );
        let page = arrived(&two, &offer_ref, many, &second, NOW + HOUR).unwrap();
        assert_eq!(
            (
                page["read"]["entries"].as_array().map(Vec::len),
                page["read"]["more"].as_bool(),
                page["read"]["after"].as_str()
            ),
            (
                Some(600 - MAX_READ_ENTRIES),
                Some(false),
                Some(last.as_str())
            )
        );
        // Nothing follows the last page, and `after` pages nothing else.
        let next = page["read"]["entries"][0]["ref"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(after(NOW + HOUR, &next).unwrap_err().code, "invalid");
        assert_eq!(
            two.taker
                .want(
                    &json!({"offer_ref": offer_ref, "ref": "project:river/docs/brief.md", "after": next}),
                    NOW + HOUR
                )
                .unwrap_err()
                .code,
            "invalid"
        );

        // Revoked: the offerer serves no page, and the taker wants none.
        store
            .revoke(
                &with(json!({"offer_ref": offer_ref}), OFFER_CAPABILITY_REF),
                NOW + HOUR,
            )
            .unwrap();
        let refused = served_read(&store, &offer_ref, many, Some(&last), NOW + HOUR);
        assert_eq!(refused["code"], "revoked");
        two.taker
            .revoked(
                &json!({"offer_ref": offer_ref, "revoked_at": NOW + HOUR}),
                NOW + HOUR,
            )
            .unwrap();
        assert_eq!(after(NOW + HOUR, &last).unwrap_err().code, "revoked");
    }

    #[test]
    fn placed_files_are_the_takers_alone_and_a_refusal_shows_little_of_a_stray_key() {
        use std::os::unix::fs::PermissionsExt;
        let brief = "project:river/docs/brief.md";
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        // A reads folder made by an earlier Factory, open to all, is made private as a read is placed.
        let reads = two.taker.root.join(FOLDER).join(READS);
        std::fs::create_dir_all(&reads).unwrap();
        std::fs::set_permissions(&reads, std::fs::Permissions::from_mode(0o755)).unwrap();
        placed(&two, &offer_ref, brief, b"The brief.\n");
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let folder = two.taker.reads_folder(&offer_ref);
        assert_eq!(
            (
                mode(&reads),
                mode(&folder),
                mode(&folder.join(key_of(brief)))
            ),
            (0o700, 0o700, 0o600)
        );

        // A key no answer holds is shown by its first 64 characters, escaped.
        let note = "project:river/docs/deep/note.md";
        want(&two, &offer_ref, note, NOW + HOUR).unwrap();
        let stray = format!("\u{202E}{}\n{}", "k".repeat(70), "tail");
        for mut answer in [
            file_answer(&offer_ref, note, b"A note."),
            refusal_answer("not_here", "Nothing stands there"),
        ] {
            answer[stray.as_str()] = json!(1);
            let refused = arrived(&two, &offer_ref, note, &answer, NOW + HOUR).unwrap_err();
            assert_eq!(refused.code, "bad_answer");
            let shown = format!("\\u{{202e}}{}…", "k".repeat(63));
            assert!(refused.message.contains(&shown), "{}", refused.message);
            assert!(!refused.message.contains('\u{202E}') && !refused.message.contains("tail"));
        }
        let request = json!({"offer_ref": offer_ref, "ref": note, stray.as_str(): 1});
        let refused = two.taker.want(&request, NOW + HOUR).unwrap_err();
        assert!(!refused.message.contains('\u{202E}') && !refused.message.contains("tail"));
    }

    #[test]
    fn the_takers_checks_of_an_answer_each_bite() {
        let two = two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        let note = "project:river/docs/deep/note.md";
        want(&two, &offer_ref, note, NOW + HOUR).unwrap();
        let refused =
            |answer: &Value| arrived(&two, &offer_ref, note, answer, NOW + HOUR).unwrap_err();
        // A size past the bound, though bytes and digest agree.
        let mut over = file_answer(&offer_ref, note, b"A note.");
        over["byte_len"] = json!(MAX_READ_BYTES + 1);
        assert!(refused(&over).message.contains("names no size within"));
        // More base64 than the bound's bytes take is refused before it is decoded, in its own words.
        let mut long = file_answer(&offer_ref, note, b"A note.");
        long["byte_len"] = json!(MAX_READ_BYTES);
        long["content_base64"] = json!("QUFB".repeat(MAX_READ_BYTES.div_ceil(3) as usize + 1));
        assert!(
            refused(&long).message.contains("more base64 than"),
            "{}",
            refused(&long).message
        );
        // A refusal whose code is no code, or that holds a key no refusal holds.
        for code in ["Bad-Code", "", "a code"] {
            assert_eq!(
                refused(&refusal_answer(code, "Nothing")).code,
                "bad_answer",
                "{code:?}"
            );
        }
        let mut smuggled = refusal_answer("not_here", "Nothing stands there");
        smuggled["content_base64"] = json!("QQ==");
        assert_eq!(refused(&smuggled).code, "bad_answer");
        assert_eq!(read_state(&two, &offer_ref, note), "wanted");
        // What the grant only touches, the task reads too.
        assert_eq!(
            want(&two, &offer_ref, "project:river/maps/delta", NOW + HOUR).unwrap()["read"]
                ["state"],
            "wanted"
        );
    }

    #[test]
    fn a_grant_spent_or_unreadable_bars_reading_and_offers_keep_their_own_reads() {
        let brief = "project:river/docs/brief.md";
        // After a return, nothing more is wanted; a return refused at the lapse drops what was read.
        let two = two();
        let returned = taken_offer(&two, NOW + 24 * HOUR);
        two.taker
            .give_back(
                &json!({"offer_ref": returned, "made": made(json!(["project:river/maps"]))}),
                NOW + HOUR,
            )
            .unwrap();
        assert_eq!(
            want(&two, &returned, brief, NOW + HOUR).unwrap_err().code,
            "spent"
        );
        let two = self::two();
        let lapsing = taken_offer(&two, NOW + 2 * HOUR);
        placed(&two, &lapsing, brief, b"The brief.\n");
        let late = two.taker.give_back(
            &json!({"offer_ref": lapsing, "made": made(json!(["project:river/maps"]))}),
            NOW + 3 * HOUR,
        );
        assert_eq!(late.unwrap_err().code, "lapsed");
        assert!(!two.taker.reads_folder(&lapsing).exists());
        assert_eq!(read_state(&two, &lapsing, brief), "dropped");

        // A record that names no time its grant lapses reads as lapsed.
        let two = self::two();
        let offer_ref = taken_offer(&two, NOW + 24 * HOUR);
        let kept = two.taker.path(Side::Taken, &offer_ref);
        let mut record: Value = serde_json::from_slice(&std::fs::read(&kept).unwrap()).unwrap();
        record["offer"]["grant"]
            .as_object_mut()
            .unwrap()
            .remove("lapses_at");
        std::fs::write(&kept, encode(&record)).unwrap();
        assert_eq!(
            want(&two, &offer_ref, brief, NOW + HOUR).unwrap_err().code,
            "lapsed"
        );

        // Two offers taken in one Factory: each keeps its own reads.
        let two = self::two();
        let first = taken_offer(&two, NOW + 24 * HOUR);
        let second = taken_offer(&two, NOW + 24 * HOUR);
        placed(&two, &first, brief, b"The first brief.\n");
        placed(&two, &second, brief, b"The second brief.\n");
        assert_ne!(
            two.taker.reads_folder(&first),
            two.taker.reads_folder(&second)
        );
        two.taker
            .revoked(
                &json!({"offer_ref": first, "revoked_at": NOW + HOUR}),
                NOW + HOUR,
            )
            .unwrap();
        assert_eq!(placed_bytes(&two, &first, brief), None);
        assert_eq!(
            placed_bytes(&two, &second, brief).as_deref(),
            Some(&b"The second brief.\n"[..])
        );
        assert_eq!(read_state(&two, &second, brief), "placed");
    }
}
