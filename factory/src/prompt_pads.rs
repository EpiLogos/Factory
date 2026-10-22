//! Prompt pads: the owner of a project's bank of sixteen pads (manual 19.22).
//!
//! Factory keeps what a pad is (its prompt, session, agent, scope, seat, Runs
//! at and choke group), the record of every run, and which scheduled runs are
//! due. Factory never runs a pad: the launch itself goes through the gateway as
//! any launch does, and the runner that presses a due pad calls these verbs.
//! Every reading here says `automatic_agent_or_model_invocation: false`, and
//! that stays true.
//!
//! The record keeps the shape of the Day's moves (Central
//! `continuous_work/day_moves.rs`): an append-only JSONL of `begin` and `end`
//! events, one run per begin, a `running` run and a `last` run per pad, and a
//! due list whose rows carry an `occurrence_ref` and whether it has begun.
//! "Nothing runs where you cannot see it": a scheduled begin is only ever an
//! occurrence of a schedule that is saved on the pad, and a run that stalls or
//! is refused says why.
//!
//! Storage, under `<state>/prompt-pads/<project>/`: `bank.json` (the sixteen
//! slots; its revision is `blake3:` of its bytes, `absent` before the first
//! write) and `runs.jsonl`. Both are published whole through
//! `NativeFileTransaction`, the way Factory publishes its other owner files:
//! one lock per file, a no-follow descriptor, an atomic replace. The record is
//! appended by publishing the old bytes plus one line, so an earlier line is
//! never rewritten. Lock order is always bank, then record.
//!
//! Schedules are Central's. Factory has no calendar of its own: it asks
//! `central.time.occurrences` (through [`OccurrenceResolver`]) and passes
//! Central's words on when Central refuses.

use crate::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use crate::build::FACTORY_NATIVE_OWNER;
use crate::native_file_transaction::{
    lock_name, read_native, NativeFileTransaction, PublicationError,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

/// The capability a caller presents to write a pad. The shell resolves the
/// person's grant; Factory only requires that the caller says it holds it.
pub const PADS_CAPABILITY_REF: &str = "capability/factory/operate-prompt-pads";
pub const PADS_BANK: &str = "factory.prompt-pads-bank/v1";
pub const PADS_READING: &str = "factory.prompt-pads-reading/v1";
pub const PADS_WRITE_RECEIPT: &str = "factory.prompt-pads-write/v1";
pub const PADS_RUN_BEGIN: &str = "factory.prompt-pads-run/v1";
pub const PADS_RUN_RECORD: &str = "factory.prompt-pads-record/v1";
pub const PADS_DUE: &str = "factory.prompt-pads-due/v1";
pub const PADS_REFUSAL: &str = "factory.prompt-pads-refusal/v1";
/// The five verbs, as `factory development <verb>`.
pub const VERBS: [&str; 5] = ["pads", "pads-write", "pads-run", "pads-record", "pads-due"];

const EVENT_SCHEMA: &str = "factory.prompt-pad-event/v1";
const PAD_COUNT: usize = 16;
const RUN_REF_PREFIX: &str = "factory:pad-run:";
const STATE_FOLDER: &str = "prompt-pads";
const BANK_FILE: &str = "bank.json";
const RECORD_FILE: &str = "runs.jsonl";
const HOUR_MS: i64 = 3_600_000;
/// How far ahead a pad's next run is looked for: the longest gap a schedule can
/// have (every 168 hours, or one named day a week) and a day's margin.
const HORIZON_MS: i64 = 8 * 24 * HOUR_MS;
const MAX_DUE_WINDOW_MS: i64 = 400 * 24 * HOUR_MS;
const MAX_NAME_CHARS: usize = 80;
const MAX_PROMPT_CHARS: usize = 20_000;
const MAX_REF_CHARS: usize = 512;
const MAX_PROJECT_CHARS: usize = 200;
const MAX_REASON_CHARS: usize = 4096;
const MAX_EVERY_HOURS: u64 = 168;
const NO_SCHEDULE_SAVED: &str =
    "No pad's Runs at is a schedule; nothing runs by itself until you save one.";

const USAGE_PADS: &str = "factory development pads <state> <project-ref> [--json]";
const USAGE_WRITE: &str = "factory development pads-write <state> [request-file|-] [--json]";
const USAGE_RUN: &str = "factory development pads-run <state> [request-file|-] [--json]";
const USAGE_RECORD: &str = "factory development pads-record <state> [request-file|-] [--json]";
const USAGE_DUE: &str =
    "factory development pads-due <state> <project-ref> --now <unix_ms> --since <unix_ms> [--json]";

pub fn help() -> String {
    format!(
        "Prompt pads (a project's bank of sixteen pads; Factory keeps the bank and the record of every run, and never runs a pad):\n  {USAGE_PADS}\n                                    the bank: sixteen slots, each pad's settings, next run, running run and last run\n  {USAGE_WRITE}\n                                    write one slot, or empty it (a person only; compare-and-swap on the bank revision)\n  {USAGE_RUN}\n                                    record a run's begin; answers the pad as it stood\n  {USAGE_RECORD}\n                                    record a run's end: done, stalled, stopped or refused\n  {USAGE_DUE}\n                                    the scheduled occurrences in (since, now] and whether each has begun\n<state> is Factory's state directory for pads; the developmental state file of a project names its directory."
    )
}

// ---- refusals ------------------------------------------------------------------

/// A refusal with a code a caller can act on and the words a person can read.
/// Under `--json` it is printed as a document on stdout and the process exits 2,
/// as the other `factory development` owner verbs do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadsRefusal {
    pub code: &'static str,
    pub message: String,
}

impl PadsRefusal {
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
    fn corrupt(message: impl Into<String>) -> Self {
        Self::new("corrupt_state", message)
    }
    fn empty_pad(index: usize) -> Self {
        Self::new(
            "empty_pad",
            format!("Pad {index} is empty: there is nothing to run"),
        )
    }

    /// The refusal as a document, with what it did to the state.
    pub fn document(&self, verb: &str) -> Value {
        let effect = if self.code == "publication_uncertain" {
            "The change may have been published; read the pads back before trying again."
        } else {
            match verb {
                "pads-write" => "Nothing was written.",
                "pads-run" | "pads-record" => "Nothing was recorded.",
                _ => "Nothing was changed.",
            }
        };
        json!({"schema":PADS_REFUSAL,"code":self.code,"message":self.message,"effect":effect})
    }
}

impl Display for PadsRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PadsRefusal {}

fn io_refusal(what: &str, error: io::Error) -> PadsRefusal {
    let code = if error.kind() == io::ErrorKind::TimedOut {
        "busy"
    } else {
        "io"
    };
    PadsRefusal::new(code, format!("{what}: {error}"))
}

// ---- Central's schedules --------------------------------------------------------

/// Why Central did not give occurrences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    /// Central refused the schedule or the window, in these words.
    Refused(String),
    /// Central could not be asked: nothing was decided.
    Unavailable(String),
}

impl ScheduleError {
    fn words(&self) -> String {
        match self {
            ScheduleError::Refused(words) => words.clone(),
            ScheduleError::Unavailable(why) => format!("Central could not be asked: {why}"),
        }
    }
}

fn schedule_refusal(error: ScheduleError) -> PadsRefusal {
    match error {
        ScheduleError::Refused(words) => PadsRefusal::new("schedule_refused", words),
        ScheduleError::Unavailable(why) => PadsRefusal::new(
            "central_unavailable",
            format!("Central could not be asked about the schedule: {why}"),
        ),
    }
}

/// Central's civil-time grammar, behind a trait so tests ask no real `ctrl`.
pub trait OccurrenceResolver {
    /// The instants (unix ms) at which `schedule` occurs in `[from, to]`.
    /// `schedule` is `central.time.occurrences`' own: `{"kind":"daily","time"}`,
    /// `{"kind":"cron","expression"}` or `{"kind":"every","interval_ms"}`.
    fn occurrences(
        &self,
        schedule: &Value,
        from_unix_ms: i64,
        to_unix_ms: i64,
    ) -> Result<Vec<i64>, ScheduleError>;
}

/// The real Central: `ctrl --json action run central.time.occurrences`, found
/// on the PATH as Factory finds `ctrl` for its other Central reads.
pub struct CtrlResolver;

impl OccurrenceResolver for CtrlResolver {
    fn occurrences(
        &self,
        schedule: &Value,
        from_unix_ms: i64,
        to_unix_ms: i64,
    ) -> Result<Vec<i64>, ScheduleError> {
        let input = json!({"schedule":schedule,"window_from_unix_ms":from_unix_ms,"window_to_unix_ms":to_unix_ms});
        let mut command = Command::new("ctrl");
        command
            .args(["--json", "action", "run", "central.time.occurrences"])
            .arg(input.to_string());
        let output = crate::native_process::output(&mut command, Duration::from_secs(30)).map_err(
            |error| ScheduleError::Unavailable(format!("`ctrl` could not be run: {error}")),
        )?;
        // Central answers a refusal with a JSON envelope and a non-zero exit, so
        // the envelope is read whatever the status.
        let envelope: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
            ScheduleError::Unavailable(format!(
                "`ctrl` answered no JSON envelope (exit {})",
                output.status
            ))
        })?;
        match envelope["ok"].as_bool() {
            Some(true) => {
                let rows = envelope["data"]["occurrences"].as_array().ok_or_else(|| {
                    ScheduleError::Unavailable("`ctrl` answered no occurrences".into())
                })?;
                Ok(rows
                    .iter()
                    .filter_map(|row| row["due_unix_ms"].as_i64())
                    .collect())
            }
            Some(false) => Err(ScheduleError::Refused(
                envelope["error"]["message"]
                    .as_str()
                    .unwrap_or("Central refused the schedule")
                    .to_owned(),
            )),
            None => Err(ScheduleError::Unavailable(
                "`ctrl` answered an envelope that says neither ok nor not ok".into(),
            )),
        }
    }
}

// ---- a pad -----------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Session {
    New,
    Existing(String),
}

impl Session {
    fn to_json(&self) -> Value {
        match self {
            Session::New => json!({"kind":"new"}),
            Session::Existing(reference) => json!({"kind":"session","session_ref":reference}),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    fn name(self) -> &'static str {
        match self {
            Weekday::Mon => "mon",
            Weekday::Tue => "tue",
            Weekday::Wed => "wed",
            Weekday::Thu => "thu",
            Weekday::Fri => "fri",
            Weekday::Sat => "sat",
            Weekday::Sun => "sun",
        }
    }
    fn parse(name: &str) -> Option<Self> {
        [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ]
        .into_iter()
        .find(|day| day.name() == name)
    }
    /// Central's cron day-of-week: Sunday is 0.
    fn cron(self) -> u8 {
        match self {
            Weekday::Sun => 0,
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum RunsAt {
    Off,
    Daily {
        time: String,
        days: Option<Vec<Weekday>>,
    },
    Every {
        hours: u64,
    },
}

impl RunsAt {
    fn parse(value: Option<&Value>) -> Result<Self, PadsRefusal> {
        let Some(value) = value.filter(|value| !value.is_null()) else {
            return Ok(RunsAt::Off);
        };
        let object = value.as_object().ok_or_else(|| {
            PadsRefusal::invalid(
                "runs_at is {kind: off} | {kind: daily, time, days?} | {kind: every, hours}",
            )
        })?;
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| PadsRefusal::invalid("runs_at needs a kind: off, daily or every"))?;
        match kind {
            "off" => {
                only_keys(object, &["kind"], "runs_at off")?;
                Ok(RunsAt::Off)
            }
            "daily" => {
                only_keys(object, &["kind", "time", "days"], "runs_at daily")?;
                let time = object
                    .get("time")
                    .and_then(Value::as_str)
                    .and_then(canonical_time)
                    .ok_or_else(|| PadsRefusal::invalid("a daily runs_at needs a time as HH:MM"))?;
                Ok(RunsAt::Daily {
                    time,
                    days: parse_days(object.get("days"))?,
                })
            }
            "every" => {
                only_keys(object, &["kind", "hours"], "runs_at every")?;
                match object.get("hours").and_then(Value::as_u64) {
                    Some(hours @ 1..=MAX_EVERY_HOURS) => Ok(RunsAt::Every { hours }),
                    _ => Err(PadsRefusal::invalid(format!(
                        "an every runs_at needs hours, a whole number from 1 to {MAX_EVERY_HOURS}"
                    ))),
                }
            }
            other => Err(PadsRefusal::invalid(format!(
                "runs_at kind is off, daily or every, not `{other}`"
            ))),
        }
    }

    fn to_json(&self) -> Value {
        match self {
            RunsAt::Off => json!({"kind":"off"}),
            RunsAt::Daily { time, days: None } => json!({"kind":"daily","time":time}),
            RunsAt::Daily {
                time,
                days: Some(days),
            } => json!({"kind":"daily","time":time,
                "days":days.iter().map(|day| day.name()).collect::<Vec<_>>()}),
            RunsAt::Every { hours } => json!({"kind":"every","hours":hours}),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            RunsAt::Off => "off",
            RunsAt::Daily { .. } => "daily",
            RunsAt::Every { .. } => "every",
        }
    }

    /// The schedule in `central.time.occurrences`' grammar; none when Off.
    /// Days of the week are a cron expression, as Central's grammar has no
    /// other way to name them; naming every day is a plain daily.
    fn schedule(&self) -> Option<Value> {
        match self {
            RunsAt::Off => None,
            RunsAt::Daily { time, days } => match days {
                Some(days) if days.len() < 7 => {
                    let (hour, minute) = time.split_once(':').unwrap_or(("0", "0"));
                    let mut numbers: Vec<u8> = days.iter().map(|day| day.cron()).collect();
                    numbers.sort_unstable();
                    let list = numbers
                        .iter()
                        .map(u8::to_string)
                        .collect::<Vec<_>>()
                        .join(",");
                    let hour: u32 = hour.parse().unwrap_or(0);
                    let minute: u32 = minute.parse().unwrap_or(0);
                    Some(json!({"kind":"cron","expression":format!("{minute} {hour} * * {list}")}))
                }
                _ => Some(json!({"kind":"daily","time":time})),
            },
            RunsAt::Every { hours } => Some(
                json!({"kind":"every","interval_ms":i64::try_from(*hours).unwrap_or(1) * HOUR_MS}),
            ),
        }
    }
}

/// `H:MM` or `HH:MM` as two zero-padded digits. Only the shape is checked here:
/// whether it is a wall time is Central's to say, in Central's words.
fn canonical_time(raw: &str) -> Option<String> {
    let (hour, minute) = raw.split_once(':')?;
    let digits = |part: &str, shortest: usize| {
        (shortest..=2).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit())
    };
    if !digits(hour, 1) || !digits(minute, 2) {
        return None;
    }
    Some(format!("{:02}:{minute}", hour.parse::<u32>().ok()?))
}

fn parse_days(value: Option<&Value>) -> Result<Option<Vec<Weekday>>, PadsRefusal> {
    let bad = || {
        PadsRefusal::invalid(
            "days names one to seven days, each once, from mon, tue, wed, thu, fri, sat, sun",
        )
    };
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(rows)) => {
            if rows.is_empty() || rows.len() > 7 {
                return Err(bad());
            }
            let mut days = Vec::new();
            for row in rows {
                let day = row.as_str().and_then(Weekday::parse).ok_or_else(bad)?;
                if days.contains(&day) {
                    return Err(bad());
                }
                days.push(day);
            }
            days.sort();
            Ok(Some(days))
        }
        Some(_) => Err(bad()),
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Pad {
    name: String,
    prompt: String,
    session: Session,
    agent_ref: Option<String>,
    scope_ref: Option<String>,
    seat: Option<String>,
    runs_at: RunsAt,
    choke: Option<u8>,
    /// The AIKit Routine that carries this pad's schedule when it runs by itself (manual 19.22, Running alone),
    /// carried verbatim: the Routine, its proof, its authority and its state are AIKit's. Absent until a person links one.
    routine_ref: Option<String>,
}

impl Pad {
    fn parse(value: &Value) -> Result<Self, PadsRefusal> {
        let object = object(value, "a pad")?;
        only_keys(
            object,
            &[
                "name",
                "prompt",
                "session",
                "agent_ref",
                "scope_ref",
                "seat",
                "runs_at",
                "choke",
                "routine_ref",
            ],
            "a pad",
        )?;
        let prompt = match object.get("prompt") {
            Some(Value::String(prompt)) => prompt.clone(),
            _ => {
                return Err(PadsRefusal::invalid(format!(
                    "prompt is text of 1 to {MAX_PROMPT_CHARS} characters"
                )))
            }
        };
        if prompt.trim().is_empty()
            || prompt.chars().count() > MAX_PROMPT_CHARS
            || prompt.contains('\0')
        {
            return Err(PadsRefusal::invalid(format!(
                "prompt is text of 1 to {MAX_PROMPT_CHARS} characters"
            )));
        }
        let name = match object.get("name") {
            None | Some(Value::Null) => default_name(&prompt),
            Some(Value::String(name)) => {
                let name = name.trim();
                if name.is_empty()
                    || name.chars().count() > MAX_NAME_CHARS
                    || name.chars().any(char::is_control)
                {
                    return Err(PadsRefusal::invalid(format!(
                        "name is text of 1 to {MAX_NAME_CHARS} characters on one line"
                    )));
                }
                name.to_owned()
            }
            Some(_) => {
                return Err(PadsRefusal::invalid(format!(
                    "name is text of 1 to {MAX_NAME_CHARS} characters on one line"
                )))
            }
        };
        Ok(Pad {
            name,
            prompt,
            session: parse_session(object.get("session"))?,
            agent_ref: optional_ref(object, "agent_ref")?,
            scope_ref: optional_ref(object, "scope_ref")?,
            seat: parse_seat(object.get("seat"))?,
            runs_at: RunsAt::parse(object.get("runs_at"))?,
            choke: parse_choke(object.get("choke"))?,
            routine_ref: optional_ref(object, "routine_ref")?,
        })
    }

    fn to_json(&self) -> Value {
        let mut pad = json!({
            "name":self.name,
            "prompt":self.prompt,
            "session":self.session.to_json(),
            "agent_ref":self.agent_ref,
            "scope_ref":self.scope_ref,
            "seat":self.seat,
            "runs_at":self.runs_at.to_json(),
            "choke":self.choke,
        });
        // Absent until linked, so a bank written before the link existed reads back byte for byte.
        if let Some(routine_ref) = &self.routine_ref {
            pad["routine_ref"] = json!(routine_ref);
        }
        pad
    }
}

/// The pad's name when none is given: the prompt's first words.
fn default_name(prompt: &str) -> String {
    let words = prompt
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    let name: String = words
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    let name = name.trim_end().to_owned();
    if name.is_empty() {
        "Untitled pad".to_owned()
    } else {
        name
    }
}

fn parse_session(value: Option<&Value>) -> Result<Session, PadsRefusal> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(Session::New);
    };
    let object = value.as_object().ok_or_else(|| {
        PadsRefusal::invalid("session is {kind: new} or {kind: session, session_ref}")
    })?;
    match object.get("kind").and_then(Value::as_str) {
        Some("new") => {
            only_keys(object, &["kind"], "session new")?;
            Ok(Session::New)
        }
        Some("session") => {
            only_keys(object, &["kind", "session_ref"], "a session")?;
            let reference = object
                .get("session_ref")
                .ok_or_else(|| PadsRefusal::invalid("a session names its session_ref"))?;
            Ok(Session::Existing(text_ref(
                reference,
                "session_ref",
                MAX_REF_CHARS,
            )?))
        }
        _ => Err(PadsRefusal::invalid("session kind is new or session")),
    }
}

