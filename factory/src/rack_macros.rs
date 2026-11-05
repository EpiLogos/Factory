//! Rack macros: the owner of where an Agent rack's macros stand (manual
//! §19.15 Macros; O-I 842).
//!
//! A rack is a session space's: its chains are the space's sessions. Three of
//! its macros turn one setting on every agent of the rack (Model, Effort and
//! Budget). Each chain's setting is its own owner's (AIKit's model select and
//! a task's Budget), so this store never holds a chain's setting. What it
//! keeps is the rack's own: where each macro stands as the person last turned
//! or recalled it, the chains each macro leaves alone (Unmap), and the
//! variations Store keeps of where every macro stands. A recall moves every
//! macro to where the variation keeps it and answers the values, so the shell
//! turns each mapped chain through that chain's own owner. Nothing here turns
//! a chain or runs an agent: every reading says
//! `automatic_agent_or_model_invocation: false`.
//!
//! Storage, `<state>/rack-macros/<project>/<space>.json`, keyed by the project
//! and the session space; its revision is `blake3:` of its bytes, `absent`
//! before the first write. It is published whole through
//! `NativeFileTransaction`, as Factory publishes its other owner files: one
//! lock per file, a no-follow descriptor, an atomic replace. Every write is
//! the person's, on the revision the rack was read at.

use crate::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use crate::build::FACTORY_NATIVE_OWNER;
use crate::native_file_transaction::{
    lock_name, read_native, NativeFileTransaction, PublicationError,
};
use serde_json::{json, Map, Value};
use std::error::Error;
use std::fmt::{self, Display};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The capability a caller presents to write a rack's macros. The host
/// resolves the person's grant; Factory requires that the caller holds it.
pub const RACK_MACROS_CAPABILITY_REF: &str = "capability/factory/operate-rack-macros";
pub const RACK_MACROS: &str = "factory.rack-macros/v1";
pub const RACK_MACROS_READING: &str = "factory.rack-macros-reading/v1";
pub const RACK_MACROS_RECEIPT: &str = "factory.rack-macros-write/v1";
pub const RACK_MACROS_REFUSAL: &str = "factory.rack-macros-refusal/v1";
/// The verbs, as `factory development <verb>`.
pub const VERBS: [&str; 2] = ["rack-macros", "rack-macros-write"];
/// The acts of a write.
pub const ACTS: [&str; 5] = ["turn", "map", "unmap", "store", "recall"];
/// The macros that turn one setting on every agent (§19.15).
pub const MACROS: [&str; 3] = ["model", "effort", "budget"];
/// The Effort words a chain's harness answers to (§19.4: Low · Medium · High · Max).
const EFFORTS: [&str; 4] = ["low", "medium", "high", "max"];
/// A task's Budget, as AIKit bounds it (`aikit encounter-task-budget`).
const MAX_BUDGET_TURNS: u64 = 1000;
/// Live's rack bounds (the kit's RACK_VARIATIONS and chain count).
const MAX_VARIATIONS: usize = 128;
const MAX_CHAINS_APART: usize = 64;
const MAX_NAME_CHARS: usize = 80;
const MAX_MODEL_CHARS: usize = 200;
const MAX_REF_CHARS: usize = 512;
const STATE_FOLDER: &str = "rack-macros";

const USAGE_READ: &str =
    "factory development rack-macros <state> <project-ref> <space-ref> [--json]";
const USAGE_WRITE: &str = "factory development rack-macros-write <state> [request-file|-] [--json]";

pub fn help() -> String {
    format!(
        "Rack macros (where an Agent rack's macros stand; Factory keeps the rack's own and never turns a chain):\n  {USAGE_READ}\n                                    where each macro stands, the chains each leaves alone, and the rack's variations\n  {USAGE_WRITE}\n                                    turn, map, unmap, store or recall (a person only; compare-and-swap on the rack's revision)\n<state> is Factory's state directory; the developmental state file of a project names its directory."
    )
}

// ---- refusals ------------------------------------------------------------------

/// A refusal with a code a caller can act on and the words a person can read.
/// Under `--json` it is printed as a document on stdout and the process exits 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RackMacrosRefusal {
    pub code: &'static str,
    pub message: String,
}

