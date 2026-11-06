//! A piece of work shown in the field (DLW §6, §17.2; story SF12).
//!
//! "Show it, and the people you let in watch it being made: its agents at
//! work, what they have produced and where it stands, without its prompts,
//! keys or machines." Factory owns the Run, so Factory says what of it may be
//! shown: `factory development show <state> <run-ref>` answers
//! `factory.shown-work/v1`, read from the Run's own records at read time.
//!
//! What it carries is chosen, never filtered: the work's name (its
//! destination), where it stands (live while the Run is open, kept once it
//! closes), each agent at work by its label with one word for how its work
//! stands (waits, runs, stalls, returned, done, failed, stopped, idle), what
//! the agents have produced by label and standing, and the count of tasks by
//! those words. It never reads a prompt or question (human requests, thought),
//! a grant, a key, a file (artifact, evidence, preview or native refs) or a
//! machine (harness, session, session space, surface, workcell, actuation). A
//! label that names one anyway — a local path, a key, an address, a session —
//! is withheld whole, and the reading counts what it withheld.
//!
//! Watching changes nothing in the run: this is a read of the state as it is
//! stored, with no lock taken and nothing written.

use crate::build::{ExecutionStatus, FACTORY_NATIVE_OWNER};
use crate::core::run::{RunLifecycle, RunRef};
use crate::developmental_read::{FactoryDevelopmentalReadError, FactoryDevelopmentalState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const FACTORY_SHOWN_WORK: &str = "factory.shown-work/v1";
pub const SHOW_USAGE: &str = "factory development show <state> <run-ref> [--json]";
pub fn help() -> String {
    format!("A work shown in the field (its agents at work, what they made and where it stands; never its prompts, keys, files or machines):\n  {SHOW_USAGE}")
}

/// The longest label shown; a longer one is cut at a character boundary.
const MAX_SHOWN_CHARS: usize = 200;
/// The name a work carries when its own names something not to be shown.
const UNNAMED_WORK: &str = "A piece of work";

/// What the people let in see of one Run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShownWork {
    pub schema: String,
    pub owner: String,
    pub work_ref: String,
    /// What a watcher's copy was shown at: the Run's revision and its Factory
    /// state's together, which move forward whenever anything shown moves.
    pub revision: u64,
    pub name: String,
    /// `live` while the Run is open, `kept` once it has closed.
    pub standing: String,
    pub lifecycle: String,
    pub agents: Vec<ShownAgent>,
    pub made: Vec<ShownMade>,
    /// Tasks by how they stand, in the agents' words.
    pub tasks: BTreeMap<String, u64>,
    /// Labels withheld because they named a file, a key, a session or a machine.
    pub withheld: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShownAgent {
    pub agent: String,
    /// How its work stands now: waits, runs, stalls, returned, done, failed,
    /// stopped, or idle with no task.
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShownMade {
    pub label: String,
    pub standing: String,
}

/// Why a work cannot be shown, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowRefusal {
    pub code: &'static str,
    pub message: String,
}

impl ShowRefusal {
    pub fn document(&self) -> serde_json::Value {
        serde_json::json!({"schema":"factory.refusal/v1","code":self.code,"fact":self.message,
            "consequence":"No reading was produced; the developmental state is unchanged.",
            "action":format!("run: {SHOW_USAGE}")})
    }
}

impl std::fmt::Display for ShowRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ShowRefusal {}

/// One task's word, from Factory's closed execution vocabulary. A
/// conformance fixture is no work, so it stands as none.
fn task_word(status: ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Queued => "waits",
        ExecutionStatus::Running => "runs",
        ExecutionStatus::Blocked => "stalls",
        ExecutionStatus::Returned => "returned",
        ExecutionStatus::Success => "done",
        ExecutionStatus::Fail => "failed",
        ExecutionStatus::Cancelled => "stopped",
        ExecutionStatus::ContractFixture => "idle",
    }
}

/// Which word an agent with several tasks stands at: the liveliest.
fn rank(word: &str) -> u8 {
    match word {
        "runs" => 0,
        "stalls" => 1,
        "waits" => 2,
        "returned" => 3,
        "failed" => 4,
        "done" => 5,
        "stopped" => 6,
        _ => 7,
    }
}

fn lifecycle_word(lifecycle: RunLifecycle) -> &'static str {
    match lifecycle {
        RunLifecycle::Seeded => "seeded",
        RunLifecycle::Active => "active",
        RunLifecycle::WaitingHuman => "waiting_human",
        RunLifecycle::Suspended => "suspended",
        RunLifecycle::Finishing => "finishing",
        RunLifecycle::Finished => "finished",
        RunLifecycle::Aborted => "aborted",
        RunLifecycle::Archived => "archived",
    }
}