/// A seat of this machine as `<env>/<product>`, for example `env-2/o-i`; a
/// machine may lead it.
fn parse_seat(value: Option<&Value>) -> Result<Option<String>, PadsRefusal> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let bad = || {
        PadsRefusal::invalid(
            "seat is a seat of this machine as <env>/<product>, for example env-2/o-i",
        )
    };
    let seat = value.as_str().ok_or_else(bad)?;
    let parts: Vec<&str> = seat.split('/').collect();
    let part_ok = |part: &&str| {
        !part.is_empty()
            && part.len() <= 64
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
    };
    if !(2..=4).contains(&parts.len()) || !parts.iter().all(part_ok) {
        return Err(bad());
    }
    Ok(Some(seat.to_owned()))
}

fn parse_choke(value: Option<&Value>) -> Result<Option<u8>, PadsRefusal> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.as_u64() {
            Some(group @ 1..=8) => Ok(Some(group as u8)),
            _ => Err(PadsRefusal::invalid(
                "choke is null (Off) or a group from 1 to 8",
            )),
        },
    }
}

// ---- field helpers ---------------------------------------------------------------

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, PadsRefusal> {
    value
        .as_object()
        .ok_or_else(|| PadsRefusal::invalid(format!("{what} is an object")))
}

fn only_keys(object: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), PadsRefusal> {
    match object.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(PadsRefusal::invalid(format!("{what} has no `{key}`"))),
        None => Ok(()),
    }
}

fn check_text(text: &str, field: &str, limit: usize) -> Result<(), PadsRefusal> {
    let length = text.chars().count();
    if text.trim() != text || length == 0 || length > limit || text.chars().any(char::is_control) {
        return Err(PadsRefusal::invalid(format!(
            "{field} is text of 1 to {limit} characters, on one line and with no padding"
        )));
    }
    Ok(())
}

fn text_ref(value: &Value, field: &str, limit: usize) -> Result<String, PadsRefusal> {
    let text = value.as_str().ok_or_else(|| {
        PadsRefusal::invalid(format!("{field} is text of 1 to {limit} characters"))
    })?;
    check_text(text, field, limit)?;
    Ok(text.to_owned())
}

fn optional_ref(object: &Map<String, Value>, field: &str) -> Result<Option<String>, PadsRefusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => text_ref(value, field, MAX_REF_CHARS).map(Some),
    }
}

fn project_of(object: &Map<String, Value>) -> Result<&str, PadsRefusal> {
    let project = object
        .get("project_ref")
        .and_then(Value::as_str)
        .ok_or_else(|| PadsRefusal::invalid("project_ref names the project"))?;
    check_project(project)
}

fn check_project(project: &str) -> Result<&str, PadsRefusal> {
    check_text(project, "project_ref", MAX_PROJECT_CHARS)?;
    Ok(project)
}

fn index_of(object: &Map<String, Value>) -> Result<usize, PadsRefusal> {
    object
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < PAD_COUNT)
        .ok_or_else(|| PadsRefusal::invalid("index is a slot from 0 to 15"))
}

fn instant_of(object: &Map<String, Value>, default: Option<i64>) -> Result<i64, PadsRefusal> {
    match (object.get("at_unix_ms"), default) {
        (None | Some(Value::Null), Some(default)) => Ok(default),
        (Some(value), _) => value.as_i64().filter(|at| *at >= 0).ok_or_else(|| {
            PadsRefusal::invalid(
                "at_unix_ms is when it happened at its source, as milliseconds since 1970",
            )
        }),
        (None, None) => Err(PadsRefusal::invalid(
            "at_unix_ms is when it happened at its source, as milliseconds since 1970",
        )),
    }
}

// ---- storage ---------------------------------------------------------------------

fn revision_of(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

/// The folder name of a project: a readable slug and a hash of the whole ref,
/// so two projects never share a folder and a run ref carries no path.
fn project_key(project_ref: &str) -> String {
    let slug: String = project_ref
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
    let slug = if slug.is_empty() { "project" } else { slug };
    let hash = blake3::hash(project_ref.as_bytes()).to_hex();
    format!("{slug}-{}", &hash.as_str()[..16])
}

fn key_is_safe(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 80
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, PadsRefusal> {
    match read_native(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_refusal(
            &format!("could not read {}", path.display()),
            error,
        )),
    }
}

fn transaction(path: &Path, create_parent: bool) -> Result<NativeFileTransaction, PadsRefusal> {
    let name = lock_name(path).map_err(|error| {
        io_refusal(
            &format!("could not name the lock of {}", path.display()),
            error,
        )
    })?;
    NativeFileTransaction::acquire(path, &name, create_parent)
        .map_err(|error| io_refusal(&format!("could not open {}", path.display()), error))
}

fn publish(
    transaction: &NativeFileTransaction,
    bytes: &[u8],
    create_new: bool,
) -> Result<(), PadsRefusal> {
    match transaction.publish(bytes, create_new) {
        Ok(()) => Ok(()),
        Err(PublicationError::Before(error)) => {
            Err(io_refusal("the change was not published", error))
        }
        Err(PublicationError::Uncertain(uncertain)) => Err(PadsRefusal::new(
            "publication_uncertain",
            format!("publication is uncertain after the replacement; read the pads back before trying again: {uncertain}"),
        )),
    }
}

struct Bank {
    revision: String,
    slots: Vec<Option<Pad>>,
}

fn load_bank(bytes: Option<&[u8]>, project_ref: &str) -> Result<Bank, PadsRefusal> {
    let Some(bytes) = bytes else {
        return Ok(Bank {
            revision: "absent".into(),
            slots: vec![None; PAD_COUNT],
        });
    };
    let corrupt = |why: &str| {
        PadsRefusal::corrupt(format!(
            "the bank of {project_ref} is damaged ({why}); it is kept as it is and nothing is guessed from it"
        ))
    };
    let document: Value = serde_json::from_slice(bytes).map_err(|_| corrupt("it is not JSON"))?;
    let object = document
        .as_object()
        .ok_or_else(|| corrupt("it is not an object"))?;
    if object.get("schema") != Some(&json!(PADS_BANK)) {
        return Err(corrupt("it is not a factory.prompt-pads-bank/v1"));
    }
    if object.get("project_ref") != Some(&json!(project_ref)) {
        return Err(corrupt("it belongs to another project"));
    }
    let rows = object
        .get("slots")
        .and_then(Value::as_array)
        .filter(|rows| rows.len() == PAD_COUNT)
        .ok_or_else(|| corrupt("it does not hold sixteen slots"))?;
    let slots = rows
        .iter()
        .map(|row| {
            if row.is_null() {
                Ok(None)
            } else {
                Pad::parse(row)
                    .map(Some)
                    .map_err(|error| corrupt(&error.message))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Bank {
        revision: revision_of(bytes),
        slots,
    })
}

fn encode_bank(project_ref: &str, slots: &[Option<Pad>]) -> Vec<u8> {
    let document = json!({
        "schema":PADS_BANK,
        "project_ref":project_ref,
        "slots":slots.iter().map(|slot| slot.as_ref().map_or(Value::Null, Pad::to_json)).collect::<Vec<_>>(),
    });
    let mut bytes = serde_json::to_vec_pretty(&document).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

struct Record {
    revision: String,
    events: Vec<Value>,
}

fn load_record(bytes: Option<&[u8]>) -> Result<Record, PadsRefusal> {
    let Some(bytes) = bytes else {
        return Ok(Record {
            revision: "absent".into(),
            events: Vec::new(),
        });
    };
    let corrupt = |why: &str| {
        PadsRefusal::corrupt(format!(
            "the record of runs is damaged ({why}); it is kept as it is and nothing is guessed from it"
        ))
    };
    let text = std::str::from_utf8(bytes).map_err(|_| corrupt("it is not text"))?;
    let mut events = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let event: Value =
            serde_json::from_str(line).map_err(|_| corrupt("a line is not an event"))?;
        if event["schema"] != EVENT_SCHEMA {
            return Err(corrupt("a line is not a factory.prompt-pad-event/v1"));
        }
        events.push(event);
    }
    Ok(Record {
        revision: revision_of(bytes),
        events,
    })
}

/// Append one event: the old bytes plus one line, published on the lock held.
/// Earlier lines are never rewritten. Returns the record's new revision.
fn append(transaction: &NativeFileTransaction, event: &Value) -> Result<String, PadsRefusal> {
    let mut content = transaction
        .current()
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    if !content.is_empty() && !content.ends_with(b"\n") {
        content.push(b'\n');
    }
    content.extend(serde_json::to_vec(event).unwrap_or_default());
    content.push(b'\n');
    publish(transaction, &content, transaction.current().is_none())?;
    Ok(revision_of(&content))
}

// ---- runs ------------------------------------------------------------------------

/// One run as the readings show it: its begin and its end, when it has one.
fn run_view(begin: &Value, end: Option<&Value>) -> Value {
    let from_end = |key: &str| {
        end.map_or(Value::Null, |end| {
            end.get(key).cloned().unwrap_or(Value::Null)
        })
    };
    json!({
        "run_ref":begin["run_ref"],
        "index":begin["index"],
        "state":end.map_or(json!("running"), |end| end["state"].clone()),
        "trigger":begin["trigger"],
        "occurrence_ref":begin.get("occurrence_ref").cloned().unwrap_or(Value::Null),
        "pad":begin["pad"],
        "begun_at_unix_ms":begin["at_unix_ms"],
        "ended_at_unix_ms":from_end("at_unix_ms"),
        "task_ref":from_end("task_ref"),
        "session_ref":from_end("session_ref"),
        "reason":from_end("reason"),
        "begin":begin,
        "end":end.cloned().unwrap_or(Value::Null),
    })
}

/// Every run of a project, oldest first.
fn runs_of(events: &[Value]) -> Result<Vec<Value>, PadsRefusal> {
    let broken = |what: &str| {
        PadsRefusal::corrupt(format!(
            "the record of runs is inconsistent ({what}); it is kept as it is"
        ))
    };
    let mut order: Vec<String> = Vec::new();
    let mut begins: BTreeMap<String, &Value> = BTreeMap::new();
    let mut ends: BTreeMap<String, &Value> = BTreeMap::new();
    for event in events {
        let run_ref = event["run_ref"]
            .as_str()
            .ok_or_else(|| broken("an event has no run_ref"))?
            .to_owned();
        match event["event"].as_str() {
            Some("begin") => {
                if begins.insert(run_ref.clone(), event).is_some() {
                    return Err(broken("a run begins twice"));
                }
                order.push(run_ref);
            }
            Some("end") => {
                if ends.insert(run_ref, event).is_some() {
                    return Err(broken("a run ends twice"));
                }
            }
            _ => return Err(broken("an event is neither a begin nor an end")),
        }
    }
    if ends.keys().any(|run_ref| !begins.contains_key(run_ref)) {
        return Err(broken("a run ends that never began"));
    }
    Ok(order
        .iter()
        .map(|run_ref| run_view(begins[run_ref], ends.get(run_ref).copied()))
        .collect())
}

fn find_by_occurrence<'a>(runs: &'a [Value], occurrence: &str) -> Option<&'a Value> {
    runs.iter().find(|run| run["occurrence_ref"] == occurrence)
}

fn running_of(runs: &[Value], index: usize) -> Option<&Value> {
    runs.iter()
        .find(|run| run["index"] == index && run["state"] == "running")
}

fn last_of(runs: &[Value], index: usize) -> Option<&Value> {
    runs.iter()
        .filter(|run| run["index"] == index && !run["end"].is_null())
        .max_by_key(|run| run["ended_at_unix_ms"].as_i64().unwrap_or(0))
}

/// The run that holds a choke group against `index`, when there is one.
fn choke_holder(runs: &[Value], index: usize, group: u8) -> Option<&Value> {
    runs.iter().find(|run| {
        run["state"] == "running" && run["index"] != index && run["pad"]["choke"] == group
    })
}

fn parse_run_ref(run_ref: &str) -> Result<(String, usize), PadsRefusal> {
    let malformed = || {
        PadsRefusal::invalid(
            "run_ref is factory:pad-run:<project>:<index>:<n>, as a begin answered it",
        )
    };
    let rest = run_ref.strip_prefix(RUN_REF_PREFIX).ok_or_else(malformed)?;
    let mut parts = rest.rsplitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(number), Some(index), Some(key))
            if number.parse::<u64>().is_ok() && key_is_safe(key) =>
        {
            let index = index
                .parse::<usize>()
                .ok()
                .filter(|index| *index < PAD_COUNT)
                .ok_or_else(malformed)?;
            Ok((key.to_owned(), index))
        }
        _ => Err(malformed()),
    }
}

