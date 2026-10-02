//! One public command projection over the existing native attempt operations.
//! Other commands continue through the Development Field/base CLI unchanged.

use crate::cli::CliError;
use serde_json::Value;

const COMMANDS: &[&str] = &[
    "attempt.prepare",
    "attempt.init",
    "attempt.attach",
    "attempt.read",
    "attempt.action",
    "attempt.owner-action",
    "attempt.list",
    "attempt.task",
    "attempt.return",
    "attempt.receiving",
    "attempt.decision",
    "attempt.learn",
    "attempt.material",
    "development.attempt.prepare",
    "development.attempt.init",
    "development.attempt.attach",
    "development.attempt.read",
    "development.attempt.action",
    "development.attempt.owner-action",
    "development.attempt.list",
    "development.attempt.task",
    "development.attempt.return",
    "development.attempt.receiving",
    "development.attempt.decision",
    "development.attempt.learn",
    "development.attempt.material",
    "owner",
];
const CONTRACTS: &[&str] = &[
    crate::attempt_central::CENTRAL_ACTION,
    crate::attempt_central::CENTRAL_RECEIPT,
    "factory.attempt-state/v1",
    "factory.attempt-action/v1",
    "factory.attempt-reading/v1",
    crate::attempt_owner_dispatch::FACTORY_ATTEMPT_OWNER_ACTION,
    crate::attempt_owner_dispatch::FACTORY_ATTEMPT_OWNER_RECEIPT,
    crate::attempt_task::TASK_LIST_READING,
    crate::attempt_task::TASK_READING,
    crate::attempt_task::RETURN_READING,
    crate::attempt_receiving::RECEIVING_ACTION,
    crate::attempt_receiving::RECEIVING_RECEIPT,
    crate::attempt_native_receiving::DECISION_READING,
    crate::attempt_learning::LEARNING_ACTION,
    crate::attempt_learning::LEARNING_RECEIPT,
    crate::attempt_material::MATERIAL_ACTION,
    crate::attempt_material::MATERIAL_RECEIPT,
];

