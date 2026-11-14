//! Factory's Action field, as the O:I Action registry holds it (`oi act`,
//! O-I #1002, `oi.owner-action-field/v1`).
//!
//! `factory actions [--json]` lists the Actions; `factory action run <id>
//! <json|->` runs one. An Action is one of Factory's own routes: this table
//! names its id, title, mutation class, the route's positionals in order and,
//! where the route reads a document from `-`, that document. Running one
//! makes the route's argv from the input, checked by name, and runs it in
//! this process through the same dispatch the binary uses, so the receipt or
//! refusal is Factory's own. A route renamed under the table is refused as an
//! unknown command, which the tests hold.

use crate::cli::CliError;
use serde_json::{json, Map, Value};
use std::io::Read;
use std::process::ExitCode;

pub const OWNER_ACTION_FIELD: &str = "oi.owner-action-field/v1";

struct FieldAction {
    id: &'static str,
    title: &'static str,
    route: &'static [&'static str],
    /// The route's positionals in order: (input name, what it is).
    positionals: &'static [(&'static str, &'static str)],
    /// The document the route reads from `-`: (input name, what it is).
    document: Option<(&'static str, &'static str)>,
    mutation_class: &'static str,
    output: &'static str,
}

const STATE: (&str, &str) = (
    "state",
    "the Factory state document (a developmental or build provider state)",
);

const AGENT: (&str, &str) = (
    "agent",
    "the agent's census slug (the World Model's agents slice)",
);
const WORLD: (&str, &str) = ("world", "the World's folder, absolute");
const PROJECT: (&str, &str) = ("project", "the project: one folder of the World's Work");
const SESSION: (&str, &str) = ("session", "the agent's session (its AIKit AgentSession)");
/// The person's grant, as the host resolved it on their press (Factory reads no token).
const AUTHORITY: (&str, &str) = (
    "authority",
    "the person's grant as the host resolved it: central:native-action-authority#<principal_ref>",
);

