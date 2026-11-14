//! A session on a track and the tasks it runs (O-I #1006; DLW manual §19.1; O-I `docs/architecture/world-shell.md` §9.3).
//!
//! A session is an agent of the census seated on a track: AIKit's AgentSession, opened by the kernel's one provision
//! (`oi desktop provision`, what the window's `encounter_provision` runs) in the checkout the project's workcell seat holds,
//! its record opened by AIKit's own hooks (`aikit hook dispatch <client> SessionStart`, PROGRAMME R36) and closed by them at
//! its end. A task is written on it as a Run of one unit, commissioned in the project's Factory state together with its
//! workflow source and its attempt field (the act a song's take is, song_take.rs); its unit's practice is the Skill Sets the
//! agent carries. Run, it is Factory's own `start-serial` on the session's Agency, under the person's grant, admitted on the
//! practice AIKit prepared in a NOW Central allocated for the task (`now-context prepare`, its receipt read back by
//! `now-context inspect`), sent to the session by Factory's agent client
//! (attempt_owner_dispatch.rs) and followed to its end: the session's turn streamed into the task's log, the agent's asks
//! answered by the session's Scope (its checkout), and the turn's end recorded on the attempt — the difference it left in the
//! checkout verified and returned, or the failure. Stop pauses the turn (AIKit cancels it and the session keeps what it had in
//! mind); pressed again it cancels the attempt. Factory runs no agent: AIKit does.
//!
//! Every answer is one JSON document. Every refusal says which owner refused and why, in its words.

use crate::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use crate::attempt_owner_dispatch::{
    execute_attempt_owner_action, FactoryAttemptOwnerRequest, FACTORY_ATTEMPT_OWNER_ACTION,
};
use crate::attempt_runtime::*;
use crate::cli::CliError;
use crate::core::run::{RunRef, WorkflowUnitRef};
use crate::execution_intelligence::{
    AikitModelRosterSelection, ExecutionDemand, ExecutionDisposition,
    EXECUTION_INTELLIGENCE_INTEROP_VERSION,
};
use crate::native_owner::{NativeOwnerInvocation, AIKIT_CAW_CONTRACT_REVISION};
use crate::orchestration::{LegStatus, RetryGrant, ReturnedArtifact};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const SESSION_CONTRACT: &str = "factory.session/v1";
pub const TASK_CONTRACT: &str = "factory.session-task/v1";
/// Factory's sessions of a project, as `factory session list` answers them.
pub const SESSION_LIST_CONTRACT: &str = "factory.session-list/v1";
/// The attempts a task's unit is given in all, its first among them: a task that fails is retried under the grant its first
/// attempt consumes (manual §19.1, "When a task on the track stalls, the track lights a retry"), twice at most.
const TASK_ATTEMPTS: u32 = 3;
/// The tracking fact a pause leaves on an attempt: AIKit cancelled the session's turn at Factory's Stop.
const PAUSE: &str = "factory-session-pause";
/// How Factory names the way it chose the body a task runs on: the agent the person seated on the track, as AIKit opened it.
const SEATED: &str = "SEATED_SESSION";
const SEATED_SELECTION: &str = "factory.seated-session-selection/v1";
const FOLLOWER: &str = "factory:session-follow";
const AIKIT_TIMEOUT: Duration = Duration::from_secs(90);
/// The longest a followed turn is waited for before the attempt records that it was not seen to end.
const FOLLOW_LIMIT: Duration = Duration::from_secs(2 * 60 * 60);

fn refuse(message: impl Into<String>) -> CliError {
    CliError::new(message.into())
}

fn clip(text: &str) -> String {
    text.chars().take(600).collect()
}

fn minted(kind: &str) -> String {
    format!("{kind}:{}", ulid::Ulid::new())
}

fn now_rfc3339() -> String {
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// An owner executable: its variable when the host names one, else the first on PATH, else its bare name.
fn executable(var: &str, name: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(var).filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|candidate| candidate.is_file())
        })
        .unwrap_or_else(|| PathBuf::from(name))
}

fn aikit_bin() -> PathBuf {
    executable("OI_AIKIT_BIN", "aikit")
}

/// A project of a World: its root, its Central project id, its Factory state and the checkout its sessions work in.
struct Place {
    world: PathBuf,
    project: String,
    root: PathBuf,
    project_id: String,
    state: PathBuf,
    checkout: PathBuf,
}

fn place(world: &str, project: &str) -> Result<Place, CliError> {
    let world = PathBuf::from(world);
    if !world.is_absolute() || !world.join("Control").is_dir() {
        return Err(refuse(format!(
            "{} is no Central World: a World is an absolute folder that keeps Control/",
            world.display()
        )));
    }
    if project.is_empty()
        || project.contains('/')
        || project.starts_with('.')
        || project.chars().any(char::is_control)
    {
        return Err(refuse(
            "a project is named by one folder of the World's Work (its name, never a path)",
        ));
    }
    let root = world.join("Work").join(project);
    let manifest = root.join("ProjectCentral/project.json");
    let source: Value = std::fs::read(&manifest)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| {
            refuse(format!(
                "{} keeps no ProjectCentral manifest, so it is no project of this World",
                root.display()
            ))
        })?;
    let project_id = source["project_id"]
        .as_str()
        .filter(|id| source["schema"] == "central.project/v1" && !id.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| refuse(format!("{} names no Central project", manifest.display())))?;
    let checkout = checkout_of(&world, project).unwrap_or_else(|| root.clone());
    Ok(Place {
        state: root.join(".factory/development-state.json"),
        world,
        project: project.to_owned(),
        root,
        project_id,
        checkout,
    })
}

/// The project a Factory state stands in (`<World>/Work/<project>/.factory/<state>`).
fn place_of_state(state: &Path) -> Result<Place, CliError> {
    let root = state
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == ".factory"))
        .and_then(Path::parent)
        .ok_or_else(|| {
            refuse(format!(
                "{} is no Factory state kept in a project's .factory folder",
                state.display()
            ))
        })?;
    let work = root
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "Work"))
        .ok_or_else(|| {
            refuse(format!(
                "{} stands in no World's Work, so its sessions have no checkout",
                root.display()
            ))
        })?;
    let world = work
        .parent()
        .ok_or_else(|| refuse("Work stands in no World"))?;
    let project = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut found = place(&world.to_string_lossy(), &project)?;
    found.state = state.to_path_buf();
    Ok(found)
}

/// The checkout a project's sessions work in (manual §19.1, "Works in … the project's workcell by default"): the seat the
/// World's workcell register holds claimed for the project, or none.
fn checkout_of(world: &Path, project: &str) -> Option<PathBuf> {
    let bytes = std::fs::read(world.join("Control/machines/workcells.register.json")).ok()?;
    let register: Value = serde_json::from_slice(&bytes).ok()?;
    for cell in register["workcells"].as_array()? {
        let Some(base) = cell["path"].as_str() else {
            continue;
        };
        for seat in cell["seats"].as_array().into_iter().flatten() {
            if seat["product"] != project || seat["state"] != "claimed" {
                continue;
            }
            let folder = cell["products"][project]["path"]
                .as_str()
                .unwrap_or(project);
            let path = Path::new(base).join(folder);
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    None
}

/// One AIKit verb in the session's checkout: its answer (unwrapped from AIKit's envelope), or its refusal in its words.
fn aikit(cwd: &Path, args: &[&str]) -> Result<Value, String> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let output = crate::native_aikit_route::output(&aikit_bin(), cwd, None, &args, AIKIT_TIMEOUT)
        .map_err(|error| format!("AIKit could not be asked: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|_| {
        format!(
            "AIKit answered `{}` with no JSON (exit {:?}): {} {}",
            args.iter().take(3).cloned().collect::<Vec<_>>().join(" "),
            output.status.code(),
            clip(&stdout),
            clip(&String::from_utf8_lossy(&output.stderr))
        )
    })?;
    if !output.status.success() || value["ok"] == false {
        let words = value["error"]["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| clip(&value.to_string()));
        return Err(format!(
            "AIKit refused `{}`: {words}",
            args.iter().take(3).cloned().collect::<Vec<_>>().join(" ")
        ));
    }
    Ok(match value.get("data") {
        Some(data) if value.get("ok").is_some() => data.clone(),
        _ => value,
    })
}

fn encounter(cwd: &Path, request: Value) -> Result<Value, String> {
    aikit(
        cwd,
        &[
            "session-space",
            "encounter",
            "--request-json",
            &request.to_string(),
        ],
    )
}

/// The session's Agency binding, as AIKit holds it: who the agent is, under which Agency and World binding, from which
/// source, which sender it hears, and the binding's revision an addressed turn names.
struct Binding {
    agent_ref: String,
    agency_ref: String,
    world_binding_ref: String,
    revision: String,
    sender: String,
    source_ref: String,
    source_revision: String,
    source_digest: String,
}

fn binding(cwd: &Path, session: &str) -> Result<Binding, CliError> {
    let read = aikit(
        cwd,
        &[
            "session-space",
            "encounter-agency-read",
            "--agent-session",
            session,
        ],
    )
    .map_err(refuse)?;
    let held = if read.get("agency_ref").is_some() {
        &read
    } else {
        &read["binding"]
    };
    let text = |value: &Value, what: &str| {
        value
            .as_str()
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                refuse(format!(
                    "AIKit's Agency binding of {session} names no {what}: {}",
                    clip(&read.to_string())
                ))
            })
    };
    Ok(Binding {
        agent_ref: text(&held["agent_ref"], "agent")?,
        agency_ref: text(&held["agency_ref"], "Agency")?,
        world_binding_ref: text(&held["world_binding_ref"], "World binding")?,
        revision: text(&held["revision"], "revision")?,
        sender: text(
            held["allowed_senders"]
                .as_array()
                .and_then(|senders| senders.first())
                .unwrap_or(&Value::Null),
            "sender it hears",
        )?,
        source_ref: text(&held["agency_source"]["source_ref"], "source")?,
        source_revision: text(&held["agency_source"]["revision"], "source revision")?,
        source_digest: text(&held["agency_source"]["content_digest"], "source digest")?,
    })
}

/// The hook client a census agent's harness speaks (AIKit's `hook dispatch <client>`).
fn hook_client(agent: &str) -> &str {
    match agent {
        "claude-code" | "claude" => "claude",
        other => other,
    }
}

/// How a harness asks AIKit's lifecycle hook (`aikit hook dispatch <client> <event>`): its arguments and the event it delivers.
/// A hook-command harness asks in plain mode with its own hook event; pi's hooks ride AIKit's extension carrier, which asks for the
/// machine envelope (`--json`) with pi's own event (`session_start` with its reason, `session_shutdown`). Pure.
fn hook_ask(client: &str, event: &str, session: &str, cwd: &Path) -> (Vec<String>, Value) {
    let mut args: Vec<String> = Vec::new();
    let payload = if client == "pi" {
        args.push("--json".into());
        if event == "SessionStart" {
            json!({"type": "session_start", "reason": "startup", "cwd": cwd, "session_id": session})
        } else {
            json!({"type": "session_shutdown", "cwd": cwd, "session_id": session})
        }
    } else {
        let mut payload = json!({"session_id": session, "cwd": cwd, "hook_event_name": event});
        if event == "SessionStart" {
            payload["source"] = json!("startup");
        } else {
            payload["reason"] = json!("other");
        }
        payload
    };
    args.extend(["hook", "dispatch", client, event].map(str::to_owned));
    (args, payload)
}

/// The text the harness takes into its session from AIKit's answer: a hook-command harness its `additionalContext`, pi's carrier
/// the envelope's `data.injected`; empty when the answer carries none. Pure.
fn ground_of(client: &str, answer: &Value) -> String {
    let ground = if client == "pi" {
        &answer["data"]["injected"]
    } else {
        &answer["hookSpecificOutput"]["additionalContext"]
    };
    ground.as_str().unwrap_or_default().to_owned()
}

/// The session's record, opened or closed by AIKit's own lifecycle hooks (`aikit hook dispatch <client> <event>`, R36): the
/// event as the harness itself would deliver it (hook_ask), AIKit's answer verbatim, and the ground the harness takes into the
/// session from it (ground_of). A refusal is kept in AIKit's words.
fn record(cwd: &Path, agent: &str, event: &str, session: &str) -> Value {
    let client = hook_client(agent);
    let (args, payload) = hook_ask(client, event, session, cwd);
    let started = Command::new(aikit_bin())
        .args(&args)
        .current_dir(cwd)
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let output = started.and_then(|mut child| {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(payload.to_string().as_bytes());
        }
        child.wait_with_output()
    });
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let answer =
                serde_json::from_str::<Value>(&stdout).unwrap_or(Value::String(clip(&stdout)));
            json!({
                "event": event,
                "client": client,
                "kept": output.status.success(),
                "ground": ground_of(client, &answer),
                "answer": answer,
                "words": clip(String::from_utf8_lossy(&output.stderr).trim()),
                "by": "aikit hook dispatch",
            })
        }
        Err(error) => json!({"event": event, "client": client, "kept": false, "ground": "",
            "words": format!("AIKit's hook dispatch could not start: {error}"), "by": "aikit hook dispatch"}),
    }
}

