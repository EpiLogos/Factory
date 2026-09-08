use crate::action_projection::{
    execute_projected_factory_action, FactoryActionProjectionRequest,
    FACTORY_ACTION_PROJECTION_CONTRACT,
};
use crate::build::{
    FactoryBuildSelection, FACTORY_BUILD_PROVIDER_CONTRACT, FACTORY_BUILD_VIEW_CONTRACT,
    FACTORY_NATIVE_OWNER,
};
use crate::build_provider::{FactoryBuildFileProvider, FACTORY_BUILD_LOCAL_PROVIDER_STATE};
use crate::core::run::{ProjectRef, RunRef, WorkflowUnitRef};
use crate::developmental_read::{
    FactoryDevelopmentalFileProvider, FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER,
    FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT, FACTORY_JOURNEY_READING_CONTRACT,
    FACTORY_PROJECT_READING_CONTRACT, FACTORY_RUN_READING_CONTRACT,
    FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT, FACTORY_WORKFLOW_UNIT_READING_CONTRACT,
};
use crate::journey::JourneyRef;
use serde::Serialize;
use std::error::Error;
use std::fmt::{self, Display};
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;
use std::str::FromStr;

pub const FACTORY_CLI_CONTRACT: &str = "factory.cli/v1";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FactoryCliCapabilities<'a> {
    contract: &'a str,
    product: &'a str,
    version: &'a str,
    commands: Vec<&'a str>,
    native_contracts: Vec<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FactoryCliVerification<'a> {
    contract: &'a str,
    product: &'a str,
    version: &'a str,
    status: &'a str,
    provider_state_checked: bool,
    native_contracts: Vec<&'a str>,
}

pub fn cli_main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match execute_cli(&args, None) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("factory: {error}");
            ExitCode::from(2)
        }
    }
}

pub fn execute_cli(args: &[String], stdin_override: Option<&str>) -> Result<String, CliError> {
    let mut args = args.to_vec();
    let json = remove_flag(&mut args, "--json");

    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => Ok(help()),
        Some("--version") | Some("version") => Ok(format!("factory {}", env!("CARGO_PKG_VERSION"))),
        Some("capabilities") => render_capabilities(json),
        Some("build") => build_command(&args[1..], json),
        Some("development") => development_command(&args[1..], json, stdin_override),
        Some("action") => action_command(&args[1..], json, stdin_override),
        Some("verify") => verify_command(&args[1..], json),
        Some(command) => Err(CliError(format!(
            "unknown command `{command}`; run `factory help`"
        ))),
    }
}

fn help() -> String {
    format!(
        "Software Factory {}\n\n\
Usage:\n  factory --version\n  factory capabilities [--json]\n  factory build snapshot <state> <project-ref> <run-ref> [--json]\n  factory build refresh  <state> <project-ref> <run-ref> [--json]\n  factory action list    <state> <project-ref> <run-ref> [--json]\n  factory action invoke  <state> <project-ref> <run-ref> [request-file|-] [--json]\n  factory verify [<state> <project-ref> <run-ref>] [--json]\n\n\
Developmental reads:\n  factory development project <state> <project-ref> [--json]\n  factory development journey <state> <journey-ref> [--json]\n  factory development run     <state> <run-ref> [--json]\n  factory development workflow-units <state> [run-ref] [--json]\n  factory development workflow-unit  <state> <workflow-unit-ref> [run-ref] [--json]\n  factory development execution-telemetry <state> <telemetry-ref> [--json]\n  factory development action  <state> [request-file|-] [--json]\n\n\
The command projects Factory-owned Build/read/Action contracts; canonical state and mutation remain in the native Factory provider.",
        env!("CARGO_PKG_VERSION")
    )
}