/// Whether a word names something never shown: a local path, a key or token,
/// an address or a machine, a session. The field refuses the same material
/// server-side (shared-field FIELD_TIME_LEAKS); Factory never sends it.
pub fn names_the_unshown(text: &str) -> bool {
    const MARKS: [&str; 22] = [
        "/Users/",
        "/home/",
        "~/",
        "file://",
        "ws://",
        "wss://",
        "http://",
        "https://",
        "Bearer ",
        "AIKIT_GATEWAY_TOKEN",
        ".central/",
        "native-token",
        "owner-token",
        "agent-session",
        "session-space",
        "session:",
        "harness:",
        "workcell:",
        "machine:",
        "-----BEGIN",
        ".ts.net",
        "localhost",
    ];
    if MARKS.iter().any(|mark| text.contains(mark)) {
        return true;
    }
    text.split(|c: char| c.is_whitespace() || "\"'()[]{}<>,;=".contains(c))
        .any(|token| {
            let local = [
                "/private/",
                "/tmp/",
                "/var/",
                "/etc/",
                "/opt/",
                "/Volumes/",
                "/mnt/",
            ]
            .iter()
            .any(|root| token.starts_with(root))
                || token.starts_with("./")
                || token.starts_with("../")
                || token.contains(":\\");
            let keyed = [
                "sk-",
                "sk_",
                "ghp_",
                "gho_",
                "ghs_",
                "github_pat_",
                "xoxb-",
                "xoxp-",
                "AKIA",
            ]
            .iter()
            .any(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + 8)
                || (token.len() >= 32
                    && token
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_+/=-".contains(c))
                    && token.chars().any(|c| c.is_ascii_digit())
                    && token.chars().any(|c| c.is_ascii_alphabetic()));
            let addressed = {
                let host = token.split(':').next().unwrap_or_default();
                let parts: Vec<&str> = host.split('.').collect();
                parts.len() == 4
                    && parts.iter().all(|part| {
                        !part.is_empty()
                            && part.len() <= 3
                            && part.chars().all(|c| c.is_ascii_digit())
                    })
            };
            local || keyed || addressed
        })
}

/// A label as shown: trimmed, cut to length, or None when it names
/// something never shown.
fn shown(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || names_the_unshown(text) {
        return None;
    }
    Some(match text.char_indices().nth(MAX_SHOWN_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    })
}

/// The shown work of one Run, from the state as it stands.
pub fn shown_work(
    state: &FactoryDevelopmentalState,
    run_ref: &RunRef,
) -> Result<ShownWork, ShowRefusal> {
    let reading = state.run_reading(run_ref).map_err(|error| match error {
        FactoryDevelopmentalReadError::RunNotFound(run) => ShowRefusal {
            code: "run_not_found",
            message: format!("This Factory state holds no Run {run} to show"),
        },
        other => ShowRefusal {
            code: "unreadable",
            message: format!("The Run could not be read to show it: {other}"),
        },
    })?;
    let mut withheld = 0u64;
    let mut keep = |text: &str| {
        let kept = shown(text);
        if kept.is_none() {
            withheld += 1;
        }
        kept
    };
    let name = keep(&reading.destination).unwrap_or_else(|| UNNAMED_WORK.to_owned());
    let mut tasks: BTreeMap<String, u64> = BTreeMap::new();
    let mut at_of: BTreeMap<&str, &'static str> = BTreeMap::new();
    for execution in &reading.executions {
        let word = ExecutionStatus::parse(&execution.status)
            .map(task_word)
            .unwrap_or("idle");
        if word != "idle" {
            *tasks.entry(word.to_owned()).or_default() += 1;
        }
        if let Some(agency) = execution.agency_ref.as_deref() {
            let held = at_of.entry(agency).or_insert(word);
            if rank(word) < rank(held) {
                *held = word;
            }
        }
    }
    let mut agents = Vec::new();
    for agency in &reading.agencies {
        let at = at_of
            .get(agency.agency_ref.as_str())
            .copied()
            .unwrap_or("idle");
        if let Some(agent) = keep(&agency.label) {
            agents.push(ShownAgent {
                agent,
                at: at.to_owned(),
            });
        }
    }
    let mut made = Vec::new();
    for candidate in &reading.candidates {
        let standing = if !candidate.status.is_empty()
            && candidate.status.len() <= 32
            && candidate
                .status
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_')
        {
            candidate.status.clone()
        } else {
            "kept".to_owned()
        };
        if let Some(label) = keep(&candidate.label) {
            made.push(ShownMade { label, standing });
        }
    }
    let lifecycle = reading.lifecycle;
    let standing = match lifecycle {
        RunLifecycle::Finished | RunLifecycle::Aborted | RunLifecycle::Archived => "kept",
        _ => "live",
    };
    Ok(ShownWork {
        schema: FACTORY_SHOWN_WORK.into(),
        owner: FACTORY_NATIVE_OWNER.into(),
        work_ref: reading.run_ref.to_string(),
        // the Run's revision and its Factory state's together: both only move
        // forward, and one of them moves whenever anything shown moves (a
        // candidate entering the Run moves the state, not the Run)
        revision: reading.revision.get() + reading.provenance.factory_state_revision.get(),
        name,
        standing: standing.into(),
        lifecycle: lifecycle_word(lifecycle).into(),
        agents,
        made,
        tasks,
        withheld,
    })
}

