//! A session on a track and the tasks it runs (O-I #1006; DLW manual §19.1; O-I `docs/architecture/world-shell.md` §9.3).
//!
//! A session is an agent of the census seated on a track: AIKit's AgentSession, opened by the kernel's one provision
//! (`oi desktop provision`, what the window's `encounter_provision` runs) in the checkout the project's workcell seat holds,
//! its record opened by AIKit's own hooks (`aikit hook dispatch <client> SessionStart`, PROGRAMME R36) and closed by them at
//! its end. A task is written on it as a Run of one unit, commissioned in the project's Factory state together with its
//! workflow source and its attempt field (the act a song's take is, song_take.rs); its unit's practice is the Skill Sets the
//! agent carries. Run, it is Factory's own launch (`factory.attempt.launch`) on AIKit's selection receipt for the session it
//! opened, under the person's grant as the host resolved it, admitted on the
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
    execute_attempt_owner_action, AttemptOwnerError, FactoryAttemptOwnerRequest,
    FACTORY_ATTEMPT_OWNER_ACTION,
};
use crate::attempt_runtime::*;
use crate::cli::CliError;
use crate::core::run::{RunRef, WorkflowUnitRef};
use crate::native_owner::{NativeOwnerInvocation, AIKIT_CAW_CONTRACT_REVISION};
use crate::orchestration::{LegStatus, ReturnedArtifact};
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
/// The tracking fact a pause leaves on an attempt: AIKit cancelled the session's turn at Factory's Stop.
const PAUSE: &str = "factory-session-pause";
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

/// The session's Agency binding, as AIKit holds it: who the agent is, under which Agency, from which source, which sender it
/// hears, and the binding's revision an addressed turn names.
struct Binding {
    agent_ref: String,
    agency_ref: String,
    revision: String,
    sender: String,
    source_ref: String,
    source_revision: String,
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

/// The project placed for Factory as the window's Setup places it (`factory project setup-central`: under the key Factory
/// encodes from the project's Central id, linked to its ProjectCentral manifest), so Factory locates its state (`project
/// locate`) and the World Model's factory slice reads its sessions and tasks. A project placed already stays as it is. The
/// answer is the key it is placed under, the key each of its tasks is commissioned under.
fn placed(place: &Place) -> Result<String, CliError> {
    let located = if place.root.join(".factory/project.json").exists() {
        crate::project_setup::locate(&place.root)?
    } else {
        crate::project_setup::setup_central(
            &place.root,
            &place.project_id,
            &place.root.join("ProjectCentral/project.json"),
        )?
    };
    located["projectKey"]
        .as_str()
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            refuse(format!(
                "Factory's placement of {} names no key: {}",
                place.root.display(),
                clip(&located.to_string())
            ))
        })
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
    let practice = seated_practice(&place.checkout, session)?;
    commission_task(&place, &held, &practice, session, instruction)
}

/// The practice of the agent seated on the session: its provider as AIKit's status of the session names it, and the skills
/// that provider's Skill Sets project here (`practice_of`).
fn seated_practice(checkout: &Path, session: &str) -> Result<Vec<String>, CliError> {
    let status = encounter(
        checkout,
        json!({"action": "status", "agent_session": session}),
    )
    .map_err(refuse)?;
    let provider = provider_of(&status)
        .ok_or_else(|| refuse(format!("AIKit's status of {session} names no provider")))?;
    practice_of(checkout, &provider)
}

