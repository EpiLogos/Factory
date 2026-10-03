//! Public CLI projection for consequential attempt owner calls.
//!
//! This is deliberately thin: the command reads one request and delegates to
//! `attempt_owner_dispatch`, which persists intent and owner evidence through
//! the existing native attempt store. It owns no coordinator or state.

use crate::attempt_owner_dispatch::{
    execute_attempt_owner_action, FactoryAttemptOwnerRequest, FACTORY_ATTEMPT_OWNER_ACTION,
};
use crate::cli::CliError;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

pub fn execute_attempt_owner_cli(
    args: &[String],
    stdin_override: Option<&str>,
) -> Result<String, CliError> {
    if matches!(
        args.first().map(String::as_str),
        None | Some("help") | Some("--help") | Some("-h")
    ) {
        return Ok(format!(
            "Factory persisted attempt owner Action\n\nUsage:\n  factory attempt owner-action <state-path> <request-json|-> [--json]\n  factory development attempt owner-action <state-path> <request-json|-> [--json]\n\nContract: {FACTORY_ATTEMPT_OWNER_ACTION}\n\nFactory durably records dispatch intent before native owner transport. Replaying the same request never implicitly resends an uncertain effect; reconcile the exact owner delivery instead. Plain addressed-session delivery refuses attempts whose required placement/protection or writing effects are not backed by an owner enforcement admission."
        ));
    }
    let positional = args
        .iter()
        .filter(|arg| arg.as_str() != "--json")
        .collect::<Vec<_>>();
    let state = positional
        .first()
        .ok_or_else(|| "missing attempt state path".to_owned())?;
    let request_path = positional.get(1).map(|value| value.as_str()).unwrap_or("-");
    let input = if request_path != "-" {
        fs::read_to_string(request_path).map_err(CliError::from_native)?
    } else if let Some(input) = stdin_override {
        input.to_owned()
    } else {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(CliError::from_native)?;
        input
    };
    let request: FactoryAttemptOwnerRequest =
        serde_json::from_str(&input).map_err(CliError::from_native)?;
    if request.contract != FACTORY_ATTEMPT_OWNER_ACTION {
        return Err(CliError::new(format!(
            "owner Action contract must be {FACTORY_ATTEMPT_OWNER_ACTION}"
        )));
    }
    let receipt = match execute_attempt_owner_action(Path::new(state.as_str()), request) {
        Ok(receipt) => receipt,
        Err(failure) => {
            let text = failure.native_result().map(owner_text);
            let native_result = match failure.native_result().map(serde_json::to_value).transpose() {
                Ok(value) => value,
                Err(secondary) => return Err(CliError::from_native(
                    failure.with_secondary_native(secondary))),
            };
            let mut error = CliError::from_native(failure);
            if let Some(value) = native_result { error = error.with_native_result(value); }
            if let Some(text) = text { error = error.with_message(text); }
            return Err(error);
        }
    };
    if args.iter().any(|arg| arg == "--json") {
        serde_json::to_string_pretty(&receipt).map_err(CliError::from_native)
    } else {
        Ok(owner_text(&receipt))
    }
}

fn owner_text(receipt: &crate::attempt_owner_dispatch::FactoryAttemptOwnerReceipt) -> String {
    format!(
        "{}\nRun: {}\nAttempt: {}\nRequest: {}\nReplayed: {}\nNeeds reconciliation: {}\nOwner phase: {:?}",
        receipt.contract,
        receipt.run_ref,
        receipt.attempt_ref,
        receipt.request_ref,
        receipt.replayed,
        receipt.needs_reconciliation,
        receipt.transport_observation.phase
    )
}