const FIELD: &[FieldAction] = &[
    FieldAction {
        id: "factory.development.commission",
        title: "Commission developmental work",
        route: &["development", "commission"],
        positionals: &[STATE],
        document: Some((
            "request",
            "the factory.commission-request/v1 document, carried verbatim",
        )),
        mutation_class: "locally-mutating",
        output: crate::commission::FACTORY_COMMISSION_RECEIPT,
    },
    FieldAction {
        id: "factory.development.commission-compose",
        title: "Compose a draft into a Commission request",
        route: &["development", "commission-compose"],
        positionals: &[],
        document: Some(("draft", "the person's factory.commission-draft/v1 draft")),
        mutation_class: "read-only",
        output: crate::commission::FACTORY_COMMISSION_COMPOSITION,
    },
    FieldAction {
        id: "factory.development.commission-read",
        title: "Read a Commission back",
        route: &["development", "commission-read"],
        positionals: &[STATE, ("request_ref", "the Commission's request ref")],
        document: None,
        mutation_class: "read-only",
        output: crate::commission::FACTORY_COMMISSION_READING,
    },
    FieldAction {
        id: "factory.development.project",
        title: "Read a project's developmental work",
        route: &["development", "project"],
        positionals: &[STATE, ("project_ref", "the project's ref")],
        document: None,
        mutation_class: "read-only",
        output: crate::developmental_read::FACTORY_PROJECT_READING_CONTRACT,
    },
    FieldAction {
        id: "factory.development.run",
        title: "Read a Run",
        route: &["development", "run"],
        positionals: &[STATE, ("run_ref", "the Run's ref")],
        document: None,
        mutation_class: "read-only",
        output: crate::developmental_read::FACTORY_RUN_READING_CONTRACT,
    },
    FieldAction {
        id: "factory.attempt.launch",
        title: "Start a unit's session",
        route: &["attempt", "launch"],
        positionals: &[STATE],
        document: Some((
            "request",
            "the factory.attempt-launch/v1 document with the owners' readings it carries",
        )),
        mutation_class: "locally-mutating",
        output: crate::attempt_application::FACTORY_ATTEMPT_LAUNCH,
    },
    FieldAction {
        id: "factory.attempt.action",
        title: "Act on an attempt: retry it, or ask it to stop",
        route: &["attempt", "action"],
        positionals: &[STATE],
        document: Some((
            "request",
            "the factory.attempt-action/v1 document (retry, request-cancellation, ...)",
        )),
        mutation_class: "locally-mutating",
        output: crate::attempt_runtime::FACTORY_ATTEMPT_ACTION,
    },
    FieldAction {
        id: "factory.attempt.read",
        title: "Read the attempts",
        route: &["attempt", "read"],
        positionals: &[STATE],
        document: None,
        mutation_class: "read-only",
        output: crate::attempt_runtime::FACTORY_ATTEMPT_READING,
    },
    FieldAction {
        id: "factory.attempt.list",
        title: "List a Run's attempts",
        route: &["attempt", "list"],
        positionals: &[STATE, ("run_ref", "the Run's ref")],
        document: None,
        mutation_class: "read-only",
        output: crate::attempt_task::TASK_LIST_READING,
    },
    FieldAction {
        id: "factory.attempt.return",
        title: "Read an attempt's Return",
        route: &["attempt", "return"],
        positionals: &[
            STATE,
            ("run_ref", "the Run's ref"),
            ("attempt_ref", "the attempt's ref"),
        ],
        document: None,
        mutation_class: "read-only",
        output: crate::attempt_task::RETURN_READING,
    },
    FieldAction {
        id: "factory.attempt.receiving",
        title: "Propose an attempt's Return to Central's receiving field",
        route: &["attempt", "receiving"],
        positionals: &[STATE],
        document: Some((
            "request",
            "the factory.attempt-receiving-action/v1 document",
        )),
        mutation_class: "externally-mutating",
        output: crate::attempt_receiving::RECEIVING_RECEIPT,
    },
    FieldAction {
        id: "factory.session.start",
        title: "Seat an agent on a track: start its session",
        route: &["session", "start"],
        positionals: &[AGENT, WORLD, PROJECT],
        document: None,
        mutation_class: "externally-mutating",
        output: crate::session::SESSION_CONTRACT,
    },
    FieldAction {
        id: "factory.session.end",
        title: "End a session",
        route: &["session", "end"],
        positionals: &[WORLD, PROJECT, SESSION],
        document: None,
        mutation_class: "externally-mutating",
        output: crate::session::SESSION_CONTRACT,
    },
    FieldAction {
        id: "factory.task.write",
        title: "Write a task on a session",
        route: &["session", "task-write"],
        positionals: &[
            WORLD,
            PROJECT,
            SESSION,
            ("instruction", "the task, in the person's words"),
        ],
        document: None,
        mutation_class: "locally-mutating",
        output: crate::session::TASK_CONTRACT,
    },
    FieldAction {
        id: "factory.task.run",
        title: "Run a task on its session",
        route: &["session", "task-run"],
        positionals: &[STATE, ("run_ref", "the task's Run"), SESSION, AUTHORITY],
        document: None,
        mutation_class: "externally-mutating",
        output: crate::session::TASK_CONTRACT,
    },
    FieldAction {
        id: "factory.task.stop",
        title: "Stop a task: pause it, and again to cancel it",
        route: &["session", "task-stop"],
        positionals: &[
            STATE,
            ("run_ref", "the task's Run"),
            ("attempt_ref", "the running attempt"),
            AUTHORITY,
        ],
        document: None,
        mutation_class: "externally-mutating",
        output: crate::session::TASK_CONTRACT,
    },
    FieldAction {
        id: "factory.session.list",
        title: "List the sessions Factory started in a project, with the tasks written on each",
        route: &["session", "list"],
        positionals: &[STATE],
        document: None,
        mutation_class: "read-only",
        output: crate::session::SESSION_LIST_CONTRACT,
    },
    FieldAction {
        id: "factory.attempt.retry",
        title: "Retry a unit: a new attempt under the retry grant its current one consumes, run again on its session",
        route: &["session", "retry"],
        positionals: &[
            STATE,
            ("run_ref", "the unit's Run"),
            ("unit_ref", "the unit whose current attempt failed"),
            AUTHORITY,
        ],
        document: None,
        mutation_class: "externally-mutating",
        output: crate::session::TASK_CONTRACT,
    },
    FieldAction {
        id: "factory.attempt.choose",
        title: "Choose the attempt that stands: its Return proposed to Central's receiving field, into its Run's Flow",
        route: &["session", "choose"],
        positionals: &[
            STATE,
            ("run_ref", "the attempt's Run"),
            ("attempt_ref", "the returned attempt you choose"),
            AUTHORITY,
        ],
        document: None,
        mutation_class: "externally-mutating",
        output: crate::session::TASK_CONTRACT,
    },
    FieldAction {
        id: "factory.task.log",
        title: "Read a task's log as it streams, and Factory's record of its attempt",
        route: &["session", "task-log"],
        positionals: &[
            STATE,
            ("run_ref", "the task's Run"),
            ("attempt_ref", "the task's attempt"),
        ],
        document: None,
        mutation_class: "read-only",
        output: crate::session::TASK_CONTRACT,
    },
    FieldAction {
        id: "factory.attempt.admission",
        title: "Read what Factory admits from a desktop caller",
        route: &["attempt", "admission"],
        positionals: &[],
        document: None,
        mutation_class: "read-only",
        output: crate::attempt_admission::ADMISSION_READING,
    },
];