/// `factory session start <agent> <world> <project>`: the agent seated on a new track.
fn start(agent: &str, world: &str, project: &str) -> Result<Value, CliError> {
    if agent.trim().is_empty() {
        return Err(refuse(
            "a session seats an agent of the census, named by its slug",
        ));
    }
    let place = place(world, project)?;
    let oi = executable("OI_BIN", "oi");
    let output = Command::new(&oi)
        .args(["desktop", "provision"])
        .arg(&place.checkout)
        .arg(&place.project_id)
        .arg(agent)
        .current_dir(&place.checkout)
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            refuse(format!(
                "the kernel's provision ({} desktop provision) could not start: {error}",
                oi.display()
            ))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let provision: Value = serde_json::from_str(stdout.trim()).unwrap_or(Value::Null);
    if !output.status.success() || !provision.is_object() {
        return Err(refuse(format!(
            "the kernel's provision did not open a session for {agent}: {} {}",
            clip(&stdout),
            clip(String::from_utf8_lossy(&output.stderr).trim())
        )));
    }
    let session = provision["agent_session"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            refuse(format!(
                "the kernel's provision named no session: {}",
                clip(&provision.to_string())
            ))
        })?;
    let space = provision["space"].as_str().unwrap_or_default().to_owned();
    let record = record(&place.checkout, agent, "SessionStart", &session);
    // The project is Factory's own from its first session, and Factory keeps the session: the window lays it as a track from
    // its start, before any task is written on it (manual §19.1, "The agent takes the seat, and the track carries its tasks").
    placed(&place)?;
    let kept = json!({
        "contract": SESSION_CONTRACT,
        "state": "open",
        "session": session,
        "space": space,
        "track": format!("{space}:{session}"),
        "agent": agent,
        "provider": provision["provider"],
        "world": place.world,
        "project": place.project,
        "checkout": place.checkout,
        "opened_at": now_rfc3339(),
        "tasks": [],
    });
    keep_session(&place.state, &session, &kept)?;
    Ok(json!({
        "contract": SESSION_CONTRACT,
        "state": "open",
        "session": session,
        "space": space,
        "track": format!("{space}:{session}"),
        "agent": agent,
        "provider": provision["provider"],
        "world": place.world,
        "project": place.project,
        "project_id": place.project_id,
        "project_root": place.root,
        "checkout": place.checkout,
        "factory_state": place.state,
        "record": record,
        "provision": provision,
        "terminal": terminal(&place.checkout, &session),
    }))
}

/// The project placed for Factory (`project setup`, keyed by the project's Central id, as each task's commission is keyed), so
/// Factory locates its state (`project locate`) and the World Model's factory slice reads its sessions and tasks. A project
/// placed already stays as it is.
fn placed(place: &Place) -> Result<(), CliError> {
    if !place.root.join(".factory/project.json").exists() {
        crate::project_setup::setup(
            &place.root,
            &place.project_id,
            Some(&place.root.join("ProjectCentral/project.json")),
        )?;
    }
    Ok(())
}

/// Where Factory keeps a session it started: beside the project's Factory state, by the session's id.
fn session_file(state: &Path, session: &str) -> PathBuf {
    state
        .parent()
        .unwrap_or(Path::new("."))
        .join("sessions")
        .join(format!("{}.session.json", session_tail(session)))
}

fn keep_session(state: &Path, session: &str, kept: &Value) -> Result<(), CliError> {
    let file = session_file(state, session);
    std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")))
        .map_err(CliError::from_native)?;
    let staged = file.with_extension("json.tmp");
    std::fs::write(
        &staged,
        serde_json::to_vec_pretty(kept).map_err(CliError::from_native)?,
    )
    .and_then(|()| std::fs::rename(&staged, &file))
    .map_err(CliError::from_native)
}

/// Factory's record of a session it started, when it keeps one.
fn kept_session(state: &Path, session: &str) -> Option<Value> {
    std::fs::read(session_file(state, session))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// `factory session list <state>`: the sessions Factory started in this state's project, each as Factory keeps it — open or
/// ended, with the tasks written on it.
fn session_list(state_text: &str) -> Result<Value, CliError> {
    let state = PathBuf::from(state_text);
    let folder = state.parent().unwrap_or(Path::new(".")).join("sessions");
    let mut sessions: Vec<Value> = match std::fs::read_dir(&folder) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(".session.json"))
            })
            .filter_map(|path| std::fs::read(path).ok())
            .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .filter(|kept| kept["contract"] == SESSION_CONTRACT)
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(CliError::from_native(error)),
    };
    sessions.sort_by(|a, b| {
        a["opened_at"]
            .as_str()
            .cmp(&b["opened_at"].as_str())
            .then_with(|| a["session"].as_str().cmp(&b["session"].as_str()))
    });
    Ok(json!({"contract": SESSION_LIST_CONTRACT, "factory_state": state, "sessions": sessions}))
}

/// Where the session runs: AIKit's own conversation of it, in the window's terminal, or attached from a terminal outside it.
fn terminal(checkout: &Path, session: &str) -> Value {
    json!({
        "cwd": checkout,
        "in_window": {"agent_session": session},
        "outside": [aikit_bin(), "session-space", "encounter", "--request-json",
            json!({"action": "view", "agent_session": session}).to_string()],
    })
}

/// `factory session end <world> <project> <session>`: the session's record closed by AIKit's own hook.
fn end(world: &str, project: &str, session: &str, agent: Option<&str>) -> Result<Value, CliError> {
    let place = place(world, project)?;
    let status = encounter(
        &place.checkout,
        json!({"action": "status", "agent_session": session}),
    )
    .map_err(refuse)?;
    let agent = agent
        .map(str::to_owned)
        .or_else(|| provider_of(&status))
        .unwrap_or_else(|| "claude-code".into());
    // A turn still running is stopped first: an ended session runs nothing.
    let stopped = encounter(
        &place.checkout,
        json!({"action": "cancel", "agent_session": session}),
    )
    .ok();
    // Then its body is released: AIKit releases only an idle, exactly identified native body (the session's native id and
    // connection generation as its status reads them), keeping the session's history. A body still settling from the stopped
    // turn is asked again for a few seconds; a refusal stands in AIKit's words.
    let mut released = Err("AIKit's status names no native body to release".to_owned());
    for _ in 0..10 {
        let now = encounter(
            &place.checkout,
            json!({"action": "status", "agent_session": session}),
        )
        .map_err(refuse)?;
        let (Some(native), Some(generation)) = (
            now["native_session_id"].as_str(),
            now["connection_generation"].as_str(),
        ) else {
            break;
        };
        released = encounter(
            &place.checkout,
            json!({"action": "release-native", "agent_session": session,
                "expected_native_session_id": native, "expected_generation": generation}),
        );
        if released.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let record = record(&place.checkout, &agent, "SessionEnd", session);
    if let Some(mut kept) = kept_session(&place.state, session) {
        kept["state"] = json!("ended");
        kept["ended_at"] = json!(now_rfc3339());
        keep_session(&place.state, session, &kept)?;
    }
    Ok(json!({
        "contract": SESSION_CONTRACT,
        "state": "ended",
        "session": session,
        "agent": agent,
        "world": place.world,
        "project": place.project,
        "record": record,
        "turn_stopped": stopped.is_some(),
        "released": match &released {
            Ok(_) => json!({"released": true}),
            Err(words) => json!({"released": false, "words": words}),
        },
    }))
}

/// The provider AIKit runs the session's body on, as its status names it (`provider.id`).
fn provider_of(status: &Value) -> Option<String> {
    status["provider"]["id"]
        .as_str()
        .or_else(|| status["provider"].as_str())
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
}

/// The last segment of a session's ref (`agent-session/<id>` → `<id>`), for the names a task's source carries.
fn session_tail(session: &str) -> String {
    session
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(session)
        .to_ascii_lowercase()
}

/// The first string under `key` anywhere in an owner's answer.
fn find_text(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
            .or_else(|| map.values().find_map(|inner| find_text(inner, key))),
        Value::Array(items) => items.iter().find_map(|inner| find_text(inner, key)),
        _ => None,
    }
}

/// `factory session task-write <world> <project> <session> <instruction>`: a task written on the session — a Run of one
/// unit commissioned in the project's Factory state with its workflow source, and its attempt field.
fn task_write(
    world: &str,
    project: &str,
    session: &str,
    instruction: &str,
) -> Result<Value, CliError> {
    let instruction = instruction.trim();
    if instruction.is_empty() || instruction.len() > 64 * 1024 {
        return Err(refuse("a task is written in words (at most 64 KiB)"));
    }
    let place = place(world, project)?;
    let held = binding(&place.checkout, session)?;
    let headline: String = instruction
        .lines()
        .next()
        .unwrap_or(instruction)
        .chars()
        .take(160)
        .collect();
    // Each task is its own commission: two written in the same second with the same words stay two.
    let task = ulid::Ulid::new().to_string().to_ascii_lowercase();
    let draft = crate::commission::FactoryCommissionDraft {
        contract: crate::commission::FACTORY_COMMISSION_DRAFT.into(),
        request_ref: Some(format!("commission-request:session-task-{task}")),
        project_key: place.project_id.clone(),
        purpose: headline.clone(),
        frontier: format!("Written on the session {session}; not yet run"),
        run_destination: format!("{}/sessions", place.project),
        commissioned_at: None,
        central_composition: None,
        root_act: crate::commission::FactoryRootActDraft {
            act_ref: Some(format!("act:session-task-{task}")),
            agent_ref: held.agent_ref.clone(),
            purpose: "Run the task the person wrote on the session".into(),
            scope_refs: vec![format!("project:{}", place.project_id)],
        },
        participant_requirements: vec![crate::commission::FactoryParticipantRequirement {
            reference: held.agent_ref.clone(),
            description: "The agent seated on the session's track".into(),
            source_owner: "aikit".into(),
            source_ref: held.source_ref.clone(),
            source_revision: held.source_revision.clone(),
        }],
    };
    let composition =
        crate::commission::compose_commission(draft, std::time::SystemTime::now().into())
            .map_err(CliError::from_native)?;
    let source = workflow_source(&place, &held, &composition.run_ref, session, instruction)?;
    let receipt = crate::developmental_read::FactoryDevelopmentalFileProvider::commission_workflow(
        &place.state,
        composition.request.clone(),
        source.clone(),
    )
    .map_err(CliError::from_native)?;
    let store = FileAttemptStore::attach(
        &place.state,
        receipt.commission.run_ref.clone(),
        &source.source.reference.to_string(),
    )
    .map_err(CliError::from_native)?;
    let reading = store.reading().map_err(CliError::from_native)?;
    let unit = reading
        .legs
        .keys()
        .next()
        .map(ToString::to_string)
        .unwrap_or_default();
    // The project is Factory's own (placed at the session's start; a session Factory did not start places it here), and the task
    // is kept on the session's record, so the window lays it as a clip on the session's track before it runs.
    placed(&place)?;
    if let Some(mut kept) = kept_session(&place.state, session) {
        if let Some(tasks) = kept["tasks"].as_array_mut() {
            tasks.push(
                json!({"run_ref": receipt.commission.run_ref.to_string(), "unit_ref": unit,
                "task": headline, "written_at": now_rfc3339()}),
            );
        }
        keep_session(&place.state, session, &kept)?;
    }
    Ok(json!({
        "contract": TASK_CONTRACT,
        "state": "written",
        "factory_state": place.state,
        "run_ref": receipt.commission.run_ref.to_string(),
        "unit_ref": unit,
        "session": session,
        "task": headline,
        "journey_ref": composition.journey_ref.to_string(),
        "revision": reading.revision,
    }))
}