impl RackMacrosRefusal {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }
    fn usage(message: impl Into<String>, usage: &str) -> Self {
        Self::new("usage", format!("{}; run: {usage}", message.into()))
    }

    /// The refusal as a document, with what it did to the rack.
    pub fn document(&self) -> Value {
        let effect = if self.code == "publication_uncertain" {
            "The change may have been published; read the rack back before trying again."
        } else {
            "Nothing was changed."
        };
        json!({"schema":RACK_MACROS_REFUSAL,"code":self.code,"message":self.message,"effect":effect})
    }
}

impl Display for RackMacrosRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RackMacrosRefusal {}

fn io_refusal(what: &str, error: io::Error) -> RackMacrosRefusal {
    let code = if error.kind() == io::ErrorKind::TimedOut {
        "busy"
    } else {
        "io"
    };
    RackMacrosRefusal::new(code, format!("{what}: {error}"))
}

// ---- the record ------------------------------------------------------------------

/// One rack's own: where each macro stands, what each leaves alone, and the
/// variations.
#[derive(Debug, Clone, PartialEq)]
struct Rack {
    revision: String,
    macros: Map<String, Value>,
    unmapped: Map<String, Value>,
    variations: Vec<(String, Map<String, Value>)>,
}

impl Rack {
    fn empty() -> Self {
        Self {
            revision: "absent".into(),
            macros: MACROS
                .iter()
                .map(|m| ((*m).to_owned(), Value::Null))
                .collect(),
            unmapped: MACROS
                .iter()
                .map(|m| ((*m).to_owned(), json!([])))
                .collect(),
            variations: Vec::new(),
        }
    }

    fn variations_json(&self) -> Value {
        Value::Array(
            self.variations
                .iter()
                .map(|(name, values)| json!({"name":name,"values":values}))
                .collect(),
        )
    }

    fn encode(&self, project_ref: &str, space_ref: &str) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(&json!({
            "schema":RACK_MACROS,
            "project_ref":project_ref,
            "space_ref":space_ref,
            "macros":self.macros,
            "unmapped":self.unmapped,
            "variations":self.variations_json(),
        }))
        .expect("a rack's macros encode");
        bytes.push(b'\n');
        bytes
    }
}

fn corrupt(message: impl Into<String>) -> RackMacrosRefusal {
    RackMacrosRefusal::new(
        "corrupt_state",
        format!("The rack's macros are damaged: {}", message.into()),
    )
}

fn load(
    bytes: Option<&[u8]>,
    project_ref: &str,
    space_ref: &str,
) -> Result<Rack, RackMacrosRefusal> {
    let Some(bytes) = bytes else {
        return Ok(Rack::empty());
    };
    let value: Value = serde_json::from_slice(bytes).map_err(|error| corrupt(error.to_string()))?;
    if value["schema"] != RACK_MACROS
        || value["project_ref"] != project_ref
        || value["space_ref"] != space_ref
    {
        return Err(corrupt("it names another schema, project or space"));
    }
    let macros = values_of(&value["macros"]).map_err(|refusal| corrupt(refusal.message))?;
    let mut unmapped = Map::new();
    for macro_name in MACROS {
        let chains = value["unmapped"][macro_name]
            .as_array()
            .ok_or_else(|| corrupt(format!("{macro_name} names no chains it leaves alone")))?;
        let mut kept = Vec::new();
        for chain in chains {
            let chain = chain
                .as_str()
                .ok_or_else(|| corrupt("a chain left alone is not named"))?;
            check_text(chain, "chain", MAX_REF_CHARS)
                .map_err(|refusal| corrupt(refusal.message))?;
            kept.push(json!(chain));
        }
        unmapped.insert(macro_name.to_owned(), Value::Array(kept));
    }
    let mut variations = Vec::new();
    for row in value["variations"]
        .as_array()
        .ok_or_else(|| corrupt("its variations are not a list"))?
    {
        let name = row["name"]
            .as_str()
            .ok_or_else(|| corrupt("a variation has no name"))?;
        check_text(name, "variation", MAX_NAME_CHARS)
            .map_err(|refusal| corrupt(refusal.message))?;
        let values = values_of(&row["values"]).map_err(|refusal| corrupt(refusal.message))?;
        variations.push((name.to_owned(), values));
    }
    Ok(Rack {
        revision: revision_of(bytes),
        macros,
        unmapped,
        variations,
    })
}