fn inputs(action: &FieldAction) -> Vec<Value> {
    let mut inputs = action
        .positionals
        .iter()
        .enumerate()
        .map(|(position, (name, description))| {
            json!({"name": name, "type": "string", "required": true,
                   "description": description, "positional": position})
        })
        .collect::<Vec<_>>();
    if let Some((name, description)) = action.document {
        inputs.push(json!({"name": name, "type": "object", "required": true,
                           "description": description, "stdin": true}));
    }
    inputs
}

/// The field `factory actions --json` prints.
pub fn field() -> Value {
    let actions = FIELD
        .iter()
        .map(|action| {
            json!({
                "id": action.id,
                "title": action.title,
                "description": format!("factory {}", route_usage(action)),
                "mutation_class": action.mutation_class,
                "inputs": inputs(action),
                "output": {"type": action.output},
                "route": action.route,
                "availability": {"available": true, "reason": null},
            })
        })
        .collect::<Vec<_>>();
    json!({"schema": OWNER_ACTION_FIELD, "owner": "factory", "actions": actions})
}

fn route_usage(action: &FieldAction) -> String {
    let mut words = action
        .route
        .iter()
        .map(|word| word.to_string())
        .collect::<Vec<_>>();
    words.extend(
        action
            .positionals
            .iter()
            .map(|(name, _)| format!("<{name}>")),
    );
    if action.document.is_some() {
        words.push("-".into());
    }
    words.join(" ")
}

/// The argv and stdin an Action's input makes: the route, its positionals in
/// order, `-` for its document, then `--json`. An input the route does not
/// take, a required one missing, or a value of the wrong shape is refused by
/// name before anything runs.
pub fn route_argv(id: &str, input: &Value) -> Result<(Vec<String>, Option<String>), String> {
    let action = FIELD.iter().find(|action| action.id == id).ok_or_else(|| {
        format!("`{id}` is not in Factory's Action field; `factory actions` lists it")
    })?;
    let given = input
        .as_object()
        .ok_or_else(|| format!("{id} takes one JSON object of named inputs"))?;
    let known = action
        .positionals
        .iter()
        .map(|(name, _)| *name)
        .chain(action.document.map(|(name, _)| name))
        .collect::<Vec<_>>();
    if let Some(name) = given.keys().find(|name| !known.contains(&name.as_str())) {
        return Err(format!("{id} takes no input `{name}`"));
    }
    let mut argv = action
        .route
        .iter()
        .map(|word| word.to_string())
        .collect::<Vec<_>>();
    for (name, _) in action.positionals {
        match given.get(*name) {
            None | Some(Value::Null) => return Err(format!("{id} needs its input `{name}`")),
            Some(Value::String(value)) => argv.push(value.clone()),
            Some(_) => return Err(format!("{id}'s input `{name}` is a string")),
        }
    }
    let stdin = match action.document {
        None => None,
        Some((name, _)) => match given.get(name) {
            None | Some(Value::Null) => return Err(format!("{id} needs its input `{name}`")),
            Some(document @ Value::Object(_)) => {
                argv.push("-".into());
                Some(document.to_string())
            }
            Some(_) => return Err(format!("{id}'s input `{name}` is one JSON object")),
        },
    };
    argv.push("--json".into());
    Ok((argv, stdin))
}