/// The practice the seated agent carries (manual §19.7, §19.8): the skills of the Skill Sets AIKit gives the session's harness
/// (`aikit client status`: the row whose harness or client is the session's provider, its `skill_sets`), each as AIKit projects
/// it here (`aikit set show <set>`: `projected`). A task demands that practice of its unit, and AIKit resolves it again when the
/// task runs. An agent that carries none is refused in words: Factory's workflow law names a unit's practice.
fn practice_of(cwd: &Path, provider: &str) -> Result<Vec<String>, CliError> {
    let status = aikit(cwd, &["--json", "client", "status"]).map_err(refuse)?;
    let row = status["clients"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["harness"] == provider || row["client"] == provider)
        .ok_or_else(|| {
            refuse(format!(
                "AIKit's client status names no client for {provider}"
            ))
        })?;
    let mut practice = BTreeSet::new();
    for set in row["skill_sets"].as_array().into_iter().flatten() {
        let Some(set) = set.as_str() else { continue };
        let shown = aikit(cwd, &["--json", "set", "show", set]).map_err(refuse)?;
        practice.extend(
            shown["projected"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    if practice.is_empty() {
        return Err(refuse(format!(
            "{provider} carries no practice here: a task names the practice its agent carries, and AIKit projects no skill of {provider}'s Skill Sets ({}) in this checkout. Give the agent a Skill Set with a skill it may use (Skillset rack; `aikit set add`, `aikit enable`)",
            row["skill_sets"]
        )));
    }
    Ok(practice.into_iter().collect())
}

/// The task's one unit: the person's words as the difference the agent is asked to make in the session's checkout, its effect
/// a proposal there (manual §16.6: unarmed, every write is a proposal; Git / landing lands it), its Return the difference; its
/// practice the agent's (`practice_of`), its capabilities reading and writing the checkout's source.
fn workflow_source(
    place: &Place,
    held: &Binding,
    run_ref: &RunRef,
    session: &str,
    instruction: &str,
) -> Result<crate::workflow::WorkflowSource, CliError> {
    let status = encounter(
        &place.checkout,
        json!({"action": "status", "agent_session": session}),
    )
    .map_err(refuse)?;
    let provider = provider_of(&status)
        .ok_or_else(|| refuse(format!("AIKit's status of {session} names no provider")))?;
    let practice = practice_of(&place.checkout, &provider)?;
    let id = ulid::Ulid::new();
    let today = now_rfc3339().chars().take(10).collect::<String>();
    let key = format!("task-{}", id.to_string().to_ascii_lowercase());
    let mut source = json!({
        "schemaVersion": "factory.agent-workflow-source/v1",
        "coordinationContract": "factory.bounded-coordination/v1",
        "source": {
            "ref": format!("workflow-source:{id}"),
            "revision": "session-task-v1",
            "digest": "0".repeat(64),
            "temporalRef": format!("flow-time:{today}"),
            "flowRef": format!("flow:session-{}", session_tail(session)),
        },
        "workflowKey": key,
        "units": [{
            "key": "task",
            "developmentalConcern": format!("The task written on the session {session} for {run_ref}"),
            "requiredDifference": instruction,
            "returnContract": "Return the difference the turn left in the session's checkout, read back from git",
            "subjectRef": format!("checkout:{}", place.project_id),
            "basisRevision": head_of(&place.checkout).unwrap_or_else(|| "unborn".into()),
            "agentRequirements": {
                "agentRefs": [held.agent_ref],
                "agentSetRefs": [],
                "agencyRefs": [held.agency_ref]
            },
            "praxisRefs": practice,
            "capabilityRefs": ["capability/source-read", "capability/source-write"],
            "permittedEffects": [format!("propose:the difference in {}", place.checkout.display())],
            "verificationObligations": ["the difference in the session's checkout is read back from git"],
            "returnAddress": format!("return:session-{}", session_tail(session)),
            "stopConditions": "Stop when the person stops the task, or the session's turn ends",
            "escalationConditions": "Ask the person before acting outside the session's checkout"
        }],
        "barriers": []
    });
    let draft: crate::workflow::WorkflowSource =
        serde_json::from_value(source.clone()).map_err(CliError::from_native)?;
    source["source"]["digest"] = json!(crate::workflow::workflow_source_digest(&draft)
        .map_err(|error| refuse(format!("{error:?}")))?);
    serde_json::from_value(source).map_err(CliError::from_native)
}

fn git(checkout: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn head_of(checkout: &Path) -> Option<String> {
    git(checkout, &["rev-parse", "HEAD"]).map(|head| head.trim().to_owned())
}

/// The person's grant for one of Factory's host-checked acts (Central `authority::HOST_AUTHENTICATED_ACTIONS`), resolved here
/// against the World's native action authority contract with the token the host handed on the person's press
/// (`CENTRAL_NATIVE_TOKEN`), at the scope of the project the Run stands in. The token is compared, never kept or echoed.
fn grant(state: &Path, action: &str) -> Result<ProjectedFactoryActionAuthority, CliError> {
    let token = std::env::var("CENTRAL_NATIVE_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| {
            refuse(format!(
                "{action} presents the person's grant: the host hands Factory its token (CENTRAL_NATIVE_TOKEN) on the person's press, and none was handed"
            ))
        })?;
    let contract_path = state
        .ancestors()
        .map(|dir| dir.join("Control/user/native-action-authority.json"))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            refuse("no native action authority contract stands above this Factory state, so the person's grant cannot be read")
        })?;
    let contract: Value = std::fs::read(&contract_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| refuse(format!("{} cannot be read", contract_path.display())))?;
    if contract["schema"] != "central.native-action-authority/v1" {
        return Err(refuse(format!(
            "{} is not central.native-action-authority/v1",
            contract_path.display()
        )));
    }
    let scope = crate::attempt_admission::run_scope(state).map_err(refuse)?;
    use sha2::{Digest, Sha256};
    let presented: String = Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let held: Vec<&Value> = contract["grants"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["actor_kind"] == "human"
                && entry["token_sha256"]
                    .as_str()
                    .is_some_and(|recorded| recorded.eq_ignore_ascii_case(&presented))
        })
        .collect();
    let [entry] = held.as_slice() else {
        return Err(refuse(format!(
            "the person's token names {} human grants in the World's contract, not one",
            held.len()
        )));
    };
    if !entry["actions"]
        .as_array()
        .is_some_and(|actions| actions.iter().any(|listed| listed == action))
    {
        return Err(refuse(format!(
            "action not granted: the person's grant does not list {action}"
        )));
    }
    if !entry["scope_refs"]
        .as_array()
        .is_some_and(|scopes| scopes.iter().any(|listed| listed == scope.as_str()))
    {
        return Err(refuse(format!(
            "scope not granted: the person's grant does not cover {scope}"
        )));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    if entry["expires_at_unix_seconds"]
        .as_u64()
        .is_none_or(|expires| now >= expires)
    {
        return Err(refuse("expired: the person's grant has expired"));
    }
    let principal = entry["principal_ref"]
        .as_str()
        .ok_or_else(|| refuse("the person's grant names no principal"))?;
    Ok(authority(&format!(
        "central:native-action-authority#{principal}"
    )))
}

fn authority(reference: &str) -> ProjectedFactoryActionAuthority {
    ProjectedFactoryActionAuthority {
        authority_ref: reference.to_owned(),
        native_owner: crate::build::FACTORY_NATIVE_OWNER.into(),
        capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
        capability_granted: true,
        action_authorised: true,
    }
}

/// The person's press through the window: the desktop projection, ending at the sender the session hears (AIKit's binding
/// names the World's grant holder, e.g. `human:owner`), so the words Factory sends are the person's, attributed.
fn caller(sender: &str) -> FactoryActionCaller {
    FactoryActionCaller {
        caller_ref: sender.to_owned(),
        projection_kind: FactoryActionProjectionKind::DesktopHuman,
        lineage: vec![
            crate::attempt_admission::DESKTOP_CALLER_REF.into(),
            sender.to_owned(),
        ],
    }
}

/// Factory's follower, continuing the person's press: it records what AIKit reports, and sends nothing.
fn follower(sender: Option<&str>) -> FactoryActionCaller {
    let mut lineage = vec![crate::attempt_admission::DESKTOP_CALLER_REF.to_owned()];
    lineage.extend(sender.map(str::to_owned));
    lineage.push(FOLLOWER.to_owned());
    FactoryActionCaller {
        caller_ref: FOLLOWER.into(),
        projection_kind: FactoryActionProjectionKind::Headless,
        lineage,
    }
}

/// One write to the Run's attempts, decided on the Run as it stands now: nothing when the Run holds it already.
fn act(
    state: &Path,
    run_ref: &RunRef,
    by: &FactoryActionCaller,
    authority: &ProjectedFactoryActionAuthority,
    operation: FactoryAttemptOperation,
) -> Result<Option<FactoryAttemptActionReceipt>, CliError> {
    act_on(
        state,
        run_ref,
        by,
        authority,
        reading(state, run_ref)?,
        |now| (!holds(now, &operation)).then(|| operation.clone()),
    )
}

/// The most readings a write is decided on. Stop and Factory's follower are an attempt's only writers, so a second reading
/// settles any race between them; a Run that moves under every reading is refused rather than written blind.
const READINGS: usize = 4;

/// One write to the Run's attempts, decided on the Run as `read` holds it: `decide` names what its writer's intent asks of that
/// reading, or nothing when the reading holds the intent already or no longer admits it. Factory's follower and the person's
/// Stop both write an attempt, so the Run can move between a writer's reading and its write, and Factory refuses the write
/// (`RevisionConflict`): the writer then reads the Run again and decides again. An intent is so never applied twice, and one
/// the Run no longer admits is never forced on it.
fn act_on(
    state: &Path,
    run_ref: &RunRef,
    by: &FactoryActionCaller,
    authority: &ProjectedFactoryActionAuthority,
    mut read: FactoryAttemptReading,
    decide: impl Fn(&FactoryAttemptReading) -> Option<FactoryAttemptOperation>,
) -> Result<Option<FactoryAttemptActionReceipt>, CliError> {
    let projection_ref = minted("projection");
    for _ in 0..READINGS {
        let Some(operation) = decide(&read) else {
            return Ok(None);
        };
        let mut store =
            FileAttemptStore::open_run(state, run_ref.clone()).map_err(CliError::from_native)?;
        match store.apply(FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: projection_ref.clone(),
            caller: by.clone(),
            run_ref: run_ref.clone(),
            expected_revision: read.revision,
            authority: authority.clone(),
            operation,
        }) {
            Err(FactoryAttemptError::RevisionConflict { .. }) => read = reading(state, run_ref)?,
            landed => return landed.map(Some).map_err(CliError::from_native),
        }
    }
    Err(refuse(format!(
        "{run_ref} moved under each of the {READINGS} readings its write was decided on: Factory wrote nothing"
    )))
}

/// Whether the Run already holds what an operation asks of its attempt: the attempt started, the fact or the pause kept, the
/// cancellation requested or accepted, the failure, the verification or the Return recorded.
fn holds(reading: &FactoryAttemptReading, operation: &FactoryAttemptOperation) -> bool {
    let attempt = |reference: &str| {
        reading
            .attempts
            .iter()
            .find(|record| record.attempt_ref == reference)
    };
    let status = |reference: &str| attempt(reference).map(|record| leg_status(reading, record));
    match operation {
        FactoryAttemptOperation::StartSerial { attempt_ref, .. }
        | FactoryAttemptOperation::Retry { attempt_ref, .. } => attempt(attempt_ref).is_some(),
        FactoryAttemptOperation::RecordTracking { attempt_ref, fact } => attempt(attempt_ref)
            .is_some_and(|record| {
                record
                    .tracking
                    .iter()
                    .any(|kept| kept.fact_ref == fact.fact_ref)
                    || (fact.evidence_refs.contains(PAUSE) && paused(record))
            }),
        FactoryAttemptOperation::RecordVerification {
            attempt_ref,
            verification,
        } => attempt(attempt_ref).is_some_and(|record| {
            record
                .verifications
                .iter()
                .any(|kept| kept.verification_ref == verification.verification_ref)
        }),
        FactoryAttemptOperation::RequestCancellation { attempt_ref } => matches!(
            status(attempt_ref),
            Some(LegStatus::CancelRequested | LegStatus::CancellationAccepted)
        ),
        FactoryAttemptOperation::AcceptCancellation { attempt_ref } => {
            status(attempt_ref) == Some(LegStatus::CancellationAccepted)
        }
        FactoryAttemptOperation::Fail { attempt_ref, .. } => {
            status(attempt_ref) == Some(LegStatus::Failed)
        }
        FactoryAttemptOperation::ReturnArtifact { attempt_ref, .. } => {
            attempt(attempt_ref).is_some_and(|record| record.readable_return.is_some())
        }
        _ => false,
    }
}

fn reading(state: &Path, run_ref: &RunRef) -> Result<FactoryAttemptReading, CliError> {
    FileAttemptStore::open_run(state, run_ref.clone())
        .and_then(|store| store.reading())
        .map_err(CliError::from_native)
}

fn run_ref_of(text: &str) -> Result<RunRef, CliError> {
    text.parse::<RunRef>()
        .map_err(|error| refuse(format!("{text} is no Run ref: {error}")))
}