/// `pad:<index>:<instant>`.
fn parse_occurrence(occurrence: &str) -> Result<(usize, i64), PadsRefusal> {
    let malformed =
        || PadsRefusal::invalid("occurrence_ref is pad:<index>:<instant> for the pad that begins");
    let parts: Vec<&str> = occurrence.split(':').collect();
    if parts.len() != 3 || parts[0] != "pad" {
        return Err(malformed());
    }
    let index = parts[1]
        .parse::<usize>()
        .ok()
        .filter(|index| *index < PAD_COUNT)
        .ok_or_else(malformed)?;
    let instant = parts[2]
        .parse::<i64>()
        .ok()
        .filter(|instant| *instant >= 0)
        .ok_or_else(malformed)?;
    Ok((index, instant))
}

/// A scheduled begin is only ever an occurrence of the schedule saved on the
/// pad: nothing runs by itself on a pad whose Runs at is Off.
fn verify_occurrence(
    pad: &Pad,
    index: usize,
    occurrence: &str,
    instant: i64,
    at_ms: i64,
    central: &dyn OccurrenceResolver,
) -> Result<(), PadsRefusal> {
    let Some(schedule) = pad.runs_at.schedule() else {
        return Err(PadsRefusal::new(
            "not_scheduled",
            format!("Pad {index} has no schedule you saved; nothing runs by itself (its Runs at is Off)"),
        ));
    };
    if instant > at_ms {
        return Err(PadsRefusal::new(
            "not_due",
            format!("{occurrence} is not due yet: it falls after now"),
        ));
    }
    let found = central
        .occurrences(&schedule, instant, instant)
        .map_err(schedule_refusal)?;
    if !found.contains(&instant) {
        return Err(PadsRefusal::new(
            "no_such_occurrence",
            format!("{occurrence} is no occurrence of pad {index}'s saved schedule"),
        ));
    }
    Ok(())
}

fn begin_result(created: bool, project_ref: &str, run: &Value, revision: &str) -> Value {
    json!({
        "schema":PADS_RUN_BEGIN,
        "project_ref":project_ref,
        "index":run["index"],
        "created":created,
        "run_ref":run["run_ref"],
        "prompt":run["pad"]["prompt"],
        "session":run["pad"]["session"],
        "agent_ref":run["pad"]["agent_ref"],
        "scope_ref":run["pad"]["scope_ref"],
        "seat":run["pad"]["seat"],
        "run":run,
        "revision":revision,
        "automatic_agent_or_model_invocation":false,
    })
}

// ---- the person ------------------------------------------------------------------

/// A pad is written by a person at the desktop. Factory recognises one as it
/// does for its other owner writes (`attempt_admission`): the caller presents
/// `projectionKind: desktop-human` with a lineage that ends at itself, and the
/// authority names Factory as the native owner with the capability granted and
/// the action authorised. The grant itself is the host's to resolve.
fn require_human(object: &Map<String, Value>) -> Result<(String, String), PadsRefusal> {
    let not_human = |fact: String| {
        PadsRefusal::new(
            "not_human",
            format!("A pad is written by a person at the desktop: {fact}"),
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
        PadsRefusal::new(
            "not_authorised",
            format!("Writing a pad needs Factory's authority: {fact}"),
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
    if authority.capability_ref.as_deref() != Some(PADS_CAPABILITY_REF) {
        return Err(not_authorised(format!(
            "the capability is {PADS_CAPABILITY_REF}"
        )));
    }
    if !authority.capability_granted
        || !authority.action_authorised
        || authority.authority_ref.trim().is_empty()
    {
        return Err(not_authorised(
            "the capability must be granted, the action authorised, and the authority named".into(),
        ));
    }
    Ok((caller.caller_ref, authority.authority_ref))
}

// ---- the store -------------------------------------------------------------------

/// Factory's pads, in `<state>/prompt-pads/`. A developmental state file as
/// `<state>` names the directory it lives in.
pub struct PadsStore {
    root: PathBuf,
}

impl PadsStore {
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

    fn folder_for_key(&self, key: &str) -> PathBuf {
        self.root.join(STATE_FOLDER).join(key)
    }

    fn folder(&self, project_ref: &str) -> PathBuf {
        self.folder_for_key(&project_key(project_ref))
    }

    fn read_bank(&self, project_ref: &str) -> Result<Bank, PadsRefusal> {
        let bytes = read_optional(&self.folder(project_ref).join(BANK_FILE))?;
        load_bank(bytes.as_deref(), project_ref)
    }

    fn read_record(&self, project_ref: &str) -> Result<Record, PadsRefusal> {
        let bytes = read_optional(&self.folder(project_ref).join(RECORD_FILE))?;
        load_record(bytes.as_deref())
    }

    /// `factory development pads`: the bank and the record, read.
    pub fn reading(
        &self,
        project_ref: &str,
        now_ms: i64,
        central: &dyn OccurrenceResolver,
    ) -> Result<Value, PadsRefusal> {
        let project_ref = check_project(project_ref)?;
        let bank = self.read_bank(project_ref)?;
        let record = self.read_record(project_ref)?;
        let runs = runs_of(&record.events)?;
        let mut asked: BTreeMap<String, Result<Option<i64>, String>> = BTreeMap::new();
        let mut scheduled = 0;
        let mut slots = Vec::with_capacity(PAD_COUNT);
        for (index, slot) in bank.slots.iter().enumerate() {
            let mut next_at = Value::Null;
            let mut reason = None;
            if let Some(schedule) = slot.as_ref().and_then(|pad| pad.runs_at.schedule()) {
                scheduled += 1;
                let answer = asked.entry(schedule.to_string()).or_insert_with(|| {
                    central
                        .occurrences(&schedule, now_ms + 1, now_ms + HORIZON_MS)
                        .map(|found| found.into_iter().find(|at| *at > now_ms))
                        .map_err(|error| error.words())
                });
                match answer {
                    Ok(Some(at)) => next_at = json!(at),
                    Ok(None) => {
                        reason = Some(
                            "Central names no occurrence of this schedule in the next eight days"
                                .to_owned(),
                        );
                    }
                    Err(words) => reason = Some(words.clone()),
                }
            }
            let mut row = json!({
                "index":index,
                "pad":slot.as_ref().map(Pad::to_json),
                "next_at_unix_ms":next_at,
                "running":running_of(&runs, index).cloned().unwrap_or(Value::Null),
                "last":last_of(&runs, index).cloned().unwrap_or(Value::Null),
            });
            if let Some(reason) = reason {
                row["next_at_reason"] = json!(reason);
            }
            slots.push(row);
        }
        Ok(json!({
            "schema":PADS_READING,
            "project_ref":project_ref,
            "revision":bank.revision,
            "run_record_revision":record.revision,
            "now_unix_ms":now_ms,
            "scheduled_count":scheduled,
            "slots":slots,
            "write_admission":{
                "caller":{"projectionKind":"desktop-human","lineage":"ends at callerRef"},
                "authority":{"nativeOwner":FACTORY_NATIVE_OWNER,"capabilityRef":PADS_CAPABILITY_REF,
                    "capabilityGranted":true,"actionAuthorised":true},
            },
            "automatic_agent_or_model_invocation":false,
        }))
    }

    /// `factory development pads-write`: one slot, written or emptied, by a
    /// person, on the revision the bank was read at.
    pub fn write(
        &self,
        request: &Value,
        now_ms: i64,
        central: &dyn OccurrenceResolver,
    ) -> Result<Value, PadsRefusal> {
        let object = object(request, "the write request")?;
        only_keys(
            object,
            &[
                "project_ref",
                "index",
                "expected_revision",
                "pad",
                "caller",
                "authority",
            ],
            "the write request",
        )?;
        let (written_by, authority_ref) = require_human(object)?;
        let project_ref = project_of(object)?;
        let index = index_of(object)?;
        let expected = object
            .get("expected_revision")
            .and_then(Value::as_str)
            .filter(|revision| !revision.is_empty())
            .ok_or_else(|| {
                PadsRefusal::invalid(
                    "expected_revision is the revision the bank was read at, or `absent`",
                )
            })?;
        let pad = match object.get("pad") {
            None => {
                return Err(PadsRefusal::invalid(
                    "pad is a pad, or null to empty the slot",
                ))
            }
            Some(Value::Null) => None,
            Some(value) => Some(Pad::parse(value)?),
        };
        // Central validates the schedule, in its own grammar and its own words.
        if let Some(schedule) = pad.as_ref().and_then(|pad| pad.runs_at.schedule()) {
            central
                .occurrences(&schedule, now_ms, now_ms + HORIZON_MS)
                .map_err(schedule_refusal)?;
        }

        let path = self.folder(project_ref).join(BANK_FILE);
        let lock = transaction(&path, true)?;
        let bank = load_bank(lock.current(), project_ref)?;
        if bank.revision != expected {
            return Err(PadsRefusal::new(
                "stale_revision",
                format!(
                    "The bank changed: it is at {}, not {expected}; read it again",
                    bank.revision
                ),
            ));
        }
        let mut slots = bank.slots;
        slots[index] = pad.clone();
        let bytes = encode_bank(project_ref, &slots);
        let changed = lock.current() != Some(bytes.as_slice());
        let revision = if changed {
            publish(&lock, &bytes, lock.current().is_none())?;
            revision_of(&bytes)
        } else {
            bank.revision.clone()
        };
        Ok(json!({
            "schema":PADS_WRITE_RECEIPT,
            "project_ref":project_ref,
            "index":index,
            "previous_revision":bank.revision,
            "revision":revision,
            "changed":changed,
            "written_by":written_by,
            "authority_ref":authority_ref,
            "pad":pad.as_ref().map(Pad::to_json),
            "automatic_agent_or_model_invocation":false,
        }))
    }

    /// `factory development pads-run`: a run's begin. The run keeps the pad as
    /// it stood at this moment, so editing the pad afterwards leaves it with the
    /// prompt it ran with.
    pub fn begin(
        &self,
        request: &Value,
        now_ms: i64,
        central: &dyn OccurrenceResolver,
    ) -> Result<Value, PadsRefusal> {
        let object = object(request, "the run request")?;
        only_keys(
            object,
            &[
                "project_ref",
                "index",
                "trigger",
                "occurrence_ref",
                "at_unix_ms",
            ],
            "the run request",
        )?;
        let project_ref = project_of(object)?;
        let index = index_of(object)?;
        let at_ms = instant_of(object, Some(now_ms))?;
        let trigger = match object.get("trigger").and_then(Value::as_str) {
            Some(trigger @ ("press" | "schedule")) => trigger,
            _ => return Err(PadsRefusal::invalid("trigger is press or schedule")),
        };
        let occurrence = match object.get("occurrence_ref") {
            None | Some(Value::Null) => None,
            Some(value) => Some(text_ref(value, "occurrence_ref", MAX_PROJECT_CHARS)?),
        };
        let parsed = match (trigger, occurrence.as_deref()) {
            ("press", None) => None,
            ("press", Some(_)) => {
                return Err(PadsRefusal::invalid(
                    "a press has no occurrence_ref; only a schedule does",
                ))
            }
            (_, None) => {
                return Err(PadsRefusal::invalid(
                    "a scheduled begin names its occurrence_ref",
                ))
            }
            (_, Some(occurrence)) => {
                let (occurrence_index, instant) = parse_occurrence(occurrence)?;
                if occurrence_index != index {
                    return Err(PadsRefusal::invalid(format!(
                        "{occurrence} is an occurrence of pad {occurrence_index}, not pad {index}"
                    )));
                }
                Some(instant)
            }
        };

        // An occurrence that already began is the run it began, however the pad
        // has been edited since.
        if let Some(occurrence) = &occurrence {
            let record = self.read_record(project_ref)?;
            let runs = runs_of(&record.events)?;
            if let Some(existing) = find_by_occurrence(&runs, occurrence) {
                return Ok(begin_result(false, project_ref, existing, &record.revision));
            }
        }

        // The pad as it stands; a slot with nothing in it has nothing to run.
        let folder = self.folder(project_ref);
        let bank_path = folder.join(BANK_FILE);
        let record_path = folder.join(RECORD_FILE);
        let seen = self.read_bank(project_ref)?.slots[index]
            .clone()
            .ok_or_else(|| PadsRefusal::empty_pad(index))?;
        if let (Some(instant), Some(occurrence)) = (parsed, occurrence.as_deref()) {
            verify_occurrence(&seen, index, occurrence, instant, at_ms, central)?;
        }

        // Under the bank's lock, then the record's: the pad cannot change
        // between the snapshot taken and the begin written.
        let bank_lock = transaction(&bank_path, false)?;
        let pad = load_bank(bank_lock.current(), project_ref)?.slots[index]
            .clone()
            .ok_or_else(|| PadsRefusal::empty_pad(index))?;
        if parsed.is_some() && pad.runs_at != seen.runs_at {
            return Err(PadsRefusal::new(
                "stale_revision",
                format!(
                    "Pad {index} changed while its occurrence was being checked; read it again"
                ),
            ));
        }
        let record_lock = transaction(&record_path, true)?;
        let record = load_record(record_lock.current())?;
        let runs = runs_of(&record.events)?;
        if let Some(occurrence) = &occurrence {
            if let Some(existing) = find_by_occurrence(&runs, occurrence) {
                return Ok(begin_result(false, project_ref, existing, &record.revision));
            }
        }
        if let Some(running) = running_of(&runs, index) {
            return Err(PadsRefusal::new(
                "pad_running",
                format!(
                    "Pad {index} is running: {} has not ended",
                    running["run_ref"].as_str().unwrap_or("its run")
                ),
            ));
        }
        if let Some(group) = pad.choke {
            if let Some(holder) = choke_holder(&runs, index, group) {
                return Err(PadsRefusal::new(
                    "choke_wait",
                    format!(
                        "Pad {index} waits: pad {} of choke group {group} is running",
                        holder["index"]
                    ),
                ));
            }
        }
        let number = runs.iter().filter(|run| run["index"] == index).count() + 1;
        let run_ref = format!(
            "{RUN_REF_PREFIX}{}:{index}:{number}",
            project_key(project_ref)
        );
        let mut event = json!({
            "schema":EVENT_SCHEMA,
            "event":"begin",
            "run_ref":run_ref,
            "project_ref":project_ref,
            "index":index,
            "at_unix_ms":at_ms,
            "recorded_at_unix_ms":now_ms,
            "trigger":trigger,
            "pad":pad.to_json(),
        });
        if let Some(occurrence) = &occurrence {
            event["occurrence_ref"] = json!(occurrence);
        }
        let revision = append(&record_lock, &event)?;
        let run = run_view(&event, None);
        Ok(begin_result(true, project_ref, &run, &revision))
    }

    /// `factory development pads-record`: a run's end. A run ends once.
    pub fn record_end(&self, request: &Value, now_ms: i64) -> Result<Value, PadsRefusal> {
        let object = object(request, "the end request")?;
        only_keys(
            object,
            &[
                "run_ref",
                "state",
                "task_ref",
                "session_ref",
                "reason",
                "at_unix_ms",
            ],
            "the end request",
        )?;
        let run_ref = object
            .get("run_ref")
            .and_then(Value::as_str)
            .ok_or_else(|| PadsRefusal::invalid("run_ref names the run that ends"))?;
        let (key, _index) = parse_run_ref(run_ref)?;
        let at_ms = instant_of(object, None)?;
        let state = match object.get("state").and_then(Value::as_str) {
            Some(state @ ("done" | "stalled" | "stopped" | "refused")) => state,
            _ => {
                return Err(PadsRefusal::invalid(
                    "state is done, stalled, stopped or refused",
                ))
            }
        };
        let reason = match object.get("reason") {
            None | Some(Value::Null) => None,
            Some(value) => Some(text_ref(value, "reason", MAX_REASON_CHARS)?),
        };
        if matches!(state, "stalled" | "refused") && reason.is_none() {
            return Err(PadsRefusal::invalid(format!(
                "a run that is {state} says why in a reason: nothing runs where you cannot see it"
            )));
        }
        let task_ref = optional_ref(object, "task_ref")?;
        let session_ref = optional_ref(object, "session_ref")?;

        let unknown = || {
            PadsRefusal::new(
                "unknown_run",
                format!("unknown run: no begin is recorded for {run_ref}"),
            )
        };
        let record_path = self.folder_for_key(&key).join(RECORD_FILE);
        if read_optional(&record_path)?.is_none() {
            return Err(unknown());
        }
        let lock = transaction(&record_path, false)?;
        let record = load_record(lock.current())?;
        let runs = runs_of(&record.events)?;
        let run = runs
            .iter()
            .find(|run| run["run_ref"] == run_ref)
            .ok_or_else(unknown)?;
        if !run["end"].is_null() {
            return Err(PadsRefusal::new(
                "already_ended",
                format!("{run_ref} has ended already"),
            ));
        }
        if at_ms < run["begun_at_unix_ms"].as_i64().unwrap_or(0) {
            return Err(PadsRefusal::invalid("a run cannot end before it began"));
        }
        let mut event = json!({
            "schema":EVENT_SCHEMA,
            "event":"end",
            "run_ref":run_ref,
            "project_ref":run["begin"]["project_ref"],
            "index":run["index"],
            "at_unix_ms":at_ms,
            "recorded_at_unix_ms":now_ms,
            "state":state,
        });
        for (field, value) in [
            ("task_ref", task_ref),
            ("session_ref", session_ref),
            ("reason", reason),
        ] {
            if let Some(value) = value {
                event[field] = json!(value);
            }
        }
        let revision = append(&lock, &event)?;
        let ended = run_view(&run["begin"], Some(&event));
        Ok(json!({
            "schema":PADS_RUN_RECORD,
            "project_ref":run["begin"]["project_ref"],
            "created":true,
            "run":ended,
            "revision":revision,
            "automatic_agent_or_model_invocation":false,
        }))
    }

    /// `factory development pads-due`: the scheduled occurrences in
    /// `(since, now]` of every pad whose Runs at is not Off, and whether each
    /// has begun. Choke conflicts are reported, never resolved.
    pub fn due(
        &self,
        project_ref: &str,
        now_ms: i64,
        since_ms: i64,
        central: &dyn OccurrenceResolver,
    ) -> Result<Value, PadsRefusal> {
        let project_ref = check_project(project_ref)?;
        if since_ms > now_ms {
            return Err(PadsRefusal::invalid("since_unix_ms is after now_unix_ms"));
        }
        if now_ms - since_ms > MAX_DUE_WINDOW_MS {
            return Err(PadsRefusal::invalid(
                "the window is longer than 400 days; narrow it to what has not been polled",
            ));
        }
        let bank = self.read_bank(project_ref)?;
        let runs = runs_of(&self.read_record(project_ref)?.events)?;
        let mut result = json!({
            "schema":PADS_DUE,
            "project_ref":project_ref,
            "now_unix_ms":now_ms,
            "since_unix_ms":since_ms,
            "due":[],
            "automatic_agent_or_model_invocation":false,
        });
        let scheduled: Vec<(usize, &Pad, Value)> = bank
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let pad = slot.as_ref()?;
                Some((index, pad, pad.runs_at.schedule()?))
            })
            .collect();
        if scheduled.is_empty() {
            result["reason"] = json!(NO_SCHEDULE_SAVED);
            return Ok(result);
        }
        // (instant, index, row)
        let mut rows: Vec<(i64, usize, Value)> = Vec::new();
        if since_ms < now_ms {
            for (index, pad, schedule) in &scheduled {
                let found = central
                    .occurrences(schedule, since_ms + 1, now_ms)
                    .map_err(schedule_refusal)?;
                for instant in found
                    .into_iter()
                    .filter(|at| *at > since_ms && *at <= now_ms)
                {
                    let occurrence_ref = format!("pad:{index}:{instant}");
                    let begun = find_by_occurrence(&runs, &occurrence_ref);
                    rows.push((
                        instant,
                        *index,
                        json!({
                            "index":index,
                            "name":pad.name,
                            "kind":pad.runs_at.kind(),
                            "occurrence_ref":occurrence_ref,
                            "due_unix_ms":instant,
                            "begun":begun.is_some(),
                            "run_ref":begun.map_or(Value::Null, |run| run["run_ref"].clone()),
                            "choke":pad.choke,
                            "choke_conflicts":[],
                        }),
                    ));
                }
            }
        }
        rows.sort_by_key(|(instant, index, _)| (*instant, *index));
        let waiting: Vec<(usize, u8, String)> = rows
            .iter()
            .filter(|(_, _, row)| row["begun"] == false)
            .filter_map(|(_, index, row)| {
                Some((
                    *index,
                    u8::try_from(row["choke"].as_u64()?).ok()?,
                    row["occurrence_ref"].as_str()?.to_owned(),
                ))
            })
            .collect();
        for (_, index, row) in &mut rows {
            let Some(group) = row["choke"].as_u64().and_then(|g| u8::try_from(g).ok()) else {
                continue;
            };
            if row["begun"] == true {
                continue;
            }
            let mut conflicts = Vec::new();
            for run in runs.iter().filter(|run| {
                run["state"] == "running" && run["index"] != *index && run["pad"]["choke"] == group
            }) {
                conflicts
                    .push(json!({"kind":"running","index":run["index"],"run_ref":run["run_ref"]}));
            }
            for (other, other_group, occurrence_ref) in &waiting {
                if *other != *index && *other_group == group {
                    conflicts
                        .push(json!({"kind":"due","index":other,"occurrence_ref":occurrence_ref}));
                }
            }
            row["choke_conflicts"] = Value::Array(conflicts);
        }
        result["due"] = Value::Array(rows.into_iter().map(|(_, _, row)| row).collect());
        Ok(result)
    }
}

// ---- the command -------------------------------------------------------------------

/// True for `factory development pads|pads-write|pads-run|pads-record|pads-due`.
pub fn is_pads_command(args: &[String]) -> bool {
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
    let verb = stripped.first().cloned().unwrap_or_default();
    match execute(&stripped, json, None) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&refusal.document(&verb))
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
    stdin_override: Option<&str>,
) -> Result<String, PadsRefusal> {
    execute_with(
        args,
        json,
        stdin_override,
        crate::sensing_sources::now_ms(),
        &CtrlResolver,
    )
}

