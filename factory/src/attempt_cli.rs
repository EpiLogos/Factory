//! One public command projection over the existing native attempt operations.
//! Other commands continue through the Development Field/base CLI unchanged.

use serde_json::Value;

const COMMANDS: &[&str] = &[
    "attempt.init",
    "attempt.attach",
    "attempt.read",
    "attempt.action",
    "attempt.owner-action",
    "attempt.task",
    "attempt.return",
    "attempt.receiving",
    "development.attempt.init",
    "development.attempt.attach",
    "development.attempt.read",
    "development.attempt.action",
    "development.attempt.owner-action",
    "development.attempt.task",
    "development.attempt.return",
    "development.attempt.receiving",
    "owner",
];
const CONTRACTS: &[&str] = &[
    "factory.attempt-state/v1",
    "factory.attempt-action/v1",
    "factory.attempt-reading/v1",
    crate::attempt_owner_dispatch::FACTORY_ATTEMPT_OWNER_ACTION,
    crate::attempt_owner_dispatch::FACTORY_ATTEMPT_OWNER_RECEIPT,
    crate::attempt_task::TASK_READING,
    crate::attempt_task::RETURN_READING,
    crate::attempt_receiving::RECEIVING_ACTION,
    crate::attempt_receiving::RECEIVING_RECEIPT,
];

pub fn execute(args: &[String], stdin: Option<&str>) -> Result<String, String> {
    if args.first().map(String::as_str) == Some("owner") {
        return crate::native_owner::execute_native_owner_cli(&args[1..], stdin)
            .map_err(|error| error.to_string());
    }
    let attempt_args = match args.first().map(String::as_str) {
        Some("attempt") => Some(&args[1..]),
        Some("development") if args.get(1).map(String::as_str) == Some("attempt") => {
            Some(&args[2..])
        }
        _ => None,
    };
    if let Some(args) = attempt_args {
        return match args.first().map(String::as_str) {
            Some("owner-action") => {
                crate::attempt_owner_cli::execute_attempt_owner_cli(&args[1..], stdin)
                    .map_err(|error| error.to_string())
            }
            Some("task" | "return") => crate::attempt_task::execute_cli(args),
            Some("receiving") => crate::attempt_receiving::execute_cli(&args[1..], stdin),
            None | Some("help" | "--help" | "-h") => {
                let base = crate::attempt_application::execute_attempt_cli(args, stdin)
                    .map_err(|error| error.to_string())?;
                let owner = crate::attempt_owner_cli::execute_attempt_owner_cli(&[], None)
                    .map_err(|error| error.to_string())?;
                let receiving = crate::attempt_receiving::execute_cli(&[], None)?;
                Ok(format!(
                    "{base}\n\n{owner}\n\n{}\n\n{receiving}",
                    task_help()
                ))
            }
            _ => crate::attempt_application::execute_attempt_cli(args, stdin)
                .map_err(|error| error.to_string()),
        };
    }
    let base = match crate::development_field_cli::execute_extension(args, stdin)
        .map_err(|error| error.to_string())?
    {
        Some(output) => output,
        None => crate::cli::execute_cli(args, stdin).map_err(|error| error.to_string())?,
    };
    match args.first().map(String::as_str) {
        Some("capabilities") if args.iter().any(|argument| argument == "--json") => {
            let mut value: Value = serde_json::from_str(&base).map_err(|error| error.to_string())?;
            for (field, additions) in [("commands", COMMANDS), ("nativeContracts", CONTRACTS)] {
                let values = value.get_mut(field).and_then(Value::as_array_mut).ok_or("invalid native CLI capabilities")?;
                for addition in additions {
                    if !values.iter().any(|value| value.as_str() == Some(*addition)) { values.push(Value::String((*addition).into())); }
                }
            }
            serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
        }
        Some("capabilities") => Ok(format!("{base}\nattempt commands: {}\nattempt contracts: {}",COMMANDS.join(", "),CONTRACTS.join(", "))),
        None | Some("help" | "--help" | "-h") => Ok(format!("{base}\n\nNative attempts:\n  factory attempt help\n  factory development attempt help\n{}",task_help())),
        _ => Ok(base),
    }
}

fn task_help() -> &'static str {
    "Native task and Return readings:\n  factory attempt task <state> <run-ref> <task-ref> [--json] [--limit 1..100] [--cursor JSON]\n  factory attempt return <state> <run-ref> <attempt-ref> [--json]\n  factory attempt receiving <state> <request-json|-> [--json]\n\nThe development.attempt alias uses the same native operations. Page cursors pin the provider revision. Telemetry values come only from owner-validated correlations; missing observations are not zero usage. Receiving and archive links are not human Recognition or lifecycle proof."
}