/// What `task_write` does once AIKit has answered the session's binding and its agent's practice: the task commissioned in the
/// project's Factory state with its workflow source and attempt field, and kept on the session's record.
fn commission_task(
    place: &Place,
    held: &Binding,
    practice: &[String],
    session: &str,
    instruction: &str,
) -> Result<Value, CliError> {
    let headline: String = instruction
        .lines()
        .next()
        .unwrap_or(instruction)
        .chars()
        .take(160)
        .collect();
    // The project is Factory's own (placed at the session's start, or by the window's Setup; a session Factory did not start
    // places it here), and the task is commissioned under the key it is placed under.
    let project_key = placed(place)?;
    // Each task is its own commission: two written in the same second with the same words stay two.
    let task = ulid::Ulid::new().to_string().to_ascii_lowercase();
    let draft = crate::commission::FactoryCommissionDraft {
        contract: crate::commission::FACTORY_COMMISSION_DRAFT.into(),
        request_ref: Some(format!("commission-request:session-task-{task}")),
        project_key,
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
    let source = workflow_source(
        place,
        held,
        practice,
        &composition.run_ref,
        session,
        instruction,
    )?;
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
    let unit = task_unit(&reading)
        .map(|unit| unit.to_string())
        .unwrap_or_default();
    // The task is kept on the session's record, so the window lays it as a clip on the session's track before it runs.
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

/// The unit a session's task runs: its Run's one leg once an attempt has started it, and before that the one unit its workflow
/// requires (a Run holds no leg until its first attempt starts). `factory.task.write` answers it, `factory.task.run` runs it.
fn task_unit(reading: &FactoryAttemptReading) -> Option<WorkflowUnitRef> {
    reading
        .legs
        .keys()
        .next()
        .cloned()
        .or_else(|| reading.required_units.iter().next().cloned())
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

/// The revision of a session's task's workflow source: a workflow written on a session, its one unit run there.
const SESSION_TASK: &str = "session-task-v1";

/// Where a session's task returns: to the session it was written on.
fn session_return(session: &str) -> String {
    format!("return:session-{}", session_tail(session))
}

/// The task's one unit: the person's words as the difference the agent is asked to make in the session's checkout, its effect
/// a proposal there (manual §16.6: unarmed, every write is a proposal; Git / landing lands it), its Return the difference; its
/// practice the agent's (`seated_practice`), its capabilities reading and writing the checkout's source.
fn workflow_source(
    place: &Place,
    held: &Binding,
    practice: &[String],
    run_ref: &RunRef,
    session: &str,
    instruction: &str,
) -> Result<crate::workflow::WorkflowSource, CliError> {
    let id = ulid::Ulid::new();
    let today = now_rfc3339().chars().take(10).collect::<String>();
    let key = format!("task-{}", id.to_string().to_ascii_lowercase());
    let mut source = json!({
        "schemaVersion": "factory.agent-workflow-source/v1",
        "coordinationContract": "factory.bounded-coordination/v1",
        "source": {
            "ref": format!("workflow-source:{id}"),
            "revision": SESSION_TASK,
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
            "returnAddress": session_return(session),
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

/// git in the checkout, on its own repository (GIT_DIR and GIT_WORK_TREE from the caller's environment never aim it
/// elsewhere), running nothing the checkout's git config or hooks name: its fsmonitor and its hooks are off, since an
/// agent's write could have set them, and git runs them as the person.
fn git_in(checkout: &Path) -> Command {
    let mut git = Command::new("git");
    git.args([
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.hooksPath=/dev/null",
    ])
    .arg("-C")
    .arg(checkout)
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .stdin(Stdio::null());
    git
}

fn git(checkout: &Path, args: &[&str]) -> Option<String> {
    let output = git_in(checkout).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn head_of(checkout: &Path) -> Option<String> {
    git(checkout, &["rev-parse", "HEAD"]).map(|head| head.trim().to_owned())
}

/// The person's grant for one of Factory's host-checked acts, as the host resolved it on their press: the host reads its own
/// token against the World's native action authority contract, for the native action and scope Factory's admission names
/// (`factory attempt admission <state>`), and hands Factory the grant's ref, as it does for `factory.attempt.launch` and
/// `factory.attempt.action`. Factory reads no token.
fn granted(grant: &str) -> Result<ProjectedFactoryActionAuthority, CliError> {
    const FORM: &str = "central:native-action-authority#";
    if !grant
        .strip_prefix(FORM)
        .is_some_and(|principal| !principal.trim().is_empty())
    {
        return Err(refuse(format!(
            "the person's grant is presented as the host resolved it, {FORM}<principal_ref>; `{grant}` is not one"
        )));
    }
    Ok(authority(grant))
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

/// `factory session task-run <state> <run-ref> <session> <grant>`: the task's unit launched on the session by Factory's own launch
/// (`factory.attempt.launch`, attempt_application.rs `launch_attempt`), on AIKit's selection receipt for the session it opened
/// (`aikit compose model selection`) and the praxis AIKit prepared in the task's NOW, under the person's grant; then its words
/// sent to the session and followed to the turn's end.
fn task_run(
    state_text: &str,
    run_text: &str,
    session: &str,
    grant: &str,
) -> Result<Value, CliError> {
    let authority = granted(grant)?;
    let state = PathBuf::from(state_text);
    let run_ref = run_ref_of(run_text)?;
    let place = place_of_state(&state)?;
    let store =
        FileAttemptStore::open_run(&state, run_ref.clone()).map_err(CliError::from_native)?;
    let before = store.reading().map_err(CliError::from_native)?;
    let unit_ref: WorkflowUnitRef =
        task_unit(&before).ok_or_else(|| refuse(format!("{run_ref} holds no unit to run")))?;
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
    // A task runs on the session it was written on: its Return is addressed there, and Retry runs it again there, as the
    // session's task (`retry`), so it is never sent to another.
    if before.workflow_source_revision != SESSION_TASK {
        return Err(refuse(format!(
            "{run_ref} is no task written on a session: factory.task.run runs a task factory.task.write wrote on its session"
        )));
    }
    if basis.unit.return_address != session_return(session) {
        return Err(refuse(format!(
            "{run_ref}'s task was not written on {session}: a task runs on the session it was written on, where its Return goes ({}) and Retry runs it again. Run it there, or write it again on {session}",
            basis.unit.return_address
        )));
    }
    // Refused before AIKit is asked anything: launch refuses it too, after the task's NOW is prepared.
    if basis.journey_ref.is_none() {
        return Err(refuse(format!(
            "Factory links {run_ref} to no Journey: a task runs under the Journey it was written in"
        )));
    }
    let held = binding(&place.checkout, session)?;
    // The session as AIKit opened it, bound as its explicit selection: its body, its harness and the model it runs.
    let realisation = aikit(
        &place.checkout,
        &["compose", "model", "selection", "--session", session],
    )
    .map_err(|words| {
        refuse(format!(
            "AIKit gave no selection receipt for {session}: {words}"
        ))
    })?;
    let unit = &basis.unit;
    let attempt_ref = minted("attempt");
    let task_ref = minted("task");
    let (now_ref, receipt) =
        prepare_now(&place, &state, &run_ref, unit, &task_ref, &held, session)?;
    drop(store);
    let projection_ref = minted("projection");
    let launch = |expected_revision| {
        crate::attempt_application::launch_attempt(
            &state,
            crate::attempt_application::FactoryAttemptLaunchRequest {
                contract: crate::attempt_application::FACTORY_ATTEMPT_LAUNCH.into(),
                projection_ref: projection_ref.clone(),
                caller: caller(&held.sender),
                run_ref: run_ref.clone(),
                expected_revision,
                authority: authority.clone(),
                workflow_unit_ref: unit_ref.clone(),
                attempt_ref: attempt_ref.clone(),
                task_ref: task_ref.clone(),
                aikit_realisation: realisation.clone(),
                praxis_receipt: Some(receipt.clone()),
                now_context_ref: Some(now_ref.clone()),
                placement: None,
            },
        )
    };
    // The launch is decided on the Run as it stands as the attempt starts (NOW's preparation took its time), and on a write
    // another track made since, read again: the project's tracks share one Factory state.
    let mut launched = None;
    for _ in 0..READINGS {
        match launch(reading(&state, &run_ref)?.revision) {
            Err(FactoryAttemptError::RevisionConflict { .. }) => {}
            landed => {
                launched = Some(landed.map_err(CliError::from_native)?);
                break;
            }
        }
    }
    let launched = launched.ok_or_else(|| {
        refuse(format!(
            "{run_ref} moved under each of the {READINGS} readings its launch was decided on: Factory started nothing"
        ))
    })?;
    let space = reading(&state, &run_ref)?
        .attempts
        .iter()
        .find(|record| record.attempt_ref == attempt_ref)
        .map(|record| record.disposition.body.session_space_ref.clone())
        .ok_or_else(|| {
            refuse(format!(
                "Factory's launch answered {} but holds no {attempt_ref}",
                launched.operation
            ))
        })?;
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
    let execution_ref = minted("execution");
    let delivery_ref = minted("delivery");
    let by = caller(&held.sender);
    // Where the session stands as the words go: the task's turn is read forward from this block, by id (its log streams the
    // turn, its Return's summary is said in it).
    // Until the words go, a refusal leaves the attempt started and never sent: it is failed (`never_sent`).
    let unsent = |words: String| {
        never_sent(
            state,
            run_ref,
            attempt_ref,
            &by,
            authority,
            &delivery_ref,
            words,
        )
    };
    let after_block =
        turn_mark(&|request| encounter(&place.checkout, request), session).map_err(|error| {
            unsent(format!(
                "the session's view could not be read before the task's words went: {error}"
            ))
        })?;
    // How the checkout stands as the words go: what the turn leaves is read against it.
    let before = task_file(state, attempt_ref, "before.json");
    std::fs::create_dir_all(before.parent().unwrap_or(Path::new(".")))
        .and_then(|()| {
            std::fs::write(
                &before,
                serde_json::to_vec(&standing(&place.checkout)).unwrap_or_default(),
            )
        })
        .map_err(|error| {
            unsent(format!(
                "how the checkout stood as the task's words went could not be kept: {error}"
            ))
        })?;
    let (request_ref, projection_ref) = (minted("owner-request"), minted("projection"));
    let sent = send_fresh(
        state,
        run_ref,
        attempt_ref,
        &by,
        authority,
        &delivery_ref,
        |expected_revision| {
            execute_attempt_owner_action(
                state,
                FactoryAttemptOwnerRequest {
                    contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
                    request_ref: request_ref.clone(),
                    projection_ref: projection_ref.clone(),
                    caller: by.clone(),
                    run_ref: run_ref.clone(),
                    expected_revision,
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
        },
    )?;
    let log = task_file(state, attempt_ref, "log.jsonl");
    std::fs::create_dir_all(log.parent().unwrap_or(Path::new(".")))
        .map_err(CliError::from_native)?;
    append(
        &log,
        &json!({"at": now_rfc3339(), "kind": "sent", "delivery_ref": delivery_ref,
            "text": words, "after_block": after_block}),
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

/// The task's words dispatched on the Run as it stands right before they go (O-I #1006, review F6). The store's revision is the
/// project's (each of its tracks writes it), so another track's write since the reading leaves it stale, and Factory's dispatch
/// then sends nothing: the Run is read again and the send decided again. A send that never goes (the Run moved under every
/// reading, or the dispatch refused before anything went) fails its attempt (`never_sent`), so it is not left Active.
fn send_fresh<T>(
    state: &Path,
    run_ref: &RunRef,
    attempt_ref: &str,
    by: &FactoryActionCaller,
    authority: &ProjectedFactoryActionAuthority,
    delivery_ref: &str,
    dispatch: impl Fn(u64) -> Result<T, AttemptOwnerError>,
) -> Result<T, CliError> {
    let unsent = |words: String| {
        never_sent(
            state,
            run_ref,
            attempt_ref,
            by,
            authority,
            delivery_ref,
            words,
        )
    };
    for _ in 0..READINGS {
        let revision = reading(state, run_ref)
            .map_err(|error| {
                unsent(format!(
                    "the Run could not be read before the task's words went: {error}"
                ))
            })?
            .revision;
        match dispatch(revision) {
            Err(error) if error.stale_before_dispatch() => {}
            Ok(sent) => return Ok(sent),
            Err(error) => {
                return Err(unsent(format!(
                    "the task's words were not sent to the session: {error}"
                )))
            }
        }
    }
    Err(unsent(format!(
        "{run_ref} moved under each of the {READINGS} readings the task's send was decided on"
    )))
}

/// The refusal of a task's send, its attempt failed when its words never went: it holds no send intent (Factory's dispatch
/// records one before anything goes), so it is failed rather than left Active, and a retry runs its unit again. An attempt that
/// holds an intent stands, since its words may have gone: its delivery is read, never sent again.
fn never_sent(
    state: &Path,
    run_ref: &RunRef,
    attempt_ref: &str,
    by: &FactoryActionCaller,
    authority: &ProjectedFactoryActionAuthority,
    delivery_ref: &str,
    words: String,
) -> CliError {
    let failed = reading(state, run_ref).and_then(|now| {
        act_on(state, run_ref, by, authority, now, |now| {
            let record = now
                .attempts
                .iter()
                .find(|record| record.attempt_ref == attempt_ref)?;
            let intended = record.observations.iter().any(|receipt| {
                receipt.contract == crate::attempt_owner_dispatch::TRANSPORT_OBSERVATION
                    && receipt.payload["action"] == "send"
            });
            if intended || record.readable_return.is_some() || ended(&leg_status(now, record)) {
                return None;
            }
            Some(FactoryAttemptOperation::Fail {
                attempt_ref: attempt_ref.to_owned(),
                reason: format!("the task's words were never sent: {words}"),
                evidence_refs: BTreeSet::from([format!("factory-session-unsent:{delivery_ref}")]),
            })
        })
    });
    refuse(match failed {
        Ok(Some(_)) => {
            format!("{words}; {attempt_ref} is failed, never sent, so a retry runs it again")
        }
        Ok(None) => format!("{words}; {attempt_ref} stands as it was"),
        Err(error) => format!("{words}; {attempt_ref} could not be failed: {error}"),
    })
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
fn retry(
    state_text: &str,
    run_text: &str,
    unit_text: &str,
    grant: &str,
) -> Result<Value, CliError> {
    // No grant, no move: the person's grant is read before anything else.
    let authority = granted(grant)?;
    let state = PathBuf::from(state_text);
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
    // A task written on a session is retried there, whichever receipt AIKit answered for the session (its seated-session
    // receipt, or the explicit pin of a session realised under a model policy): its workflow is a session's task, its Return
    // addressed to the session. Any other unit is retried as its Run's own.
    let session = prior.disposition.body.agent_session_ref.clone();
    let on_session = now.workflow_source_revision == SESSION_TASK
        && prior.disposition.return_address == session_return(&session);
    let on = if on_session {
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
fn choose(
    state_text: &str,
    run_text: &str,
    attempt_ref: &str,
    grant: &str,
) -> Result<Value, CliError> {
    // No grant, no move: the person's grant is read before anything else.
    let authority = granted(grant)?;
    let state = PathBuf::from(state_text);
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

/// The keys under which a harness's input names where an edit moves a file (codex's `move_path`, and its kin).
const MOVED_TO: [&str; 5] = [
    "move_path",
    "movePath",
    "destination",
    "new_path",
    "newPath",
];

/// Where an edit's input (ACP's `rawInput`) says it moves a file, anywhere in it.
fn moved_to(input: &Value, into: &mut Vec<String>) {
    match input {
        Value::Object(map) => {
            for (key, value) in map {
                match value.as_str() {
                    Some(path) if MOVED_TO.contains(&key.as_str()) => into.push(path.to_owned()),
                    _ => moved_to(value, into),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| moved_to(item, into)),
        _ => {}
    }
}

/// Python's whitespace (`str.isspace`, and `\s` in a `str` pattern), which hermes's executor reads a patch's file lines
/// with: Rust's, and the four separators U+001C to U+001F.
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// A move's two files, `a -> b`, as hermes's executor reads them (`(.+?)\s*->\s*(.+)`): the shortest start before an
/// arrow with something after it, and the rest.
fn move_sides(text: &str) -> Option<(String, String)> {
    text.char_indices().skip(1).find_map(|(end, _)| {
        let to = text[end..]
            .trim_start_matches(py_space)
            .strip_prefix("->")
            .filter(|to| !to.is_empty())?;
        Some((
            text[..end].trim_matches(py_space).to_owned(),
            to.trim_matches(py_space).to_owned(),
        ))
    })
}

/// The files one line of a patch says it writes, or None for a line that is no file line. A file line is `***`, any space
/// or none, then `Update`, `Add` or `Delete`, space, `File:` and the file; or `Move`, space, `File:` and `a -> b` (hermes's
/// V4A executor, tools/patch_parser.py); or `Move`, space, `to:` and the file (codex's apply_patch). A file line whose
/// file is not read names an empty one, which is no path.
fn file_line(line: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix("***")?.trim_start_matches(py_space);
    let (word, rest) = ["Update", "Add", "Delete", "Move"]
        .into_iter()
        .find_map(|word| Some((word, rest.strip_prefix(word)?)))?;
    let after = rest.trim_start_matches(py_space);
    if after.len() == rest.len() {
        return None;
    }
    match (word, after.strip_prefix("File:"), after.strip_prefix("to:")) {
        ("Move", Some(sides), _) => Some(
            move_sides(sides.trim_start_matches(py_space))
                .map_or_else(|| vec![String::new()], |(from, to)| vec![from, to]),
        ),
        (_, Some(file), _) | ("Move", None, Some(file)) => {
            Some(vec![file.trim_matches(py_space).to_owned()])
        }
        _ => None,
    }
}

/// The patches an edit's input carries: each text under a `patch` key, anywhere in it (hermes's `patch` tool in mode
/// `patch`).
fn patches<'a>(input: &'a Value, into: &mut Vec<&'a str>) {
    match input {
        Value::Object(map) => {
            for (key, value) in map {
                match value.as_str() {
                    Some(text) if key == "patch" => into.push(text),
                    _ => patches(value, into),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| patches(item, into)),
        _ => {}
    }
}

/// The files the patches in an edit's input write, every file line of each (`file_line`), the lines outside its
/// `*** Begin Patch` and `*** End Patch` among them; None when its input carries no patch.
fn patch_targets(input: &Value) -> Option<Vec<String>> {
    let mut texts = Vec::new();
    patches(input, &mut texts);
    (!texts.is_empty()).then(|| {
        texts
            .iter()
            .flat_map(|text| text.split('\n'))
            .filter_map(|line| file_line(line.strip_suffix('\r').unwrap_or(line)))
            .flatten()
            .collect()
    })
}

/// The path of each diff an edit carries (`content[]` of type `diff`).
fn diff_paths(call: &Value) -> impl Iterator<Item = &str> {
    call["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "diff")
        .filter_map(|item| item["path"].as_str())
}

/// hermes joins the files of a patch of several into its one diff's path (acp_adapter/edit_approval.py): that path is read
/// as the files it joins.
const JOINED: &str = ", ";

/// The places an edit says it writes: the file of each diff it carries, every file a patch in its input writes, and where
/// its input moves a file. An edit that declares none does not say where it writes: codex asks for a move (apply_patch's
/// `*** Move to:`) as an edit of the file it starts from, with no content. A diff of a patch of several files names them
/// all in its one path, joined, and is read as each of them (`unread` leaves the ask whose path does not read so).
fn written_paths(call: &Value) -> Vec<String> {
    let targets = patch_targets(&call["rawInput"]);
    let joined = targets.as_ref().is_some_and(|targets| targets.len() > 1);
    let mut written: Vec<String> = diff_paths(call)
        .flat_map(|path| match joined {
            true => path.split(JOINED).collect::<Vec<_>>(),
            false => vec![path],
        })
        .map(str::to_owned)
        .collect();
    written.extend(targets.into_iter().flatten());
    moved_to(&call["rawInput"], &mut written);
    written
}

/// Why an edit's paths cannot be read as one real path each, if they cannot: a patch in its input that names no file it
/// writes, or one it does not name in full; or a diff of a patch of several files whose path does not join exactly as
/// many (a file whose name holds ", " among them).
fn unread(call: &Value) -> Option<&'static str> {
    let targets = patch_targets(&call["rawInput"])?;
    if targets.is_empty() || targets.iter().any(String::is_empty) {
        return Some("left for you: the edit's patch does not name in full every file it writes");
    }
    (targets.len() > 1
        && diff_paths(call).any(|path| path.split(JOINED).count() != targets.len()))
    .then_some("left for you: the edit's diff names the files of its patch in one path that does not read as each of them")
}

/// The paths an ask declares, as ACP carries its tool call: its locations (`locations[].path`) and the places an edit writes
/// (`written_paths`). No other string of the call is read as a path: not a command, not an edit's text.
fn declared_paths(call: &Value) -> Vec<String> {
    let mut declared: Vec<String> = call["locations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|location| location["path"].as_str())
        .map(str::to_owned)
        .collect();
    declared.extend(written_paths(call));
    declared
}

/// Where a declared path leads, taken where the filesystem takes it: relative to the checkout, each part that exists followed
/// as the filesystem follows it (a link to where it leads, and a `..` after it from there), and only the part still to be
/// made taken as written. None where it leads to no place a write could be judged at: an empty path, `~` (the person's
/// home), a `..` above the root, or a link that leads nowhere (a write through it lands wherever it points).
fn led_to(path: &str, checkout: &Path) -> Option<PathBuf> {
    use std::path::Component;
    if path.is_empty() || path.starts_with('~') {
        return None;
    }
    let mut resolved = PathBuf::new();
    for component in checkout.join(path).components() {
        match component {
            Component::Prefix(_) | Component::RootDir => resolved.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if !resolved.pop() {
                    return None;
                }
            }
            Component::Normal(name) => {
                resolved.push(name);
                if std::fs::symlink_metadata(&resolved).is_ok() {
                    resolved = resolved.canonicalize().ok()?;
                }
            }
        }
    }
    Some(resolved)
}

/// Whether a declared path resolves inside the checkout (`led_to`).
fn resolves_inside(path: &str, checkout: &Path) -> bool {
    let Ok(root) = checkout.canonicalize() else {
        return false;
    };
    led_to(path, checkout).is_some_and(|resolved| resolved.starts_with(root))
}

/// The checkout's own git directories, wherever git keeps them (`git rev-parse --absolute-git-dir --git-common-dir`): its
/// gitdir (`.git`, one kept apart with `--separate-git-dir`, or a linked worktree's) and the common one its config and hooks
/// live in. None for a checkout git does not read as a repository.
fn git_dirs(checkout: &Path) -> Vec<std::fs::Metadata> {
    git(
        checkout,
        &["rev-parse", "--absolute-git-dir", "--git-common-dir"],
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|dir| std::fs::metadata(checkout.join(dir.trim())).ok())
    .collect()
}

/// Whether two things on disk are one, whatever they are called.
#[cfg(unix)]
fn same_file(one: &std::fs::Metadata, other: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    one.dev() == other.dev() && one.ino() == other.ino()
}

#[cfg(not(unix))]
fn same_file(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    false
}

/// Whether a path inside the checkout leads into a git directory: a folder named `.git` in any case (the Mac's disk reads
/// `.GIT` as `.git`; a nested repository's among them, and `.git` itself, a linked worktree's file), or one of the checkout's
/// own (`dirs`), met as the same folder on disk whatever it is called. git runs what a write there sets, core.fsmonitor in
/// its config or a hook, as the person when it reads the checkout (`standing`).
fn into_git(path: &str, checkout: &Path, dirs: &[std::fs::Metadata]) -> bool {
    let (Ok(root), Some(resolved)) = (checkout.canonicalize(), led_to(path, checkout)) else {
        return true;
    };
    resolved
        .ancestors()
        .take_while(|part| part.starts_with(&root) && *part != root.as_path())
        .any(|part| {
            part.file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(".git"))
                || std::fs::symlink_metadata(part)
                    .is_ok_and(|meta| dirs.iter().any(|dir| same_file(dir, &meta)))
        })
}

/// The kinds of ask that act only on the files they declare (ACP's ToolKind). A command, a fetch, a move or any other tool may
/// act anywhere, whatever it declares.
const FILE_KINDS: [&str; 4] = ["read", "edit", "delete", "search"];

/// The kinds of ask among them that write the files they declare.
const WRITE_KINDS: [&str; 2] = ["edit", "delete"];

/// The follower's answer to one ask: allowed once (the harness's `allow_once`, never a standing allow) where the ask acts on files,
/// declares every place it writes, each as one path, every path it declares resolves inside the session's checkout, and none it
/// writes is in a git directory; else left for the person, who answers it where AIKit shows its asks, under Scope's Wait and On
/// no answer, and why.
fn scope_answer(ask: &Value, checkout: &Path) -> Result<String, &'static str> {
    let call = &ask["tool_call"];
    let kind = call["kind"].as_str().unwrap_or_default();
    if !FILE_KINDS.contains(&kind) {
        return Err("left for you: a command, a fetch or another tool can act outside the session's checkout");
    }
    if kind == "edit" && written_paths(call).is_empty() {
        return Err("left for you: the edit does not say where it writes (a move names only the file it starts from)");
    }
    if let Some(why) = unread(call) {
        return Err(why);
    }
    let paths = declared_paths(call);
    if paths.is_empty() {
        return Err("left for you: it declares no path in the session's checkout");
    }
    if !paths.iter().all(|path| resolves_inside(path, checkout)) {
        return Err("left for you: a path it declares resolves outside the session's checkout");
    }
    if WRITE_KINDS.contains(&kind) {
        let dirs = git_dirs(checkout);
        if paths.iter().any(|path| into_git(path, checkout, &dirs)) {
            return Err("left for you: it writes into a git directory, whose config and hooks git runs as you");
        }
    }
    ask["choices"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|choice| choice["kind"] == "allow_once")
        .and_then(|choice| {
            choice["option_id"]
                .as_str()
                .or_else(|| choice["id"].as_str())
        })
        .map(str::to_owned)
        .ok_or("left for you: its harness offers no one-time allow")
}

/// How the checkout stands, path by path, as git reads it (`git status --porcelain -z --untracked-files=all`): each changed or
/// untracked path with a digest of what it holds (a link's target, a file's bytes, or its absence). A rename stands at both
/// its paths, the one it left and the one it made; a copy at the one it made (its source stands as it was).
fn standing(checkout: &Path) -> std::collections::BTreeMap<String, String> {
    let listed = git(
        checkout,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
    .unwrap_or_default();
    // An entry is `XY path`; a rename's or a copy's is followed by the path it came from, alone.
    let mut entries = listed.split('\0').filter(|entry| !entry.is_empty());
    let mut paths = Vec::new();
    while let Some(entry) = entries.next() {
        let Some((code, path)) = entry.split_at_checked(3) else {
            continue;
        };
        paths.push(path.to_owned());
        if code.starts_with('R') || code.starts_with('C') {
            let origin = entries.next();
            if let Some(origin) = origin.filter(|_| code.starts_with('R')) {
                paths.push(origin.to_owned());
            }
        }
    }
    let mut held = std::collections::BTreeMap::new();
    for path in paths {
        let at = checkout.join(&path);
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
        held.insert(path, digest);
    }
    held
}

/// The difference the turn left in the checkout, read from git: every path whose standing moved since the task was sent
/// (`before`, kept beside the task's log when it was sent), tracked paths against HEAD and new ones in whole, and the files it
/// names are those paths, as git's porcelain reads them: a file made, changed, deleted, or renamed (both its paths). What stood
/// there already (the person's own edits, the seat's build pool link, the harness roots AIKit projects into a checkout) is never
/// the agent's.
/// The difference a turn left: git's text of it, and the files it names.
#[derive(Debug, PartialEq)]
struct Difference {
    text: String,
    files: Vec<String>,
}

fn difference(checkout: &Path, before: &std::collections::BTreeMap<String, String>) -> Difference {
    let mut diff = String::new();
    let mut files = Vec::new();
    for (path, digest) in standing(checkout) {
        if before.get(&path) == Some(&digest) || pool_link(checkout, &path) {
            continue;
        }
        // Tracked: HEAD holds it (a deleted file, a rename's old path) or the index does (a file added); else new and untracked.
        let tracked = git(checkout, &["cat-file", "-e", &format!("HEAD:{path}")]).is_some()
            || git(checkout, &["ls-files", "--error-unmatch", "--", &path]).is_some();
        let args: Vec<&str> = if tracked {
            vec!["diff", "HEAD", "--no-color", "--", &path]
        } else {
            vec!["diff", "--no-color", "--no-index", "--", "/dev/null", &path]
        };
        // `--no-index` exits 1 when the files differ, so its answer is read whatever its status.
        if let Ok(output) = git_in(checkout).args(&args).output() {
            // A path git shows no difference at (its index alone moved) names no file.
            if !output.stdout.is_empty() {
                diff.push_str(&String::from_utf8_lossy(&output.stdout));
                files.push(path);
            }
        }
    }
    Difference { text: diff, files }
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
    let delivery_ref = delivery_of(&record)
        .ok_or_else(|| refuse(format!("{attempt_ref} names no delivery to follow")))?;
    let begun = Instant::now();
    // The task's turn is read forward by block id from where the session stood as its words went (`send`); a follower started
    // again streams on from what the log holds.
    let mark = mark_of(&log);
    let mut streamed: std::collections::BTreeMap<u64, String> = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|line| line["kind"] == "turn")
        .filter_map(|line| {
            Some((
                line["block"]["id"].as_u64()?,
                line["block"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            ))
        })
        .collect();
    let read = |request: Value| encounter(&place.checkout, request);
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
        let view = match mark {
            Some(mark) => turn(&read, &session, mark).map(|(blocks, view)| {
                for block in to_stream(&blocks, &mut streamed) {
                    append(
                        &log,
                        &json!({"at": now_rfc3339(), "kind": "turn", "block": block}),
                    );
                }
                view
            }),
            None => read(json!({"action": "view", "agent_session": session})),
        };
        if let Ok(view) = view {
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
                // The follower allows only what acts on files inside the checkout; it answers nothing else for the person.
                let option = match scope_answer(&ask, &place.checkout) {
                    Ok(option) => option,
                    Err(why) => {
                        append(
                            &log,
                            &json!({"at": now_rfc3339(), "kind": "ask", "request_id": id, "answer": why}),
                        );
                        continue;
                    }
                };
                append(
                    &log,
                    &json!({"at": now_rfc3339(), "kind": "ask", "request_id": id,
                    "answer": "allowed once: every path it declares is inside the session's checkout"}),
                );
                if let Err(words) = encounter(
                    &place.checkout,
                    json!({"action": "permission", "agent_session": session, "request_id": id,
                        "decision": {"outcome": "selected", "option_id": option}}),
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
    let Difference { text: diff, files } =
        difference(&place.checkout, &standing_before(state, attempt_ref));
    let digest = blake3::hash(diff.as_bytes()).to_hex().to_string();
    let kept = task_file(state, attempt_ref, "diff");
    std::fs::write(&kept, &diff).map_err(CliError::from_native)?;
    let evidence = BTreeSet::from([
        format!("factory-session-diff:blake3:{digest}"),
        format!("aikit-delivery:{delivery_ref}"),
    ]);
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
    let said = mark_of(log)
        .map(|mark| {
            said_in_turn(
                &|request| encounter(&place.checkout, request),
                session,
                mark,
            )
        })
        .unwrap_or_default();
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

/// One read of a session's encounter: AIKit's (`encounter`), or a test's stand-in for it.
type Read<'a> = &'a dyn Fn(Value) -> Result<Value, String>;

/// Where the session stands as a task's words go (O-I #1006, review F5): the id of the newest block AIKit holds, after which the
/// task's turn is read forward by id, however many blocks the session held before. 0: it holds none.
fn turn_mark(read: Read<'_>, session: &str) -> Result<u64, String> {
    let view = read(json!({"action": "view", "agent_session": session}))?;
    Ok(view["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["id"].as_u64())
        .max()
        .unwrap_or(0))
}

/// The task's turn as AIKit holds it, and the last view read (the asks pending on the session ride on it): the blocks after
/// `mark`, read forward by id a view at a time (AIKit's view holds 16 blocks), up to the turn's end (its completed, cancelled or
/// error block) or the person's next words, whichever stands first. The task's words are an addressed send, which AIKit
/// journals as the machine's request and shows as no block, so the turn opens on the agent's first block after the mark.
fn turn(read: Read<'_>, session: &str, mark: u64) -> Result<(Vec<Value>, Value), String> {
    let mut blocks: Vec<Value> = Vec::new();
    let mut after = mark;
    loop {
        let view = read(json!({"action": "view", "agent_session": session, "after": after}))?;
        let page = view["blocks"].as_array().cloned().unwrap_or_default();
        let mut last = after;
        for block in page {
            last = block["id"].as_u64().filter(|id| *id > last).ok_or_else(|| {
                format!(
                    "AIKit's view of {session} did not read forward from block {after}: it answered block {} after {last}",
                    block["id"]
                )
            })?;
            if block["kind"] == "user" {
                return Ok((blocks, view));
            }
            let ends = matches!(
                block["kind"].as_str(),
                Some("completed" | "cancelled" | "error")
            );
            blocks.push(block);
            if ends {
                return Ok((blocks, view));
            }
        }
        if view["more"] != true || last == after {
            return Ok((blocks, view));
        }
        after = last;
    }
}

/// The agent's words in the task's turn: its Return's summary.
fn said_in_turn(read: Read<'_>, session: &str, mark: u64) -> String {
    turn(read, session, mark)
        .map(|(blocks, _)| said_in(&blocks))
        .unwrap_or_default()
}

/// The block the session stood at as the task's words went, as its log records the send (`send`).
fn mark_of(log: &Path) -> Option<u64> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|line| line["kind"] == "sent")
        .and_then(|line| line["after_block"].as_u64())
}

/// The agent's words among a turn's blocks: its messages, joined.
fn said_in(turn: &[Value]) -> String {
    turn.iter()
        .filter(|block| block["kind"] == "assistant")
        .filter_map(|block| block["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
}

/// What the follower streams of the turn: each block it has not streamed yet, or whose text has changed since it did (AIKit
/// writes a message's chunks into one block as they come), each noted in `streamed`.
fn to_stream(turn: &[Value], streamed: &mut std::collections::BTreeMap<u64, String>) -> Vec<Value> {
    turn.iter()
        .filter(|block| {
            let Some(id) = block["id"].as_u64() else {
                return false;
            };
            let text = block["text"].as_str().unwrap_or_default();
            if streamed.get(&id).is_some_and(|held| held == text) {
                return false;
            }
            streamed.insert(id, text.to_owned());
            true
        })
        .cloned()
        .collect()
}

/// `factory session task-stop <state> <run-ref> <attempt-ref>`: Stop pauses the task's turn; pressed again it cancels.
fn task_stop(
    state_text: &str,
    run_text: &str,
    attempt_ref: &str,
    grant: &str,
) -> Result<Value, CliError> {
    let authority = granted(grant)?;
    let state = PathBuf::from(state_text);
    let run_ref = run_ref_of(run_text)?;
    let place = place_of_state(&state)?;
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
    // Only the task's own turn: the session may run another task's turn by now (`stop_turn`).
    let delivery = delivery_of(&record);
    let read = |request: Value| encounter(&place.checkout, request);
    let stop_turn = || stop_turn(&read, &session, delivery.as_deref());
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
        // The task's turn stops at AIKit, if it still runs.
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

/// Stop's word to AIKit for the task's turn (O-I #1006, review F4). AIKit's cancel stops whatever turn its session runs, so it
/// is asked only while the task's own delivery holds the session's turn: AIKit keeps one active delivery a session, and the
/// task's is the one dispatching, submitted or uncertain. Otherwise the session's turn, if it runs one, is another's (the
/// task's own turn ended, or has not begun), and it is left running.
fn stop_turn(read: Read<'_>, session: &str, delivery_ref: Option<&str>) -> Result<Value, String> {
    let delivery_ref = delivery_ref
        .ok_or("the task names no delivery, so AIKit was asked to stop no turn of the session's")?;
    let phase = read(
        json!({"action": "delivery", "agent_session": session, "delivery_ref": delivery_ref}),
    )
    .map(|answer| delivery_phase(&answer, delivery_ref))
    .map_err(|words| {
        format!("AIKit was asked to stop no turn: the task's delivery could not be read: {words}")
    })?;
    match phase.as_deref() {
        Some("dispatching" | "submitted" | "uncertain") => {
            read(json!({"action": "cancel", "agent_session": session}))
        }
        Some(phase) => Err(format!(
            "the session's turn is not the task's (its delivery stands {phase}), so AIKit was asked to stop nothing"
        )),
        None => Err(format!(
            "AIKit names no phase for the task's delivery {delivery_ref}, so it was asked to stop nothing"
        )),
    }
}

/// The delivery an attempt's words went as (`send`): its dispatch's, else the one AIKit's receipt observed.
fn delivery_of(record: &FactoryAttemptRecord) -> Option<String> {
    record
        .dispatch
        .as_ref()
        .and_then(|dispatch| find_text(&dispatch.payload, "delivery_ref"))
        .or_else(|| {
            record
                .observations
                .iter()
                .find_map(|receipt| find_text(&receipt.payload, "deliveryRef"))
        })
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
        ["retry", state, run, unit, grant] => retry(state, run, unit, grant)?,
        ["choose", state, run, attempt, grant] => choose(state, run, attempt, grant)?,
        ["end", world, project, session] => end(world, project, session, None)?,
        ["task-write", world, project, session, instruction] => {
            task_write(world, project, session, instruction)?
        }
        ["task-run", state, run, session, grant] => task_run(state, run, session, grant)?,
        ["task-stop", state, run, attempt, grant] => task_stop(state, run, attempt, grant)?,
        ["task-log", state, run, attempt] => task_log(state, run, attempt)?,
        ["follow", state, run, attempt, authority] => follow(state, run, attempt, authority)?,
        _ => {
            return Err(refuse(
                "usage: factory session start <agent> <world> <project> | list <state> | end <world> <project> <session> | task-write <world> <project> <session> <instruction> | task-run <state> <run-ref> <session> <grant> | task-stop <state> <run-ref> <attempt-ref> <grant> | task-log <state> <run-ref> <attempt-ref> | retry <state> <run-ref> <unit-ref> <grant> | choose <state> <run-ref> <attempt-ref> <grant> (a grant is the person's, as the host resolved it: central:native-action-authority#<principal_ref>)",
            ))
        }
    };
    serde_json::to_string_pretty(&answer).map_err(CliError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    // REPLACED (review F1): an ask outside the checkout was refused once on the person's behalf; now it is left for them.
    #[test]
    fn the_follower_allows_once_an_ask_on_files_inside_the_checkout_and_leaves_the_rest_for_the_person(
    ) {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let file = |rest: &str| format!("{}/{rest}", checkout.display());
        let read = |path: String| json!({"kind": "read", "locations": [{"path": path}]});
        assert_eq!(
            scope_answer(&ask_of(read(file("a.md")), &ONCE), &checkout),
            Ok("yes".into())
        );
        assert!(
            scope_answer(&ask_of(read("/etc/passwd".into()), &ONCE), &checkout).is_err(),
            "outside: left, not refused"
        );
        assert!(
            scope_answer(&ask_of(read("~/notes/a.md".into()), &ONCE), &checkout).is_err(),
            "the person's home"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path(), checkout.join("out")).unwrap();
            assert!(
                scope_answer(&ask_of(read(file("out/x.md")), &ONCE), &checkout).is_err(),
                "a link leading out"
            );
        }
        assert!(
            scope_answer(&ask_of(json!({"kind": "edit"}), &ONCE), &checkout).is_err(),
            "no path declared"
        );
        assert!(
            scope_answer(
                &ask_of(read(file("a.md")), &[("always", "allow_always")]),
                &checkout
            )
            .is_err(),
            "never a standing allow"
        );
        let fetch = json!({"kind": "fetch", "locations": [{"path": file("a.md")}]});
        assert!(
            scope_answer(&ask_of(fetch, &ONCE), &checkout).is_err(),
            "a fetch, whatever it declares"
        );
    }

    /// One ask as AIKit carries it: the ACP toolCall unchanged, with the harness's choices.
    fn ask_of(call: Value, choices: &[(&str, &str)]) -> Value {
        json!({"native_request_id": "r1", "tool_call": call, "choices": choices.iter()
            .map(|(id, kind)| json!({"option_id": id, "kind": kind})).collect::<Vec<_>>()})
    }
    const ONCE: [(&str, &str); 2] = [("yes", "allow_once"), ("no", "reject_once")];

    /// The option the follower allows the ask with, if it allows it.
    fn allowed(ask: &Value, checkout: &Path) -> Option<String> {
        scope_answer(ask, checkout).ok()
    }

    /// A checkout of the test's own.
    fn checkout_of(temp: &tempfile::TempDir) -> PathBuf {
        let checkout = temp.path().join("notes");
        std::fs::create_dir_all(&checkout).unwrap();
        checkout
    }

    #[test]
    fn a_shell_command_is_never_allowed_by_the_follower() {
        // A command declares no path: whatever it touches, it is left for the person.
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let rm = json!({"kind": "execute", "title": "rm -rf ~/x", "rawInput": {"command": "rm -rf ~/x"}});
        assert_eq!(allowed(&ask_of(rm, &ONCE), &checkout), None);
    }

    /// An edit as a harness declares what it writes: the file, and its diff at that path.
    fn edit_at(path: &str) -> Value {
        json!({"kind": "edit", "locations": [{"path": path}],
            "content": [{"type": "diff", "path": path, "oldText": null, "newText": "x"}]})
    }

    #[test]
    fn an_edit_that_climbs_out_of_the_checkout_is_never_allowed() {
        // `..` climbs out of the checkout, however the path begins.
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let climb = edit_at(&format!("{}/../../x", checkout.display()));
        assert_eq!(allowed(&ask_of(climb, &ONCE), &checkout), None);
    }

    /// O-I #1006 (re-check 1a): codex's apply_patch with `*** Move to: <outside>` asks as an edit of the file the move
    /// starts from, with no content, so where the file lands is nowhere in the ask. The follower allows only an ask that
    /// declares every place it writes: an edit with no content stays with the person, unless the harness's input names
    /// where it moves the file, and then that is read.
    #[test]
    fn a_move_out_of_the_checkout_is_never_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let inside = format!("{}/notes.md", checkout.display());
        let kept = format!("{}/kept.md", checkout.display());
        let outside = format!("{}/stolen.md", temp.path().display());
        // codex-acp 1.13.1's ask for the move, as it builds it (fileChangeToolCall): the file it starts from, nothing else.
        let codex = json!({"toolCallId": "c1", "kind": "edit", "status": "pending",
            "title": "Edit files", "locations": [{"path": inside}]});
        assert_eq!(allowed(&ask_of(codex, &ONCE), &checkout), None);
        // Where the input names the move's target, it is read: out is left for the person, in is allowed once.
        let moved = |to: &str| {
            let mut call = edit_at(&inside);
            call["rawInput"] =
                json!({"changes": {inside.clone(): {"type": "update", "move_path": to}}});
            call
        };
        assert_eq!(allowed(&ask_of(moved(&outside), &ONCE), &checkout), None);
        assert_eq!(
            allowed(&ask_of(moved(&kept), &ONCE), &checkout),
            Some("yes".into())
        );
        let bare = |to: &str| json!({"kind": "edit", "locations": [{"path": inside}], "rawInput": {"move_path": to}});
        assert_eq!(allowed(&ask_of(bare(&outside), &ONCE), &checkout), None);
        assert_eq!(
            allowed(&ask_of(bare(&kept), &ONCE), &checkout),
            Some("yes".into())
        );
    }

    /// O-I #1006 (re-check 1b): a path is judged where the filesystem takes it. A link is followed before a `..` after it
    /// is taken, so the `..` climbs from where the link leads; a link that leads nowhere is outside, since a write through
    /// it lands wherever it points.
    #[cfg(unix)]
    #[test]
    fn a_link_in_the_checkout_never_carries_a_write_out() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(outside.join("deeper")).unwrap();
        std::fs::create_dir_all(checkout.join("documentation")).unwrap();
        let link = |name: &str, to: &Path| {
            std::os::unix::fs::symlink(to, checkout.join(name)).unwrap();
        };
        link("link", &outside.join("deeper"));
        link("dangling", &outside.join("authorized_keys"));
        link("docs", &checkout.join("documentation"));
        let at = |path: &str| {
            let edit = edit_at(&format!("{}/{path}", checkout.display()));
            allowed(&ask_of(edit, &ONCE), &checkout)
        };
        assert_eq!(
            at("link/../x.md"),
            None,
            "a `..` after a link climbs from where it leads"
        );
        assert_eq!(
            at("dangling"),
            None,
            "a dangling link writes where it points"
        );
        assert_eq!(at("dangling/x"), None);
        assert_eq!(
            at("new/../link/x.md"),
            None,
            "a link past a folder still to be made"
        );
        assert_eq!(
            at("docs/a.md"),
            Some("yes".into()),
            "a link that stays inside"
        );
        assert_eq!(
            at("new/a.md"),
            Some("yes".into()),
            "a file still to be made"
        );
    }

    #[test]
    fn an_edits_new_text_is_never_read_as_a_path() {
        // An edit inside the checkout is allowed once, whatever its new text says.
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let file = format!("{}/a.rs", checkout.display());
        let comment = json!({"kind": "edit", "locations": [{"path": file}],
            "content": [{"type": "diff", "path": file, "oldText": "x", "newText": "// SAFETY: y"}]});
        assert_eq!(
            allowed(&ask_of(comment, &ONCE), &checkout),
            Some("yes".into())
        );
    }

    /// O-I #1006 (re-check 2, F322-1): hermes-acp asks for a V4A patch of several files as one edit whose one diff's path
    /// joins every file with ", " (acp_adapter/edit_approval.py, `_proposal_for_patch_v4a`), and that is no file. Each file
    /// the patch writes is read from the patch as hermes's executor reads it (tools/patch_parser.py: `***`, any space or
    /// none, then `Update File:`, `Add File:`, `Delete File:` or `Move File: a -> b`) and judged as its own path, and the
    /// diff's path as the files it joins, one each; a diff's path that does not read so leaves the ask for the person.
    #[test]
    fn each_file_a_patch_writes_is_judged_as_its_own_path() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let inside = |name: &str| format!("{}/{name}", checkout.display());
        let outside = format!("{}/stolen.md", temp.path().display());
        // hermes's ask for `patch` in mode `patch`, as build_acp_edit_tool_call builds it, `shown` its diff's path, and its
        // two choices (make_acp_edit_approval_requester).
        let hermes = |shown: &str, patch: &str| {
            json!({"toolCallId": "edit-approval-1", "title": format!("Approve edit: {shown}"),
                "kind": "edit", "status": "pending",
                "content": [{"type": "diff", "path": shown, "oldText": null, "newText": patch}],
                "rawInput": {"tool": "patch", "arguments": {"mode": "patch", "patch": patch}}})
        };
        let choices = [("allow_once", "allow_once"), ("deny", "reject_once")];
        let at = |shown: &str, files: &[String]| {
            let patch = format!("*** Begin Patch\n{}*** End Patch\n", files.concat());
            allowed(&ask_of(hermes(shown, &patch), &choices), &checkout)
        };
        let update = |path: &str| format!("*** Update File: {path}\n@@\n-x\n+y\n");
        let (a, b) = (inside("a.md"), inside("b.md"));
        let yes = Some("allow_once".to_owned());
        assert_eq!(at(&a, &[update(&a)]), yes, "one file inside");
        assert_eq!(
            at(&format!("{a}, {b}"), &[update(&a), update(&b)]),
            yes,
            "two files inside"
        );
        assert_eq!(
            at(&format!("{a}, {outside}"), &[update(&a), update(&outside)]),
            None,
            "a file outside, in the join"
        );
        assert_eq!(
            at(
                &format!("{a}, {outside}"),
                &[update(&a), format!("*** Add File: {outside}\n+z\n")]
            ),
            None,
            "a file made outside"
        );
        assert_eq!(
            at(
                &format!("{a}, {outside}"),
                &[update(&a), format!("*** Delete File: {outside}\n")]
            ),
            None,
            "a file deleted outside"
        );
        assert_eq!(
            at(
                &format!("{a}, {b}"),
                &[format!("*** Move File: {a} -> {b}\n")]
            ),
            yes,
            "a move inside: hermes names both its files"
        );
        assert_eq!(
            at(
                &format!("{a}, {outside}"),
                &[format!("*** Move File: {a} -> {outside}\n")]
            ),
            None,
            "a move out"
        );
        // A file line with no space after `***`: the executor writes it, and hermes's own reading of the patch for its ask
        // passes over it, so the ask names only the file inside.
        assert_eq!(
            at(
                &a,
                &[
                    update(&a),
                    format!("***Update File: {outside}\n@@\n-x\n+y\n")
                ]
            ),
            None,
            "a file the ask does not name"
        );
        // A relative file is written where the session stands, the checkout.
        assert_eq!(
            at(
                &format!("{a}, {}", temp.path().join("x.md").display()),
                &[update("a.md"), update("../x.md")]
            ),
            None,
            "a `..` out of the checkout"
        );
        assert_eq!(
            at(&a, &[update(&a), update(&b)]),
            None,
            "two files and one path"
        );
        assert_eq!(
            at(&format!("{a}, {b}"), &[]),
            None,
            "a patch that names no file"
        );
    }

    /// O-I #1006 (re-check 2, the .git hole): the follower never allows a write into the checkout's git directory, whose
    /// config and hooks git runs as the person when `standing` reads the checkout at the task's end (core.fsmonitor, a
    /// hook), outside any harness's sandbox. The git directory is any folder named `.git` in any case (the Mac's disk reads
    /// `.GIT` as `.git`), a nested repository's among them, and the checkout's own wherever git keeps it (`git rev-parse
    /// --git-dir`, `--git-common-dir`): a gitdir kept apart (`--separate-git-dir`), even inside the checkout under another name.
    #[cfg(unix)]
    #[test]
    fn a_write_into_the_checkouts_git_directory_is_never_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(dir)
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        let write = |checkout: &Path, path: &str| {
            let edit = edit_at(&format!("{}/{path}", checkout.display()));
            allowed(&ask_of(edit, &ONCE), checkout)
        };
        let checkout = checkout_of(&temp);
        git(&checkout, &["init", "--quiet"]);
        assert_eq!(
            write(&checkout, "NOTES.md"),
            Some("yes".into()),
            "a file of the checkout's"
        );
        for path in [
            ".git/config",
            ".git/hooks/post-index-change",
            ".git",
            ".GIT/config",
            "vendor/.git/config",
        ] {
            assert_eq!(write(&checkout, path), None, "{path}");
        }
        std::os::unix::fs::symlink(checkout.join(".git"), checkout.join("store")).unwrap();
        assert_eq!(write(&checkout, "store/config"), None, "a link into it");
        let delete = json!({"kind": "delete",
            "locations": [{"path": format!("{}/.git/index", checkout.display())}]});
        assert_eq!(allowed(&ask_of(delete, &ONCE), &checkout), None, "a delete");
        // A read there writes nothing.
        let read = json!({"kind": "read",
            "locations": [{"path": format!("{}/.git/HEAD", checkout.display())}]});
        assert_eq!(allowed(&ask_of(read, &ONCE), &checkout), Some("yes".into()));
        // A gitdir kept apart, inside the checkout under another name.
        let apart = temp.path().join("apart");
        std::fs::create_dir_all(&apart).unwrap();
        let store = apart.join("gitstore");
        git(
            &apart,
            &[
                "init",
                "--quiet",
                "--separate-git-dir",
                store.to_str().unwrap(),
            ],
        );
        assert_eq!(write(&apart, "gitstore/config"), None, "its gitdir");
        if apart.join("GITSTORE").exists() {
            assert_eq!(
                write(&apart, "GITSTORE/hooks/x"),
                None,
                "its gitdir, as the disk reads another case"
            );
        }
        assert_eq!(write(&apart, "notes.md"), Some("yes".into()));
    }

    /// O-I #1006 (re-check 2, the .git hole): reading how the checkout stands, and the difference a turn left, runs nothing
    /// the checkout's git config or hooks name: git's fsmonitor and hooks are off, whatever was written there.
    #[cfg(unix)]
    #[test]
    fn reading_the_checkout_runs_nothing_its_git_names() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let git = |args: &[&str]| {
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
        git(&["init", "--quiet", "--initial-branch", "main"]);
        std::fs::write(checkout.join("README.md"), "notes\n").unwrap();
        git(&["add", "README.md"]);
        git(&["commit", "--quiet", "-m", "first"]);
        let ran = temp.path().join("ran");
        let script = temp.path().join("planted.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\necho \"$0 $*\" >> '{}'\n", ran.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        git(&["config", "core.fsmonitor", script.to_str().unwrap()]);
        let hooks = checkout.join(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        for hook in ["post-index-change", "pre-auto-gc", "reference-transaction"] {
            std::fs::copy(&script, hooks.join(hook)).unwrap();
        }
        std::fs::write(checkout.join("README.md"), "notes\nmore\n").unwrap();
        std::fs::write(checkout.join("new.md"), "new\n").unwrap();
        let before = standing(&checkout);
        let left = difference(&checkout, &std::collections::BTreeMap::new());
        assert!(
            !ran.exists(),
            "git ran {}",
            std::fs::read_to_string(&ran).unwrap_or_default()
        );
        assert!(before.contains_key("README.md") && before.contains_key("new.md"));
        assert_eq!(left.files, ["README.md", "new.md"], "{}", left.text);
    }

    /// O-I #1006 (act 4 on claude-code, 14 Nov): claude-agent-acp 0.81.0 asks for Claude Code's Edit and Write as edits with
    /// their diff at the file (permissions/presentation.js over tools.js `toolInfoFromToolUse`), and the follower allows them
    /// once inside the checkout. A file changed by a shell command (`printf … >> NOTES.md`) is asked as `execute`, its
    /// location the file the CLI saw it touch; a command can act anywhere, so it stays with the person.
    #[test]
    fn claude_codes_edits_inside_are_allowed_once_and_its_commands_left_for_the_person() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = checkout_of(&temp);
        let notes = format!("{}/NOTES.md", checkout.display());
        let choices = [
            ("allow-once", "allow_once"),
            ("allow-with-updates", "allow_always"),
            ("reject", "reject_once"),
        ];
        let edit = json!({"toolCallId": "toolu_1", "name": "Edit", "status": "pending",
            "rawInput": {"file_path": notes, "old_string": "# Notes\n", "new_string": "# Notes\nThe gate ran.\n"},
            "title": "Edit NOTES.md", "kind": "edit",
            "content": [{"type": "diff", "path": notes, "oldText": "# Notes\n", "newText": "# Notes\nThe gate ran.\n"}],
            "locations": [{"path": notes}]});
        assert_eq!(
            allowed(&ask_of(edit, &choices), &checkout),
            Some("allow-once".into())
        );
        let write = json!({"toolCallId": "toolu_2", "name": "Write", "status": "pending",
            "rawInput": {"file_path": notes, "content": "# Notes\n"},
            "title": "Write NOTES.md", "kind": "edit",
            "content": [{"type": "diff", "path": notes, "oldText": null, "newText": "# Notes\n"}],
            "locations": [{"path": notes}]});
        assert_eq!(
            allowed(&ask_of(write, &choices), &checkout),
            Some("allow-once".into())
        );
        let command = "printf 'The foundation gate ran a task here.\\n' >> NOTES.md && git diff && git status --short";
        let bash = json!({"toolCallId": "toolu_3", "name": "Bash", "status": "pending", "kind": "execute",
            "title": command, "rawInput": {"command": command, "description": "Append line and read back diff"},
            "content": [{"type": "content", "content": {"type": "text", "text": "Append line and read back diff"}}],
            "locations": [{"path": notes}]});
        assert_eq!(allowed(&ask_of(bash, &choices), &checkout), None);
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
            Difference {
                text: String::new(),
                files: Vec::new()
            },
            "the pool link alone is no difference"
        );
        // What stood in the checkout when the task was sent is not the agent's.
        std::fs::write(checkout.join("mine.md"), "the person's own\n").unwrap();
        std::os::unix::fs::symlink(temp.path().join("pool"), checkout.join("projected")).unwrap();
        let before = standing(&checkout);
        std::fs::write(checkout.join("README.md"), "notes\nmore\n").unwrap();
        std::fs::write(checkout.join("new.md"), "new\n").unwrap();
        let diff = difference(&checkout, &before).text;
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
        assert!(difference(&checkout, &before)
            .text
            .contains("+and the agent's"));
    }

    /// O-I #1006 (review F7): a turn's difference names every file it moved, as git's porcelain reads it: one that only deletes
    /// a tracked file returns that deletion, and a rename names the file it left and the file it made.
    #[cfg(unix)]
    #[test]
    fn a_task_that_only_deletes_or_renames_tracked_files_leaves_a_difference() {
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
        for file in ["gone.md", "old.md", "kept.md"] {
            std::fs::write(checkout.join(file), format!("{file}\n")).unwrap();
        }
        run(&["add", "."]);
        run(&["commit", "--quiet", "-m", "first"]);
        let before = standing(&checkout);
        std::fs::remove_file(checkout.join("gone.md")).unwrap();
        let deleted = difference(&checkout, &before);
        assert_eq!(deleted.files, ["gone.md"], "{}", deleted.text);
        assert!(
            deleted.text.contains("deleted file mode"),
            "{}",
            deleted.text
        );
        run(&["mv", "old.md", "new.md"]);
        let moved = difference(&checkout, &before);
        assert_eq!(
            moved.files,
            ["gone.md", "new.md", "old.md"],
            "{}",
            moved.text
        );
        assert!(moved.text.contains("+++ b/new.md"), "{}", moved.text);
        assert!(moved.text.contains("--- a/old.md"), "{}", moved.text);
        // A deletion staged with `git rm` is read against HEAD, which still holds the file.
        run(&["rm", "--quiet", "kept.md"]);
        let removed = difference(&checkout, &before);
        assert_eq!(
            removed.files,
            ["gone.md", "kept.md", "new.md", "old.md"],
            "{}",
            removed.text
        );
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
    fn a_written_task_names_the_unit_its_run_runs_before_its_first_attempt() {
        let (_dir, state, run, _attempt) = started_task();
        let started = reading(&state, &run).unwrap();
        let unit = started
            .legs
            .keys()
            .next()
            .cloned()
            .expect("a started task's leg");
        assert_eq!(task_unit(&started), Some(unit.clone()));
        // Written and not yet run: a session task's Run requires its one unit, and holds no leg until its first attempt starts.
        let mut written = started;
        written.legs.clear();
        written.required_units = BTreeSet::from([unit.clone()]);
        assert_eq!(
            task_unit(&written),
            Some(unit),
            "factory.task.write answers the unit factory.task.run runs, and factory.attempt.retry takes"
        );
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

    /// O-I #1006 (review F4): Stop asks AIKit to cancel a session's turn only while the task's own delivery holds it. Task A
    /// paused (its turn cancelled at the first press) and task B running on the same session: Stop pressed on A again cancels A
    /// at Factory and leaves B's turn running; B's own Stop stops B's turn.
    #[test]
    fn a_second_stop_on_a_paused_task_leaves_the_next_task_on_its_session_running() {
        let asked = std::cell::RefCell::new(Vec::<Value>::new());
        // AIKit as it answers: A's delivery cancelled at the first press, B's dispatching (a session holds one active delivery).
        let read = |request: Value| -> Result<Value, String> {
            asked.borrow_mut().push(request.clone());
            match request["action"].as_str() {
                Some("delivery") => {
                    let phase = if request["delivery_ref"] == "delivery:a" {
                        "cancelled"
                    } else {
                        "dispatching"
                    };
                    Ok(
                        json!({"delivery": {"delivery_ref": request["delivery_ref"], "phase": phase}}),
                    )
                }
                Some("cancel") => Ok(json!({"commands": 1, "requested_at_sequence": 7})),
                _ => Err(format!("AIKit was asked {request}")),
            }
        };
        let cancels = || {
            asked
                .borrow()
                .iter()
                .filter(|request| request["action"] == "cancel")
                .count()
        };
        assert!(
            stop_turn(&read, "session:shared", Some("delivery:a")).is_err(),
            "A's turn is not the session's"
        );
        assert_eq!(
            cancels(),
            0,
            "B carries on: AIKit was asked to cancel nothing"
        );
        assert!(stop_turn(&read, "session:shared", Some("delivery:b")).is_ok());
        assert_eq!(cancels(), 1, "B's own Stop stops B's turn");
        assert!(
            stop_turn(&read, "session:shared", None).is_err(),
            "a task that names no delivery stops no turn"
        );
        assert_eq!(cancels(), 1);
    }

    /// O-I #1006 (review F6): another track's write between an attempt's start and its send leaves the send's reading stale, and
    /// Factory's dispatch sends nothing on it. The send is decided again on the Run read again and goes; a Run that moves under
    /// every reading sends nothing, and the attempt, started and never sent, is failed rather than left Active, so a retry runs
    /// it again.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_write_between_an_attempts_start_and_its_send_neither_sends_it_stale_nor_strands_it() {
        let (by, grant) = (caller("human:owner"), authority("authority:test"));
        // Another track's write: the store's revision is the project's, so any write moves it.
        let interleave = |state: &Path, run: &RunRef, attempt: &str, n: u64| {
            act(
                state,
                run,
                &follower(Some("human:owner")),
                &grant,
                FactoryAttemptOperation::RecordTracking {
                    attempt_ref: attempt.to_owned(),
                    fact: AttemptTrackingFact {
                        fact_ref: format!("fact:another-track-{n}"),
                        kind: "session".into(),
                        owner_ref: "aikit".into(),
                        subject_ref: "session:another-track".into(),
                        source_revision: now_rfc3339(),
                        evidence_refs: BTreeSet::from([format!("another-track:{n}")]),
                    },
                },
            )
            .unwrap();
        };
        // Factory's own dispatch, on the revision it is given: stale, it refuses before anything goes.
        let dispatch = |state: &Path, run: &RunRef, attempt: &str, revision: u64| {
            execute_attempt_owner_action(
                state,
                FactoryAttemptOwnerRequest {
                    contract: FACTORY_ATTEMPT_OWNER_ACTION.into(),
                    request_ref: "owner-request:f6".into(),
                    projection_ref: "projection:f6".into(),
                    caller: by.clone(),
                    run_ref: run.clone(),
                    expected_revision: revision,
                    authority: grant.clone(),
                    attempt_ref: attempt.to_owned(),
                    execution_ref: "execution:f6".into(),
                    invocation: NativeOwnerInvocation::AikitEncounter {
                        binary: PathBuf::from("/nonexistent/aikit"),
                        cwd: PathBuf::from("/nonexistent"),
                        transport: None,
                        contract_revision: AIKIT_CAW_CONTRACT_REVISION.into(),
                        request: json!({"action": "send"}),
                    },
                },
            )
            .map(|_| revision)
        };
        // A write lands once between the reading and the dispatch: the send goes on the Run read again.
        let (_dir, state, run, attempt) = started_task();
        let asked = std::cell::RefCell::new(Vec::new());
        let sent = send_fresh(
            &state,
            &run,
            &attempt,
            &by,
            &grant,
            "delivery:f6",
            |revision| {
                asked.borrow_mut().push(revision);
                if asked.borrow().len() == 1 {
                    interleave(&state, &run, &attempt, 1);
                    return dispatch(&state, &run, &attempt, revision);
                }
                Ok(revision)
            },
        );
        let asked = asked.into_inner();
        assert_eq!(
            asked.len(),
            2,
            "decided again on the Run read again: {asked:?}"
        );
        assert_eq!(
            sent.ok(),
            Some(reading(&state, &run).unwrap().revision),
            "the send goes on the reading that holds the other track's write"
        );
        // The Run moves under every reading: nothing goes, and the attempt is failed, never sent.
        let (_dir, state, run, attempt) = started_task();
        let tried = std::cell::Cell::new(0);
        let refused = send_fresh(
            &state,
            &run,
            &attempt,
            &by,
            &grant,
            "delivery:f6",
            |revision| {
                tried.set(tried.get() + 1);
                interleave(&state, &run, &attempt, tried.get());
                dispatch(&state, &run, &attempt, revision)
            },
        )
        .expect_err("nothing goes on a stale reading");
        assert!(refused.to_string().contains("never sent"), "{refused}");
        let now = reading(&state, &run).unwrap();
        let record = record_of(&now, &attempt).unwrap();
        assert_eq!(leg_status(&now, record), LegStatus::Failed, "{refused}");
        assert!(record
            .failure_evidence_refs
            .contains("factory-session-unsent:delivery:f6"));
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

    /// A World as Central keeps one: its `Control/`, and one project whose ProjectCentral manifest names its Central id.
    fn world_with_project(project: &str, id: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let world = temp.path().canonicalize().unwrap();
        std::fs::create_dir_all(world.join("Control")).unwrap();
        let central = world.join("Work").join(project).join("ProjectCentral");
        std::fs::create_dir_all(&central).unwrap();
        std::fs::write(
            central.join("project.json"),
            serde_json::to_vec(&json!({"schema": "central.project/v1", "project_id": id})).unwrap(),
        )
        .unwrap();
        (temp, world)
    }

    /// O-I #1006 (review F5): a session that already holds 20 blocks streams the task's turn, each block once and again as it
    /// grows, through a turn longer than one view, and the turn's Return summary holds that turn's words alone: the turn is read
    /// forward by block id from where the session stood as the words went, never by a count over AIKit's 16-block view. A task's
    /// words are an addressed send, which AIKit journals as the machine's request and shows as no block (aikit-store
    /// encounter_delivery.rs `reserve_with_phase`, encounter.rs `project_block`): the turn opens on the agent's first block and
    /// closes on its end, before the next task's turn or the person's next words.
    #[test]
    fn a_session_holding_twenty_blocks_streams_the_tasks_turn_and_its_summary_holds_only_that_turn()
    {
        let held = std::cell::RefCell::new(Vec::<Value>::new());
        // Block ids are AIKit's store rowids, shared by every session: one session's run with gaps.
        let next = std::cell::Cell::new(100u64);
        let add = |kind: &str, text: &str| {
            held.borrow_mut()
                .push(json!({"id": next.get(), "kind": kind, "text": text}));
            next.set(next.get() + 3);
        };
        // AIKit's view as its store reads it (aikit-store encounter.rs `view_page`): the newest 16 blocks, oldest first, or
        // with `after` the 16 after that block, oldest first; `more` when a 17th stands.
        let read = |request: Value| -> Result<Value, String> {
            let blocks = held.borrow();
            let mut page: Vec<Value> = match request["after"].as_u64() {
                Some(after) => blocks
                    .iter()
                    .filter(|block| block["id"].as_u64().unwrap() > after)
                    .take(17)
                    .cloned()
                    .collect(),
                None => blocks.iter().rev().take(17).cloned().collect(),
            };
            let more = page.len() > 16;
            page.truncate(16);
            if request["after"].as_u64().is_none() {
                page.reverse();
            }
            Ok(json!({"blocks": page, "more": more, "permissions": []}))
        };
        for turn in 0..5 {
            add("user", &format!("earlier question {turn}"));
            add("thinking", &format!("earlier thought {turn}"));
            add("assistant", &format!("earlier answer {turn}. "));
            add("completed", "");
        }
        // The task's words go: AIKit shows no block for them.
        let mark = turn_mark(&read, "session").unwrap();
        add("thinking", "Reading NOTES.md");
        add("assistant", "Appending");
        let texts = |blocks: Vec<Value>| -> Vec<String> {
            blocks
                .iter()
                .map(|block| block["text"].as_str().unwrap_or_default().to_owned())
                .collect()
        };
        let mut streamed = std::collections::BTreeMap::new();
        let (blocks, _) = turn(&read, "session", mark).unwrap();
        assert_eq!(
            texts(to_stream(&blocks, &mut streamed)),
            ["Reading NOTES.md", "Appending"],
            "the turn's first blocks stream, nothing of the 20 blocks before"
        );
        held.borrow_mut().last_mut().unwrap()["text"] = json!("Appending the line now.");
        for edit in 0..17 {
            add("tool", &format!("{{\"title\":\"edit {edit}\"}}"));
        }
        add("completed", "");
        let (blocks, _) = turn(&read, "session", mark).unwrap();
        let streamed_now = texts(to_stream(&blocks, &mut streamed));
        assert_eq!(
            streamed_now.len(),
            19,
            "the grown answer, 17 edits over two views, the turn's end: {streamed_now:?}"
        );
        assert_eq!(streamed_now[0], "Appending the line now.");
        assert_eq!(streamed_now[18], "");
        assert!(
            texts(to_stream(&blocks, &mut streamed)).is_empty(),
            "streamed once"
        );
        // The next task's turn on the session (an addressed send again: no block for its words), then the person's.
        add("assistant", "Renamed the file.");
        add("completed", "");
        add("user", "and what else did you see?");
        add("assistant", "Nothing else.");
        add("completed", "");
        let (blocks, _) = turn(&read, "session", mark).unwrap();
        assert!(
            texts(to_stream(&blocks, &mut streamed)).is_empty(),
            "nothing after the turn's end streams as the task's"
        );
        assert_eq!(
            said_in_turn(&read, "session", mark),
            "Appending the line now.",
            "the summary holds the task's turn alone: no earlier answer, not the next task's, not the person's next turn"
        );
    }

    /// The session's Agency binding as AIKit answers it (`encounter-agency-read`).
    fn seated_binding() -> Binding {
        Binding {
            agent_ref: "agent/claude-code".into(),
            agency_ref: "agency:aikit-mint-notes-00000000".into(),
            revision: "revision/1".into(),
            sender: "human/owner".into(),
            source_ref: "source/agency-notes".into(),
            source_revision: "revision/1".into(),
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_task_is_written_on_a_project_the_windows_setup_placed() {
        // The window's Setup places a project through Factory's own `factory project setup-central` (the kernel's
        // factory_owner project-setup), under the key Factory encodes from the project's Central id. A task written on a
        // session there is commissioned under that key, and admitted; the placement stays as Setup made it.
        let (_temp, world) = world_with_project("notes", "Notes");
        let root = world.join("Work/notes");
        let setup = crate::project_setup::setup_central(
            &root,
            "Notes",
            &root.join("ProjectCentral/project.json"),
        )
        .unwrap();
        let place = place(world.to_str().unwrap(), "notes").unwrap();
        let written = commission_task(
            &place,
            &seated_binding(),
            &["skill/central-session-strap".to_owned()],
            "agent-session/notes-1",
            "Write the notes' index",
        )
        .unwrap();
        assert_eq!(written["state"], "written", "{written}");
        let located = crate::project_setup::locate(&root).unwrap();
        assert_eq!(located["projectKey"], setup["projectKey"], "{located}");
        assert_eq!(located["runCount"], 1, "{located}");
    }
}