fn capabilities() -> FactoryCliCapabilities<'static> {
    FactoryCliCapabilities {
        contract: FACTORY_CLI_CONTRACT,
        product: "software-factory",
        version: env!("CARGO_PKG_VERSION"),
        commands: vec![
            "build.snapshot",
            "build.refresh",
            "development.project",
            "development.journey",
            "development.run",
            "development.workflow-units",
            "development.workflow-unit",
            "development.execution-telemetry",
            "development.action",
            "action.list",
            "action.invoke",
            "verify",
        ],
        native_contracts: vec![
            FACTORY_BUILD_VIEW_CONTRACT,
            FACTORY_BUILD_PROVIDER_CONTRACT,
            FACTORY_BUILD_LOCAL_PROVIDER_STATE,
            FACTORY_ACTION_PROJECTION_CONTRACT,
            FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER,
            FACTORY_PROJECT_READING_CONTRACT,
            FACTORY_JOURNEY_READING_CONTRACT,
            FACTORY_RUN_READING_CONTRACT,
            FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT,
            FACTORY_WORKFLOW_UNIT_READING_CONTRACT,
            FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT,
        ],
    }
}

fn render_capabilities(json: bool) -> Result<String, CliError> {
    let capabilities = capabilities();
    if json {
        return serde_json::to_string_pretty(&capabilities).map_err(CliError::from);
    }
    Ok(format!(
        "Software Factory {}\ncommands: {}\ncontracts: {}",
        capabilities.version,
        capabilities.commands.join(", "),
        capabilities.native_contracts.join(", ")
    ))
}