/// `factory development show <state> <run-ref>`: read the state as stored
/// (no lock, nothing written) and answer what may be shown of the Run.
pub fn show(path: &Path, run_ref: &str) -> Result<ShownWork, ShowRefusal> {
    let run_ref: RunRef = run_ref.parse().map_err(|error| ShowRefusal {
        code: "invalid",
        message: format!("{run_ref} is no run-ref: {error}"),
    })?;
    let state =
        crate::project_development_store::read_developmental_state(path).map_err(|error| {
            ShowRefusal {
                code: "unreadable",
                message: format!(
                    "The Factory state at {} could not be read: {error}",
                    path.display()
                ),
            }
        })?;
    shown_work(&state, &run_ref)
}

pub fn is_show_command(args: &[String]) -> bool {
    args.first().map(String::as_str) == Some("development")
        && args.get(1).map(String::as_str) == Some("show")
}

/// Process entry: `args` start at `development`. A refusal is a document on
/// stdout under `--json` and the process exits 2, as the other owner verbs do.
pub fn main(args: &[String]) -> std::process::ExitCode {
    let json = args.iter().any(|argument| argument == "--json");
    let rest: Vec<&String> = args
        .iter()
        .skip(2)
        .filter(|argument| argument.as_str() != "--json")
        .collect();
    let outcome = match rest.as_slice() {
        [state, run] if !state.starts_with("--") && !run.starts_with("--") => {
            show(Path::new(state.as_str()), run)
        }
        _ => Err(ShowRefusal {
            code: "usage",
            message: format!("expected <state> <run-ref>; run: {SHOW_USAGE}"),
        }),
    };
    match outcome {
        Ok(work) if json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&work).expect("a shown work encodes")
            );
            std::process::ExitCode::SUCCESS
        }
        Ok(work) => {
            println!(
                "{} ({}): {} agent(s), {} made, {} withheld",
                work.name,
                work.standing,
                work.agents.len(),
                work.made.len(),
                work.withheld
            );
            std::process::ExitCode::SUCCESS
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
            std::process::ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{
        AgencyRecord, CandidateRecord, ExecutionRecord, FactoryBuildState, HumanRequestRecord,
    };
    use crate::core::run::{Project, ProjectRef, Run, RunLifecycleCommand};
    use crate::developmental_read::FactoryDevelopmentalFileProvider;

    const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FAA";
    const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
    /// What a shown work never carries, planted where a Run keeps it.
    const UNSHOWN: [&str; 10] = [
        "PROMPT-SENTINEL",
        "KEY-SENTINEL",
        "/Users/frank/secret",
        "harness:mac-mini",
        "agent-session:0123abcd",
        "session-space:frank",
        "workcell:mac",
        "actuation:frank-mac",
        "map.svg",
        "ghp_0123456789abcdefABCDEF",
    ];

    fn agency(reference: &str, label: &str) -> AgencyRecord {
        AgencyRecord {
            run_ref: RUN.parse().unwrap(),
            agency_ref: reference.into(),
            agent_ref: format!("agent:{reference}"),
            label: label.into(),
            position: Some("central:position:frank/cartographer".into()),
            root_scope_ref: Some("/Users/frank/secret".into()),
            metagency_grant_refs: vec!["grant:KEY-SENTINEL".into()],
            actuation_ref: Some("actuation:frank-mac".into()),
            return_ref: None,
            return_state: None,
        }
    }

    fn execution(reference: &str, agency: &str, status: &str) -> ExecutionRecord {
        ExecutionRecord {
            run_ref: RUN.parse().unwrap(),
            execution_ref: reference.into(),
            status: status.into(),
            agency_ref: Some(agency.into()),
            agent_ref: None,
            harness_ref: Some("harness:mac-mini".into()),
            harness_composition_ref: None,
            agent_session_ref: Some("agent-session:0123abcd".into()),
            session_space_ref: Some("session-space:frank".into()),
            surface_refs: vec!["/Users/frank/secret/surface".into()],
            workcell_binding_refs: vec!["workcell:mac".into()],
            native_trajectory_ref: None,
        }
    }

    /// A Run with two agents at work, one whose label names a path, what they
    /// made, and a question for the person; `close` closes it.
    fn state(close: bool) -> FactoryDevelopmentalState {
        let mut run = Run::new(
            RUN.parse().unwrap(),
            PROJECT.parse::<ProjectRef>().unwrap(),
            "Chart the river",
            "factory",
        )
        .unwrap();
        if close {
            let authority = run.mutation_authority();
            let revision = run.revision();
            run.apply_lifecycle_command(
                &authority,
                RunLifecycleCommand {
                    command_id: "close".into(),
                    expected_revision: revision,
                    lifecycle: RunLifecycle::Aborted,
                },
            )
            .unwrap();
        }
        let mut build =
            FactoryBuildState::new(Project::new(PROJECT.parse().unwrap()), run).unwrap();
        build
            .insert_agency(agency("agency:cartographer", "Cartographer"))
            .unwrap();
        build
            .insert_agency(agency("agency:reviewer", "Reviewer"))
            .unwrap();
        build
            .insert_agency(agency("agency:leaky", "Reads /Users/frank/secret/notes.md"))
            .unwrap();
        for (reference, agency, status) in [
            ("execution:1", "agency:cartographer", "success"),
            ("execution:2", "agency:cartographer", "running"),
            ("execution:3", "agency:reviewer", "blocked"),
            ("execution:4", "agency:leaky", "queued"),
        ] {
            build
                .insert_execution(execution(reference, agency, status))
                .unwrap();
        }
        build
            .insert_candidate(CandidateRecord {
                run_ref: RUN.parse().unwrap(),
                candidate_ref: "candidate:map".into(),
                revision: 1,
                label: "The river map".into(),
                status: "proposed".into(),
                producing_execution_refs: vec!["execution:1".into()],
                claim_refs: vec![],
                evidence_refs: vec!["evidence:KEY-SENTINEL".into()],
                artifact_refs: vec!["/Users/frank/secret/map.svg".into()],
                preview_ref: Some("file:///Users/frank/secret/map.svg".into()),
                tradeoffs: vec!["PROMPT-SENTINEL tradeoff".into()],
            })
            .unwrap();
        build
            .insert_candidate(CandidateRecord {
                run_ref: RUN.parse().unwrap(),
                candidate_ref: "candidate:token".into(),
                revision: 1,
                label: "Token ghp_0123456789abcdefABCDEF".into(),
                status: "proposed".into(),
                producing_execution_refs: vec![],
                claim_refs: vec![],
                evidence_refs: vec![],
                artifact_refs: vec![],
                preview_ref: None,
                tradeoffs: vec![],
            })
            .unwrap();
        build
            .insert_human_request(HumanRequestRecord {
                run_ref: RUN.parse().unwrap(),
                human_request_ref: "human-request:1".into(),
                decision_ref: "decision:1".into(),
                question: "PROMPT-SENTINEL: which bank?".into(),
                why_human: "PROMPT-SENTINEL".into(),
                blocked_execution_refs: vec!["execution:3".into()],
                unit_decision_basis: None,
                unit_decision_response: None,
                unit_decision_retirement: None,
                evidence_refs: vec![],
            })
            .unwrap();
        FactoryDevelopmentalState::new(build, vec![]).unwrap()
    }

    #[test]
    fn a_shown_work_carries_its_agents_what_they_made_and_where_it_stands_and_nothing_else() {
        let work = shown_work(&state(false), &RUN.parse().unwrap()).unwrap();
        assert_eq!(work.schema, FACTORY_SHOWN_WORK);
        assert_eq!(work.name, "Chart the river");
        assert_eq!(
            (work.standing.as_str(), work.lifecycle.as_str()),
            ("live", "seeded"),
            "live while the Run is open"
        );
        assert_eq!(
            work.agents,
            vec![
                ShownAgent {
                    agent: "Cartographer".into(),
                    at: "runs".into()
                },
                ShownAgent {
                    agent: "Reviewer".into(),
                    at: "stalls".into()
                },
            ],
            "each agent at work by its label, at its liveliest task's word"
        );
        assert_eq!(
            work.made,
            vec![ShownMade {
                label: "The river map".into(),
                standing: "proposed".into()
            }]
        );
        assert_eq!(
            work.tasks,
            BTreeMap::from([
                ("done".into(), 1),
                ("runs".into(), 1),
                ("stalls".into(), 1),
                ("waits".into(), 1)
            ]),
            "every task counted by how it stands, the withheld agent's too"
        );
        assert_eq!(
            work.withheld, 2,
            "the agent naming a path and the made thing naming a key are withheld, and counted"
        );
        let text = serde_json::to_string(&work).unwrap();
        for unshown in UNSHOWN {
            assert!(
                !text.contains(unshown),
                "a shown work carries no prompt, key, file or machine: {unshown} in {text}"
            );
        }
    }

    #[test]
    fn a_closed_run_is_shown_as_it_was_kept() {
        let work = shown_work(&state(true), &RUN.parse().unwrap()).unwrap();
        assert_eq!(
            (work.standing.as_str(), work.lifecycle.as_str()),
            ("kept", "aborted")
        );
        assert!(work.revision >= 2, "at least the revision it closed at");
    }

    /// The revision a work is shown at moves with whatever is shown: a watcher's
    /// copy is never replaced by an older reading, nor stands beside a newer
    /// one at the same revision.
    #[test]
    fn a_work_shown_again_after_something_entered_its_run_is_shown_at_a_later_revision() {
        let mut state = state(false);
        let before = shown_work(&state, &RUN.parse().unwrap()).unwrap();
        state
            .build
            .insert_candidate(CandidateRecord {
                run_ref: RUN.parse().unwrap(),
                candidate_ref: "candidate:late".into(),
                revision: 1,
                label: "The delta chart — by Ada".into(),
                status: "accepted".into(),
                producing_execution_refs: Vec::new(),
                claim_refs: Vec::new(),
                evidence_refs: Vec::new(),
                artifact_refs: Vec::new(),
                preview_ref: None,
                tradeoffs: Vec::new(),
            })
            .unwrap();
        let after = shown_work(&state, &RUN.parse().unwrap()).unwrap();
        assert_eq!(after.made.len(), before.made.len() + 1, "it is shown");
        assert!(
            after.revision > before.revision,
            "and at a later revision: {} after {}",
            after.revision,
            before.revision
        );
    }

    #[test]
    fn watching_changes_nothing_in_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        FactoryDevelopmentalFileProvider::create_new(&path, state(false)).unwrap();
        let before = std::fs::read(&path).unwrap();
        let entries = || {
            let mut names: Vec<_> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let listed = entries();
        for _ in 0..3 {
            let work = show(&path, RUN).unwrap();
            assert_eq!(work.agents.len(), 2);
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "the state is byte for byte as it was"
        );
        assert_eq!(entries(), listed, "and nothing was written beside it");
    }

    #[test]
    fn a_run_the_state_does_not_hold_is_refused_in_words() {
        let refusal = shown_work(
            &state(false),
            &"run:01ARZ3NDEKTSV4RRFFQ69G5FAB".parse().unwrap(),
        )
        .unwrap_err();
        assert_eq!(refusal.code, "run_not_found");
        assert!(refusal.message.contains("run:01ARZ3NDEKTSV4RRFFQ69G5FAB"));
        assert_eq!(refusal.document()["schema"], "factory.refusal/v1");
    }

    #[test]
    fn the_words_never_shown_are_named() {
        for unshown in [
            "/Users/frank/x",
            "see ~/notes",
            "/private/tmp/x",
            "ws://100.64.0.7:3000",
            "10.0.0.2:22",
            "Bearer abcdefghijkl",
            "ghp_0123456789abcdef",
            "sk-proj-0123456789",
            "AbCdEf0123456789AbCdEf0123456789xyz",
            "agent-session:1234",
            "workcell:mac",
            "harness:x",
            "file:///etc/passwd",
            "./build/out",
        ] {
            assert!(names_the_unshown(unshown), "{unshown} is never shown");
        }
        for shown in [
            "Chart the river",
            "Cartographer",
            "O-I/Factory map",
            "run 3 of 4",
            "v1.2.3",
        ] {
            assert!(!names_the_unshown(shown), "{shown} may be shown");
        }
    }
}