pub fn execute(args: &[String], stdin: Option<&str>) -> Result<String, CliError> {
    if args.first().map(String::as_str) == Some("workflow") {
        let value = crate::workflow_authoring::cli::execute(args).map_err(|error| {
            let native_result = error.native_result.clone();
            let error = CliError::from_native(error);
            if let Some(result) = native_result {
                error.with_native_result(result)
            } else {
                error
            }
        })?;
        return serde_json::to_string_pretty(&value).map_err(CliError::from);
    }
    if args.first().map(String::as_str) == Some("owner") {
        return crate::native_owner::execute_native_owner_cli(&args[1..], stdin)
            .map_err(CliError::from_native);
    }
    let attempt_args = match args.first().map(String::as_str) {
        Some("attempt") => Some(&args[1..]),
        Some("development") if args.get(1).map(String::as_str) == Some("attempt") => {
            Some(&args[2..])
        }
        _ => None,
    };
    if let Some(args) = attempt_args {
        if args.first().map(String::as_str) == Some("learn") {
            return crate::attempt_learning::execute_cli(&args[1..], stdin);
        }
        if args.first().map(String::as_str) == Some("owner-action") {
            return crate::attempt_owner_cli::execute_attempt_owner_cli(&args[1..], stdin);
        }
        if args.first().is_some_and(|command| {
            matches!(command.as_str(), "init" | "attach" | "read" | "action")
        }) {
            return crate::attempt_application::execute_attempt_cli(args, stdin)
                .map_err(CliError::from_native);
        }
        match args.first().map(String::as_str) {
            Some("prepare") => return crate::attempt_central::execute_cli(&args[1..], stdin),
            Some("receiving") => return crate::attempt_receiving::execute_cli(&args[1..], stdin),
            Some("decision") => {
                return crate::attempt_native_receiving::execute_cli(&args[1..], stdin)
            }
            Some("material") => return crate::attempt_material::execute_cli(&args[1..], stdin),
            _ => {}
        }
        let result: Result<String, String> = (|| match args.first().map(String::as_str) {
            Some("prepare") => crate::attempt_central::execute_cli(&args[1..], stdin)
                .map_err(|error| error.to_string()),
            Some("owner-action") => {
                crate::attempt_owner_cli::execute_attempt_owner_cli(&args[1..], stdin)
                    .map_err(|error| error.to_string())
            }
            Some("list" | "task" | "return") => crate::attempt_task::execute_cli(args),
            Some("receiving") => crate::attempt_receiving::execute_cli(&args[1..], stdin)
                .map_err(|error| error.to_string()),
            Some("decision") => crate::attempt_native_receiving::execute_cli(&args[1..], stdin)
                .map_err(|error| error.to_string()),
            Some("learn") => crate::attempt_learning::execute_cli(&args[1..], stdin)
                .map_err(|error| error.to_string()),
            Some("material") => crate::attempt_material::execute_cli(&args[1..], stdin)
                .map_err(|error| error.to_string()),
            None | Some("help" | "--help" | "-h") => {
                let base = crate::attempt_application::execute_attempt_cli(args, stdin)
                    .map_err(|error| error.to_string())?;
                let owner = crate::attempt_owner_cli::execute_attempt_owner_cli(&[], None)
                    .map_err(|error| error.to_string())?;
                let receiving = crate::attempt_receiving::execute_cli(&[], None)
                    .map_err(|error| error.to_string())?;
                let decision = crate::attempt_native_receiving::execute_cli(&[], None)
                    .map_err(|error| error.to_string())?;
                let learning = crate::attempt_learning::execute_cli(&[], None)
                    .map_err(|error| error.to_string())?;
                let preparation = crate::attempt_central::execute_cli(&[], None)
                    .map_err(|error| error.to_string())?;
                let material = crate::attempt_material::execute_cli(&[], None)
                    .map_err(|error| error.to_string())?;
                Ok(format!(
                    "{base}\n\n{owner}\n\n{}\n\n{receiving}\n\n{decision}\n\n{learning}\n\n{preparation}\n\n{material}",
                    task_help()
                ))
            }
            _ => crate::attempt_application::execute_attempt_cli(args, stdin)
                .map_err(|error| error.to_string()),
        })();
        return result.map_err(CliError::new);
    }
    let mut base = match crate::development_field_cli::execute_extension(args, stdin)
        .map_err(CliError::from_native)?
    {
        Some(output) => output,
        None => crate::cli::execute_cli(args, stdin)?,
    };
    if args.is_empty()
        || args
            .first()
            .is_some_and(|s| matches!(s.as_str(), "help" | "--help" | "-h"))
    {
        base.push('\n');
        base.push_str(crate::workflow_authoring::cli::HELP);
    }
    match args.first().map(String::as_str) {
        Some("capabilities") if args.iter().any(|argument| argument == "--json") => {
            let mut value: Value = serde_json::from_str(&base).map_err(|error| error.to_string())?;
            for (field, additions) in [("commands", COMMANDS), ("nativeContracts", CONTRACTS)] {
                let values = value.get_mut(field).and_then(Value::as_array_mut).ok_or("invalid native CLI capabilities")?;
                for addition in additions {
                    if !values.iter().any(|value| value.as_str() == Some(*addition)) { values.push(Value::String((*addition).into())); }
                }
            }
            for (field, additions) in [("commands",crate::workflow_authoring::cli::COMMANDS), ("nativeContracts",crate::workflow_authoring::cli::CONTRACTS)] {
                let values = value[field].as_array_mut().ok_or("invalid capabilities")?;
                for name in additions { values.push(Value::String((*name).into())); }
            }
            serde_json::to_string_pretty(&value).map_err(CliError::from)
        }
        Some("capabilities") => Ok(format!("{base}\nattempt commands: {}\nattempt contracts: {}\nworkflow commands: {}\nworkflow contracts: {}",COMMANDS.join(", "),CONTRACTS.join(", "),crate::workflow_authoring::cli::COMMANDS.join(", "),crate::workflow_authoring::cli::CONTRACTS.join(", "))),
        None | Some("help" | "--help" | "-h") => Ok(format!("{base}\n\nNative attempts:\n  factory attempt help\n  factory development attempt help\n{}",task_help())),
        _ => Ok(base),
    }
}

fn task_help() -> &'static str {
    "Native task and Return readings:\n  factory attempt prepare <state> <request-json|-> [--json]\n  factory attempt list <state> <run-ref> [--json]\n  factory attempt task <state> <run-ref> <task-ref> [--json] [--limit 1..100] [--cursor JSON]\n  factory attempt return <state> <run-ref> <attempt-ref> [--json]\n  factory attempt receiving <state> <request-json|-> [--json]\n  factory attempt learn <state> <request-json|-> [--json]\n  factory attempt material <state> <request-json|-> [--json]\n\nThe development.attempt alias uses the same native operations. Page cursors pin the provider revision. Telemetry values come only from owner-validated correlations; missing observations are not zero usage. Receiving and archive links are not human Recognition or lifecycle proof."
}