/// Where a task's log and difference are kept: beside the project's Factory state.
fn task_file(state: &Path, attempt_ref: &str, suffix: &str) -> PathBuf {
    let name = attempt_ref.replace([':', '/'], "-");
    state
        .parent()
        .unwrap_or(Path::new("."))
        .join("sessions")
        .join(format!("{name}.{suffix}"))
}

/// `factory session task-run <state> <run-ref> <session>`: the task started on the session under the person's grant, its
/// words sent to the agent, and its follower started.
/// One of Central's Actions in the World (`ctrl --json --root <World> action run <id> <input>`), with the person's token the
/// host handed on the press: its data, or its refusal in its words.
fn central(place: &Place, action: &str, input: Value) -> Result<Value, CliError> {
    let output = Command::new(executable("OI_CENTRAL_CTRL_BIN", "ctrl"))
        .args(["--json", "--root"])
        .arg(&place.world)
        .args(["action", "run", action, &input.to_string()])
        .current_dir(&place.world)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| {
            refuse(format!(
                "Central's ctrl could not be asked {action}: {error}"
            ))
        })?;
    let answer: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    if !output.status.success() || answer["ok"] == false {
        let words = answer["error"]["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| clip(&String::from_utf8_lossy(&output.stderr)));
        return Err(refuse(format!("Central refused {action}: {words}")));
    }
    Ok(answer.get("data").cloned().unwrap_or(answer))
}

/// What a task's practice is resolved against: a NOW for the task (Central's `central.now.allocate`, under the work policy's
/// revision `central.work.policy` reads), and AIKit's preparation of the agent's view of it (`aikit now-context prepare`, on the
/// World's NOW service its own lifecycle reports), which resolves the unit's praxisRefs against what AIKit catalogues here. The
/// praxis receipt is read back from the published view (`now-context inspect`), never derived here: Factory's admission law
/// demands it of every unit that names a practice. Answers the NOW's ref and the receipt.
#[allow(clippy::too_many_arguments)]
fn prepare_now(
    place: &Place,
    state: &Path,
    run_ref: &RunRef,
    unit: &crate::workflow::CompiledWorkflowUnit,
    task_ref: &str,
    held: &Binding,
    session: &str,
) -> Result<(String, Value), CliError> {
    let service = aikit(
        &place.checkout,
        &["--json", "now-context", "service", "status"],
    )
    .map_err(refuse)?;
    if service["state"] != "running" {
        return Err(refuse(format!(
            "AIKit's NOW service stands {} in this World: a task's practice is resolved against the NOW AIKit prepares for it, so the service runs first (`aikit now-context service start`)",
            service["state"]
        )));
    }
    let election = service["election_file"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| refuse("AIKit's NOW service names no election document"))?;
    let redis: Value = std::fs::read(&election)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| {
            refuse(format!(
                "AIKit's NOW election {} is not readable",
                election.display()
            ))
        })?;
    let policy = central(
        place,
        "central.work.policy",
        json!({"project": place.project}),
    )?;
    let policy_revision = policy["revision"]
        .as_str()
        .ok_or_else(|| refuse("Central's work policy names no revision"))?;
    let concern: String = unit.required_difference.chars().take(400).collect();
    let allocated = central(
        place,
        "central.now.allocate",
        json!({
            "project": place.project,
            "task_ref": task_ref,
            "purpose": concern,
            "expected_policy_revision": policy_revision,
            "participant_refs": [held.agent_ref],
        }),
    )?;
    let now_ref = allocated["now_ref"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| refuse("Central allocated no NOW ref for the task"))?;
    let election_text = election.to_string_lossy().into_owned();
    let inspected = aikit(
        &place.checkout,
        &[
            "--json",
            "now-context",
            "inspect",
            "--config-file",
            &election_text,
            "--participant-ref",
            &held.agent_ref,
        ],
    )
    .map_err(refuse)?;
    let request = json!({
        "schema": "aikit.now-preparation-request/v1",
        "redis": redis,
        "project_ref": format!("project:{}", place.project_id),
        "now_ref": now_ref,
        "participant_ref": held.agent_ref,
        "agent_session": session,
        "concern": concern,
        "disclosure_revision": task_ref,
        "practice_refs": unit.praxis_refs,
        "central": {"root": place.world, "project": place.project, "ctrl_bin": executable("OI_CENTRAL_CTRL_BIN", "ctrl")},
        "factory": {
            "state": state,
            "run_ref": run_ref.to_string(),
            "factory_bin": std::env::current_exe().ok(),
            "workflow_unit_refs": [unit.reference.to_string()],
        },
        "expected_version": inspected["currentVersion"].as_u64().unwrap_or(0),
        "selection": {"mode": "all"},
    });
    let request_file = task_file(state, task_ref, "now-request.json");
    if let Some(dir) = request_file.parent() {
        std::fs::create_dir_all(dir).map_err(|error| refuse(error.to_string()))?;
    }
    std::fs::write(&request_file, request.to_string())
        .map_err(|error| refuse(error.to_string()))?;
    let request_text = request_file.to_string_lossy().into_owned();
    aikit(
        &place.checkout,
        &[
            "--json",
            "now-context",
            "prepare",
            "--request-file",
            &request_text,
        ],
    )
    .map_err(|words| refuse(format!("AIKit did not prepare the task's NOW: {words}")))?;
    let binary = aikit_bin();
    let receipt = crate::attempt_praxis_receipt::inspect_admission_praxis_receipt(
        &crate::attempt_praxis_receipt::NowContextInspect {
            binary: &binary,
            cwd: &place.checkout,
            transport: None,
            config_file: &election,
            participant_ref: &held.agent_ref,
            timeout: AIKIT_TIMEOUT,
        },
    )
    .map_err(refuse)?
    .ok_or_else(|| {
        refuse("AIKit's prepared NOW carries no praxis receipt for the task's practice")
    })?;
    Ok((now_ref, receipt))
}

fn task_run(state_text: &str, run_text: &str, session: &str) -> Result<Value, CliError> {
    let state = PathBuf::from(state_text);
    let run_ref = run_ref_of(run_text)?;
    let place = place_of_state(&state)?;
    let authority = grant(&state, "factory.attempt.launch")?;
    let store =
        FileAttemptStore::open_run(&state, run_ref.clone()).map_err(CliError::from_native)?;
    let before = store.reading().map_err(CliError::from_native)?;
    let unit_ref: WorkflowUnitRef = before
        .legs
        .keys()
        .next()
        .cloned()
        .or_else(|| before.required_units.iter().next().cloned())
        .ok_or_else(|| refuse(format!("{run_ref} holds no unit to run")))?;
    if before
        .attempts
        .iter()
        .any(|record| record.workflow_unit_ref == unit_ref)
    {
        return Err(refuse(format!(
            "{run_ref}'s task has run already: its attempts are read with factory.attempt.list, and a stopped task is run again as a new task"
        )));
    }
    let basis = store
        .launch_basis(&unit_ref)
        .map_err(CliError::from_native)?;
    let journey = basis.journey_ref.clone().ok_or_else(|| {
        refuse(format!(
            "Factory links {run_ref} to no Journey: a task runs under the Journey it was written in"
        ))
    })?;
    let held = binding(&place.checkout, session)?;
    let status = encounter(
        &place.checkout,
        json!({"action": "status", "agent_session": session}),
    )
    .map_err(refuse)?;
    let space = space_of(&place.checkout, session)
        .ok_or_else(|| refuse(format!("AIKit places {session} in no SessionSpace")))?;
    let provider = provider_of(&status).unwrap_or_else(|| "aikit-encounter".into());
    let model = status["model_observation"]["current_model_id"]
        .as_str()
        .filter(|model| !model.trim().is_empty())
        .map(|model| {
            let model = model.to_owned();
            if model.starts_with("model:") {
                model
            } else {
                format!("model:{provider}/{model}")
            }
        })
        .unwrap_or_else(|| format!("model:{provider}/session-default"));
    let unit = &basis.unit;
    let attempt_ref = minted("attempt");
    let task_ref = minted("task");
    // The grant a retry of this task spends (factory.attempt.retry): its first attempt consumes it.
    let retry_grant = RetryGrant::new(format!("retry-grant:{task_ref}"), TASK_ATTEMPTS)
        .map_err(|error| refuse(format!("Factory refused the task's retry grant: {error:?}")))?;
    let (now_ref, receipt) =
        prepare_now(&place, &state, &run_ref, unit, &task_ref, &held, session)?;
    let demand = ExecutionDemand {
        project_ref: basis.run.project_ref().to_string(),
        run_ref: run_ref.to_string(),
        workflow_unit_ref: Some(unit.reference.to_string()),
        agency_ref: Some(held.agency_ref.clone()),
        profile_ref: None,
        use_type: unit.key.clone(),
        required_capabilities: unit.capability_refs.clone(),
        required_modalities: BTreeSet::new(),
        required_actions: BTreeSet::new(),
        required_tools: BTreeSet::new(),
        context_characteristics: BTreeSet::new(),
        independence_from: BTreeSet::new(),
        cost_ceiling_usd: None,
        latency_preference_ms: None,
        requires_local_materialisation: false,
    };
    let disposition = SituatedExecutionDisposition {
        selected_inputs: Vec::new(),
        selection: ExecutionDisposition {
            schema_version: EXECUTION_INTELLIGENCE_INTEROP_VERSION.into(),
            demand,
            selection: AikitModelRosterSelection {
                roster_version: SEATED_SELECTION.into(),
                model_ref: model.clone(),
                provider_ref: provider.clone(),
                ranking_policy: SEATED.into(),
                ranking_explanation: json!({
                    "schema": SEATED_SELECTION,
                    "standing": "the agent the person seated on the track, as AIKit opened its session; no roster ranked it",
                    "agent_session": session,
                    "status": status,
                }),
                provenance: vec![format!("aikit-encounter-status:{session}")],
            },
            decided_at: factory_admission_time(),
        },
        participant: SituatedParticipant {
            agent_ref: held.agent_ref.clone(),
            agency_ref: held.agency_ref.clone(),
            world_binding_ref: held.world_binding_ref.clone(),
            profile_ref: None,
            position_ref: None,
            source_ref: held.source_ref.clone(),
            source_revision: held.source_revision.clone(),
            source_digest: held.source_digest.clone(),
        },
        context_refs: BTreeSet::new(),
        praxis_refs: unit.praxis_refs.clone(),
        capability_refs: unit.capability_refs.clone(),
        praxis_receipt: Some(receipt),
        now_context_ref: Some(now_ref.clone()),
        body: ExecutionBody {
            model_ref: model,
            provider_ref: provider.clone(),
            route_ref: format!("route:aikit-encounter/{provider}"),
            harness_ref: format!("harness/{provider}"),
            harness_composition_ref: format!("harness-composition:aikit-encounter-open/{provider}"),
            agent_session_ref: session.to_owned(),
            session_space_ref: space.clone(),
            material_world_ref: None,
            workcell_ref: None,
        },
        placement: None,
        permitted_effects: unit.permitted_effects.clone(),
        verification_obligations: unit.verification_obligations.clone(),
        return_address: unit.return_address.clone(),
        stop_conditions: unit.stop_conditions.clone(),
        escalation_conditions: unit.escalation_conditions.clone(),
        budget: ExecutionBudget {
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            wall_clock_timeout_ms: None,
            retry_grant_ref: Some(retry_grant.grant_ref.clone()),
            maximum_attempts: Some(retry_grant.attempts_allowed),
        },
    };
    drop(store);
    act(
        &state,
        &run_ref,
        &caller(&held.sender),
        &authority,
        FactoryAttemptOperation::StartSerial {
            attempt_ref: attempt_ref.clone(),
            task_ref: task_ref.clone(),
            parent_journey_ref: journey,
            workflow_unit_ref: unit_ref.clone(),
            disposition,
            retry_grant: Some(retry_grant),
            tracking: Vec::new(),
            place_grant: None,
        },
    )?;
    let sent = send(
        &state,
        &run_ref,
        &attempt_ref,
        &authority,
        &place,
        &held,
        session,
        &basis.unit.required_difference,
    )?;
    let mut answer = json!({
        "contract": TASK_CONTRACT,
        "state": "running",
        "factory_state": state,
        "run_ref": run_ref.to_string(),
        "unit_ref": unit_ref.to_string(),
        "attempt_ref": attempt_ref,
        "task_ref": task_ref,
        "session": session,
        "space": space,
        "checkout": place.checkout,
    });
    merge(&mut answer, sent);
    Ok(answer)
}

/// Fields of `more` laid into `answer`.
fn merge(answer: &mut Value, more: Value) {
    if let (Some(into), Value::Object(from)) = (answer.as_object_mut(), more) {
        into.extend(from);
    }
}