fn build_command(args: &[String], json: bool) -> Result<String, CliError> {
    let operation = args
        .first()
        .ok_or_else(|| CliError("missing build operation".into()))?;
    let (state_path, selection) = selection_from_args(&args[1..])?;
    let mut provider = FactoryBuildFileProvider::open(state_path, selection)?;
    let snapshot = match operation.as_str() {
        "snapshot" => provider.snapshot()?,
        "refresh" => provider.refresh()?,
        other => return Err(CliError(format!("unknown build operation `{other}`"))),
    };
    if json {
        return snapshot.to_json().map_err(CliError::from);
    }
    Ok(format!(
        "{}\nProject: {}\nRun: {} ({})\nFrontier: {} — {}\nRevision: {}\nActions: {}",
        snapshot.contract,
        snapshot.view.project.project_ref,
        snapshot.view.run.run_ref,
        snapshot.view.run.status,
        snapshot.view.frontier.title,
        snapshot.view.frontier.summary,
        snapshot.revision,
        snapshot
            .view
            .actions
            .iter()
            .map(|action| action.label.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn development_command(
    args: &[String],
    json: bool,
    stdin_override: Option<&str>,
) -> Result<String, CliError> {
    let operation = args
        .first()
        .ok_or_else(|| CliError("missing development operation".into()))?;
    let state_path = args
        .get(1)
        .ok_or_else(|| CliError("missing developmental state path".into()))?;
    let mut provider = FactoryDevelopmentalFileProvider::open(state_path)
        .map_err(|error| CliError(error.to_string()))?;

    match operation.as_str() {
        "project" => {
            let project_ref = args
                .get(2)
                .ok_or_else(|| CliError("missing project-ref".into()))?
                .parse::<ProjectRef>()
                .map_err(|error| CliError(format!("invalid project-ref: {error}")))?;
            let reading = provider
                .project_reading(&project_ref)
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nProject: {} @ {}\nJourneys: {}",
                    reading.contract,
                    reading.project_ref,
                    reading.project_revision.get(),
                    reading
                        .journeys
                        .iter()
                        .map(|journey| journey.journey_ref.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        "journey" => {
            let journey_ref = args
                .get(2)
                .ok_or_else(|| CliError("missing journey-ref".into()))?
                .parse::<JourneyRef>()
                .map_err(|error| CliError(format!("invalid journey-ref: {error}")))?;
            let reading = provider
                .journey_reading(&journey_ref)
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nJourney: {} @ {}\nStatus: {:?}\nFrontier: {}\nRuns: {}",
                    reading.contract,
                    reading.journey_ref,
                    reading.revision.get(),
                    reading.status,
                    reading.frontier,
                    reading
                        .run_refs
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        "run" => {
            let run_ref = args
                .get(2)
                .ok_or_else(|| CliError("missing run-ref".into()))?
                .parse::<RunRef>()
                .map_err(|error| CliError(format!("invalid run-ref: {error}")))?;
            let reading = provider
                .run_reading(&run_ref)
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nRun: {} @ {}\nLifecycle: {:?}\nDestination: {}\nRunMap: {} @ {}",
                    reading.contract,
                    reading.run_ref,
                    reading.revision.get(),
                    reading.lifecycle,
                    reading.destination,
                    reading.run_map.address(),
                    reading.run_map.topology_revision().get()
                ))
            }
        }
        "workflow-units" => {
            let run_ref = args
                .get(2)
                .map(|value| {
                    value
                        .parse::<RunRef>()
                        .map_err(|error| CliError(format!("invalid run-ref: {error}")))
                })
                .transpose()?;
            let reading = provider
                .workflow_units_reading(run_ref.as_ref())
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else if reading.units.is_empty() {
                Ok(format!("{}\nNo compiled WorkflowUnits.", reading.contract))
            } else {
                Ok(format!(
                    "{}\nWorkflowUnits: {}",
                    reading.contract,
                    reading
                        .units
                        .iter()
                        .map(|unit| format!("{} ({})", unit.workflow_unit_ref, unit.locator))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        "workflow-unit" => {
            let workflow_unit_ref = args
                .get(2)
                .ok_or_else(|| CliError("missing workflow-unit-ref".into()))?
                .parse::<WorkflowUnitRef>()
                .map_err(|error| CliError(format!("invalid workflow-unit-ref: {error}")))?;
            let run_ref = args
                .get(3)
                .map(|value| {
                    value
                        .parse::<RunRef>()
                        .map_err(|error| CliError(format!("invalid run-ref: {error}")))
                })
                .transpose()?;
            let reading = provider
                .workflow_unit_reading(&workflow_unit_ref, run_ref.as_ref())
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nWorkflowUnit: {}\nLocator: {}\nSource: {} @ {} ({})\nRun correlation: {:?}",
                    reading.contract,
                    reading.workflow_unit_ref,
                    reading.locator,
                    reading.provenance.source_ref,
                    reading.provenance.source_revision,
                    reading.provenance.source_digest,
                    reading.current_correlation.run_journey_status
                ))
            }
        }
        "execution-telemetry" => {
            let telemetry_ref = args
                .get(2)
                .ok_or_else(|| CliError("missing telemetry-ref".into()))?
                .parse::<crate::core::identity::Ref>()
                .map_err(|error| CliError(format!("invalid telemetry-ref: {error}")))?;
            let reading = provider
                .execution_telemetry_reading(&telemetry_ref)
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nTelemetry: {}\nWorkflowUnit: {}\nExecution: {}\nAgency: {}\nModel usage: {:?}\nMaterial usage: {:?}",
                    reading.contract,
                    reading.telemetry_ref,
                    reading.workflow_unit_ref,
                    reading.execution_ref,
                    reading.condition.agency_ref,
                    reading.model_usage.availability,
                    reading.material_usage.availability
                ))
            }
        }
        "action" => {
            let request_path = args.get(2).map(String::as_str).unwrap_or("-");
            let input = read_input(request_path, stdin_override)?;
            let request: FactoryActionProjectionRequest = serde_json::from_str(&input)?;
            let receipt = execute_projected_factory_action(&mut provider, &request)
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&receipt).map_err(CliError::from)
            } else {
                Ok(format!(
                    "Action {} applied to {} through Factory revision {} -> {}",
                    receipt.action_ref,
                    receipt.subject_ref,
                    receipt.native_result.previous_revision,
                    receipt.native_result.next_revision
                ))
            }
        }
        other => Err(CliError(format!("unknown development operation `{other}`"))),
    }
}