struct Arguments {
    positional: Vec<String>,
    flags: BTreeMap<String, String>,
}

fn parse_arguments(args: &[String], flags: &[&str], usage: &str) -> Result<Arguments, PadsRefusal> {
    let mut parsed = Arguments {
        positional: Vec::new(),
        flags: BTreeMap::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument.starts_with("--") {
            if !flags.contains(&argument.as_str()) {
                return Err(PadsRefusal::usage(
                    format!("unknown option {argument}"),
                    usage,
                ));
            }
            let value = args
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| PadsRefusal::usage(format!("{argument} needs a value"), usage))?;
            if parsed
                .flags
                .insert(argument.clone(), value.clone())
                .is_some()
            {
                return Err(PadsRefusal::usage(
                    format!("{argument} was given more than once"),
                    usage,
                ));
            }
            index += 1;
        } else {
            parsed.positional.push(argument.clone());
        }
        index += 1;
    }
    Ok(parsed)
}

fn read_request(path: &str, stdin: Option<&str>) -> Result<Value, PadsRefusal> {
    let text = if path == "-" {
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
        std::fs::read_to_string(path).map_err(|error| {
            io_refusal(&format!("could not read the request file {path}"), error)
        })?
    };
    serde_json::from_str(&text)
        .map_err(|error| PadsRefusal::invalid(format!("the request is not JSON: {error}")))
}

fn instant_flag(arguments: &Arguments, flag: &str, usage: &str) -> Result<i64, PadsRefusal> {
    let value = arguments
        .flags
        .get(flag)
        .ok_or_else(|| PadsRefusal::usage(format!("{flag} is required"), usage))?;
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .ok_or_else(|| {
            PadsRefusal::usage(
                format!("{flag} is milliseconds since 1970, not `{value}`"),
                usage,
            )
        })
}

/// The command with its clock and Central given, so a test can ask neither a
/// real clock nor a real `ctrl`.
pub fn execute_with(
    args: &[String],
    json: bool,
    stdin: Option<&str>,
    now_ms: i64,
    central: &dyn OccurrenceResolver,
) -> Result<String, PadsRefusal> {
    let verb = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| PadsRefusal::usage("missing prompt pads operation", USAGE_PADS))?;
    let rest = &args[1..];
    let (document, text): (Value, fn(&Value) -> String) = match verb {
        "pads" => {
            let arguments = parse_arguments(rest, &[], USAGE_PADS)?;
            let [state, project] = arguments.positional.as_slice() else {
                return Err(PadsRefusal::usage(
                    "expected <state> <project-ref>",
                    USAGE_PADS,
                ));
            };
            (
                PadsStore::at(state).reading(project, now_ms, central)?,
                text_reading as fn(&Value) -> String,
            )
        }
        "pads-due" => {
            let arguments = parse_arguments(rest, &["--now", "--since"], USAGE_DUE)?;
            let [state, project] = arguments.positional.as_slice() else {
                return Err(PadsRefusal::usage(
                    "expected <state> <project-ref>",
                    USAGE_DUE,
                ));
            };
            let now = instant_flag(&arguments, "--now", USAGE_DUE)?;
            let since = instant_flag(&arguments, "--since", USAGE_DUE)?;
            (
                PadsStore::at(state).due(project, now, since, central)?,
                text_due as fn(&Value) -> String,
            )
        }
        "pads-write" | "pads-run" | "pads-record" => {
            let usage = match verb {
                "pads-write" => USAGE_WRITE,
                "pads-run" => USAGE_RUN,
                _ => USAGE_RECORD,
            };
            let arguments = parse_arguments(rest, &[], usage)?;
            let (state, request_path) = match arguments.positional.as_slice() {
                [state] => (state, "-"),
                [state, request] => (state, request.as_str()),
                _ => {
                    return Err(PadsRefusal::usage(
                        "expected <state> and at most one request",
                        usage,
                    ))
                }
            };
            let request = read_request(request_path, stdin)?;
            let store = PadsStore::at(state);
            match verb {
                "pads-write" => (
                    store.write(&request, now_ms, central)?,
                    text_write as fn(&Value) -> String,
                ),
                "pads-run" => (
                    store.begin(&request, now_ms, central)?,
                    text_run as fn(&Value) -> String,
                ),
                _ => (
                    store.record_end(&request, now_ms)?,
                    text_record as fn(&Value) -> String,
                ),
            }
        }
        other => {
            return Err(PadsRefusal::usage(
                format!("unknown prompt pads operation `{other}`"),
                USAGE_PADS,
            ))
        }
    };
    if json {
        serde_json::to_string_pretty(&document).map_err(|error| {
            PadsRefusal::new("io", format!("could not encode the answer: {error}"))
        })
    } else {
        Ok(text(&document))
    }
}