pub fn is_action_field_command(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("actions") => true,
        Some("action") => args.get(1).map(String::as_str) == Some("run"),
        _ => false,
    }
}

/// Run an Action in this process: the route's own dispatch, its own answer.
pub fn run(id: &str, input: &Value) -> Result<String, CliError> {
    let (argv, stdin) = route_argv(id, input).map_err(CliError::new)?;
    crate::attempt_cli::execute(&argv, stdin.as_deref())
}

pub fn main(args: &[String]) -> ExitCode {
    let json = args.iter().any(|arg| arg == "--json");
    let words = args
        .iter()
        .filter(|arg| *arg != "--json")
        .collect::<Vec<_>>();
    if words.first().map(|word| word.as_str()) == Some("actions") {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&field()).unwrap_or_default()
            );
        } else {
            for action in FIELD {
                println!(
                    "  {:<40} {:<20} {}",
                    action.id, action.mutation_class, action.title
                );
            }
        }
        return ExitCode::SUCCESS;
    }
    let (Some(id), raw) = (words.get(2), words.get(3)) else {
        eprintln!("factory: usage: factory action run <id> <json|->");
        return ExitCode::from(2);
    };
    let raw = match raw.map(|raw| raw.as_str()) {
        None => "{}".to_owned(),
        Some("-") => {
            let mut text = String::new();
            if let Err(error) = std::io::stdin().read_to_string(&mut text) {
                eprintln!("factory: {id}'s input could not be read: {error}");
                return ExitCode::from(2);
            }
            text
        }
        Some(raw) => raw.to_owned(),
    };
    let input = if raw.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        match serde_json::from_str::<Value>(&raw) {
            Ok(input) => input,
            Err(error) => {
                eprintln!("factory: {id} takes one JSON object: {error}");
                return ExitCode::from(2);
            }
        }
    };
    match run(id, &input) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if let Some(result) = error
                .native_publication_failure()
                .or_else(|| error.native_owner_failure_result().cloned())
            {
                println!("{result}");
            }
            eprintln!("factory: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_lists_each_action_with_its_inputs() {
        let field = field();
        assert_eq!(field["schema"], OWNER_ACTION_FIELD);
        assert_eq!(field["owner"], "factory");
        let actions = field["actions"].as_array().unwrap();
        assert_eq!(actions.len(), FIELD.len());
        let commission = actions
            .iter()
            .find(|action| action["id"] == "factory.development.commission")
            .unwrap();
        assert_eq!(commission["mutation_class"], "locally-mutating");
        assert_eq!(
            commission["inputs"],
            json!([
                {"name": "state", "type": "string", "required": true,
                 "description": STATE.1, "positional": 0},
                {"name": "request", "type": "object", "required": true,
                 "description": "the factory.commission-request/v1 document, carried verbatim",
                 "stdin": true}
            ])
        );
        for action in actions {
            let id = action["id"].as_str().unwrap();
            assert!(
                id.starts_with("factory."),
                "{id} sits under Factory's prefix"
            );
        }
    }

    #[test]
    fn an_input_makes_the_routes_argv_and_is_refused_by_name() {
        let (argv, stdin) = route_argv(
            "factory.development.commission",
            &json!({"state": "/s.json", "request": {"contract": "factory.commission-request/v1"}}),
        )
        .unwrap();
        assert_eq!(
            argv,
            ["development", "commission", "/s.json", "-", "--json"]
        );
        assert_eq!(
            stdin.as_deref(),
            Some(r#"{"contract":"factory.commission-request/v1"}"#)
        );
        let (argv, stdin) = route_argv(
            "factory.attempt.return",
            &json!({"state": "/s", "run_ref": "run:1", "attempt_ref": "a:1"}),
        )
        .unwrap();
        assert_eq!(argv, ["attempt", "return", "/s", "run:1", "a:1", "--json"]);
        assert_eq!(stdin, None);
        for (id, input, words) in [
            (
                "factory.attempt.read",
                json!({"state": "/s", "colour": "red"}),
                "takes no input `colour`",
            ),
            ("factory.attempt.read", json!({}), "needs its input `state`"),
            ("factory.attempt.read", json!({"state": 7}), "is a string"),
            (
                "factory.development.commission",
                json!({"state": "/s"}),
                "needs its input `request`",
            ),
            (
                "factory.development.commission",
                json!({"state": "/s", "request": "x"}),
                "is one JSON object",
            ),
            ("factory.attempt.read", json!([]), "one JSON object"),
            (
                "factory.teleport",
                json!({}),
                "is not in Factory's Action field",
            ),
        ] {
            let refused = route_argv(id, &input).unwrap_err();
            assert!(refused.contains(words), "{id}: {refused}");
        }
    }

    #[test]
    fn a_session_starts_from_the_census_slug_and_its_tasks_run_by_their_refs() {
        let (argv, stdin) = route_argv(
            "factory.session.start",
            &json!({"agent": "claude-code", "world": "/w/Central", "project": "notes"}),
        )
        .unwrap();
        assert_eq!(
            argv,
            [
                "session",
                "start",
                "claude-code",
                "/w/Central",
                "notes",
                "--json"
            ]
        );
        assert_eq!(stdin, None);
        let grant = "central:native-action-authority#human:owner";
        let (argv, _) = route_argv(
            "factory.task.stop",
            &json!({"state": "/s", "run_ref": "run:1", "attempt_ref": "attempt:1", "authority": grant}),
        )
        .unwrap();
        assert_eq!(
            argv,
            [
                "session",
                "task-stop",
                "/s",
                "run:1",
                "attempt:1",
                grant,
                "--json"
            ]
        );
        let refused = route_argv("factory.session.start", &json!({"agent": "pi"})).unwrap_err();
        assert!(refused.contains("needs its input `world`"), "{refused}");
    }

    #[test]
    fn every_route_in_the_field_is_one_factory_routes() {
        let missing = std::env::temp_dir().join("factory-action-field-no-such-state.json");
        let state = missing.to_str().unwrap();
        for action in FIELD {
            let mut input = Map::new();
            for (name, _) in action.positionals {
                input.insert(
                    (*name).into(),
                    Value::String(if *name == "state" {
                        state.into()
                    } else {
                        "ref:none".into()
                    }),
                );
            }
            if let Some((name, _)) = action.document {
                input.insert(name.into(), json!({}));
            }
            match run(action.id, &Value::Object(input)) {
                Ok(_) => {}
                Err(error) => {
                    let words = error.to_string();
                    assert!(
                        !words.contains("unknown command")
                            && !words.contains("unknown attempt command"),
                        "{} names a route Factory does not have: {words}",
                        action.id
                    );
                }
            }
        }
        assert!(!missing.exists(), "no route wrote a state it was not given");
    }

    #[test]
    fn the_admission_reading_runs_through_the_field_in_process() {
        let output = run("factory.attempt.admission", &json!({})).unwrap();
        let reading: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            reading["contract"],
            crate::attempt_admission::ADMISSION_READING
        );
    }
}