fn action_command(
    args: &[String],
    json: bool,
    stdin_override: Option<&str>,
) -> Result<String, CliError> {
    let operation = args
        .first()
        .ok_or_else(|| CliError("missing action operation".into()))?;
    let (state_path, selection) = selection_from_args(&args[1..])?;
    let mut provider = FactoryBuildFileProvider::open(state_path, selection)?;

    match operation.as_str() {
        "list" => {
            let actions = provider.snapshot()?.view.actions;
            if json {
                serde_json::to_string_pretty(&actions).map_err(CliError::from)
            } else if actions.is_empty() {
                Ok("No Actions available for the selected Run.".into())
            } else {
                Ok(actions
                    .iter()
                    .map(|action| {
                        format!(
                            "{}\t{}\t{}",
                            action.action_ref, action.label, action.required_capability_ref
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
        "invoke" => {
            let request_path = args.get(4).map(String::as_str).unwrap_or("-");
            let input = read_input(request_path, stdin_override)?;
            let request: FactoryActionProjectionRequest = serde_json::from_str(&input)?;
            let receipt = execute_projected_factory_action(&mut provider, &request)
                .map_err(|error| CliError(error.to_string()))?;
            if json {
                serde_json::to_string_pretty(&receipt).map_err(CliError::from)
            } else {
                Ok(format!(
                    "Action {} applied to {}\nRun: {}\nCaller: {}\nAuthority: {}\nFactory revision: {} -> {}\nCreated: {}",
                    receipt.action_ref,
                    receipt.subject_ref,
                    receipt.run_ref,
                    receipt.caller.caller_ref,
                    receipt.authority_ref,
                    receipt.native_result.previous_revision,
                    receipt.native_result.next_revision,
                    receipt.native_result.created_human_request_ref
                ))
            }
        }
        other => Err(CliError(format!("unknown action operation `{other}`"))),
    }
}

fn verify_command(args: &[String], json: bool) -> Result<String, CliError> {
    let provider_state_checked = if args.is_empty() {
        false
    } else {
        let (state_path, selection) = selection_from_args(args)?;
        FactoryBuildFileProvider::open(state_path, selection)?.snapshot()?;
        true
    };
    let result = FactoryCliVerification {
        contract: FACTORY_CLI_CONTRACT,
        product: FACTORY_NATIVE_OWNER,
        version: env!("CARGO_PKG_VERSION"),
        status: "ok",
        provider_state_checked,
        native_contracts: capabilities().native_contracts,
    };
    if json {
        serde_json::to_string_pretty(&result).map_err(CliError::from)
    } else {
        Ok(format!(
            "Factory CLI verification: ok (provider state checked: {provider_state_checked})"
        ))
    }
}

fn selection_from_args(args: &[String]) -> Result<(String, FactoryBuildSelection), CliError> {
    if args.len() < 3 {
        return Err(CliError("expected <state> <project-ref> <run-ref>".into()));
    }
    let project_ref = ProjectRef::from_str(&args[1])
        .map_err(|error| CliError(format!("invalid project-ref: {error}")))?;
    let run_ref = RunRef::from_str(&args[2])
        .map_err(|error| CliError(format!("invalid run-ref: {error}")))?;
    Ok((
        args[0].clone(),
        FactoryBuildSelection {
            project_ref,
            run_ref,
        },
    ))
}

fn read_input(path: &str, stdin_override: Option<&str>) -> Result<String, CliError> {
    if path != "-" {
        return fs::read_to_string(path).map_err(CliError::from);
    }
    if let Some(value) = stdin_override {
        return Ok(value.to_owned());
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|arg| arg != flag);
    args.len() != before
}

#[derive(Debug)]
pub struct CliError(String);

impl Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for CliError {}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<crate::build_provider::FactoryBuildProviderError> for CliError {
    fn from(error: crate::build_provider::FactoryBuildProviderError) -> Self {
        Self(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_and_help_are_native_and_stable() {
        let version = execute_cli(&["--version".into()], None).unwrap();
        assert_eq!(version, format!("factory {}", env!("CARGO_PKG_VERSION")));
        let help = execute_cli(&[], None).unwrap();
        assert!(help.contains("factory build snapshot"));
        assert!(help.contains("factory action invoke"));
        assert!(help.contains("factory development execution-telemetry"));
    }

    #[test]
    fn capabilities_expose_native_contracts_as_json() {
        let result = execute_cli(&["capabilities".into(), "--json".into()], None).unwrap();
        let value: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(value["contract"], FACTORY_CLI_CONTRACT);
        assert_eq!(value["product"], "software-factory");
        let contracts = value["nativeContracts"].as_array().unwrap();
        assert!(contracts
            .iter()
            .any(|value| value == FACTORY_BUILD_VIEW_CONTRACT));
        assert!(contracts
            .iter()
            .any(|value| value == FACTORY_ACTION_PROJECTION_CONTRACT));
        assert!(contracts
            .iter()
            .any(|value| value == FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT));
    }

    #[test]
    fn unknown_command_fails_instead_of_falling_through() {
        let error = execute_cli(&["nope".into()], None).unwrap_err();
        assert!(error.to_string().contains("unknown command"));
    }
}