fn runs_at_text(runs_at: &Value) -> String {
    match runs_at["kind"].as_str() {
        Some("daily") => {
            let days = runs_at["days"].as_array().map_or(String::new(), |days| {
                format!(
                    " ({})",
                    days.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
            format!("daily {}{days}", runs_at["time"].as_str().unwrap_or("?"))
        }
        Some("every") => format!("every {}h", runs_at["hours"]),
        _ => "off".to_owned(),
    }
}

fn text_reading(reading: &Value) -> String {
    let mut lines = vec![format!(
        "Prompt pads of {} (revision {}); {} with a schedule",
        reading["project_ref"].as_str().unwrap_or("?"),
        reading["revision"].as_str().unwrap_or("?"),
        reading["scheduled_count"]
    )];
    let slots = reading["slots"].as_array().cloned().unwrap_or_default();
    if slots.iter().all(|slot| slot["pad"].is_null()) {
        lines.push("no pads yet: all sixteen slots are empty".to_owned());
    }
    for slot in &slots {
        if slot["pad"].is_null() {
            continue;
        }
        let pad = &slot["pad"];
        let mut line = format!(
            "pad {}  {}  {}",
            slot["index"],
            pad["name"].as_str().unwrap_or("?"),
            runs_at_text(&pad["runs_at"])
        );
        if let Some(next) = slot["next_at_unix_ms"].as_i64() {
            line.push_str(&format!("  next {next}"));
        }
        if slot["running"].is_object() {
            line.push_str(&format!(
                "  RUNNING {}",
                slot["running"]["run_ref"].as_str().unwrap_or("?")
            ));
        }
        if let Some(state) = slot["last"]["state"].as_str() {
            line.push_str(&format!("  last {state}"));
        }
        lines.push(line);
    }
    lines.join("\n")
}

fn text_write(receipt: &Value) -> String {
    let what = if receipt["pad"].is_null() {
        "emptied"
    } else {
        "written"
    };
    let changed = if receipt["changed"] == true {
        what
    } else {
        "unchanged"
    };
    format!(
        "Pad {} {changed}; the bank is at {}",
        receipt["index"],
        receipt["revision"].as_str().unwrap_or("?")
    )
}

fn text_run(begun: &Value) -> String {
    format!(
        "{} {} on pad {}",
        begun["run_ref"].as_str().unwrap_or("?"),
        if begun["created"] == true {
            "begun"
        } else {
            "had begun already"
        },
        begun["index"]
    )
}

fn text_record(ended: &Value) -> String {
    format!(
        "{} ended: {}",
        ended["run"]["run_ref"].as_str().unwrap_or("?"),
        ended["run"]["state"].as_str().unwrap_or("?")
    )
}

fn text_due(due: &Value) -> String {
    let rows = due["due"].as_array().cloned().unwrap_or_default();
    let mut lines = vec![format!(
        "{} occurrence(s) due in ({}, {}]",
        rows.len(),
        due["since_unix_ms"],
        due["now_unix_ms"]
    )];
    if let Some(reason) = due["reason"].as_str() {
        lines.push(reason.to_owned());
    }
    for row in &rows {
        lines.push(format!(
            "{}  {}  {}",
            row["occurrence_ref"].as_str().unwrap_or("?"),
            row["name"].as_str().unwrap_or("?"),
            if row["begun"] == true {
                "begun"
            } else {
                "not begun"
            }
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    //! Red-first tests for the owner of the Prompt pads. Every test uses a
    //! temporary state directory and a fake Central: no real `ctrl` runs here,
    //! and nothing outside the temporary directory is read or written.
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;

    const PROJECT: &str = "project:O-I";
    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;
    /// 2026-10-19 08:53:20 UTC.
    const NOW: i64 = 1_792_400_000_000;

    /// Today's 08:00 UTC: an occurrence of "daily 08:00" before NOW.
    fn eight_today() -> i64 {
        NOW.div_euclid(DAY) * DAY + 8 * HOUR
    }

    /// Central, as far as the pads ask it: occurrences in UTC, Central's own
    /// words for a time that is not a wall time, and switches for refusal and
    /// absence. It records every question it was asked.
    #[derive(Default)]
    struct FakeCentral {
        refusal: Option<String>,
        unavailable: Option<String>,
        asked: RefCell<Vec<Value>>,
    }

    impl OccurrenceResolver for FakeCentral {
        fn occurrences(
            &self,
            schedule: &Value,
            from_unix_ms: i64,
            to_unix_ms: i64,
        ) -> Result<Vec<i64>, ScheduleError> {
            self.asked.borrow_mut().push(
                json!({"schedule":schedule,"window_from_unix_ms":from_unix_ms,"window_to_unix_ms":to_unix_ms}),
            );
            if let Some(words) = &self.refusal {
                return Err(ScheduleError::Refused(words.clone()));
            }
            if let Some(why) = &self.unavailable {
                return Err(ScheduleError::Unavailable(why.clone()));
            }
            let clock = |time: &str| -> Result<i64, ScheduleError> {
                let (hour, minute) = time.split_once(':').unwrap();
                let (hour, minute): (i64, i64) = (hour.parse().unwrap(), minute.parse().unwrap());
                if hour > 23 || minute > 59 {
                    return Err(ScheduleError::Refused(
                        "daily time must be a valid HH:MM wall time".into(),
                    ));
                }
                Ok((hour * 60 + minute) * 60_000)
            };
            let mut found = Vec::new();
            match schedule["kind"].as_str() {
                Some("daily") => {
                    let at = clock(schedule["time"].as_str().unwrap())?;
                    for day in from_unix_ms.div_euclid(DAY)..=to_unix_ms.div_euclid(DAY) {
                        found.push(day * DAY + at);
                    }
                }
                Some("cron") => {
                    let expression = schedule["expression"].as_str().unwrap();
                    let fields: Vec<&str> = expression.split_whitespace().collect();
                    let at = clock(&format!("{}:{}", fields[1], fields[0]))?;
                    let weekdays: Vec<i64> = fields[4]
                        .split(',')
                        .map(|day| day.parse().unwrap())
                        .collect();
                    for day in from_unix_ms.div_euclid(DAY)..=to_unix_ms.div_euclid(DAY) {
                        // 1970-01-01 was a Thursday; Sunday is 0.
                        if weekdays.contains(&(day + 4).rem_euclid(7)) {
                            found.push(day * DAY + at);
                        }
                    }
                }
                Some("every") => {
                    let step = schedule["interval_ms"].as_i64().unwrap();
                    let mut at = (from_unix_ms + step - 1).div_euclid(step) * step;
                    while at <= to_unix_ms {
                        found.push(at);
                        at += step;
                    }
                }
                other => panic!("the fake Central was asked for a {other:?} schedule"),
            }
            found.retain(|at| *at >= from_unix_ms && *at <= to_unix_ms);
            Ok(found)
        }
    }

    fn store() -> (tempfile::TempDir, PadsStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = PadsStore::at(dir.path());
        (dir, store)
    }

    fn human() -> (Value, Value) {
        (
            json!({"callerRef":"oi:desktop","projectionKind":"desktop-human","lineage":["oi:desktop"]}),
            json!({
                "authorityRef":"central:native-action-authority#owner",
                "nativeOwner":"factory",
                "capabilityRef":PADS_CAPABILITY_REF,
                "capabilityGranted":true,
                "actionAuthorised":true
            }),
        )
    }

    fn write_request(project: &str, index: usize, expected: &str, pad: Value) -> Value {
        let (caller, authority) = human();
        json!({"project_ref":project,"index":index,"expected_revision":expected,
            "pad":pad,"caller":caller,"authority":authority})
    }

    fn revision(store: &PadsStore, central: &FakeCentral, project: &str) -> String {
        store.reading(project, NOW, central).unwrap()["revision"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn put(
        store: &PadsStore,
        central: &FakeCentral,
        project: &str,
        index: usize,
        pad: Value,
    ) -> Value {
        let expected = revision(store, central, project);
        store
            .write(&write_request(project, index, &expected, pad), NOW, central)
            .unwrap()
    }

    fn try_put(
        store: &PadsStore,
        central: &FakeCentral,
        index: usize,
        pad: Value,
    ) -> Result<Value, PadsRefusal> {
        let expected = revision(store, central, PROJECT);
        store.write(&write_request(PROJECT, index, &expected, pad), NOW, central)
    }

    fn press(store: &PadsStore, central: &FakeCentral, index: usize) -> Result<Value, PadsRefusal> {
        store.begin(
            &json!({"project_ref":PROJECT,"index":index,"trigger":"press"}),
            NOW,
            central,
        )
    }

    fn finish(
        store: &PadsStore,
        run_ref: &str,
        state: &str,
        at: i64,
    ) -> Result<Value, PadsRefusal> {
        store.record_end(
            &json!({"run_ref":run_ref,"state":state,"at_unix_ms":at,"reason":"the agent said so"}),
            NOW,
        )
    }

    fn refusal_of(result: Result<Value, PadsRefusal>) -> PadsRefusal {
        result.expect_err("the owner should have refused")
    }

    fn record_lines(dir: &tempfile::TempDir, project: &str) -> Vec<Value> {
        let path = dir
            .path()
            .join("prompt-pads")
            .join(project_key(project))
            .join("runs.jsonl");
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn is_untouched(dir: &tempfile::TempDir) -> bool {
        std::fs::read_dir(dir.path()).unwrap().next().is_none()
    }

    // ---- the bank -------------------------------------------------------

    #[test]
    fn an_unwritten_bank_reads_as_sixteen_empty_slots_and_creates_nothing() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(reading["schema"], PADS_READING);
        assert_eq!(reading["project_ref"], PROJECT);
        assert_eq!(reading["revision"], "absent");
        assert_eq!(reading["run_record_revision"], "absent");
        assert_eq!(reading["scheduled_count"], 0);
        let slots = reading["slots"].as_array().unwrap();
        assert_eq!(slots.len(), 16);
        for (index, slot) in slots.iter().enumerate() {
            assert_eq!(slot["index"], index);
            assert!(slot["pad"].is_null());
            assert!(slot["next_at_unix_ms"].is_null());
            assert!(slot["running"].is_null());
            assert!(slot["last"].is_null());
        }
        assert_eq!(reading["automatic_agent_or_model_invocation"], false);
        assert!(central.asked.borrow().is_empty());
        assert!(is_untouched(&dir), "a read writes nothing");
    }

    #[test]
    fn a_written_pad_reads_back_whole_with_a_content_hash_revision() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        let receipt = put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"Run the checks on the branch and say what failed in one paragraph."}),
        );
        assert_eq!(receipt["schema"], PADS_WRITE_RECEIPT);
        assert_eq!(receipt["previous_revision"], "absent");
        assert_eq!(receipt["changed"], true);
        assert_eq!(receipt["written_by"], "oi:desktop");
        let revision = receipt["revision"].as_str().unwrap();
        let file = dir
            .path()
            .join("prompt-pads")
            .join(project_key(PROJECT))
            .join("bank.json");
        let bytes = std::fs::read(file).unwrap();
        assert_eq!(
            revision,
            format!("blake3:{}", blake3::hash(&bytes).to_hex()),
            "the revision is the hash of the bank's bytes"
        );

        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(reading["revision"], revision);
        assert_eq!(reading["scheduled_count"], 0);
        let slots = reading["slots"].as_array().unwrap();
        assert_eq!(
            slots[3]["pad"],
            json!({
                "name":"Run the checks on the branch",
                "prompt":"Run the checks on the branch and say what failed in one paragraph.",
                "session":{"kind":"new"},
                "agent_ref":null,
                "scope_ref":null,
                "seat":null,
                "runs_at":{"kind":"off"},
                "choke":null
            }),
            "defaults: the name is the prompt's first words, a new session, the project's agent, \
             the session's scope, no seat, Runs at Off, no choke"
        );
        assert!(slots[3]["next_at_unix_ms"].is_null());
        for index in (0..16).filter(|index| *index != 3) {
            assert!(slots[index]["pad"].is_null());
        }
    }

    #[test]
    fn a_pads_routine_link_is_kept_verbatim_and_absent_until_a_person_links_one() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        let schedule =
            json!({"kind":"daily","time":"08:00","days":["mon","tue","wed","thu","fri"]});
        // Unlinked: the pad carries no routine_ref at all, so earlier banks read back unchanged.
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"Run the checks","runs_at":schedule}),
        );
        let unlinked = store.reading(PROJECT, NOW, &central).unwrap();
        assert!(unlinked["slots"][0]["pad"]
            .as_object()
            .unwrap()
            .get("routine_ref")
            .is_none());

        // Linked: the AIKit Routine's ref, verbatim, on the write's receipt and on every reading.
        let written = put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"Run the checks","runs_at":schedule,"routine_ref":"routine/morning-checks"}),
        );
        assert_eq!(written["pad"]["routine_ref"], "routine/morning-checks");
        assert_eq!(written["changed"], true);
        let linked = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(
            linked["slots"][0]["pad"]["routine_ref"],
            "routine/morning-checks"
        );
        assert_eq!(linked["slots"][0]["pad"]["runs_at"]["kind"], "daily");

        // A begun run keeps the pad as it stood, link included.
        let begun = press(&store, &central, 0).unwrap();
        assert_eq!(begun["run"]["pad"]["routine_ref"], "routine/morning-checks");

        // Unlinking is a write without it (null is the same as absent).
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"Run the checks","runs_at":schedule,"routine_ref":null}),
        );
        let again = store.reading(PROJECT, NOW, &central).unwrap();
        assert!(again["slots"][0]["pad"]
            .as_object()
            .unwrap()
            .get("routine_ref")
            .is_none());
        assert_eq!(
            again["revision"], unlinked["revision"],
            "the same pad, the same bytes"
        );
    }

    #[test]
    fn a_pad_can_be_emptied_and_the_other_slots_stay() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(&store, &central, PROJECT, 1, json!({"prompt":"one"}));
        put(&store, &central, PROJECT, 2, json!({"prompt":"two"}));
        put(&store, &central, PROJECT, 1, Value::Null);
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert!(reading["slots"][1]["pad"].is_null());
        assert_eq!(reading["slots"][2]["pad"]["prompt"], "two");
    }

    #[test]
    fn banks_are_kept_per_project() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(&store, &central, "project:O-I", 0, json!({"prompt":"here"}));
        let other = store.reading("project:Factory", NOW, &central).unwrap();
        assert_eq!(other["revision"], "absent");
        assert!(other["slots"][0]["pad"].is_null());
    }

    #[test]
    fn a_write_on_a_stale_revision_is_refused_and_changes_nothing() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        let first = put(&store, &central, PROJECT, 0, json!({"prompt":"first"}));
        let stale = store
            .write(
                &write_request(PROJECT, 1, "absent", json!({"prompt":"second"})),
                NOW,
                &central,
            )
            .unwrap_err();
        assert_eq!(stale.code, "stale_revision");
        assert!(stale.message.contains("absent"), "{}", stale.message);
        assert!(
            stale.message.contains(first["revision"].as_str().unwrap()),
            "{}",
            stale.message
        );
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(reading["revision"], first["revision"], "nothing changed");
        assert!(reading["slots"][1]["pad"].is_null());

        // The same write on the revision just read goes through.
        let fresh = store
            .write(
                &write_request(
                    PROJECT,
                    1,
                    first["revision"].as_str().unwrap(),
                    json!({"prompt":"second"}),
                ),
                NOW,
                &central,
            )
            .unwrap();
        assert_ne!(fresh["revision"], first["revision"]);
    }

    #[test]
    fn writing_what_is_already_there_changes_nothing() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        let first = put(&store, &central, PROJECT, 0, json!({"prompt":"same"}));
        let again = put(&store, &central, PROJECT, 0, json!({"prompt":"same"}));
        assert_eq!(again["changed"], false);
        assert_eq!(again["revision"], first["revision"]);
    }

    // ---- the person ------------------------------------------------------

    #[test]
    fn only_a_person_writes_a_pad() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        let revision = revision(&store, &central, PROJECT);
        type Change = fn(&mut Value);
        let cases: Vec<(&str, &str, Change)> = vec![
            ("a situated agent", "not_human", |r| {
                r["caller"]["projectionKind"] = json!("situated-agent");
            }),
            ("a headless caller", "not_human", |r| {
                r["caller"]["projectionKind"] = json!("headless");
            }),
            ("no caller", "not_human", |r| {
                r.as_object_mut().unwrap().remove("caller");
            }),
            ("an empty caller", "not_human", |r| {
                r["caller"]["callerRef"] = json!("");
                r["caller"]["lineage"] = json!([""]);
            }),
            (
                "a lineage that does not end at the caller",
                "not_human",
                |r| {
                    r["caller"]["lineage"] = json!(["somebody:else"]);
                },
            ),
            ("no authority", "not_authorised", |r| {
                r.as_object_mut().unwrap().remove("authority");
            }),
            ("another native owner", "not_authorised", |r| {
                r["authority"]["nativeOwner"] = json!("central");
            }),
            ("another capability", "not_authorised", |r| {
                r["authority"]["capabilityRef"] = json!("capability/factory/operate-attempt");
            }),
            ("a capability not granted", "not_authorised", |r| {
                r["authority"]["capabilityGranted"] = json!(false);
            }),
            ("an action not authorised", "not_authorised", |r| {
                r["authority"]["actionAuthorised"] = json!(false);
            }),
            ("an empty authority ref", "not_authorised", |r| {
                r["authority"]["authorityRef"] = json!("");
            }),
        ];
        for (label, code, change) in cases {
            let mut request =
                write_request(PROJECT, 0, &revision, json!({"prompt":"run the checks"}));
            change(&mut request);
            let refusal = store.write(&request, NOW, &central).unwrap_err();
            assert_eq!(refusal.code, code, "{label}: {}", refusal.message);
        }
        assert!(
            is_untouched(&dir),
            "no refused write leaves anything behind"
        );
    }

    // ---- every field -----------------------------------------------------

    #[test]
    fn every_field_is_validated() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        let long = |length: usize| "x".repeat(length);
        let cases: Vec<(&str, Value, &str)> = vec![
            ("empty name", json!({"prompt":"p","name":""}), "name"),
            ("blank name", json!({"prompt":"p","name":"   "}), "name"),
            ("name of 81", json!({"prompt":"p","name":long(81)}), "name"),
            (
                "name with a newline",
                json!({"prompt":"p","name":"a\nb"}),
                "name",
            ),
            (
                "name that is a number",
                json!({"prompt":"p","name":5}),
                "name",
            ),
            ("no prompt", json!({}), "prompt"),
            ("empty prompt", json!({"prompt":""}), "prompt"),
            ("blank prompt", json!({"prompt":" \n\t "}), "prompt"),
            ("prompt of 20001", json!({"prompt":long(20_001)}), "prompt"),
            ("prompt that is a number", json!({"prompt":1}), "prompt"),
            (
                "unknown session kind",
                json!({"prompt":"p","session":{"kind":"other"}}),
                "session",
            ),
            (
                "a session with no ref",
                json!({"prompt":"p","session":{"kind":"session"}}),
                "session_ref",
            ),
            (
                "a session with an empty ref",
                json!({"prompt":"p","session":{"kind":"session","session_ref":""}}),
                "session_ref",
            ),
            (
                "new with a session ref",
                json!({"prompt":"p","session":{"kind":"new","session_ref":"s"}}),
                "session_ref",
            ),
            (
                "session as a string",
                json!({"prompt":"p","session":"new"}),
                "session",
            ),
            (
                "empty agent",
                json!({"prompt":"p","agent_ref":""}),
                "agent_ref",
            ),
            (
                "agent of 513",
                json!({"prompt":"p","agent_ref":long(513)}),
                "agent_ref",
            ),
            (
                "agent as a number",
                json!({"prompt":"p","agent_ref":3}),
                "agent_ref",
            ),
            (
                "padded scope",
                json!({"prompt":"p","scope_ref":" project:O-I "}),
                "scope_ref",
            ),
            (
                "empty routine",
                json!({"prompt":"p","routine_ref":""}),
                "routine_ref",
            ),
            (
                "routine of 513",
                json!({"prompt":"p","routine_ref":long(513)}),
                "routine_ref",
            ),
            (
                "routine as an object",
                json!({"prompt":"p","routine_ref":{"ref":"routine/morning"}}),
                "routine_ref",
            ),
            (
                "empty scope",
                json!({"prompt":"p","scope_ref":""}),
                "scope_ref",
            ),
            (
                "one-part seat",
                json!({"prompt":"p","seat":"env-2"}),
                "seat",
            ),
            (
                "seat with an empty part",
                json!({"prompt":"p","seat":"env-2//o-i"}),
                "seat",
            ),
            (
                "seat with a space",
                json!({"prompt":"p","seat":"env-2/o i"}),
                "seat",
            ),
            (
                "seat with a leading slash",
                json!({"prompt":"p","seat":"/env-2/o-i"}),
                "seat",
            ),
            (
                "runs_at as a string",
                json!({"prompt":"p","runs_at":"daily"}),
                "runs_at",
            ),
            (
                "runs_at of an unknown kind",
                json!({"prompt":"p","runs_at":{"kind":"weekly"}}),
                "runs_at",
            ),
            (
                "daily with no time",
                json!({"prompt":"p","runs_at":{"kind":"daily"}}),
                "time",
            ),
            (
                "daily with a loose time",
                json!({"prompt":"p","runs_at":{"kind":"daily","time":"8am"}}),
                "time",
            ),
            (
                "daily with no days named",
                json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00","days":[]}}),
                "days",
            ),
            (
                "daily naming a day twice",
                json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00","days":["mon","mon"]}}),
                "days",
            ),
            (
                "daily naming no such day",
                json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00","days":["funday"]}}),
                "days",
            ),
            (
                "days that are not a list",
                json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00","days":"mon"}}),
                "days",
            ),
            (
                "every 0 hours",
                json!({"prompt":"p","runs_at":{"kind":"every","hours":0}}),
                "hours",
            ),
            (
                "every 169 hours",
                json!({"prompt":"p","runs_at":{"kind":"every","hours":169}}),
                "hours",
            ),
            (
                "every 1.5 hours",
                json!({"prompt":"p","runs_at":{"kind":"every","hours":1.5}}),
                "hours",
            ),
            (
                "every with no hours",
                json!({"prompt":"p","runs_at":{"kind":"every"}}),
                "hours",
            ),
            (
                "off with a time",
                json!({"prompt":"p","runs_at":{"kind":"off","time":"08:00"}}),
                "time",
            ),
            ("choke 0", json!({"prompt":"p","choke":0}), "choke"),
            ("choke 9", json!({"prompt":"p","choke":9}), "choke"),
            ("choke as text", json!({"prompt":"p","choke":"2"}), "choke"),
            ("choke 1.5", json!({"prompt":"p","choke":1.5}), "choke"),
            (
                "a field a pad does not have",
                json!({"prompt":"p","colour":"red"}),
                "colour",
            ),
        ];
        for (label, pad, fragment) in cases {
            let refusal = try_put(&store, &central, 0, pad).unwrap_err();
            assert_eq!(refusal.code, "invalid", "{label}: {}", refusal.message);
            assert!(
                refusal.message.contains(fragment),
                "{label}: `{}` should name `{fragment}`",
                refusal.message
            );
        }
        // A bad slot, a bad project and a bad revision are refused too.
        let (caller, authority) = human();
        for (label, request) in [
            (
                "slot 16",
                json!({"project_ref":PROJECT,"index":16,"expected_revision":"absent","pad":null,"caller":caller,"authority":authority}),
            ),
            (
                "slot -1",
                json!({"project_ref":PROJECT,"index":-1,"expected_revision":"absent","pad":null,"caller":caller,"authority":authority}),
            ),
            (
                "no project",
                json!({"index":0,"expected_revision":"absent","pad":null,"caller":caller,"authority":authority}),
            ),
            (
                "empty project",
                json!({"project_ref":"","index":0,"expected_revision":"absent","pad":null,"caller":caller,"authority":authority}),
            ),
            (
                "no revision",
                json!({"project_ref":PROJECT,"index":0,"pad":null,"caller":caller,"authority":authority}),
            ),
            (
                "no pad",
                json!({"project_ref":PROJECT,"index":0,"expected_revision":"absent","caller":caller,"authority":authority}),
            ),
            (
                "an unknown key",
                json!({"project_ref":PROJECT,"index":0,"expected_revision":"absent","pad":null,"caller":caller,"authority":authority,"force":true}),
            ),
        ] {
            let refusal = store.write(&request, NOW, &central).unwrap_err();
            assert_eq!(refusal.code, "invalid", "{label}: {}", refusal.message);
        }
        assert!(
            is_untouched(&dir),
            "no refused write leaves anything behind"
        );
        assert!(
            central.asked.borrow().is_empty(),
            "a malformed pad never reaches Central"
        );

        // The limits themselves are allowed.
        let good = [
            json!({"prompt":long(20_000)}),
            json!({"prompt":"p","name":long(80)}),
            json!({"prompt":"p","choke":1}),
            json!({"prompt":"p","choke":8}),
            json!({"prompt":"p","agent_ref":long(512),"scope_ref":"project:Factory","seat":"env-2/o-i"}),
            json!({"prompt":"p","session":{"kind":"session","session_ref":"session:abc"}}),
            json!({"prompt":"p","runs_at":{"kind":"every","hours":1}}),
            json!({"prompt":"p","runs_at":{"kind":"every","hours":168}}),
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"8:05"}}),
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"23:59","days":["mon","tue","wed","thu","fri","sat","sun"]}}),
        ];
        for pad in good {
            try_put(&store, &central, 0, pad.clone())
                .unwrap_or_else(|refusal| panic!("{pad} should be admitted: {}", refusal.message));
        }
        let kept = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(
            kept["slots"][0]["pad"]["runs_at"],
            json!({"kind":"daily","time":"23:59","days":["mon","tue","wed","thu","fri","sat","sun"]})
        );
    }

    #[test]
    fn a_loose_time_is_kept_as_two_digits() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"8:05"}}),
        );
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(
            reading["slots"][0]["pad"]["runs_at"],
            json!({"kind":"daily","time":"08:05"})
        );
    }

    // ---- schedules, in Central's grammar ---------------------------------

    #[test]
    fn a_schedule_central_refuses_is_refused_in_centrals_words() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        let refusal = try_put(
            &store,
            &central,
            0,
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"25:00"}}),
        )
        .unwrap_err();
        assert_eq!(refusal.code, "schedule_refused");
        assert_eq!(
            refusal.message, "daily time must be a valid HH:MM wall time",
            "Central's own words, not a second grammar"
        );
        assert!(is_untouched(&dir));

        let refusing = FakeCentral {
            refusal: Some("window resolves more than 4096 occurrences; narrow the window".into()),
            ..FakeCentral::default()
        };
        let refusal = try_put(
            &store,
            &refusing,
            0,
            json!({"prompt":"p","runs_at":{"kind":"every","hours":1}}),
        )
        .unwrap_err();
        assert_eq!(refusal.code, "schedule_refused");
        assert_eq!(
            refusal.message,
            "window resolves more than 4096 occurrences; narrow the window"
        );
    }

    #[test]
    fn a_schedule_is_not_saved_when_central_cannot_be_asked() {
        let (dir, store) = store();
        let absent = FakeCentral {
            unavailable: Some("ctrl is not on the PATH".into()),
            ..FakeCentral::default()
        };
        let refusal = try_put(
            &store,
            &absent,
            0,
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00"}}),
        )
        .unwrap_err();
        assert_eq!(refusal.code, "central_unavailable");
        assert!(refusal.message.contains("ctrl is not on the PATH"));
        assert!(is_untouched(&dir), "an unchecked schedule is never saved");
        // A pad with no schedule needs no Central at all.
        absent.asked.borrow_mut().clear();
        try_put(&store, &absent, 0, json!({"prompt":"p"})).unwrap();
        assert!(absent.asked.borrow().is_empty());
    }

    #[test]
    fn next_at_comes_from_centrals_occurrences_and_is_null_when_runs_at_is_off() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(&store, &central, PROJECT, 0, json!({"prompt":"off"}));
        put(
            &store,
            &central,
            PROJECT,
            1,
            json!({"prompt":"daily","runs_at":{"kind":"daily","time":"08:00"}}),
        );
        put(
            &store,
            &central,
            PROJECT,
            2,
            json!({"prompt":"every","runs_at":{"kind":"every","hours":6}}),
        );
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"days","runs_at":{"kind":"daily","time":"09:30","days":["mon","wed","fri"]}}),
        );
        central.asked.borrow_mut().clear();
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        let slots = &reading["slots"];
        assert_eq!(reading["scheduled_count"], 3);
        assert!(slots[0]["next_at_unix_ms"].is_null());
        assert_eq!(
            slots[1]["next_at_unix_ms"],
            NOW.div_euclid(DAY) * DAY + DAY + 8 * HOUR,
            "NOW is past today's 08:00, so the next is tomorrow's"
        );
        assert_eq!(
            slots[2]["next_at_unix_ms"],
            (NOW.div_euclid(6 * HOUR) + 1) * 6 * HOUR
        );
        // 2026-10-19 is a Monday; 09:30 is after 08:53, so the next is today's.
        assert_eq!(
            slots[3]["next_at_unix_ms"],
            NOW.div_euclid(DAY) * DAY + 9 * HOUR + 30 * 60_000
        );

        // What Central was asked is Central's own schedule grammar.
        let asked: Vec<Value> = central
            .asked
            .borrow()
            .iter()
            .map(|question| question["schedule"].clone())
            .collect();
        assert!(asked.contains(&json!({"kind":"daily","time":"08:00"})));
        assert!(asked.contains(&json!({"kind":"every","interval_ms":6 * HOUR})));
        assert!(asked.contains(&json!({"kind":"cron","expression":"30 9 * * 1,3,5"})));
        assert_eq!(asked.len(), 3, "an Off pad asks nothing: {asked:?}");
    }

    #[test]
    fn every_day_of_the_week_is_a_daily_schedule() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00",
                "days":["sun","mon","tue","wed","thu","fri","sat"]}}),
        );
        let asked = central.asked.borrow();
        assert_eq!(asked[0]["schedule"], json!({"kind":"daily","time":"08:00"}));
    }

    #[test]
    fn next_at_is_absent_with_its_reason_when_central_cannot_say() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00"}}),
        );
        let absent = FakeCentral {
            unavailable: Some("ctrl is not on the PATH".into()),
            ..FakeCentral::default()
        };
        let reading = store.reading(PROJECT, NOW, &absent).unwrap();
        assert!(reading["slots"][0]["next_at_unix_ms"].is_null());
        assert!(reading["slots"][0]["next_at_reason"]
            .as_str()
            .unwrap()
            .contains("ctrl is not on the PATH"));
        assert_eq!(reading["scheduled_count"], 1);
    }

    #[test]
    fn editing_a_pad_changes_its_next_run() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"08:00"}}),
        );
        let before = store.reading(PROJECT, NOW, &central).unwrap()["slots"][0]["next_at_unix_ms"]
            .as_i64()
            .unwrap();
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"p","runs_at":{"kind":"daily","time":"20:00"}}),
        );
        let after = store.reading(PROJECT, NOW, &central).unwrap()["slots"][0]["next_at_unix_ms"]
            .as_i64()
            .unwrap();
        assert_eq!(before, NOW.div_euclid(DAY) * DAY + DAY + 8 * HOUR);
        assert_eq!(after, NOW.div_euclid(DAY) * DAY + 20 * HOUR);
    }

    // ---- a run -----------------------------------------------------------

    #[test]
    fn a_begin_returns_the_pad_as_it_stood() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({
                "prompt":"Review the branch.",
                "name":"Review",
                "session":{"kind":"session","session_ref":"session:abc"},
                "agent_ref":"agent:reviewer",
                "scope_ref":"project:O-I",
                "seat":"env-2/o-i",
                "choke":2
            }),
        );
        let begun = press(&store, &central, 3).unwrap();
        assert_eq!(begun["schema"], PADS_RUN_BEGIN);
        assert_eq!(begun["created"], true);
        assert_eq!(begun["prompt"], "Review the branch.");
        assert_eq!(
            begun["session"],
            json!({"kind":"session","session_ref":"session:abc"})
        );
        assert_eq!(begun["agent_ref"], "agent:reviewer");
        assert_eq!(begun["scope_ref"], "project:O-I");
        assert_eq!(begun["seat"], "env-2/o-i");
        let run_ref = begun["run_ref"].as_str().unwrap();
        assert!(run_ref.starts_with("factory:pad-run:"), "{run_ref}");
        assert!(run_ref.ends_with(":3:1"), "{run_ref}");

        // The begin is on the record, with the trigger and the instant.
        let lines = record_lines(&dir, PROJECT);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["event"], "begin");
        assert_eq!(lines[0]["trigger"], "press");
        assert_eq!(lines[0]["at_unix_ms"], NOW);
        assert_eq!(lines[0]["pad"]["choke"], 2);

        // The pad shows the run as running, and nothing ended yet.
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(reading["slots"][3]["running"]["run_ref"], run_ref);
        assert_eq!(reading["slots"][3]["running"]["state"], "running");
        assert!(reading["slots"][3]["last"].is_null());
    }

    #[test]
    fn editing_a_pad_after_a_begin_leaves_the_run_with_the_prompt_it_ran_with() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"the old prompt"}),
        );
        let begun = press(&store, &central, 3).unwrap();
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"the new prompt"}),
        );

        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(reading["slots"][3]["pad"]["prompt"], "the new prompt");
        assert_eq!(
            reading["slots"][3]["running"]["pad"]["prompt"], "the old prompt",
            "the run keeps the prompt it began with"
        );

        finish(
            &store,
            begun["run_ref"].as_str().unwrap(),
            "done",
            NOW + 1000,
        )
        .unwrap();
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(
            reading["slots"][3]["last"]["pad"]["prompt"],
            "the old prompt"
        );
        assert_eq!(reading["slots"][3]["pad"]["prompt"], "the new prompt");
        // The next press runs the pad as it now stands.
        let next = press(&store, &central, 3).unwrap();
        assert_eq!(next["prompt"], "the new prompt");
        assert!(next["run_ref"].as_str().unwrap().ends_with(":3:2"));
    }

    #[test]
    fn a_scheduled_begin_is_idempotent_per_occurrence() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"morning checks","runs_at":{"kind":"daily","time":"08:00"}}),
        );
        let occurrence = format!("pad:3:{}", eight_today());
        let request = json!({"project_ref":PROJECT,"index":3,"trigger":"schedule","occurrence_ref":occurrence});
        let first = store.begin(&request, NOW, &central).unwrap();
        assert_eq!(first["created"], true);
        let again = store.begin(&request, NOW + 5000, &central).unwrap();
        assert_eq!(again["created"], false);
        assert_eq!(again["run_ref"], first["run_ref"]);
        assert_eq!(again["prompt"], "morning checks");
        assert_eq!(
            record_lines(&dir, PROJECT).len(),
            1,
            "one begin on the record"
        );

        // It stays the same run after the run has ended and the pad was edited.
        finish(
            &store,
            first["run_ref"].as_str().unwrap(),
            "done",
            NOW + 1000,
        )
        .unwrap();
        put(&store, &central, PROJECT, 3, json!({"prompt":"changed"}));
        let after = store.begin(&request, NOW + 9000, &central).unwrap();
        assert_eq!(after["created"], false);
        assert_eq!(after["prompt"], "morning checks");
        assert_eq!(
            record_lines(&dir, PROJECT).len(),
            2,
            "begin and end; no second begin"
        );
    }

    #[test]
    fn a_scheduled_begin_is_only_ever_an_occurrence_of_the_saved_schedule() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            2,
            json!({"prompt":"no schedule"}),
        );
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"daily","runs_at":{"kind":"daily","time":"08:00"}}),
        );
        let scheduled = |index: usize, occurrence: String| {
            store.begin(
                &json!({"project_ref":PROJECT,"index":index,"trigger":"schedule","occurrence_ref":occurrence}),
                NOW,
                &central,
            )
        };
        let at = eight_today();

        let off = refusal_of(scheduled(2, format!("pad:2:{at}")));
        assert_eq!(off.code, "not_scheduled");
        assert!(off.message.contains("Pad 2"), "{}", off.message);

        let off_the_clock = refusal_of(scheduled(3, format!("pad:3:{}", at + 1)));
        assert_eq!(off_the_clock.code, "no_such_occurrence");

        let not_yet = refusal_of(scheduled(3, format!("pad:3:{}", at + DAY)));
        assert_eq!(not_yet.code, "not_due");

        let other_pad = refusal_of(scheduled(3, format!("pad:2:{at}")));
        assert_eq!(other_pad.code, "invalid");

        for malformed in ["pad:3", "pad:x:1", "slot:3:1", "pad:3:soon", "pad:3:1:2"] {
            let refusal = refusal_of(scheduled(3, malformed.into()));
            assert_eq!(refusal.code, "invalid", "{malformed}: {}", refusal.message);
        }

        // A press names no occurrence; a schedule must.
        let press_with = refusal_of(store.begin(
            &json!({"project_ref":PROJECT,"index":3,"trigger":"press","occurrence_ref":format!("pad:3:{at}")}),
            NOW,
            &central,
        ));
        assert_eq!(press_with.code, "invalid");
        let schedule_without = refusal_of(store.begin(
            &json!({"project_ref":PROJECT,"index":3,"trigger":"schedule"}),
            NOW,
            &central,
        ));
        assert_eq!(schedule_without.code, "invalid");
        let nonsense = refusal_of(store.begin(
            &json!({"project_ref":PROJECT,"index":3,"trigger":"cron"}),
            NOW,
            &central,
        ));
        assert_eq!(nonsense.code, "invalid");
        assert!(
            record_lines(&dir, PROJECT).is_empty(),
            "nothing was recorded"
        );
    }

    #[test]
    fn an_empty_pad_does_not_begin_and_a_running_pad_does_not_begin_again() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        let empty = refusal_of(press(&store, &central, 5));
        assert_eq!(empty.code, "empty_pad");
        assert!(is_untouched(&dir), "a refused begin leaves nothing behind");

        put(&store, &central, PROJECT, 5, json!({"prompt":"p"}));
        let first = press(&store, &central, 5).unwrap();
        let again = refusal_of(press(&store, &central, 5));
        assert_eq!(again.code, "pad_running");
        assert!(again.message.contains("Pad 5"), "{}", again.message);
        assert!(
            again.message.contains(first["run_ref"].as_str().unwrap()),
            "{}",
            again.message
        );
        assert_eq!(record_lines(&dir, PROJECT).len(), 1);

        // An emptied slot keeps its running run, and still cannot begin.
        put(&store, &central, PROJECT, 5, Value::Null);
        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(reading["slots"][5]["running"]["run_ref"], first["run_ref"]);
    }

    #[test]
    fn choke_is_enforced_by_the_owner() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"build A","choke":2}),
        );
        put(
            &store,
            &central,
            PROJECT,
            7,
            json!({"prompt":"build B","choke":2}),
        );
        put(
            &store,
            &central,
            PROJECT,
            9,
            json!({"prompt":"other group","choke":3}),
        );
        put(&store, &central, PROJECT, 4, json!({"prompt":"no group"}));

        let seven = press(&store, &central, 7).unwrap();
        let waits = refusal_of(press(&store, &central, 3));
        assert_eq!(waits.code, "choke_wait");
        assert_eq!(
            waits.message,
            "Pad 3 waits: pad 7 of choke group 2 is running"
        );
        assert_eq!(
            record_lines(&dir, PROJECT).len(),
            1,
            "the refused begin recorded nothing"
        );

        // Another group, and a pad with no group, are free to run.
        press(&store, &central, 9).unwrap();
        press(&store, &central, 4).unwrap();

        // Once pad 7's run has ended, pad 3 runs.
        finish(
            &store,
            seven["run_ref"].as_str().unwrap(),
            "stopped",
            NOW + 1,
        )
        .unwrap();
        press(&store, &central, 3).unwrap();
        // And now pad 7 waits on pad 3.
        let waits = refusal_of(press(&store, &central, 7));
        assert_eq!(
            waits.message,
            "Pad 7 waits: pad 3 of choke group 2 is running"
        );
    }

    #[test]
    fn a_run_holds_the_choke_group_it_began_in_even_when_its_pad_is_edited() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"A","choke":2}),
        );
        put(
            &store,
            &central,
            PROJECT,
            7,
            json!({"prompt":"B","choke":2}),
        );
        press(&store, &central, 3).unwrap();
        // Pad 3 leaves the group while it runs: its run still holds the seat.
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"A","choke":4}),
        );
        let waits = refusal_of(press(&store, &central, 7));
        assert_eq!(waits.code, "choke_wait");
        assert_eq!(
            waits.message,
            "Pad 7 waits: pad 3 of choke group 2 is running"
        );
    }

    // ---- the end of a run ------------------------------------------------

    #[test]
    fn a_run_ends_once_and_the_pad_shows_how() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        put(&store, &central, PROJECT, 3, json!({"prompt":"p"}));
        let begun = press(&store, &central, 3).unwrap();
        let run_ref = begun["run_ref"].as_str().unwrap().to_owned();

        let ended = store
            .record_end(
                &json!({"run_ref":run_ref,"state":"done","task_ref":"task:1","session_ref":"session:9","at_unix_ms":NOW + 5000}),
                NOW + 5000,
            )
            .unwrap();
        assert_eq!(ended["schema"], PADS_RUN_RECORD);
        assert_eq!(ended["run"]["state"], "done");
        assert_eq!(ended["run"]["task_ref"], "task:1");
        assert_eq!(ended["run"]["session_ref"], "session:9");
        assert_eq!(ended["run"]["ended_at_unix_ms"], NOW + 5000);

        let twice = refusal_of(finish(&store, &run_ref, "stopped", NOW + 6000));
        assert_eq!(twice.code, "already_ended");
        assert_eq!(
            record_lines(&dir, PROJECT).len(),
            2,
            "the second end is not recorded"
        );

        let reading = store.reading(PROJECT, NOW, &central).unwrap();
        let last = &reading["slots"][3]["last"];
        assert_eq!(last["state"], "done");
        assert_eq!(last["task_ref"], "task:1");
        assert_eq!(last["begin"]["event"], "begin");
        assert_eq!(last["end"]["event"], "end");
        assert!(reading["slots"][3]["running"].is_null());
    }

    #[test]
    fn an_unknown_or_malformed_run_cannot_end() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        // No record at all.
        let key = project_key(PROJECT);
        let unknown = refusal_of(finish(
            &store,
            &format!("factory:pad-run:{key}:3:1"),
            "done",
            NOW,
        ));
        assert_eq!(unknown.code, "unknown_run");
        assert!(is_untouched(&dir), "an unknown run creates no record");

        put(&store, &central, PROJECT, 3, json!({"prompt":"p"}));
        press(&store, &central, 3).unwrap();
        // A well-formed ref that never began.
        let never = refusal_of(finish(
            &store,
            &format!("factory:pad-run:{key}:3:9"),
            "done",
            NOW,
        ));
        assert_eq!(never.code, "unknown_run");
        let other_pad = refusal_of(finish(
            &store,
            &format!("factory:pad-run:{key}:4:1"),
            "done",
            NOW,
        ));
        assert_eq!(other_pad.code, "unknown_run");
        let malformed_refs = vec![
            "not a ref".to_owned(),
            "factory:pad-run:".to_owned(),
            "factory:pad-run:../../x:3:1".to_owned(),
            "factory:pad-run:a/b:3:1".to_owned(),
            format!("factory:pad-run:{key}:16:1"),
            format!("factory:pad-run:{key}:3:x"),
        ];
        for malformed in &malformed_refs {
            let refusal = refusal_of(finish(&store, malformed, "done", NOW));
            assert_eq!(refusal.code, "invalid", "{malformed}: {}", refusal.message);
        }
        assert_eq!(record_lines(&dir, PROJECT).len(), 1);
    }

    #[test]
    fn the_end_of_a_run_is_validated() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        put(&store, &central, PROJECT, 3, json!({"prompt":"p"}));
        let begun = press(&store, &central, 3).unwrap();
        let run_ref = begun["run_ref"].as_str().unwrap();
        let end = |body: Value| {
            let mut request = json!({"run_ref":run_ref,"state":"done","at_unix_ms":NOW + 10});
            for (key, value) in body.as_object().unwrap() {
                if value.is_null() {
                    request.as_object_mut().unwrap().remove(key);
                } else {
                    request[key] = value.clone();
                }
            }
            store.record_end(&request, NOW + 10)
        };
        for (label, body, fragment) in [
            ("an unknown state", json!({"state":"finished"}), "state"),
            ("no state", json!({"state":null}), "state"),
            (
                "stalled without a reason",
                json!({"state":"stalled"}),
                "reason",
            ),
            (
                "refused without a reason",
                json!({"state":"refused"}),
                "reason",
            ),
            (
                "an empty reason",
                json!({"state":"stopped","reason":""}),
                "reason",
            ),
            ("no instant", json!({"at_unix_ms":null}), "at_unix_ms"),
            ("a negative instant", json!({"at_unix_ms":-1}), "at_unix_ms"),
            (
                "an end before the begin",
                json!({"at_unix_ms":NOW - 1}),
                "before it began",
            ),
            ("an empty task", json!({"task_ref":""}), "task_ref"),
            ("an empty session", json!({"session_ref":""}), "session_ref"),
            (
                "a key a record does not have",
                json!({"colour":"red"}),
                "colour",
            ),
        ] {
            let refusal = refusal_of(end(body));
            assert_eq!(refusal.code, "invalid", "{label}: {}", refusal.message);
            assert!(
                refusal.message.contains(fragment),
                "{label}: `{}` should name `{fragment}`",
                refusal.message
            );
        }
        assert_eq!(
            record_lines(&dir, PROJECT).len(),
            1,
            "no refused end was recorded"
        );
        // The four ways a run can end.
        for (offset, state) in ["stalled", "stopped", "refused", "done"]
            .into_iter()
            .enumerate()
        {
            let index = 8 + offset;
            put(&store, &central, PROJECT, index, json!({"prompt":"p"}));
            let run = press(&store, &central, index).unwrap();
            let run_ref = run["run_ref"].as_str().unwrap();
            let ended = finish(&store, run_ref, state, NOW + 20).unwrap();
            assert_eq!(ended["run"]["state"], state);
            assert_eq!(ended["run"]["reason"], "the agent said so");
        }
    }

    // ---- what is due -----------------------------------------------------

    #[test]
    fn due_lists_in_window_occurrences_and_marks_the_ones_that_began() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(&store, &central, PROJECT, 0, json!({"prompt":"off"}));
        put(
            &store,
            &central,
            PROJECT,
            1,
            json!({"prompt":"daily","name":"Daily","runs_at":{"kind":"daily","time":"08:00"}}),
        );
        put(
            &store,
            &central,
            PROJECT,
            2,
            json!({"prompt":"every 8","runs_at":{"kind":"every","hours":8}}),
        );
        let t0 = eight_today();
        // The window is (since, now]: the instant at `since` is not in it, the
        // instant at `now` is.
        let due = store.due(PROJECT, t0, t0 - 2 * DAY, &central).unwrap();
        assert_eq!(due["schema"], PADS_DUE);
        assert_eq!(due["now_unix_ms"], t0);
        assert_eq!(due["since_unix_ms"], t0 - 2 * DAY);
        assert_eq!(due["automatic_agent_or_model_invocation"], false);
        let rows = due["due"].as_array().unwrap();
        let of = |index: usize| -> Vec<i64> {
            rows.iter()
                .filter(|row| row["index"] == index)
                .map(|row| row["due_unix_ms"].as_i64().unwrap())
                .collect()
        };
        assert_eq!(of(0), Vec::<i64>::new(), "Runs at Off is never due");
        assert_eq!(of(1), vec![t0 - DAY, t0]);
        assert_eq!(
            of(2),
            (1..=6)
                .map(|step| t0 - 2 * DAY + step * 8 * HOUR)
                .collect::<Vec<_>>()
        );
        assert!(
            rows.windows(2)
                .all(|pair| pair[0]["due_unix_ms"].as_i64() <= pair[1]["due_unix_ms"].as_i64()),
            "oldest first"
        );
        let daily = rows.iter().find(|row| row["index"] == 1).unwrap();
        assert_eq!(daily["occurrence_ref"], format!("pad:1:{}", t0 - DAY));
        assert_eq!(daily["kind"], "daily");
        assert_eq!(daily["name"], "Daily");
        assert_eq!(daily["begun"], false);
        assert!(daily["run_ref"].is_null());

        // Begin one occurrence: it is marked, with its run.
        let begun = store
            .begin(
                &json!({"project_ref":PROJECT,"index":1,"trigger":"schedule","occurrence_ref":format!("pad:1:{t0}")}),
                t0,
                &central,
            )
            .unwrap();
        let due = store.due(PROJECT, t0, t0 - 2 * DAY, &central).unwrap();
        let rows = due["due"].as_array().unwrap();
        let marked = rows
            .iter()
            .find(|row| row["occurrence_ref"] == format!("pad:1:{t0}"))
            .unwrap();
        assert_eq!(marked["begun"], true);
        assert_eq!(marked["run_ref"], begun["run_ref"]);
        let unmarked = rows
            .iter()
            .find(|row| row["occurrence_ref"] == format!("pad:1:{}", t0 - DAY))
            .unwrap();
        assert_eq!(unmarked["begun"], false);
    }

    #[test]
    fn due_reports_choke_conflicts_and_resolves_none() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            1,
            json!({"prompt":"a","choke":2,"runs_at":{"kind":"daily","time":"08:00"}}),
        );
        put(
            &store,
            &central,
            PROJECT,
            2,
            json!({"prompt":"b","choke":2,"runs_at":{"kind":"daily","time":"08:00"}}),
        );
        put(
            &store,
            &central,
            PROJECT,
            3,
            json!({"prompt":"c","runs_at":{"kind":"daily","time":"08:00"}}),
        );
        put(
            &store,
            &central,
            PROJECT,
            4,
            json!({"prompt":"d","choke":2}),
        );
        let t0 = eight_today();
        // Pad 4 is running in the group when the poll is made.
        press(&store, &central, 4).unwrap();
        let due = store.due(PROJECT, t0, t0 - 1, &central).unwrap();
        let rows = due["due"].as_array().unwrap();
        assert_eq!(
            rows.len(),
            3,
            "all three occurrences are listed, none dropped"
        );
        let one = rows.iter().find(|row| row["index"] == 1).unwrap();
        assert_eq!(one["choke"], 2);
        let conflicts = one["choke_conflicts"].as_array().unwrap();
        assert!(conflicts
            .iter()
            .any(|conflict| conflict["kind"] == "running" && conflict["index"] == 4));
        assert!(conflicts.iter().any(|conflict| conflict["kind"] == "due"
            && conflict["index"] == 2
            && conflict["occurrence_ref"] == format!("pad:2:{t0}")));
        let three = rows.iter().find(|row| row["index"] == 3).unwrap();
        assert!(three["choke"].is_null());
        assert_eq!(three["choke_conflicts"], json!([]));
    }

    #[test]
    fn due_says_why_when_nothing_is_scheduled() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        let due = store.due(PROJECT, NOW, NOW - DAY, &central).unwrap();
        assert_eq!(due["due"], json!([]));
        assert!(due["reason"]
            .as_str()
            .unwrap()
            .contains("nothing runs by itself"));
        put(&store, &central, PROJECT, 0, json!({"prompt":"off"}));
        let due = store.due(PROJECT, NOW, NOW - DAY, &central).unwrap();
        assert_eq!(due["due"], json!([]));
        assert!(due["reason"].is_string());
        assert!(
            central.asked.borrow().is_empty(),
            "no schedule, no question"
        );
    }

    #[test]
    fn due_refuses_a_backwards_or_a_very_long_window() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        let backwards = store.due(PROJECT, NOW - 1, NOW, &central).unwrap_err();
        assert_eq!(backwards.code, "invalid");
        let long = store
            .due(PROJECT, NOW, NOW - 401 * DAY, &central)
            .unwrap_err();
        assert_eq!(long.code, "invalid");
        assert!(long.message.contains("400 days"), "{}", long.message);
        let empty = store.due(PROJECT, NOW, NOW, &central).unwrap();
        assert_eq!(empty["due"], json!([]));
    }

    #[test]
    fn a_schedule_central_now_refuses_makes_due_fail_in_centrals_words() {
        let (_dir, store) = store();
        let central = FakeCentral::default();
        put(
            &store,
            &central,
            PROJECT,
            0,
            json!({"prompt":"p","runs_at":{"kind":"every","hours":1}}),
        );
        let refusing = FakeCentral {
            refusal: Some("window resolves more than 4096 occurrences; narrow the window".into()),
            ..FakeCentral::default()
        };
        let error = store
            .due(PROJECT, NOW, NOW - 399 * DAY, &refusing)
            .unwrap_err();
        assert_eq!(error.code, "schedule_refused");
        assert_eq!(
            error.message,
            "window resolves more than 4096 occurrences; narrow the window"
        );
    }

    // ---- storage ---------------------------------------------------------

    #[test]
    fn the_state_may_be_a_directory_or_the_developmental_state_file_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let state_file = dir.path().join("development-state.json");
        std::fs::write(&state_file, "{}").unwrap();
        let central = FakeCentral::default();
        let by_file = PadsStore::at(&state_file);
        put(&by_file, &central, PROJECT, 0, json!({"prompt":"here"}));
        let by_directory = PadsStore::at(dir.path());
        let reading = by_directory.reading(PROJECT, NOW, &central).unwrap();
        assert_eq!(reading["slots"][0]["pad"]["prompt"], "here");
        assert!(dir.path().join("prompt-pads").is_dir());
    }

    #[test]
    fn a_damaged_bank_or_record_is_refused_and_kept_as_it_is() {
        let (dir, store) = store();
        let central = FakeCentral::default();
        put(&store, &central, PROJECT, 3, json!({"prompt":"p"}));
        press(&store, &central, 3).unwrap();
        let folder = dir.path().join("prompt-pads").join(project_key(PROJECT));

        let record = folder.join("runs.jsonl");
        let good = std::fs::read_to_string(&record).unwrap();
        std::fs::write(&record, format!("{good}this is not an event\n")).unwrap();
        let broken = store.reading(PROJECT, NOW, &central).unwrap_err();
        assert_eq!(broken.code, "corrupt_state");
        assert_eq!(
            std::fs::read_to_string(&record).unwrap(),
            format!("{good}this is not an event\n"),
            "kept as it is"
        );
        std::fs::write(&record, &good).unwrap();

        let bank = folder.join("bank.json");
        std::fs::write(&bank, "{\"schema\":\"factory.prompt-pads-bank/v1\"}").unwrap();
        let broken = store.reading(PROJECT, NOW, &central).unwrap_err();
        assert_eq!(broken.code, "corrupt_state");
        let revision = "blake3:0";
        let refused = store
            .write(
                &write_request(PROJECT, 0, revision, json!({"prompt":"x"})),
                NOW,
                &central,
            )
            .unwrap_err();
        assert_eq!(
            refused.code, "corrupt_state",
            "a damaged bank is never overwritten"
        );
    }

    // ---- the command -----------------------------------------------------

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    fn run_cli(
        parts: &[&str],
        stdin: Option<&str>,
        now: i64,
        central: &FakeCentral,
    ) -> Result<Value, PadsRefusal> {
        execute_with(&args(parts), true, stdin, now, central)
            .map(|text| serde_json::from_str(&text).expect("--json answers JSON"))
    }

    #[test]
    fn the_five_verbs_walk_one_pad_through_its_life() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_str().unwrap();
        let central = FakeCentral::default();
        let t0 = eight_today();

        // pads: the empty bank.
        let empty = run_cli(&["pads", state, PROJECT], None, NOW, &central).unwrap();
        assert_eq!(empty["revision"], "absent");
        assert_eq!(empty["slots"].as_array().unwrap().len(), 16);

        // pads-write
        let (caller, authority) = human();
        let request = json!({
            "project_ref":PROJECT,"index":3,"expected_revision":"absent",
            "pad":{"name":"Morning checks","prompt":"Run the checks on main and report what failed.",
                   "session":{"kind":"new"},"agent_ref":null,"scope_ref":null,"seat":"env-2/o-i",
                   "runs_at":{"kind":"daily","time":"08:00"},"choke":2},
            "caller":caller,"authority":authority
        });
        let written = run_cli(
            &["pads-write", state, "-"],
            Some(&request.to_string()),
            NOW,
            &central,
        )
        .unwrap();
        println!("pads-write request: {request}");
        println!("pads-write answer: {written}");
        assert_eq!(written["schema"], PADS_WRITE_RECEIPT);
        assert_eq!(written["index"], 3);
        assert_eq!(written["changed"], true);

        // pads: the bank read back.
        let read = run_cli(&["pads", state, PROJECT], None, NOW, &central).unwrap();
        println!("pads answer slot 3: {}", read["slots"][3]);
        assert_eq!(read["revision"], written["revision"]);
        assert_eq!(read["scheduled_count"], 1);
        assert_eq!(read["slots"][3]["next_at_unix_ms"], t0 + DAY);

        // pads-due: today's 08:00 has come.
        let due = run_cli(
            &[
                "pads-due",
                state,
                PROJECT,
                "--now",
                &NOW.to_string(),
                "--since",
                &(NOW - 6 * HOUR).to_string(),
            ],
            None,
            NOW,
            &central,
        )
        .unwrap();
        println!("pads-due answer: {due}");
        assert_eq!(due["due"].as_array().unwrap().len(), 1);
        assert_eq!(due["due"][0]["occurrence_ref"], format!("pad:3:{t0}"));
        assert_eq!(due["due"][0]["begun"], false);

        // pads-run: the scheduled occurrence begins.
        let run_request = json!({"project_ref":PROJECT,"index":3,"trigger":"schedule",
            "occurrence_ref":format!("pad:3:{t0}")});
        let begun = run_cli(
            &["pads-run", state, "-"],
            Some(&run_request.to_string()),
            NOW,
            &central,
        )
        .unwrap();
        println!("pads-run request: {run_request}");
        println!("pads-run answer: {begun}");
        assert_eq!(begun["created"], true);
        assert_eq!(
            begun["prompt"],
            "Run the checks on main and report what failed."
        );
        let run_ref = begun["run_ref"].as_str().unwrap();

        // pads-record: it ends.
        let end_request = json!({"run_ref":run_ref,"state":"done","task_ref":"task:morning-1",
            "session_ref":"session:morning-1","at_unix_ms":NOW + 90_000});
        let ended = run_cli(
            &["pads-record", state, "-"],
            Some(&end_request.to_string()),
            NOW + 90_000,
            &central,
        )
        .unwrap();
        println!("pads-record request: {end_request}");
        println!("pads-record answer: {ended}");
        assert_eq!(ended["run"]["state"], "done");

        // pads-due again: the occurrence is marked as begun.
        let due = run_cli(
            &[
                "pads-due",
                state,
                PROJECT,
                "--now",
                &NOW.to_string(),
                "--since",
                &(NOW - 6 * HOUR).to_string(),
            ],
            None,
            NOW,
            &central,
        )
        .unwrap();
        assert_eq!(due["due"][0]["begun"], true);
        assert_eq!(due["due"][0]["run_ref"], begun["run_ref"]);

        // pads: the last run, begin and end.
        let read = run_cli(&["pads", state, PROJECT], None, NOW + 100_000, &central).unwrap();
        println!("pads answer slot 3 after: {}", read["slots"][3]);
        assert_eq!(read["slots"][3]["last"]["state"], "done");
        assert!(read["slots"][3]["running"].is_null());
    }

    #[test]
    fn the_command_refuses_in_a_document_with_a_code() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_str().unwrap();
        let central = FakeCentral::default();
        let refusal = execute_with(
            &args(&["pads-run", state, "-"]),
            true,
            Some(&json!({"project_ref":PROJECT,"index":3,"trigger":"press"}).to_string()),
            NOW,
            &central,
        )
        .unwrap_err();
        assert_eq!(refusal.code, "empty_pad");
        let document = refusal.document("pads-run");
        assert_eq!(document["schema"], PADS_REFUSAL);
        assert_eq!(document["code"], "empty_pad");
        assert_eq!(document["message"], refusal.message);
        assert_eq!(document["effect"], "Nothing was recorded.");
    }

    #[test]
    fn the_command_takes_the_arguments_it_documents_and_no_others() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_str().unwrap();
        let central = FakeCentral::default();
        for bad in [
            vec!["pads"],
            vec!["pads", state],
            vec!["pads", state, PROJECT, "extra"],
            vec!["pads", state, PROJECT, "--now", "1"],
            vec!["pads-due", state, PROJECT],
            vec!["pads-due", state, PROJECT, "--now", "9"],
            vec!["pads-due", state, PROJECT, "--now", "x", "--since", "1"],
            vec![
                "pads-due", state, PROJECT, "--now", "9", "--since", "1", "--now", "9",
            ],
            vec!["pads-write"],
            vec!["pads-write", state, "-", "extra"],
            vec!["pads-run", state, "/nonexistent/request.json"],
            vec!["pads-record", state, "-"],
            vec!["pads-launch", state],
        ] {
            let stdin = if bad.contains(&"-") {
                Some("not json")
            } else {
                None
            };
            let refusal = execute_with(&args(&bad), true, stdin, NOW, &central).unwrap_err();
            assert!(
                matches!(refusal.code, "usage" | "invalid" | "io"),
                "{bad:?}: {} {}",
                refusal.code,
                refusal.message
            );
        }
        assert!(is_untouched(&dir));
    }

    #[test]
    fn without_json_the_command_answers_in_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().to_str().unwrap();
        let central = FakeCentral::default();
        let text =
            execute_with(&args(&["pads", state, PROJECT]), false, None, NOW, &central).unwrap();
        assert!(text.contains(PROJECT), "{text}");
        assert!(text.contains("no pads"), "{text}");
        assert!(serde_json::from_str::<Value>(&text).is_err());
    }
}