/// An attempt's words sent to its session as the person's, and its follower started: the session's view and the checkout
/// read as the words go (the task's log streams what comes after them, its difference is read against them), the send
/// through Factory's agent client (attempt_owner_dispatch.rs), and the follower that records the turn's end. Answers the
/// execution, the delivery, the log, the follower and AIKit's observation of the send.
#[allow(clippy::too_many_arguments)]
fn send(
    state: &Path,
    run_ref: &RunRef,
    attempt_ref: &str,
    authority: &ProjectedFactoryActionAuthority,
    place: &Place,
    held: &Binding,
    session: &str,
    words: &str,
) -> Result<Value, CliError> {
    let started = reading(state, run_ref)?;
    let execution_ref = minted("execution");
    let delivery_ref = minted("delivery");
    // Where the session's view stands as the words go: the task's log streams what comes after it.
    let blocks_before = encounter(
        &place.checkout,
        json!({"action": "view", "agent_session": session}),
    )
    .ok()
    .and_then(|view| view["blocks"].as_array().map(Vec::len))
    .unwrap_or(0);
    // How the checkout stands as the words go: what the turn leaves is read against it.
    let before = task_file(state, attempt_ref, "before.json");
    std::fs::create_dir_all(before.parent().unwrap_or(Path::new(".")))
        .map_err(CliError::from_native)?;
    std::fs::write(
        &before,
        serde_json::to_vec(&standing(&place.checkout)).map_err(CliError::from_native)?,
    )
    .map_err(CliError::from_native)?;
    let sent = execute_attempt_owner_action(
        state,
        FactoryAttemptOwnerRequest {
            contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
            request_ref: minted("owner-request"),
            projection_ref: minted("projection"),
            caller: caller(&held.sender),
            run_ref: run_ref.clone(),
            expected_revision: started.revision,
            authority: authority.clone(),
            attempt_ref: attempt_ref.to_owned(),
            execution_ref: execution_ref.clone(),
            invocation: NativeOwnerInvocation::AikitEncounter {
                binary: aikit_bin(),
                cwd: place.checkout.clone(),
                transport: None,
                contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
                request: json!({
                    "action": "send",
                    "agent_session": session,
                    "turn": {
                        "delivery_ref": delivery_ref,
                        "sender": held.sender,
                        "expected_binding_revision": held.revision,
                        "packet": {
                            "text": words,
                            "source_refs": [],
                            "audience": [held.agent_ref],
                        },
                    },
                }),
            },
        },
    )
    .map_err(|error| {
        refuse(format!(
            "the task's words were not sent to the session: {error}"
        ))
    })?;
    let log = task_file(state, attempt_ref, "log.jsonl");
    std::fs::create_dir_all(log.parent().unwrap_or(Path::new(".")))
        .map_err(CliError::from_native)?;
    append(
        &log,
        &json!({"at": now_rfc3339(), "kind": "sent", "delivery_ref": delivery_ref,
            "text": words, "blocks_before": blocks_before}),
    );
    let follower = follow_in_background(state, run_ref, attempt_ref, &authority.authority_ref)?;
    Ok(json!({
        "execution_ref": execution_ref,
        "delivery_ref": delivery_ref,
        "log": log,
        "follower": follower,
        "sent": serde_json::to_value(&sent.transport_observation).unwrap_or(Value::Null),
    }))
}

/// The caller a desktop press acts as where no session hears it (Factory's admission, attempt_admission.rs `desktopCaller`).
fn desktop_caller() -> FactoryActionCaller {
    FactoryActionCaller {
        caller_ref: crate::attempt_admission::DESKTOP_CALLER_REF.into(),
        projection_kind: FactoryActionProjectionKind::DesktopHuman,
        lineage: vec![crate::attempt_admission::DESKTOP_CALLER_REF.into()],
    }
}

/// `factory session retry <state> <run-ref> <unit-ref>`: a unit's current attempt retried on the person's press, under their grant
/// (`factory.attempt.retry`, manual §19.1 "When a task on the track stalls, the track lights a retry. Retry starts again from the
/// last good step, on the same basis"). It is Factory's own `retry`: a new attempt under the retry grant the current one consumes,
/// with its task, its parent Journey and its disposition — the same basis; Factory refuses a unit that has not failed, or whose
/// grant is spent, in its own words. A task run on a session runs again there: its words sent to the session as the person's and
/// followed to its end, as `task-run` does. Its earlier attempt stays, a take of its own.
fn retry(state_text: &str, run_text: &str, unit_text: &str) -> Result<Value, CliError> {
    let state = PathBuf::from(state_text);
    // No grant, no move: the person's grant is read before anything else.
    let authority = grant(&state, "factory.attempt.retry")?;
    let run_ref = run_ref_of(run_text)?;
    let now = reading(&state, &run_ref)?;
    let (unit, leg) = now
        .legs
        .iter()
        .find(|(unit, _)| unit.to_string() == unit_text)
        .ok_or_else(|| refuse(format!("{run_ref} holds no unit {unit_text}")))?;
    let prior = now
        .attempts
        .iter()
        .find(|record| {
            &record.workflow_unit_ref == unit
                && record
                    .execution_ref
                    .as_deref()
                    .unwrap_or(&record.reserved_execution_ref)
                    == leg.execution_ref
        })
        .ok_or_else(|| refuse(format!("{unit_text} has no current attempt to retry")))?;
    let grant_ref = prior
        .disposition
        .budget
        .retry_grant_ref
        .clone()
        .ok_or_else(|| {
            refuse(format!(
                "{} carries no retry grant: Factory retries a unit only under the grant its attempt consumes, and this one was started with none",
                prior.attempt_ref
            ))
        })?;
    // A task Factory ran on a seated session is retried there; any other unit is retried as its Run's own.
    let seated = prior.disposition.selection.selection.ranking_policy == SEATED;
    let session = prior.disposition.body.agent_session_ref.clone();
    let on = if seated {
        let place = place_of_state(&state)?;
        let held = binding(&place.checkout, &session)?;
        Some((place, held))
    } else {
        None
    };
    let by = on
        .as_ref()
        .map_or_else(desktop_caller, |(_, held)| caller(&held.sender));
    let attempt_ref = minted("attempt");
    act(
        &state,
        &run_ref,
        &by,
        &authority,
        FactoryAttemptOperation::Retry {
            attempt_ref: attempt_ref.clone(),
            task_ref: prior.task_ref.clone(),
            parent_journey_ref: leg.delegation.parent_journey_ref.clone(),
            workflow_unit_ref: unit.clone(),
            grant_ref,
            disposition: prior.disposition.clone(),
            tracking: Vec::new(),
            reresolution: None,
            place_grant: None,
        },
    )?;
    let mut answer = json!({
        "contract": TASK_CONTRACT,
        "state": "retried",
        "factory_state": state,
        "run_ref": run_ref.to_string(),
        "unit_ref": unit.to_string(),
        "attempt_ref": attempt_ref,
        "prior_attempt_ref": prior.attempt_ref,
        "task_ref": prior.task_ref,
    });
    if let Some((place, held)) = on {
        let words = FileAttemptStore::open_run(&state, run_ref.clone())
            .and_then(|store| store.launch_basis(unit))
            .map_err(CliError::from_native)?
            .unit
            .required_difference;
        let sent = send(
            &state,
            &run_ref,
            &attempt_ref,
            &authority,
            &place,
            &held,
            &session,
            &words,
        )?;
        answer["state"] = json!("running");
        answer["session"] = json!(session);
        answer["checkout"] = json!(place.checkout);
        merge(&mut answer, sent);
    }
    Ok(answer)
}

/// `factory session choose <state> <run-ref> <attempt-ref>`: the attempt chosen to stand, on the person's press (manual §19.1
/// "Choosing a take that stands sends it to review in Approvals"; §19.14): its Return proposed to Central's receiving field under
/// the person's grant (`central.receiving.submit`), into the Central Flow its Run belongs to, by Factory's own receiving
/// (attempt_receiving.rs; the same choice twice is Factory's replay, never a second proposal). A Run that belongs to no Flow has
/// nowhere to receive a Return: a task written on a session's track is commissioned from the person's words with no Flow, so its
/// Return stays on its attempt, its difference in the session's checkout, which Git / landing lands. Factory refuses it in those
/// words.
fn choose(state_text: &str, run_text: &str, attempt_ref: &str) -> Result<Value, CliError> {
    let state = PathBuf::from(state_text);
    // No grant, no move: the person's grant is read before anything else.
    let authority = grant(&state, "central.receiving.submit")?;
    let run_ref = run_ref_of(run_text)?;
    let now = reading(&state, &run_ref)?;
    let record = record_of(&now, attempt_ref)?;
    let returned = record.readable_return.as_ref().ok_or_else(|| {
        refuse(format!(
            "{attempt_ref} has no Return yet: there is nothing to choose"
        ))
    })?;
    if let Some(receiving) = &returned.receiving_ref {
        return Ok(
            json!({"contract": TASK_CONTRACT, "state": "already-received",
            "run_ref": run_ref.to_string(), "attempt_ref": attempt_ref, "receiving_ref": receiving}),
        );
    }
    let run = crate::developmental_read::FactoryDevelopmentalFileProvider::open(&state)
        .and_then(|provider| provider.run_reading(&run_ref))
        .map_err(CliError::from_native)?;
    let flows: Vec<_> = run
        .flow_associations
        .iter()
        .filter(|association| association.run_ref == run_ref)
        .collect();
    let [association] = flows.as_slice() else {
        return Err(refuse(if flows.is_empty() {
            format!("{run_ref} belongs to no Central Flow, so its Return has nowhere to be received: a task written on a session's track is commissioned from your words, with no Flow, and its difference stays in the session's checkout, where Git / landing lands it")
        } else {
            format!("{run_ref} belongs to {} Central Flows, and Factory names no one of them to receive into", flows.len())
        }));
    };
    let flow = &association.flow;
    let central = executable("OI_CENTRAL_CTRL_BIN", "ctrl");
    if !central.is_absolute() {
        return Err(refuse(
            "Central's ctrl is not found (OI_CENTRAL_CTRL_BIN, or ctrl on PATH): the Return cannot reach Central's receiving field",
        ));
    }
    use sha2::{Digest, Sha256};
    let suffix: String = Sha256::digest(
        [
            run_ref.to_string().as_str(),
            attempt_ref,
            &flow.location.reference,
            &flow.document_id,
            &flow.source_revision,
        ]
        .join("\n")
        .as_bytes(),
    )
    .iter()
    .take(12)
    .map(|byte| format!("{byte:02x}"))
    .collect();
    let receipt = crate::attempt_receiving::execute(
        &state,
        crate::attempt_receiving::ReceivingRequest {
            contract: crate::attempt_receiving::RECEIVING_ACTION.into(),
            request_ref: format!("request:session-choose:{suffix}"),
            projection_ref: format!("projection:session-choose:{suffix}"),
            caller: desktop_caller(),
            run_ref: run_ref.clone(),
            expected_revision: now.revision,
            authority,
            attempt_ref: attempt_ref.to_owned(),
            central: crate::attempt_receiving::CentralReceivingEndpoint {
                binary: central,
                root: PathBuf::from(&flow.location.root),
                contract_revision: crate::attempt_receiving::CENTRAL_CONTRACT_REVISION.into(),
                project: None,
            },
            lookup_endpoint: None,
            target: crate::attempt_receiving::ReceivingTarget {
                source_ref: flow.location.reference.clone(),
                document_id: flow.document_id.clone(),
                source_revision: flow.source_revision.clone(),
                expected_authority_revision: None,
            },
            occurred_at_unix_seconds: None,
            recover: false,
        },
    )?;
    let unconfirmed =
        receipt["needsReconciliation"] == true || receipt["centralResponse"].is_null();
    Ok(json!({
        "contract": TASK_CONTRACT,
        "state": if unconfirmed { "needs-reconciliation" } else { "received" },
        "run_ref": run_ref.to_string(),
        "attempt_ref": attempt_ref,
        "receipt": receipt,
    }))
}

/// The SessionSpace that holds a session, read from AIKit's list.
fn space_of(cwd: &Path, session: &str) -> Option<String> {
    let listing = aikit(cwd, &["session-space", "list"]).ok()?;
    let spaces = listing
        .as_array()
        .cloned()
        .or_else(|| listing["spaces"].as_array().cloned())?;
    spaces.iter().find_map(|space| {
        space["agent_sessions"]
            .get(session)
            .and(space["definition"]["id"].as_str().map(str::to_owned))
    })
}