// ---- checks ------------------------------------------------------------------

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, RackMacrosRefusal> {
    value
        .as_object()
        .ok_or_else(|| RackMacrosRefusal::invalid(format!("{what} is an object")))
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), RackMacrosRefusal> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(RackMacrosRefusal::invalid(format!(
            "the write request has no `{key}`"
        ))),
        None => Ok(()),
    }
}

fn check_text(text: &str, field: &str, limit: usize) -> Result<(), RackMacrosRefusal> {
    let length = text.chars().count();
    if text.trim() != text || length == 0 || length > limit || text.chars().any(char::is_control) {
        return Err(RackMacrosRefusal::invalid(format!(
            "{field} is text of 1 to {limit} characters, on one line and with no padding"
        )));
    }
    Ok(())
}

fn text_of<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    limit: usize,
) -> Result<&'a str, RackMacrosRefusal> {
    let text = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| RackMacrosRefusal::invalid(format!("{key} is required")))?;
    check_text(text, key, limit)?;
    Ok(text)
}

fn macro_of(object: &Map<String, Value>) -> Result<&'static str, RackMacrosRefusal> {
    let named = object
        .get("macro")
        .and_then(Value::as_str)
        .unwrap_or_default();
    MACROS
        .into_iter()
        .find(|macro_name| *macro_name == named)
        .ok_or_else(|| {
            RackMacrosRefusal::invalid(format!(
                "macro is one of {}, not `{named}`",
                MACROS.join(", ")
            ))
        })
}

/// One macro's value: Model a model's id, Effort one of the harness's words,
/// Budget a number of turns, or null where the macro stands nowhere.
fn check_value(macro_name: &str, value: &Value) -> Result<(), RackMacrosRefusal> {
    let fits = match (macro_name, value) {
        (_, Value::Null) => true,
        ("model", Value::String(model)) => check_text(model, "Model", MAX_MODEL_CHARS).is_ok(),
        ("effort", Value::String(effort)) => EFFORTS.contains(&effort.as_str()),
        ("budget", Value::Number(turns)) => turns
            .as_u64()
            .is_some_and(|turns| (1..=MAX_BUDGET_TURNS).contains(&turns)),
        _ => false,
    };
    if fits {
        return Ok(());
    }
    Err(RackMacrosRefusal::invalid(match macro_name {
        "model" => {
            format!("Model stands on a model's id of 1 to {MAX_MODEL_CHARS} characters, or on none")
        }
        "effort" => format!("Effort stands on one of {}, or on none", EFFORTS.join(", ")),
        _ => format!("Budget stands on 1 to {MAX_BUDGET_TURNS} turns, or on none"),
    }))
}

/// Where every macro stands: each of the three, and nothing else.
fn values_of(value: &Value) -> Result<Map<String, Value>, RackMacrosRefusal> {
    let object = value.as_object().ok_or_else(|| {
        RackMacrosRefusal::invalid("values name where every macro stands: model, effort and budget")
    })?;
    if let Some(key) = object.keys().find(|key| !MACROS.contains(&key.as_str())) {
        return Err(RackMacrosRefusal::invalid(format!(
            "`{key}` is no macro of the rack"
        )));
    }
    let mut values = Map::new();
    for macro_name in MACROS {
        let value = object.get(macro_name).ok_or_else(|| {
            RackMacrosRefusal::invalid(format!("values name where {macro_name} stands, or null"))
        })?;
        check_value(macro_name, value)?;
        values.insert(macro_name.to_owned(), value.clone());
    }
    Ok(values)
}