fn append(path: &Path, line: &Value) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// The follower, started apart from the press so the task runs to its end without the window: Factory's own process,
/// observing AIKit and recording on the attempt. It runs no agent.
fn follow_in_background(
    state: &Path,
    run_ref: &RunRef,
    attempt_ref: &str,
    authority_ref: &str,
) -> Result<Value, CliError> {
    let me = std::env::current_exe().map_err(CliError::from_native)?;
    let words = task_file(state, attempt_ref, "follow.log");
    let out = std::fs::File::create(&words).map_err(CliError::from_native)?;
    let err = out.try_clone().map_err(CliError::from_native)?;
    let mut command = Command::new(me);
    command
        .args(["session", "follow"])
        .arg(state)
        .arg(run_ref.to_string())
        .arg(attempt_ref)
        .arg(authority_ref)
        .env_remove("CENTRAL_NATIVE_TOKEN")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn().map_err(CliError::from_native)?;
    Ok(json!({"pid": child.id(), "words": words}))
}

fn record_of<'a>(
    reading: &'a FactoryAttemptReading,
    attempt_ref: &str,
) -> Result<&'a FactoryAttemptRecord, CliError> {
    reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt_ref)
        .ok_or_else(|| {
            refuse(format!(
                "Factory holds no attempt {attempt_ref} in this Run"
            ))
        })
}

fn leg_status(reading: &FactoryAttemptReading, record: &FactoryAttemptRecord) -> LegStatus {
    reading
        .legs
        .get(&record.workflow_unit_ref)
        .map(|leg| leg.status)
        .unwrap_or(LegStatus::Active)
}

fn paused(record: &FactoryAttemptRecord) -> bool {
    record
        .tracking
        .iter()
        .any(|fact| fact.evidence_refs.contains(PAUSE))
}

fn ended(status: &LegStatus) -> bool {
    !matches!(
        status,
        LegStatus::Active | LegStatus::Detached | LegStatus::CancelRequested
    )
}

/// What a turn that ended other than returned leaves to record on its attempt, decided on the Run as `reading` holds it: the
/// cancellation Stop asked for accepted, or the failure; nothing when the attempt has ended already or Stop paused it. Stop
/// records its pause or its request before it stops the turn, so a turn that ends cancelled on an attempt that is pausing or
/// being cancelled ended as the person asked, and is never the attempt's failure.
fn turn_end(
    reading: &FactoryAttemptReading,
    attempt_ref: &str,
    phase: &str,
    delivery_ref: &str,
) -> Option<FactoryAttemptOperation> {
    let record = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt_ref)?;
    let status = leg_status(reading, record);
    if record.readable_return.is_some() || ended(&status) {
        return None;
    }
    let attempt_ref = attempt_ref.to_owned();
    if phase == "cancelled" {
        if status == LegStatus::CancelRequested {
            return Some(FactoryAttemptOperation::AcceptCancellation { attempt_ref });
        }
        if paused(record) {
            return None;
        }
    }
    Some(FactoryAttemptOperation::Fail {
        attempt_ref,
        reason: format!("the session's turn ended {phase}"),
        evidence_refs: BTreeSet::from([format!("aikit-delivery:{delivery_ref}")]),
    })
}

/// The delivery's phase, as AIKit reports it (its answer to `delivery` carries the phase at its top).
fn delivery_phase(answer: &Value, delivery_ref: &str) -> Option<String> {
    answer["phase"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| phase_within(answer, delivery_ref))
}

fn phase_within(answer: &Value, delivery_ref: &str) -> Option<String> {
    match answer {
        Value::Object(map) => {
            if map.get("delivery_ref").and_then(Value::as_str) == Some(delivery_ref) {
                if let Some(phase) = map.get("phase").and_then(Value::as_str) {
                    return Some(phase.to_owned());
                }
            }
            map.values()
                .find_map(|inner| phase_within(inner, delivery_ref))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|inner| phase_within(inner, delivery_ref)),
        _ => None,
    }
}

/// Every pending ask of the agent in an owner's answer: AIKit's permission requests (`native_request_id`, `choices`).
fn asks(answer: &Value, into: &mut Vec<Value>) {
    match answer {
        Value::Object(map) => {
            if map
                .get("native_request_id")
                .and_then(Value::as_str)
                .is_some()
                && map.get("choices").is_some_and(Value::is_array)
            {
                into.push(answer.clone());
                return;
            }
            for inner in map.values() {
                asks(inner, into);
            }
        }
        Value::Array(items) => {
            for inner in items {
                asks(inner, into);
            }
        }
        _ => {}
    }
}

fn paths_in(value: &Value, into: &mut Vec<String>) {
    match value {
        Value::String(text) if text.starts_with('/') && !text.contains('\n') => {
            into.push(text.clone())
        }
        Value::Object(map) => map.values().for_each(|inner| paths_in(inner, into)),
        Value::Array(items) => items.iter().for_each(|inner| paths_in(inner, into)),
        _ => {}
    }
}

/// The Scope's answer to one ask: allowed once when every path it names lies in the session's checkout, else refused once.
fn scope_answer(ask: &Value, checkout: &Path) -> Option<(String, &'static str)> {
    let mut paths = Vec::new();
    paths_in(&ask["tool_call"], &mut paths);
    let inside = paths
        .iter()
        .all(|path| Path::new(path).starts_with(checkout));
    let choices = ask["choices"].as_array()?;
    let pick = |wanted: &str| {
        choices.iter().find_map(|choice| {
            let kind = choice["kind"].as_str().unwrap_or_default();
            (kind == wanted).then(|| {
                choice["option_id"]
                    .as_str()
                    .or_else(|| choice["id"].as_str())
                    .map(str::to_owned)
            })?
        })
    };
    if inside {
        pick("allow_once")
            .or_else(|| pick("allow_always"))
            .map(|option| (option, "allowed: inside the session's checkout"))
    } else {
        pick("reject_once")
            .or_else(|| pick("reject_always"))
            .map(|option| (option, "refused: outside the session's checkout"))
    }
}

/// How the checkout stands, path by path, as git reads it (`git status --porcelain -z --untracked-files=all`): each changed or
/// untracked path with a digest of what it holds (a link's target, a file's bytes, or its absence).
fn standing(checkout: &Path) -> std::collections::BTreeMap<String, String> {
    let listed = git(
        checkout,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
    .unwrap_or_default();
    let mut entries = listed.split('\0').filter(|entry| entry.len() > 3);
    let mut held = std::collections::BTreeMap::new();
    while let Some(entry) = entries.next() {
        let (code, path) = entry.split_at(3);
        if code.starts_with('R') || code.starts_with('C') {
            entries.next();
        }
        let at = checkout.join(path);
        let digest = match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => std::fs::read_link(&at)
                .map(|target| format!("link:{}", target.display()))
                .unwrap_or_else(|_| "link:?".into()),
            Ok(meta) if meta.is_file() => std::fs::read(&at)
                .map(|bytes| format!("blake3:{}", blake3::hash(&bytes).to_hex()))
                .unwrap_or_else(|_| "unreadable".into()),
            Ok(_) => "other".into(),
            Err(_) => "absent".into(),
        };
        held.insert(path.to_owned(), digest);
    }
    held
}