/// A rack is written by a person at the desktop, recognised as Factory's other
/// owner writes are: a `desktop-human` caller whose lineage ends at itself,
/// and Factory's authority with the capability granted and the act authorised.
fn require_human(object: &Map<String, Value>) -> Result<(String, String), RackMacrosRefusal> {
    let not_human = |fact: String| {
        RackMacrosRefusal::new(
            "not_human",
            format!("A rack's macros are turned by a person at the desktop: {fact}"),
        )
    };
    let caller: FactoryActionCaller = match object.get("caller") {
        Some(value) if !value.is_null() => {
            serde_json::from_value(value.clone()).map_err(|error| {
                not_human(format!(
                    "the caller is not a Factory action caller ({error})"
                ))
            })?
        }
        _ => return Err(not_human("the request names no caller".into())),
    };
    if caller.projection_kind != FactoryActionProjectionKind::DesktopHuman {
        return Err(not_human(format!(
            "the caller {} is not a desktop-human",
            caller.caller_ref
        )));
    }
    if caller.caller_ref.trim().is_empty()
        || caller.lineage.last().map(String::as_str) != Some(caller.caller_ref.as_str())
    {
        return Err(not_human(
            "the caller's lineage must end at the caller".into(),
        ));
    }
    let not_authorised = |fact: String| {
        RackMacrosRefusal::new(
            "not_authorised",
            format!("Turning a rack's macros needs Factory's authority: {fact}"),
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
    if authority.native_owner != FACTORY_NATIVE_OWNER {
        return Err(not_authorised(format!(
            "the native owner is {}, not {FACTORY_NATIVE_OWNER}",
            authority.native_owner
        )));
    }
    if authority.capability_ref.as_deref() != Some(RACK_MACROS_CAPABILITY_REF) {
        return Err(not_authorised(format!(
            "the capability is {RACK_MACROS_CAPABILITY_REF}"
        )));
    }
    if !authority.capability_granted
        || !authority.action_authorised
        || authority.authority_ref.trim().is_empty()
    {
        return Err(not_authorised(
            "the capability must be granted, the act authorised, and the authority named".into(),
        ));
    }
    Ok((caller.caller_ref, authority.authority_ref))
}

// ---- storage ------------------------------------------------------------------

fn revision_of(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

/// A path component for a project or a space: a readable slug and a digest.
fn key_of(reference: &str) -> String {
    let slug: String = reference
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(32)
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "rack" } else { slug };
    let hash = blake3::hash(reference.as_bytes()).to_hex();
    format!("{slug}-{}", &hash.as_str()[..16])
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, RackMacrosRefusal> {
    match read_native(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_refusal(
            &format!("could not read {}", path.display()),
            error,
        )),
    }
}

/// Factory's rack macros, in `<state>/rack-macros/`. A developmental state
/// file as `<state>` names the directory it lives in.
pub struct RackMacrosStore {
    root: PathBuf,
}

impl RackMacrosStore {
    pub fn at(state: impl AsRef<Path>) -> Self {
        let path = state.as_ref();
        let root = if path.is_file() {
            match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
                _ => PathBuf::from("."),
            }
        } else {
            path.to_path_buf()
        };
        Self { root }
    }

    fn path(&self, project_ref: &str, space_ref: &str) -> PathBuf {
        self.root
            .join(STATE_FOLDER)
            .join(key_of(project_ref))
            .join(format!("{}.json", key_of(space_ref)))
    }

    /// `factory development rack-macros`: the rack's own, read.
    pub fn reading(&self, project_ref: &str, space_ref: &str) -> Result<Value, RackMacrosRefusal> {
        check_text(project_ref, "project_ref", MAX_REF_CHARS)?;
        check_text(space_ref, "space_ref", MAX_REF_CHARS)?;
        let bytes = read_optional(&self.path(project_ref, space_ref))?;
        let rack = load(bytes.as_deref(), project_ref, space_ref)?;
        Ok(json!({
            "schema":RACK_MACROS_READING,
            "project_ref":project_ref,
            "space_ref":space_ref,
            "revision":rack.revision,
            "macros":rack.macros,
            "unmapped":rack.unmapped,
            "variations":rack.variations_json(),
            "write_admission":{
                "caller":{"projectionKind":"desktop-human","lineage":"ends at callerRef"},
                "authority":{"nativeOwner":FACTORY_NATIVE_OWNER,"capabilityRef":RACK_MACROS_CAPABILITY_REF,
                    "capabilityGranted":true,"actionAuthorised":true},
            },
            "automatic_agent_or_model_invocation":false,
        }))
    }

    /// `factory development rack-macros-write`: one act, by a person, on the
    /// revision the rack was read at.
    ///
    /// - `turn {macro, value}`: where the macro stands now.
    /// - `unmap {macro, chain}` / `map {macro, chain}`: the macro leaves the
    ///   chain alone from now on, or turns it again.
    /// - `store {variation, values}`: a variation keeps where every macro
    ///   stands, as the rack reads it; a name already kept is replaced.
    /// - `recall {variation}`: every macro moves to where the variation keeps
    ///   it, and the receipt answers the values for the shell to turn.
    pub fn write(&self, request: &Value) -> Result<Value, RackMacrosRefusal> {
        let object = object(request, "the write request")?;
        only_keys(
            object,
            &[
                "project_ref",
                "space_ref",
                "expected_revision",
                "act",
                "macro",
                "value",
                "chain",
                "variation",
                "values",
                "caller",
                "authority",
            ],
        )?;
        let (written_by, authority_ref) = require_human(object)?;
        let project_ref = text_of(object, "project_ref", MAX_REF_CHARS)?;
        let space_ref = text_of(object, "space_ref", MAX_REF_CHARS)?;
        let expected = object
            .get("expected_revision")
            .and_then(Value::as_str)
            .filter(|revision| !revision.is_empty())
            .ok_or_else(|| {
                RackMacrosRefusal::invalid(
                    "expected_revision is the revision the rack was read at, or `absent`",
                )
            })?;
        let act = object
            .get("act")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let fields: &[&str] = match act {
            "turn" => &["macro", "value"],
            "map" | "unmap" => &["macro", "chain"],
            "store" => &["variation", "values"],
            "recall" => &["variation"],
            _ => {
                return Err(RackMacrosRefusal::invalid(format!(
                    "act is one of {}, not `{act}`",
                    ACTS.join(", ")
                )))
            }
        };
        if let Some(key) = ["macro", "value", "chain", "variation", "values"]
            .into_iter()
            .find(|key| object.contains_key(*key) && !fields.contains(key))
        {
            return Err(RackMacrosRefusal::invalid(format!(
                "{act} takes no `{key}`"
            )));
        }

        let path = self.path(project_ref, space_ref);
        let name = lock_name(&path).map_err(|error| {
            io_refusal(
                &format!("could not name the lock of {}", path.display()),
                error,
            )
        })?;
        let lock = NativeFileTransaction::acquire(&path, &name, true)
            .map_err(|error| io_refusal(&format!("could not open {}", path.display()), error))?;
        let rack = load(lock.current(), project_ref, space_ref)?;
        if rack.revision != expected {
            return Err(RackMacrosRefusal::new(
                "stale_revision",
                format!(
                    "The rack changed: it is at {}, not {expected}; read it again",
                    rack.revision
                ),
            ));
        }
        let mut next = rack.clone();
        let mut recalled = Value::Null;
        match act {
            "turn" => {
                let macro_name = macro_of(object)?;
                let value = object
                    .get("value")
                    .ok_or_else(|| RackMacrosRefusal::invalid("turn names the value, or null"))?;
                check_value(macro_name, value)?;
                next.macros.insert(macro_name.to_owned(), value.clone());
            }
            "map" | "unmap" => {
                let macro_name = macro_of(object)?;
                let chain = text_of(object, "chain", MAX_REF_CHARS)?;
                let apart = next
                    .unmapped
                    .get_mut(macro_name)
                    .and_then(Value::as_array_mut)
                    .expect("every macro keeps a list");
                let held = apart.iter().any(|kept| kept == chain);
                if act == "unmap" && !held {
                    if apart.len() >= MAX_CHAINS_APART {
                        return Err(RackMacrosRefusal::new(
                            "full",
                            format!("{macro_name} already leaves {MAX_CHAINS_APART} chains alone"),
                        ));
                    }
                    apart.push(json!(chain));
                } else if act == "map" {
                    apart.retain(|kept| kept != chain);
                }
            }
            "store" => {
                let variation = text_of(object, "variation", MAX_NAME_CHARS)?;
                let values = values_of(object.get("values").unwrap_or(&Value::Null))?;
                let kept = next.variations.len();
                match next
                    .variations
                    .iter_mut()
                    .find(|(name, _)| name == variation)
                {
                    Some((_, held)) => *held = values,
                    None if kept >= MAX_VARIATIONS => {
                        return Err(RackMacrosRefusal::new(
                            "full",
                            format!("The rack already keeps {MAX_VARIATIONS} variations"),
                        ))
                    }
                    None => next.variations.push((variation.to_owned(), values)),
                }
            }
            _ => {
                let variation = text_of(object, "variation", MAX_NAME_CHARS)?;
                let (_, values) = next
                    .variations
                    .iter()
                    .find(|(name, _)| name == variation)
                    .cloned()
                    .ok_or_else(|| {
                        RackMacrosRefusal::new(
                            "no_such_variation",
                            format!("The rack keeps no variation named {variation}"),
                        )
                    })?;
                recalled = Value::Object(values.clone());
                next.macros = values;
            }
        }
        let bytes = next.encode(project_ref, space_ref);
        let changed = lock.current() != Some(bytes.as_slice());
        let revision = if changed {
            match lock.publish(&bytes, lock.current().is_none()) {
                Ok(()) => revision_of(&bytes),
                Err(PublicationError::Before(error)) => {
                    return Err(io_refusal("the change was not published", error))
                }
                Err(PublicationError::Uncertain(uncertain)) => {
                    return Err(RackMacrosRefusal::new(
                        "publication_uncertain",
                        format!("publication is uncertain after the replacement; read the rack back before trying again: {uncertain}"),
                    ))
                }
            }
        } else {
            rack.revision.clone()
        };
        Ok(json!({
            "schema":RACK_MACROS_RECEIPT,
            "project_ref":project_ref,
            "space_ref":space_ref,
            "act":act,
            "previous_revision":rack.revision,
            "revision":revision,
            "changed":changed,
            "written_by":written_by,
            "authority_ref":authority_ref,
            "macros":next.macros,
            "unmapped":next.unmapped,
            "variations":next.variations_json(),
            "recalled":recalled,
            "automatic_agent_or_model_invocation":false,
        }))
    }
}

// ---- the command ------------------------------------------------------------------

pub fn is_rack_macros_command(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("development")
        && args
            .get(1)
            .is_some_and(|verb| VERBS.contains(&verb.as_str()))
}

/// Process entry: `args` start at `development`. A refusal is a document on
/// stdout under `--json` and the process exits 2, as the other owner verbs do.
pub fn main(args: &[String]) -> ExitCode {
    let json = args.iter().any(|argument| argument == "--json");
    let stripped = args
        .iter()
        .skip(1)
        .filter(|argument| argument.as_str() != "--json")
        .cloned()
        .collect::<Vec<_>>();
    match execute(&stripped, json, None) {
        Ok(output) => {
            println!("{output}");
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

/// `args` start at the verb, with `--json` already removed.
pub fn execute(
    args: &[String],
    json: bool,
    stdin: Option<&str>,
) -> Result<String, RackMacrosRefusal> {
    let verb = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| RackMacrosRefusal::usage("missing rack macros operation", USAGE_READ))?;
    let rest = &args[1..];
    if let Some(option) = rest.iter().find(|argument| argument.starts_with("--")) {
        return Err(RackMacrosRefusal::usage(
            format!("unknown option {option}"),
            if verb == "rack-macros" {
                USAGE_READ
            } else {
                USAGE_WRITE
            },
        ));
    }
    let document = match verb {
        "rack-macros" => {
            let [state, project, space] = rest else {
                return Err(RackMacrosRefusal::usage(
                    "expected <state> <project-ref> <space-ref>",
                    USAGE_READ,
                ));
            };
            RackMacrosStore::at(state).reading(project, space)?
        }
        "rack-macros-write" => {
            let (state, request_path) = match rest {
                [state] => (state, "-"),
                [state, request] => (state, request.as_str()),
                _ => {
                    return Err(RackMacrosRefusal::usage(
                        "expected <state> and at most one request",
                        USAGE_WRITE,
                    ))
                }
            };
            let text = if request_path == "-" {
                match stdin {
                    Some(text) => text.to_owned(),
                    None => {
                        let mut text = String::new();
                        io::stdin().read_to_string(&mut text).map_err(|error| {
                            io_refusal("could not read the request from stdin", error)
                        })?;
                        text
                    }
                }
            } else {
                std::fs::read_to_string(request_path).map_err(|error| {
                    io_refusal(
                        &format!("could not read the request file {request_path}"),
                        error,
                    )
                })?
            };
            let request: Value = serde_json::from_str(&text).map_err(|error| {
                RackMacrosRefusal::invalid(format!("the request is not JSON: {error}"))
            })?;
            RackMacrosStore::at(state).write(&request)?
        }
        other => {
            return Err(RackMacrosRefusal::usage(
                format!("unknown rack macros operation `{other}`"),
                USAGE_READ,
            ))
        }
    };
    if json {
        serde_json::to_string_pretty(&document).map_err(|error| {
            RackMacrosRefusal::new("io", format!("could not encode the answer: {error}"))
        })
    } else {
        Ok(text(&document))
    }
}

fn text(document: &Value) -> String {
    let stands = |macros: &Value| {
        MACROS
            .iter()
            .map(|macro_name| match &macros[*macro_name] {
                Value::Null => format!("{macro_name} stands nowhere"),
                value => format!("{macro_name} {}", value.to_string().trim_matches('"')),
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let variations = document["variations"].as_array().map_or(0, Vec::len);
    match document["act"].as_str() {
        None => format!(
            "The rack of {}: {}; {variations} variation(s).",
            document["space_ref"].as_str().unwrap_or_default(),
            stands(&document["macros"])
        ),
        Some(act) if document["changed"] == true => format!(
            "{act}: the rack now stands at {}.",
            stands(&document["macros"])
        ),
        Some(act) => format!("{act}: the rack already stood so; nothing changed."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "project:demo";
    const SPACE: &str = "session-space/demo-rack";

    fn person() -> Value {
        json!({
            "caller": {"callerRef": "oi:desk-rack-macros", "projectionKind": "desktop-human",
                       "lineage": ["oi:desk-rack-macros"]},
            "authority": {"authorityRef": "central:native-action-authority#person",
                          "nativeOwner": FACTORY_NATIVE_OWNER,
                          "capabilityRef": RACK_MACROS_CAPABILITY_REF,
                          "capabilityGranted": true, "actionAuthorised": true},
        })
    }

    fn act(revision: &str, fields: Value) -> Value {
        let mut request = person();
        request["project_ref"] = json!(PROJECT);
        request["space_ref"] = json!(SPACE);
        request["expected_revision"] = json!(revision);
        for (key, value) in fields.as_object().unwrap() {
            request[key] = value.clone();
        }
        request
    }

    #[test]
    fn a_person_turns_unmaps_stores_and_recalls_a_rack_s_macros_on_the_revision_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = RackMacrosStore::at(dir.path());
        let first = store.reading(PROJECT, SPACE).unwrap();
        assert_eq!(first["schema"], RACK_MACROS_READING);
        assert_eq!(first["revision"], "absent");
        assert_eq!(
            first["macros"],
            json!({"model":null,"effort":null,"budget":null})
        );
        assert_eq!(first["automatic_agent_or_model_invocation"], false);

        let turned = store
            .write(&act(
                "absent",
                json!({"act":"turn","macro":"budget","value":40}),
            ))
            .unwrap();
        assert_eq!(turned["schema"], RACK_MACROS_RECEIPT);
        assert_eq!(turned["changed"], true);
        assert_eq!(turned["macros"]["budget"], 40);
        assert_eq!(turned["written_by"], "oi:desk-rack-macros");
        let at = turned["revision"].as_str().unwrap().to_owned();
        assert!(at.starts_with("blake3:"));

        // The rack read before the turn is stale now: nothing is written.
        let stale = store
            .write(&act(
                "absent",
                json!({"act":"turn","macro":"budget","value":10}),
            ))
            .unwrap_err();
        assert_eq!(stale.code, "stale_revision");
        assert_eq!(
            store.reading(PROJECT, SPACE).unwrap()["macros"]["budget"],
            40
        );

        let unmapped = store
            .write(&act(
                &at,
                json!({"act":"unmap","macro":"model","chain":"agent-session/b"}),
            ))
            .unwrap();
        assert_eq!(unmapped["unmapped"]["model"], json!(["agent-session/b"]));
        let at = unmapped["revision"].as_str().unwrap().to_owned();
        let stored = store
            .write(&act(
                &at,
                json!({"act":"store","variation":"Night",
                       "values":{"model":"claude-opus","effort":"high","budget":40}}),
            ))
            .unwrap();
        assert_eq!(stored["variations"][0]["name"], "Night");
        let at = stored["revision"].as_str().unwrap().to_owned();
        let turned = store
            .write(&act(
                &at,
                json!({"act":"turn","macro":"effort","value":"low"}),
            ))
            .unwrap();
        let at = turned["revision"].as_str().unwrap().to_owned();
        let recalled = store
            .write(&act(&at, json!({"act":"recall","variation":"Night"})))
            .unwrap();
        assert_eq!(
            recalled["recalled"],
            json!({"model":"claude-opus","effort":"high","budget":40})
        );
        assert_eq!(recalled["macros"], recalled["recalled"]);
        let at = recalled["revision"].as_str().unwrap().to_owned();
        let mapped = store
            .write(&act(
                &at,
                json!({"act":"map","macro":"model","chain":"agent-session/b"}),
            ))
            .unwrap();
        assert_eq!(mapped["unmapped"]["model"], json!([]));
        // The same act again changes nothing and keeps the revision.
        let at = mapped["revision"].as_str().unwrap().to_owned();
        let again = store
            .write(&act(
                &at,
                json!({"act":"map","macro":"model","chain":"agent-session/b"}),
            ))
            .unwrap();
        assert_eq!(again["changed"], false);
        assert_eq!(again["revision"], at);

        // Another space's rack is its own.
        let other = store.reading(PROJECT, "session-space/other").unwrap();
        assert_eq!(other["revision"], "absent");
        let reading = store.reading(PROJECT, SPACE).unwrap();
        assert_eq!(reading["revision"], at);
        assert_eq!(reading["variations"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_rack_s_macros_are_refused_in_words_and_nothing_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let store = RackMacrosStore::at(dir.path());
        let mut machine = act("absent", json!({"act":"turn","macro":"budget","value":4}));
        machine["caller"]["projectionKind"] = json!("situated-agent");
        let mut ungranted = act("absent", json!({"act":"turn","macro":"budget","value":4}));
        ungranted["authority"]["capabilityGranted"] = json!(false);
        let mut nobody = act("absent", json!({"act":"turn","macro":"budget","value":4}));
        nobody.as_object_mut().unwrap().remove("caller");
        for (request, code, words) in [
            (nobody, "not_human", "the request names no caller"),
            (machine, "not_human", "is not a desktop-human"),
            (
                ungranted,
                "not_authorised",
                "the capability must be granted",
            ),
            (
                act("absent", json!({"act":"turn","macro":"depth","value":4})),
                "invalid",
                "macro is one of model, effort, budget",
            ),
            (
                act("absent", json!({"act":"turn","macro":"budget","value":0})),
                "invalid",
                "Budget stands on 1 to 1000 turns",
            ),
            (
                act(
                    "absent",
                    json!({"act":"turn","macro":"effort","value":"extreme"}),
                ),
                "invalid",
                "Effort stands on one of low",
            ),
            (
                act("absent", json!({"act":"recall","variation":"Night"})),
                "no_such_variation",
                "keeps no variation named Night",
            ),
            (
                act(
                    "absent",
                    json!({"act":"store","variation":"Night","values":{"model":null}}),
                ),
                "invalid",
                "where effort stands",
            ),
            (
                act(
                    "absent",
                    json!({"act":"recall","variation":"Night","value":3}),
                ),
                "invalid",
                "recall takes no `value`",
            ),
            (
                act("absent", json!({"act":"swap"})),
                "invalid",
                "act is one of turn, map, unmap, store, recall",
            ),
        ] {
            let refusal = store.write(&request).unwrap_err();
            assert_eq!(refusal.code, code, "{refusal}");
            assert!(refusal.message.contains(words), "{refusal}");
            assert_eq!(refusal.document()["effect"], "Nothing was changed.");
        }
        assert!(!dir
            .path()
            .join(STATE_FOLDER)
            .join(key_of(PROJECT))
            .join(format!("{}.json", key_of(SPACE)))
            .exists());

        // A damaged rack is named damaged, never read past.
        let path = store.path(PROJECT, SPACE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"schema":"factory.rack-macros/v1"}"#).unwrap();
        assert_eq!(
            store.reading(PROJECT, SPACE).unwrap_err().code,
            "corrupt_state"
        );
    }

    #[test]
    fn the_command_reads_and_writes_as_documents() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().display().to_string();
        let read = |args: &[&str]| {
            execute(
                &args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(),
                true,
                None,
            )
        };
        let reading: Value =
            serde_json::from_str(&read(&["rack-macros", &state, PROJECT, SPACE]).unwrap()).unwrap();
        assert_eq!(reading["schema"], RACK_MACROS_READING);
        let request = act(
            "absent",
            json!({"act":"turn","macro":"model","value":"claude-opus"}),
        );
        let receipt: Value = serde_json::from_str(
            &execute(
                &[
                    "rack-macros-write".to_owned(),
                    state.clone(),
                    "-".to_owned(),
                ],
                true,
                Some(&request.to_string()),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["macros"]["model"], "claude-opus");
        assert_eq!(
            read(&["rack-macros", &state, PROJECT]).unwrap_err().code,
            "usage"
        );
        assert!(is_rack_macros_command(&[
            "development".into(),
            "rack-macros-write".into()
        ]));
        assert!(!is_rack_macros_command(&[
            "development".into(),
            "pads".into()
        ]));
    }
}