/// The difference the turn left in the checkout, read from git: every path whose standing moved since the task was sent
/// (`before`, kept beside the task's log when it was sent), tracked paths against HEAD and new ones in whole. What stood there
/// already (the person's own edits, the seat's build pool link, the harness roots AIKit projects into a checkout) is never the
/// agent's.
fn difference(checkout: &Path, before: &std::collections::BTreeMap<String, String>) -> String {
    let mut diff = String::new();
    for (path, digest) in standing(checkout) {
        if before.get(&path) == Some(&digest) || pool_link(checkout, &path) {
            continue;
        }
        let tracked = git(checkout, &["ls-files", "--error-unmatch", "--", &path]).is_some();
        let args: Vec<&str> = if tracked {
            vec!["diff", "HEAD", "--no-color", "--", &path]
        } else {
            vec!["diff", "--no-color", "--no-index", "--", "/dev/null", &path]
        };
        // `--no-index` exits 1 when the files differ, so its answer is read whatever its status.
        if let Ok(output) = Command::new("git")
            .arg("-C")
            .arg(checkout)
            .args(&args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .stdin(Stdio::null())
            .output()
        {
            diff.push_str(&String::from_utf8_lossy(&output.stdout));
        }
    }
    diff
}

/// How the checkout stood when the task was sent, as kept beside its log (empty when none was kept).
fn standing_before(state: &Path, attempt_ref: &str) -> std::collections::BTreeMap<String, String> {
    std::fs::read(task_file(state, attempt_ref, "before.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The seat's build pool link: the `target` symlink the seat instrument puts at a joined checkout's root (one build tree
/// per product). It is the seat's, never a difference the agent made, as the instrument's own release reads it.
fn pool_link(checkout: &Path, file: &str) -> bool {
    file == "target"
        && std::fs::symlink_metadata(checkout.join(file))
            .is_ok_and(|meta| meta.file_type().is_symlink())
}

/// `factory session follow <state> <run-ref> <attempt-ref> <authority-ref>`: the task followed to its end.
fn follow(
    state_text: &str,
    run_text: &str,
    attempt_ref: &str,
    authority_ref: &str,
) -> Result<Value, CliError> {
    let state = PathBuf::from(state_text);
    let run_ref = run_ref_of(run_text)?;
    let place = place_of_state(&state)?;
    let authority = authority(authority_ref);
    let log = task_file(&state, attempt_ref, "log.jsonl");
    let first = reading(&state, &run_ref)?;
    let record = record_of(&first, attempt_ref)?.clone();
    let session = record.disposition.body.agent_session_ref.clone();
    let sender = binding(&place.checkout, &session)
        .ok()
        .map(|held| held.sender);
    let by = follower(sender.as_deref());
    let delivery_ref = record
        .dispatch
        .as_ref()
        .and_then(|dispatch| find_text(&dispatch.payload, "delivery_ref"))
        .or_else(|| {
            record
                .observations
                .iter()
                .find_map(|receipt| find_text(&receipt.payload, "deliveryRef"))
        })
        .ok_or_else(|| refuse(format!("{attempt_ref} names no delivery to follow")))?;
    let begun = Instant::now();
    // The view's blocks before the task's words went belong to earlier turns.
    let mut seen = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["kind"] == "sent")
        .and_then(|line| line["blocks_before"].as_u64())
        .unwrap_or(0) as usize;
    let mut answered: BTreeSet<String> = BTreeSet::new();
    loop {
        let now = reading(&state, &run_ref)?;
        let current = record_of(&now, attempt_ref)?;
        if current.readable_return.is_some() || ended(&leg_status(&now, current)) {
            return Ok(
                json!({"followed": attempt_ref, "ended": format!("{:?}", leg_status(&now, current))}),
            );
        }
        // The turn, streamed into the task's log as AIKit shows it.
        if let Ok(view) = encounter(
            &place.checkout,
            json!({"action": "view", "agent_session": session}),
        ) {
            let blocks = view["blocks"].as_array().cloned().unwrap_or_default();
            for block in blocks.iter().skip(seen) {
                append(
                    &log,
                    &json!({"at": now_rfc3339(), "kind": "turn", "block": block}),
                );
            }
            seen = seen.max(blocks.len());
            let mut pending = Vec::new();
            asks(&view, &mut pending);
            if let Ok(status) = encounter(
                &place.checkout,
                json!({"action": "status", "agent_session": session}),
            ) {
                asks(&status, &mut pending);
            }
            for ask in pending {
                let id = ask["native_request_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                if id.is_empty() || !answered.insert(id.clone()) {
                    continue;
                }
                let decision = match scope_answer(&ask, &place.checkout) {
                    Some((option, why)) => {
                        append(
                            &log,
                            &json!({"at": now_rfc3339(), "kind": "ask", "request_id": id, "answer": why}),
                        );
                        json!({"outcome": "selected", "option_id": option})
                    }
                    None => {
                        append(
                            &log,
                            &json!({"at": now_rfc3339(), "kind": "ask", "request_id": id,
                            "answer": "cancelled: the Scope finds no choice to give"}),
                        );
                        json!({"outcome": "cancelled"})
                    }
                };
                if let Err(words) = encounter(
                    &place.checkout,
                    json!({"action": "permission", "agent_session": session, "request_id": id, "decision": decision}),
                ) {
                    append(
                        &log,
                        &json!({"at": now_rfc3339(), "kind": "ask-unanswered", "request_id": id, "words": words}),
                    );
                }
            }
        }
        let phase = encounter(
            &place.checkout,
            json!({"action": "delivery", "agent_session": session, "delivery_ref": delivery_ref}),
        )
        .ok()
        .and_then(|answer| delivery_phase(&answer, &delivery_ref));
        if let Some(phase) =
            phase.filter(|phase| matches!(phase.as_str(), "returned" | "failed" | "cancelled"))
        {
            append(
                &log,
                &json!({"at": now_rfc3339(), "kind": "turn-ended", "phase": phase}),
            );
            return settle(
                &state,
                &run_ref,
                attempt_ref,
                &by,
                &authority,
                &place,
                &session,
                &delivery_ref,
                &phase,
                &log,
            );
        }
        if begun.elapsed() > FOLLOW_LIMIT {
            let reason = "the session's turn was not seen to end within two hours".to_owned();
            act(
                &state,
                &run_ref,
                &by,
                &authority,
                FactoryAttemptOperation::Fail {
                    attempt_ref: attempt_ref.to_owned(),
                    reason: reason.clone(),
                    evidence_refs: BTreeSet::from([format!("aikit-delivery:{delivery_ref}")]),
                },
            )?;
            return Ok(json!({"followed": attempt_ref, "ended": "Failed", "reason": reason}));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// The turn's end, recorded on the attempt: AIKit's delivery observed, then the difference returned, the pause kept, the
/// cancellation accepted, or the failure.
#[allow(clippy::too_many_arguments)]
fn settle(
    state: &Path,
    run_ref: &RunRef,
    attempt_ref: &str,
    by: &FactoryActionCaller,
    authority: &ProjectedFactoryActionAuthority,
    place: &Place,
    session: &str,
    delivery_ref: &str,
    phase: &str,
    log: &Path,
) -> Result<Value, CliError> {
    let now = reading(state, run_ref)?;
    let record = record_of(&now, attempt_ref)?.clone();
    let execution_ref = record
        .execution_ref
        .clone()
        .unwrap_or_else(|| record.reserved_execution_ref.clone());
    let observed = execute_attempt_owner_action(
        state,
        FactoryAttemptOwnerRequest {
            contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
            request_ref: minted("owner-request"),
            projection_ref: minted("projection"),
            caller: by.clone(),
            run_ref: run_ref.clone(),
            expected_revision: now.revision,
            authority: authority.clone(),
            attempt_ref: attempt_ref.to_owned(),
            execution_ref: execution_ref.clone(),
            invocation: NativeOwnerInvocation::AikitEncounter {
                binary: aikit_bin(),
                cwd: place.checkout.clone(),
                transport: None,
                contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
                request: json!({"action": "delivery", "agent_session": session, "delivery_ref": delivery_ref}),
            },
        },
    );
    if let Err(error) = &observed {
        append(
            log,
            &json!({"at": now_rfc3339(), "kind": "delivery-unrecorded", "words": error.to_string()}),
        );
    }
    let now = reading(state, run_ref)?;
    let record = record_of(&now, attempt_ref)?.clone();
    let status = leg_status(&now, &record);
    if record.readable_return.is_some() || ended(&status) {
        // Its end was recorded while the turn ended (Stop cancelled it): nothing is left to record.
        return Ok(json!({"followed": attempt_ref, "ended": format!("{status:?}")}));
    }
    if phase != "returned" {
        act_on(state, run_ref, by, authority, now, |now| {
            turn_end(now, attempt_ref, phase, delivery_ref)
        })?;
        let after = reading(state, run_ref)?;
        let record = record_of(&after, attempt_ref)?;
        let status = leg_status(&after, record);
        if status == LegStatus::Failed {
            let reason = format!("the session's turn ended {phase}");
            return Ok(json!({"followed": attempt_ref, "ended": "failed", "reason": reason}));
        }
        if paused(record) && !ended(&status) {
            append(log, &json!({"at": now_rfc3339(), "kind": "paused"}));
            return Ok(json!({"followed": attempt_ref, "ended": "paused"}));
        }
        let end = if status == LegStatus::CancellationAccepted {
            "cancelled".to_owned()
        } else {
            format!("{status:?}")
        };
        return Ok(json!({"followed": attempt_ref, "ended": end}));
    }
    let diff = difference(&place.checkout, &standing_before(state, attempt_ref));
    let digest = blake3::hash(diff.as_bytes()).to_hex().to_string();
    let kept = task_file(state, attempt_ref, "diff");
    std::fs::write(&kept, &diff).map_err(CliError::from_native)?;
    let evidence = BTreeSet::from([
        format!("factory-session-diff:blake3:{digest}"),
        format!("aikit-delivery:{delivery_ref}"),
    ]);
    let files: Vec<String> = diff
        .lines()
        .filter_map(|line| line.strip_prefix("+++ b/"))
        .map(str::to_owned)
        .collect();
    act(
        state,
        run_ref,
        by,
        authority,
        FactoryAttemptOperation::RecordVerification {
            attempt_ref: attempt_ref.to_owned(),
            verification: VerificationReceipt {
                verification_ref: minted("verification"),
                owner_ref: "factory/session".into(),
                source_revision: head_of(&place.checkout).unwrap_or_else(|| "unborn".into()),
                outcome: if files.is_empty() {
                    VerificationOutcome::Failed
                } else {
                    VerificationOutcome::Passed
                },
                obligations: record.disposition.verification_obligations.clone(),
                evidence_refs: evidence.clone(),
            },
        },
    )?;
    if files.is_empty() {
        let reason =
            "the session's turn returned and left no difference in its checkout".to_owned();
        act(
            state,
            run_ref,
            by,
            authority,
            FactoryAttemptOperation::Fail {
                attempt_ref: attempt_ref.to_owned(),
                reason: reason.clone(),
                evidence_refs: evidence,
            },
        )?;
        return Ok(json!({"followed": attempt_ref, "ended": "failed", "reason": reason}));
    }
    let said = said_since(&place.checkout, session);
    let artifact_ref = minted("artifact");
    let unit = FileAttemptStore::open_run(state, run_ref.clone())
        .and_then(|store| store.launch_basis(&record.workflow_unit_ref))
        .map_err(CliError::from_native)?
        .unit;
    act(
        state,
        run_ref,
        by,
        authority,
        FactoryAttemptOperation::ReturnArtifact {
            attempt_ref: attempt_ref.to_owned(),
            artifact: ReturnedArtifact {
                artifact_ref: artifact_ref.clone(),
                subject_ref: unit.subject_ref.to_string(),
                subject_revision: unit.basis_revision.clone(),
                producing_execution_ref: execution_ref,
                evidence_refs: evidence.clone(),
                semantic_difference: format!(
                    "{} in the session's checkout: {}",
                    plural(files.len()),
                    files.join(", ")
                ),
            },
            readable_return: ReadableReturn {
                return_ref: minted("return"),
                summary: if said.trim().is_empty() {
                    format!("Changed {}", files.join(", "))
                } else {
                    said.chars().take(2000).collect()
                },
                artifact_refs: BTreeSet::from([artifact_ref]),
                evidence_refs: evidence,
                receiving_ref: None,
                receiving_source_revision: None,
                archive_refs: BTreeSet::new(),
                regression_observation_refs: BTreeSet::new(),
            },
        },
    )?;
    append(
        log,
        &json!({"at": now_rfc3339(), "kind": "returned", "files": files, "diff": kept}),
    );
    Ok(json!({"followed": attempt_ref, "ended": "returned", "files": files, "diff": kept}))
}

fn plural(count: usize) -> String {
    if count == 1 {
        "1 file".into()
    } else {
        format!("{count} files")
    }
}

/// The agent's words in its session, as AIKit shows them.
fn said_since(checkout: &Path, session: &str) -> String {
    encounter(
        checkout,
        json!({"action": "view", "agent_session": session}),
    )
    .ok()
    .and_then(|view| view["blocks"].as_array().cloned())
    .map(|blocks| {
        blocks
            .iter()
            .filter(|block| block["kind"] == "assistant")
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("")
    })
    .unwrap_or_default()
}

/// `factory session task-stop <state> <run-ref> <attempt-ref>`: Stop pauses the task's turn; pressed again it cancels.
fn task_stop(state_text: &str, run_text: &str, attempt_ref: &str) -> Result<Value, CliError> {
    let state = PathBuf::from(state_text);
    let run_ref = run_ref_of(run_text)?;
    let place = place_of_state(&state)?;
    let authority = grant(&state, "factory.attempt.request-cancellation")?;
    let now = reading(&state, &run_ref)?;
    let record = record_of(&now, attempt_ref)?.clone();
    let status = leg_status(&now, &record);
    if record.readable_return.is_some() || ended(&status) {
        return Err(refuse(format!(
            "{attempt_ref} has ended ({status:?}): there is nothing to stop"
        )));
    }
    let session = record.disposition.body.agent_session_ref.clone();
    let by = binding(&place.checkout, &session)
        .map(|held| caller(&held.sender))
        .unwrap_or_else(|_| caller(crate::attempt_admission::DESKTOP_CALLER_REF));
    let log = task_file(&state, attempt_ref, "log.jsonl");
    // Stop records on the attempt why its turn stops before it stops the turn at AIKit (the session keeps what it had in mind),
    // so Factory's follower, finding the turn cancelled, finds the attempt paused or being cancelled and records no failure.
    let stop_turn = || {
        encounter(
            &place.checkout,
            json!({"action": "cancel", "agent_session": session}),
        )
    };
    if paused(&record) || status == LegStatus::CancelRequested {
        act(
            &state,
            &run_ref,
            &by,
            &authority,
            FactoryAttemptOperation::RequestCancellation {
                attempt_ref: attempt_ref.to_owned(),
            },
        )?;
        // The turn stops at AIKit either way.
        let _ = stop_turn();
        act(
            &state,
            &run_ref,
            &by,
            &authority,
            FactoryAttemptOperation::AcceptCancellation {
                attempt_ref: attempt_ref.to_owned(),
            },
        )?;
        append(&log, &json!({"at": now_rfc3339(), "kind": "cancelled"}));
        let after = reading(&state, &run_ref)?;
        let record = record_of(&after, attempt_ref)?;
        return Ok(
            json!({"contract": TASK_CONTRACT, "state": "cancelled", "attempt_ref": attempt_ref,
            "leg": format!("{:?}", leg_status(&after, record))}),
        );
    }
    // Pressed once: the task is paused, unless its attempt ended, or a press before this one paused it, since it was read.
    act_on(&state, &run_ref, &by, &authority, now, |now| {
        pause(now, attempt_ref, &session)
    })?;
    let after = reading(&state, &run_ref)?;
    let status = leg_status(&after, record_of(&after, attempt_ref)?);
    if ended(&status) {
        return Err(refuse(format!(
            "{attempt_ref} has ended ({status:?}): there is nothing to stop"
        )));
    }
    let cancelled = stop_turn();
    let words = match &cancelled {
        Ok(answer) => clip(&answer.to_string()),
        Err(words) => words.clone(),
    };
    append(
        &log,
        &json!({"at": now_rfc3339(), "kind": "paused", "aikit": words}),
    );
    Ok(
        json!({"contract": TASK_CONTRACT, "state": "paused", "attempt_ref": attempt_ref,
        "turn_cancelled_by_aikit": cancelled.is_ok(), "aikit": words,
        "next": "Stop again cancels the task"}),
    )
}

/// The pause a press of Stop records on the Run as `reading` holds it, the turn stopped at AIKit and the session keeping what it
/// had in mind: none once the attempt has ended, or a press before this one paused it.
fn pause(
    reading: &FactoryAttemptReading,
    attempt_ref: &str,
    session: &str,
) -> Option<FactoryAttemptOperation> {
    let record = reading
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt_ref)?;
    let status = leg_status(reading, record);
    if record.readable_return.is_some() || ended(&status) || paused(record) {
        return None;
    }
    Some(FactoryAttemptOperation::RecordTracking {
        attempt_ref: attempt_ref.to_owned(),
        fact: AttemptTrackingFact {
            fact_ref: minted("fact"),
            kind: "session".into(),
            owner_ref: "aikit".into(),
            subject_ref: session.to_owned(),
            source_revision: now_rfc3339(),
            evidence_refs: BTreeSet::from([
                PAUSE.to_owned(),
                format!("aikit-encounter-cancel:{session}"),
            ]),
        },
    })
}

/// `factory session task-log <state> <attempt-ref>`: the task's log as it streams.
/// `factory session task-log <state> <run-ref> <attempt-ref>`: the task's log as it streams, beside Factory's own record of the
/// attempt (its leg's status, its Return's summary, the artifact it returned and how it was verified), read from the Run's
/// attempt field.
fn task_log(state_text: &str, run_text: &str, attempt_ref: &str) -> Result<Value, CliError> {
    let state = PathBuf::from(state_text);
    let run_ref = run_ref_of(run_text)?;
    let log = task_file(&state, attempt_ref, "log.jsonl");
    let lines: Vec<Value> = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let now = reading(&state, &run_ref)?;
    let record = record_of(&now, attempt_ref)?;
    let attempt = json!({
        "status": format!("{:?}", leg_status(&now, record)),
        "task_ref": record.task_ref,
        "agent_ref": record.disposition.participant.agent_ref,
        "agent_session": record.disposition.body.agent_session_ref,
        "now_context_ref": record.disposition.now_context_ref,
        "returned": record.readable_return.as_ref().map(|r| serde_json::to_value(r).unwrap_or(Value::Null)),
        "verifications": serde_json::to_value(&record.verifications).unwrap_or(Value::Null),
        "failure_evidence_refs": record.failure_evidence_refs,
    });
    Ok(
        json!({"contract": TASK_CONTRACT, "attempt_ref": attempt_ref, "run_ref": run_ref.to_string(),
        "attempt": attempt, "log": log, "lines": lines}),
    )
}

pub fn is_session_command(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("session")
}

/// `factory session <verb> …`: every answer is JSON.
pub fn execute(args: &[String]) -> Result<String, CliError> {
    let words: Vec<&str> = args
        .iter()
        .skip(1)
        .map(String::as_str)
        .filter(|word| *word != "--json")
        .collect();
    let answer = match words.as_slice() {
        ["start", agent, world, project] => start(agent, world, project)?,
        ["list", state] => session_list(state)?,
        ["retry", state, run, unit] => retry(state, run, unit)?,
        ["choose", state, run, attempt] => choose(state, run, attempt)?,
        ["end", world, project, session] => end(world, project, session, None)?,
        ["task-write", world, project, session, instruction] => {
            task_write(world, project, session, instruction)?
        }
        ["task-run", state, run, session] => task_run(state, run, session)?,
        ["task-stop", state, run, attempt] => task_stop(state, run, attempt)?,
        ["task-log", state, run, attempt] => task_log(state, run, attempt)?,
        ["follow", state, run, attempt, authority] => follow(state, run, attempt, authority)?,
        _ => {
            return Err(refuse(
                "usage: factory session start <agent> <world> <project> | list <state> | end <world> <project> <session> | task-write <world> <project> <session> <instruction> | task-run <state> <run-ref> <session> | task-stop <state> <run-ref> <attempt-ref> | task-log <state> <run-ref> <attempt-ref> | retry <state> <run-ref> <unit-ref> | choose <state> <run-ref> <attempt-ref>",
            ))
        }
    };
    serde_json::to_string_pretty(&answer).map_err(CliError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_allows_an_ask_inside_the_checkout_and_refuses_one_outside() {
        let checkout = Path::new("/w/worktrees/env-1/notes");
        let ask = |path: &str| {
            json!({"native_request_id": "r1", "tool_call": {"locations": [{"path": path}]},
                "choices": [{"option_id": "yes", "kind": "allow_once"}, {"option_id": "no", "kind": "reject_once"}]})
        };
        assert_eq!(
            scope_answer(&ask("/w/worktrees/env-1/notes/a.md"), checkout).map(|(o, _)| o),
            Some("yes".into())
        );
        assert_eq!(
            scope_answer(&ask("/etc/passwd"), checkout).map(|(o, _)| o),
            Some("no".into())
        );
    }

    #[test]
    fn a_difference_is_what_the_agent_left_and_never_the_seats_pool_link() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("notes");
        std::fs::create_dir_all(&checkout).unwrap();
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(&checkout)
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        run(&["init", "--quiet", "--initial-branch", "main"]);
        std::fs::write(checkout.join("README.md"), "notes\n").unwrap();
        run(&["add", "README.md"]);
        run(&["commit", "--quiet", "-m", "first"]);
        std::fs::create_dir_all(temp.path().join("pool")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("pool"), checkout.join("target")).unwrap();
        let nothing = std::collections::BTreeMap::new();
        assert_eq!(
            difference(&checkout, &nothing),
            "",
            "the pool link alone is no difference"
        );
        // What stood in the checkout when the task was sent is not the agent's.
        std::fs::write(checkout.join("mine.md"), "the person's own\n").unwrap();
        std::os::unix::fs::symlink(temp.path().join("pool"), checkout.join("projected")).unwrap();
        let before = standing(&checkout);
        std::fs::write(checkout.join("README.md"), "notes\nmore\n").unwrap();
        std::fs::write(checkout.join("new.md"), "new\n").unwrap();
        let diff = difference(&checkout, &before);
        assert!(diff.contains("+more"), "{diff}");
        assert!(diff.contains("+new"), "{diff}");
        assert!(!diff.contains("target"), "{diff}");
        assert!(
            !diff.contains("mine.md") && !diff.contains("projected"),
            "{diff}"
        );
        // A path that stood there and the turn changed is the agent's.
        std::fs::write(
            checkout.join("mine.md"),
            "the person's own\nand the agent's\n",
        )
        .unwrap();
        assert!(difference(&checkout, &before).contains("+and the agent's"));
    }

    #[test]
    fn pi_asks_its_hooks_as_its_carrier_does_and_the_ground_is_read_from_each_harness_answer() {
        let cwd = Path::new("/w/worktrees/env-1/notes");
        // pi: the machine envelope, and pi's own event.
        let (args, payload) = hook_ask("pi", "SessionStart", "agent-session/s1", cwd);
        assert_eq!(
            args,
            ["--json", "hook", "dispatch", "pi", "SessionStart"].map(str::to_owned)
        );
        assert_eq!(
            payload,
            json!({"type": "session_start", "reason": "startup", "cwd": cwd, "session_id": "agent-session/s1"})
        );
        let (args, payload) = hook_ask("pi", "SessionEnd", "agent-session/s1", cwd);
        assert_eq!(
            args,
            ["--json", "hook", "dispatch", "pi", "SessionEnd"].map(str::to_owned)
        );
        assert_eq!(
            payload,
            json!({"type": "session_shutdown", "cwd": cwd, "session_id": "agent-session/s1"})
        );
        // A hook-command harness asks as before: plain mode, its own hook event.
        let (args, payload) = hook_ask("claude", "SessionStart", "agent-session/s1", cwd);
        assert_eq!(
            args,
            ["hook", "dispatch", "claude", "SessionStart"].map(str::to_owned)
        );
        assert_eq!(
            payload,
            json!({"session_id": "agent-session/s1", "cwd": cwd, "hook_event_name": "SessionStart", "source": "startup"})
        );
        // The ground: what each harness takes into its session from AIKit's answer, empty where it carries none.
        let hooked = json!({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "World: w · Project: notes"}});
        let carried =
            json!({"ok": true, "data": {"allowed": true, "injected": "World: w · Project: notes"}});
        assert_eq!(ground_of("claude", &hooked), "World: w · Project: notes");
        assert_eq!(ground_of("codex", &hooked), "World: w · Project: notes");
        assert_eq!(ground_of("pi", &carried), "World: w · Project: notes");
        assert_eq!(ground_of("pi", &hooked), "");
        assert_eq!(ground_of("claude", &carried), "");
        assert_eq!(
            ground_of(
                "pi",
                &Value::String("context not delivered: pi/SessionStart has no verified additional-context output adapter".into())
            ),
            ""
        );
    }

    /// A Run of one unit with its attempt started, as a task's is once it runs: Factory's own controlled native store.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn started_task() -> (tempfile::TempDir, PathBuf, RunRef, String) {
        let (dir, _store, request) =
            crate::attempt_learning::publication_tests::native_retention_source();
        let state = dir.path().join("state.json");
        (dir, state, request.run_ref, "attempt:learning".to_owned())
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn stop_lands_its_pause_when_the_follower_wrote_after_stop_read_the_run_and_never_twice() {
        let (_dir, state, run, attempt) = started_task();
        let (stop, grant) = (caller("human:owner"), authority("authority:test"));
        let decided = reading(&state, &run).unwrap();
        // Factory's follower records on the attempt after Stop read the Run (settle's delivery reading does).
        act(
            &state,
            &run,
            &follower(Some("human:owner")),
            &grant,
            FactoryAttemptOperation::RecordTracking {
                attempt_ref: attempt.clone(),
                fact: AttemptTrackingFact {
                    fact_ref: "fact:follower".into(),
                    kind: "session".into(),
                    owner_ref: "aikit".into(),
                    subject_ref: "session:test".into(),
                    source_revision: now_rfc3339(),
                    evidence_refs: BTreeSet::from(["aikit-delivery:delivery:1".to_owned()]),
                },
            },
        )
        .unwrap();
        let landed = act_on(&state, &run, &stop, &grant, decided.clone(), |now| {
            pause(now, &attempt, "session:test")
        })
        .expect("Stop's pause lands on the Run the follower moved on");
        assert!(landed.is_some());
        // A second press that read the Run before the pause landed finds it paused: it records no second pause.
        let again = act_on(&state, &run, &stop, &grant, decided, |now| {
            pause(now, &attempt, "session:test")
        })
        .unwrap();
        assert!(again.is_none());
        let now = reading(&state, &run).unwrap();
        let record = record_of(&now, &attempt).unwrap();
        let pauses = record
            .tracking
            .iter()
            .filter(|fact| fact.evidence_refs.contains(PAUSE))
            .count();
        assert_eq!(pauses, 1);
        assert_eq!(leg_status(&now, record), LegStatus::Active);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_turn_cancelled_on_an_attempt_stop_is_pausing_is_no_failure() {
        let (_dir, state, run, attempt) = started_task();
        let grant = authority("authority:test");
        // The follower read the Run before Stop's pause landed, then saw the turn end cancelled.
        let decided = reading(&state, &run).unwrap();
        act(
            &state,
            &run,
            &caller("human:owner"),
            &grant,
            pause(&decided, &attempt, "session:test").unwrap(),
        )
        .unwrap();
        act_on(
            &state,
            &run,
            &follower(Some("human:owner")),
            &grant,
            decided.clone(),
            |now| turn_end(now, &attempt, "cancelled", "delivery:1"),
        )
        .expect("the follower settles the cancelled turn on the paused attempt");
        let now = reading(&state, &run).unwrap();
        let record = record_of(&now, &attempt).unwrap();
        assert_eq!(leg_status(&now, record), LegStatus::Active);
        assert!(paused(record));
        // With no word from Stop, a turn that ends cancelled is the attempt's failure, and so is one that fails while paused.
        assert!(matches!(
            turn_end(&decided, &attempt, "cancelled", "delivery:1"),
            Some(FactoryAttemptOperation::Fail { .. })
        ));
        assert!(matches!(
            turn_end(&now, &attempt, "failed", "delivery:1"),
            Some(FactoryAttemptOperation::Fail { .. })
        ));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn stops_cancellation_and_the_followers_acceptance_land_once() {
        let (_dir, state, run, attempt) = started_task();
        let (stop, grant) = (caller("human:owner"), authority("authority:test"));
        let now = reading(&state, &run).unwrap();
        act(
            &state,
            &run,
            &stop,
            &grant,
            pause(&now, &attempt, "session:test").unwrap(),
        )
        .unwrap();
        let request = FactoryAttemptOperation::RequestCancellation {
            attempt_ref: attempt.clone(),
        };
        let accept = FactoryAttemptOperation::AcceptCancellation {
            attempt_ref: attempt.clone(),
        };
        act(&state, &run, &stop, &grant, request.clone()).unwrap();
        // The follower read the Run while its cancellation was requested; Stop accepts it first.
        let decided = reading(&state, &run).unwrap();
        assert!(act(&state, &run, &stop, &grant, accept.clone())
            .unwrap()
            .is_some());
        let settled = act_on(
            &state,
            &run,
            &follower(Some("human:owner")),
            &grant,
            decided,
            |now| turn_end(now, &attempt, "cancelled", "delivery:1"),
        )
        .expect("the follower finds the cancellation accepted");
        assert!(settled.is_none());
        // Stop's intents the Run holds already are not applied a second time.
        assert!(act(&state, &run, &stop, &grant, request).unwrap().is_none());
        assert!(act(&state, &run, &stop, &grant, accept).unwrap().is_none());
        let now = reading(&state, &run).unwrap();
        assert_eq!(
            leg_status(&now, record_of(&now, &attempt).unwrap()),
            LegStatus::CancellationAccepted
        );
    }

    #[test]
    fn a_delivery_phase_is_read_wherever_aikit_puts_it() {
        let answer =
            json!({"data": {"delivery": {"delivery_ref": "delivery:1", "phase": "returned"}}});
        assert_eq!(
            delivery_phase(&answer, "delivery:1").as_deref(),
            Some("returned")
        );
        assert_eq!(delivery_phase(&answer, "delivery:2"), None);
    }

    #[test]
    fn a_session_refuses_a_world_that_is_not_one_before_any_owner_is_asked() {
        let refused = start("claude-code", "/no/such/world", "notes").unwrap_err();
        assert!(
            refused.to_string().contains("no Central World"),
            "{refused}"
        );
        let refused = task_write("/no/such/world", "../x", "s", "words").unwrap_err();
        assert!(
            refused.to_string().contains("no Central World"),
            "{refused}"
        );
    }
}
