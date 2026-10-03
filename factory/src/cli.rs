use crate::action_projection::{
    execute_projected_factory_action, FactoryActionProjectionError, FactoryActionProjectionRequest,
    FactoryProjectedActionProvider, FACTORY_ACTION_PROJECTION_CONTRACT,
};
use crate::build::{
    FactoryActionAuthority, FactoryActionInvocation, FactoryBuildSelection, FactoryBuildSnapshot,
    FACTORY_BUILD_PROVIDER_CONTRACT, FACTORY_BUILD_VIEW_CONTRACT, FACTORY_NATIVE_OWNER,
};
use crate::build_provider::{FactoryBuildFileProvider, FACTORY_BUILD_LOCAL_PROVIDER_STATE};
use crate::commission::{
    FactoryCommissionRequest, FactoryDevelopmentalMutationRequest, FACTORY_COMMISSION_READING,
    FACTORY_COMMISSION_RECEIPT, FACTORY_DEVELOPMENTAL_MUTATION_RECEIPT,
};
use crate::conformance::{
    create_developmental_conformance_state, FACTORY_DEVELOPMENTAL_CONFORMANCE_MANIFEST,
};
use crate::core::run::{ProjectRef, RunRef, WorkflowUnitRef};
use crate::developmental_read::{
    FactoryCentralProjectLinkRequest, FactoryDevelopmentalFileProvider,
    FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER, FACTORY_EXECUTION_TELEMETRY_READING_CONTRACT,
    FACTORY_JOURNEY_READING_CONTRACT, FACTORY_PROJECT_READING_CONTRACT,
    FACTORY_RUN_READING_CONTRACT, FACTORY_WORKFLOW_UNIT_LIST_READING_CONTRACT,
    FACTORY_WORKFLOW_UNIT_READING_CONTRACT,
};
use crate::journey::JourneyRef;
use crate::project_development::{
    DevelopmentObservation, DevelopmentObservationKind, OwnerReturnProposal,
    PROJECT_DEVELOPMENT_VERSION,
};
use crate::project_development_store::{FileProjectDevelopmentStore, ProjectDevelopmentStore};
use crate::routine_continuation::{
    FactoryRoutineContinuationRequest, FACTORY_ROUTINE_CONTINUATION_ADMISSION,
    FACTORY_ROUTINE_CONTINUATION_READING,
};
use serde::{Deserialize, Serialize};
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
    // The configuration plane contract (C0 §6) puts its structured failure
    // document on stdout while the process still exits non-zero, so those two
    // command heads report through their own entry.
    if crate::configuration::is_config_command(args.first().map(String::as_str)) {
        return crate::configuration::config_main(&args);
    }
    match execute_cli(&args, None) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.iter().any(|arg| arg == "--json") {
                if let Some(result) = error.native_owner_failure_result() {
                    println!("{result}");
                } else if let Some(result) = error.native_publication_failure() {
                    println!("{result}");
                }
            }
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
        Some("--version") | Some("version") => Ok(format!(
            "factory {} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("SUITE_BUILD_REVISION").unwrap_or("unknown")
        )),
        Some("capabilities") => render_capabilities(json),
        Some("project") => crate::project_setup::execute(&args[1..]),
        Some("build") => build_command(&args[1..], json),
        Some("conformance") => conformance_command(&args[1..], json),
        Some("development") => development_command(&args[1..], json, stdin_override),
        Some("telemetry") => crate::telemetry_cli::execute(&args[1..], json),
        Some("action") => action_command(&args[1..], json, stdin_override),
        Some("system") => crate::system::system_command(json),
        Some("config-contribution") | Some("config") => {
            crate::configuration::execute_config(&args, stdin_override, json)
                .map_err(CliError::from_native)
        }
        Some("verify") => verify_command(&args[1..], json),
        Some(command) => Err(CliError::new(format!(
            "unknown command `{command}`; run `factory help`"
        ))),
    }
}

fn help() -> String {
    format!(
        "Software Factory {} — developmental work through Commissions: commission,\nobserve, Return. The routes below are the exact native spellings; the groups\nsay what each is for.\n\n\
Developmental work (everyday entry, never execution):\n  factory development commission <state> [request-file|-] [--json]\n                                    submit a Commission through the exact owner contract\n  factory development commission-read <state> <request-ref> [--json]\n                                    read a Commission back\n  factory development current-work [<state>] --position <central:position:...> [--json]\n                                    the work this position is carrying right now\n  factory development mutate <state> [request-file|-] [--json]\n                                    a declared owner effect via the applicable Method/subject action\n  factory development action <state> [request-file|-] [--json]\n                                    one declared Action effect\n  factory development custody assign|update|list ...\n                                    execution coordination over the existing custody facts\n\n\
Work, history and evidence reads (contextual):\n  factory development project <state> <project-ref> [--json]\n  factory development journey <state> <journey-ref> [--json]\n  factory development run <state> <run-ref> [--json]\n  factory development build <state> <run-ref> [--json]\n  factory development workflow-units <state> [run-ref] [--json]\n  factory development workflow-unit <state> <workflow-unit-ref> [run-ref] [--json]\n  factory development execution-telemetry <state> <telemetry-ref> [--json]\n  factory development central-project-link <state> <request> [--json]\n  factory development central-project-link-read <state> <central-project-ref> [--json]\n  factory development observe <ledger-root> <run-ref> [request-file|-] [--json]\n                                    write a Return/ledger observation\n  factory development observations <ledger-root> <run-ref> [--json]\n                                    read them back exactly as written\n  factory development routine-continuation <state> <invocation-ref> [--json]\n                                    inspect a recurrence; admission is its own route\n  factory development admit-routine-continuation <state> [request-file|-] [--json]\n\n\
Builds and Actions (contextual):\n  factory build snapshot <state> <project-ref> <run-ref> [--json]\n                                    the current build reading (discloses any refresh effect)\n  factory build refresh <state> <project-ref> <run-ref> [--json]\n  factory action list <state> <project-ref> <run-ref> [--json]\n  factory action invoke <state> <project-ref> <run-ref> [request-file|-] [--json]\n  factory verify [<state> <project-ref> <run-ref>] [--json]\n\n\
Telemetry (contextual inspection):\n  factory telemetry status <state> [--json]\n  factory telemetry inspect <state> <telemetry-ref> [--json]\n  factory telemetry search <state> <query> [--regex] [--limit N] [--aikit <bin>] [--json]\n  factory telemetry stats <state> [--day YYYY-MM-DD | --from-day YYYY-MM-DD --through-day YYYY-MM-DD | --last-days N] [--compare-previous] [--template <name>] [--drill-down] [--json]\n  factory telemetry watch <state> [--interval S] [--max-events N] [--duration S] [--resume <cursor>]\n  factory telemetry compare <original-state> <changed-state> [--json]\n  factory telemetry signals|field|digest|lookback|day <state> [--policy <path>] [--day ...] [--limit N] [--json]\n  factory telemetry signal <state> <signal-ref> [--json]\n\n\
Telemetry collection and repair (Operator):\n  factory telemetry collect <state> --policy <path> [--day YYYY-MM-DD | --last-days N] [--json]\n  factory telemetry classify|commission|return <state> --request <path> [--policy <path>] [--json]\n  factory telemetry export <state> [--json]\n  factory telemetry doctor <state> [--json]\n  factory telemetry policy <state> [--policy <path>] [--json]\n\n\
Binding and specimen (Operator):\n  factory project setup <root> <native-project-key> [--central-source <project.json>] [--json]\n  factory project setup-central <root> <central-project-ref> <project.json> [--json]\n  factory project locate <root> [--json]\n                                    Contextual discovery of an existing binding\n  factory conformance developmental-state <output> [--json]\n                                    test specimen; never ordinary work creation\n\n\
System and configuration:\n  factory system [--json]\n  factory config-contribution [--json]\n  factory config validate --setting <setting-ref> [--scope <kind>:<ref>] (--value <json>|--value-file <path|->) [--json]\n  factory config plan --setting <setting-ref> [--scope <kind>:<ref>] (--value <json>|--value-file <path|->) [--json]\n  factory config apply --plan-file <path|-> [--changeset <id>] [--json]\n  factory config reset --setting <setting-ref> [--scope <kind>:<ref>] [--changeset <id>] [--json]\n  factory capabilities [--json]\n  factory --version\n\n\
{}\n\n\
<state> in build/action/verify accepts a `factory.build-local-provider-state/v1` document or a `factory.developmental-local-provider/v1` developmental state, such as the document `factory conformance developmental-state` writes.\n\n\
The command projects Factory-owned Build/read/Action contracts; canonical state and mutation remain in the native Factory provider.\n\nFor attempt/session work: `factory attempt --help`. World inhabitation, custody and current-work detail: `factory development inhabitation|custody|current-work --help`. Workflow authoring: `factory workflow --help`.",
        env!("CARGO_PKG_VERSION"),
        crate::inhabitation_cli::help()
    )
}
fn capabilities() -> FactoryCliCapabilities<'static> {
    FactoryCliCapabilities {
        contract: FACTORY_CLI_CONTRACT,
        product: "software-factory",
        version: env!("CARGO_PKG_VERSION"),
        commands: vec![
            "project.setup",
            "project.setup-central",
            "project.locate",
            "build.snapshot",
            "build.refresh",
            "conformance.developmental-state",
            "development.project",
            "development.journey",
            "development.run",
            "development.build",
            "development.workflow-units",
            "development.workflow-unit",
            "development.execution-telemetry",
            "development.central-project-link",
            "development.central-project-link-read",
            "development.commission",
            "development.commission-read",
            "development.mutate",
            "development.admit-routine-continuation",
            "development.routine-continuation",
            "development.action",
            "development.observe",
            "development.observations",
            "development.custody.assign",
            "development.custody.update",
            "development.custody.list",
            "development.current-work",
            "development.inhabitation",
            "action.list",
            "action.invoke",
            "system",
            "config-contribution",
            "config.validate",
            "config.plan",
            "config.apply",
            "config.reset",
            "telemetry.status",
            "telemetry.inspect",
            "telemetry.search",
            "telemetry.stats",
            "telemetry.watch",
            "telemetry.compare",
            "telemetry.export",
            "telemetry.doctor",
            "telemetry.collect",
            "telemetry.signals",
            "telemetry.signal",
            "telemetry.classify",
            "telemetry.commission",
            "telemetry.return",
            "telemetry.digest",
            "telemetry.lookback",
            "telemetry.day",
            "telemetry.field",
            "telemetry.policy",
            "verify",
        ],
        native_contracts: vec![
            crate::FACTORY_PUBLICATION_FAILURE_CONTRACT,
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
            FACTORY_COMMISSION_RECEIPT,
            FACTORY_COMMISSION_READING,
            FACTORY_DEVELOPMENTAL_MUTATION_RECEIPT,
            FACTORY_ROUTINE_CONTINUATION_ADMISSION,
            FACTORY_ROUTINE_CONTINUATION_READING,
            FACTORY_DEVELOPMENTAL_CONFORMANCE_MANIFEST,
            crate::sensing::POLICY_SCHEMA,
            crate::sensing::FIELD_SCHEMA,
            crate::sensing::COLLECTION_SCHEMA,
            "factory.signal-decision-receipt/v1",
            "factory.signal-work-receipt/v1",
            "factory.signal-return-receipt/v1",
            "factory.signal-reading/v1",
            "factory.telemetry-signals/v1",
            "factory.telemetry-digest/v1",
            "factory.telemetry-lookback/v1",
            "factory.telemetry-day/v1",
            crate::work_custody::FACTORY_WORK_CUSTODY,
            crate::work_custody::FACTORY_WORK_CUSTODY_RECEIPT,
            crate::work_custody::FACTORY_WORK_CUSTODY_LISTING,
            crate::current_work::FACTORY_CURRENT_WORK,
            crate::inhabitation::FACTORY_INHABITATION_READING,
            crate::work_custody::FACTORY_REFUSAL,
            crate::telemetry_cli::FACTORY_TELEMETRY_STATUS_CONTRACT,
            crate::telemetry_cli::FACTORY_TELEMETRY_INSPECT_CONTRACT,
            crate::telemetry_cli::FACTORY_TELEMETRY_SEARCH_CONTRACT,
            crate::telemetry_cli::FACTORY_TELEMETRY_STATS_CONTRACT,
            crate::telemetry_cli::FACTORY_TELEMETRY_COMPARE_CONTRACT,
            crate::telemetry_cli::FACTORY_TELEMETRY_DOCTOR_CONTRACT,
        ],
    }
}

fn conformance_command(args: &[String], json: bool) -> Result<String, CliError> {
    let operation = args
        .first()
        .ok_or_else(|| CliError::new("missing conformance operation"))?;
    if operation != "developmental-state" {
        return Err(CliError::new(format!(
            "unknown conformance operation `{operation}`"
        )));
    }
    let output = args
        .get(1)
        .ok_or_else(|| CliError::new("missing conformance state output path"))?;
    let manifest = create_developmental_conformance_state(std::path::Path::new(output))
        .map_err(CliError::from_boxed)?;
    if json {
        serde_json::to_string_pretty(&manifest).map_err(CliError::from)
    } else {
        Ok(format!(
            "{}\nProvider state: {}\nProject: {}\nJourney: {}\nRun: {}\nRoutine Run: {}\nWorkflowUnit: {}\nTelemetry: {}\nInvocation: {}",
            manifest.contract,
            manifest.provider_state,
            manifest.project_ref,
            manifest.journey_ref,
            manifest.run_ref,
            manifest.routine_run_ref,
            manifest.workflow_unit_ref,
            manifest.telemetry_ref,
            manifest.invocation_ref
        ))
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
        .ok_or_else(|| CliError::new("missing build operation"))?;
    let (state_path, selection) = selection_from_args(&args[1..])?;
    let mut provider = BuildStateDocument::open(&state_path, selection)?;
    let snapshot = match operation.as_str() {
        "snapshot" => provider.snapshot()?,
        "refresh" => provider.refresh()?,
        other => return Err(CliError::new(format!("unknown build operation `{other}`"))),
    };
    if json {
        return snapshot.to_json().map_err(CliError::from);
    }
    Ok(format!(
        "{}\nProject: {} ({})\nRun: {} ({})\nFrontier: {}{}\nRevision: {}\nActions: {}",
        snapshot.contract,
        snapshot.view.project.label,
        snapshot.view.project.project_ref,
        snapshot.view.run.run_ref,
        snapshot.view.run.status,
        snapshot.view.frontier.title,
        if snapshot.view.frontier.summary.is_empty() {
            String::new()
        } else {
            format!(" — {}", snapshot.view.frontier.summary)
        },
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
        .ok_or_else(|| CliError::new("missing development operation"))?;

    // The run-scoped development ledger is a different provider from the
    // developmental read state: it retains the observations a Run returns to
    // its native owners, and it is addressed by a ledger root rather than by a
    // developmental state document. Route those operations before the read
    // provider is opened, so recording an observation never demands a state
    // document the recorder does not own.
    match operation.as_str() {
        // World inhabitation verbs locate their own state and refuse in three
        // parts; the process entry prints those refusals as documents.
        "custody" | "current-work" | "inhabitation" => {
            return crate::inhabitation_cli::execute(args, json).map_err(CliError::from_native);
        }
        "observe" => {
            let ledger_root = args
                .get(1)
                .ok_or_else(|| CliError::new("missing development ledger root"))?;
            return observe_operation(ledger_root, &args[2..], json, stdin_override);
        }
        "observations" => {
            let ledger_root = args
                .get(1)
                .ok_or_else(|| CliError::new("missing development ledger root"))?;
            return observations_operation(ledger_root, &args[2..], json);
        }
        "central-project-link" => {
            let state_path = args
                .get(1)
                .ok_or_else(|| CliError::new("missing developmental state path"))?;
            let request_path = args.get(2).map(String::as_str).unwrap_or("-");
            let request: FactoryCentralProjectLinkRequest =
                serde_json::from_str(&read_input(request_path, stdin_override)?)?;
            let mut provider = FactoryDevelopmentalFileProvider::open(state_path)
                .map_err(CliError::from_native)?;
            let receipt = provider
                .admit_central_project_link(request)
                .map_err(CliError::from_native)?;
            return serde_json::to_string_pretty(&receipt).map_err(CliError::from);
        }
        "commission" => {
            let state_path = args
                .get(1)
                .ok_or_else(|| CliError::new("missing developmental state path"))?;
            let request_path = args.get(2).map(String::as_str).unwrap_or("-");
            let request: FactoryCommissionRequest =
                serde_json::from_str(&read_input(request_path, stdin_override)?)?;
            let receipt = FactoryDevelopmentalFileProvider::commission(state_path, request)
                .map_err(CliError::from_native)?;
            return if json {
                serde_json::to_string_pretty(&receipt).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nCommission: {}\nProject: {}\nJourney: {}\nRun: {}\nStatus: {:?}",
                    receipt.contract,
                    receipt.commission.request.request_ref,
                    receipt.commission.project_ref,
                    receipt.commission.journey_ref,
                    receipt.commission.run_ref,
                    receipt.status
                ))
            };
        }
        _ => {}
    }

    let state_path = args
        .get(1)
        .ok_or_else(|| CliError::new("missing developmental state path"))?;
    let mut provider =
        FactoryDevelopmentalFileProvider::open(state_path).map_err(CliError::from_native)?;

    match operation.as_str() {
        "central-project-link-read" => {
            let central_project_ref = args
                .get(2)
                .ok_or_else(|| CliError::new("missing central-project-ref"))?;
            let reading = provider
                .central_project_link_reading(central_project_ref)
                .map_err(CliError::from_native)?;
            serde_json::to_string_pretty(&reading).map_err(CliError::from)
        }
        "project" => {
            let project_ref = args
                .get(2)
                .ok_or_else(|| CliError::new("missing project-ref"))?
                .parse::<ProjectRef>()
                .map_err(|error| CliError::new(format!("invalid project-ref: {error}")))?;
            let reading = provider
                .project_reading(&project_ref)
                .map_err(CliError::from_native)?;
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
                .ok_or_else(|| CliError::new("missing journey-ref"))?
                .parse::<JourneyRef>()
                .map_err(|error| CliError::new(format!("invalid journey-ref: {error}")))?;
            let reading = provider
                .journey_reading(&journey_ref)
                .map_err(CliError::from_native)?;
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
                .ok_or_else(|| CliError::new("missing run-ref"))?
                .parse::<RunRef>()
                .map_err(|error| CliError::new(format!("invalid run-ref: {error}")))?;
            let reading = provider
                .run_reading(&run_ref)
                .map_err(CliError::from_native)?;
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
        "build" => {
            let run_ref = args
                .get(2)
                .ok_or_else(|| CliError::new("missing run-ref"))?
                .parse::<RunRef>()
                .map_err(|error| CliError::new(format!("invalid run-ref: {error}")))?;
            let snapshot = provider
                .build_snapshot(&run_ref)
                .map_err(CliError::from_native)?;
            if json {
                snapshot.to_json().map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nProject: {}\nRun: {}\nRevision: {}",
                    snapshot.contract,
                    snapshot.view.project.project_ref,
                    snapshot.view.run.run_ref,
                    snapshot.revision
                ))
            }
        }
        "workflow-units" => {
            let run_ref = args
                .get(2)
                .map(|value| {
                    value
                        .parse::<RunRef>()
                        .map_err(|error| CliError::new(format!("invalid run-ref: {error}")))
                })
                .transpose()?;
            let reading = provider
                .workflow_units_reading(run_ref.as_ref())
                .map_err(CliError::from_native)?;
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
                .ok_or_else(|| CliError::new("missing workflow-unit-ref"))?
                .parse::<WorkflowUnitRef>()
                .map_err(|error| CliError::new(format!("invalid workflow-unit-ref: {error}")))?;
            let run_ref = args
                .get(3)
                .map(|value| {
                    value
                        .parse::<RunRef>()
                        .map_err(|error| CliError::new(format!("invalid run-ref: {error}")))
                })
                .transpose()?;
            let reading = provider
                .workflow_unit_reading(&workflow_unit_ref, run_ref.as_ref())
                .map_err(CliError::from_native)?;
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
                .ok_or_else(|| CliError::new("missing telemetry-ref"))?
                .parse::<crate::core::identity::Ref>()
                .map_err(|error| CliError::new(format!("invalid telemetry-ref: {error}")))?;
            let reading = provider
                .execution_telemetry_reading(&telemetry_ref)
                .map_err(CliError::from_native)?;
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
        "commission-read" => {
            let request_ref = args
                .get(2)
                .ok_or_else(|| CliError::new("missing commission request-ref"))?;
            let reading = provider
                .commission_reading(request_ref)
                .map_err(CliError::from_native)?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nCommission: {}\nProject: {}\nJourney: {}\nRun: {}",
                    reading.contract,
                    reading.commission.request.request_ref,
                    reading.commission.project_ref,
                    reading.commission.journey_ref,
                    reading.commission.run_ref
                ))
            }
        }
        "mutate" => {
            let request_path = args.get(2).map(String::as_str).unwrap_or("-");
            let request: FactoryDevelopmentalMutationRequest =
                serde_json::from_str(&read_input(request_path, stdin_override)?)?;
            let receipt = provider
                .apply_developmental_mutation(request)
                .map_err(CliError::from_native)?;
            if json {
                serde_json::to_string_pretty(&receipt).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nMutation: {}\nOccurrence: {}\nStatus: {:?}",
                    receipt.contract,
                    receipt.record.request.mutation_ref,
                    receipt.record.request.occurrence_ref,
                    receipt.status
                ))
            }
        }
        "admit-routine-continuation" => {
            let request_path = args.get(2).map(String::as_str).unwrap_or("-");
            let input = read_input(request_path, stdin_override)?;
            let request: FactoryRoutineContinuationRequest = serde_json::from_str(&input)?;
            let admission = provider
                .admit_routine_continuation(request)
                .map_err(CliError::from_native)?;
            if json {
                serde_json::to_string_pretty(&admission).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nInvocation: {}\nJourney: {}\nRun: {}\nStatus: {:?}",
                    admission.contract,
                    admission.continuation.invocation_evidence.invocation_ref,
                    admission.continuation.journey_ref,
                    admission.continuation.run_ref,
                    admission.status
                ))
            }
        }
        "routine-continuation" => {
            let invocation_ref = args
                .get(2)
                .ok_or_else(|| CliError::new("missing invocation-ref"))?;
            let reading = provider
                .routine_continuation_reading(invocation_ref)
                .map_err(CliError::from_native)?;
            if json {
                serde_json::to_string_pretty(&reading).map_err(CliError::from)
            } else {
                Ok(format!(
                    "{}\nInvocation: {}\nJourney: {}\nRun: {}",
                    reading.contract,
                    reading.continuation.invocation_evidence.invocation_ref,
                    reading.continuation.journey_ref,
                    reading.continuation.run_ref
                ))
            }
        }
        "action" => {
            let request_path = args.get(2).map(String::as_str).unwrap_or("-");
            let input = read_input(request_path, stdin_override)?;
            let request: FactoryActionProjectionRequest = serde_json::from_str(&input)?;
            let receipt = execute_projected_factory_action(&mut provider, &request)
                .map_err(CliError::from_native)?;
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
        other => Err(CliError::new(format!(
            "unknown development operation `{other}`"
        ))),
    }
}

/// The request body `factory development observe` accepts.
///
/// `run_ref` is not part of the request: the Run the observation belongs to is
/// named on the command line, so a request body can never smuggle an
/// observation into a different Run's ledger. A body that does carry `run_ref`
/// must agree with it — a disagreement is refused rather than silently
/// preferred one way or the other.
#[derive(Debug, Deserialize)]
struct ObservationRequest {
    #[serde(default)]
    run_ref: Option<String>,
    observation_ref: String,
    kind: DevelopmentObservationKind,
    statement: String,
    #[serde(default)]
    subject_refs: Vec<String>,
    #[serde(default)]
    evidence_refs: Vec<String>,
    #[serde(default)]
    owner_return: Option<OwnerReturnProposal>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FactoryObservationReceipt<'a> {
    contract: &'a str,
    run_ref: String,
    observation_ref: &'a str,
    observation_count: usize,
    owner_return_required: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FactoryObservationReading<'a> {
    contract: &'a str,
    run_ref: String,
    observation_count: usize,
    observations: &'a [DevelopmentObservation],
}

/// Record one `DevelopmentObservation` into the Run's development ledger.
///
/// Additive by construction: the ledger keeps every observation already
/// recorded for the Run and refuses a duplicate `observation_ref`, so several
/// deferred pieces of work can stand open at once without any of them
/// overwriting another. A ledger that does not exist yet is created for the
/// named Run — the store is the Run's own retention, not an identity mint:
/// the RunRef is supplied by the caller and only ever validated here.
fn observe_operation(
    ledger_root: &str,
    args: &[String],
    json: bool,
    stdin_override: Option<&str>,
) -> Result<String, CliError> {
    let run_ref = args
        .first()
        .ok_or_else(|| CliError::new("missing run-ref"))?
        .parse::<RunRef>()
        .map_err(|error| CliError::new(format!("invalid run-ref: {error}")))?;
    let request_path = args.get(1).map(String::as_str).unwrap_or("-");
    let input = read_input(request_path, stdin_override)?;
    let request: ObservationRequest = serde_json::from_str(&input)?;
    if let Some(declared) = &request.run_ref {
        let declared = declared
            .parse::<RunRef>()
            .map_err(|error| CliError::new(format!("invalid run_ref in request: {error}")))?;
        if declared != run_ref {
            return Err(CliError::new(format!(
                "request run_ref {declared} does not match the addressed Run {run_ref}"
            )));
        }
    }

    let store = FileProjectDevelopmentStore::new(ledger_root);

    let observation = DevelopmentObservation {
        run_ref: run_ref.clone(),
        observation_ref: request.observation_ref,
        kind: request.kind,
        statement: request.statement,
        subject_refs: request.subject_refs,
        evidence_refs: request.evidence_refs,
        owner_return: request.owner_return,
    };
    let ledger = store
        .transact(&run_ref, true, |ledger| {
            ledger.add_observation(observation).map_err(|error| {
                crate::project_development_store::ProjectDevelopmentStoreError::Native(
                    error.to_string(),
                )
            })
        })
        .map_err(CliError::from_native)?;

    let recorded = ledger
        .observations
        .last()
        .expect("the observation just recorded is in the ledger");
    let receipt = FactoryObservationReceipt {
        contract: PROJECT_DEVELOPMENT_VERSION,
        run_ref: run_ref.to_string(),
        observation_ref: &recorded.observation_ref,
        observation_count: ledger.observations.len(),
        owner_return_required: recorded
            .owner_return
            .as_ref()
            .is_some_and(|proposal| proposal.recognition_required),
    };
    if json {
        return serde_json::to_string_pretty(&receipt).map_err(CliError::from);
    }
    Ok(format!(
        "{}\nRun: {}\nObservation: {} ({:?})\nOpen observations: {}\nOwner return required: {}",
        receipt.contract,
        receipt.run_ref,
        receipt.observation_ref,
        recorded.kind,
        receipt.observation_count,
        receipt.owner_return_required
    ))
}

/// Read a Run's recorded observations back out of its development ledger.
///
/// A Run with no ledger reads as zero observations rather than as an error:
/// "nothing was deferred here" is a real answer, and it is the answer a
/// verification wants when it asks whether a close-out registered anything.
fn observations_operation(
    ledger_root: &str,
    args: &[String],
    json: bool,
) -> Result<String, CliError> {
    let run_ref = args
        .first()
        .ok_or_else(|| CliError::new("missing run-ref"))?
        .parse::<RunRef>()
        .map_err(|error| CliError::new(format!("invalid run-ref: {error}")))?;
    let store = FileProjectDevelopmentStore::new(ledger_root);
    let ledger = store.load(&run_ref).map_err(CliError::from_native)?;
    let observations = ledger
        .as_ref()
        .map(|ledger| ledger.observations.as_slice())
        .unwrap_or(&[]);
    let reading = FactoryObservationReading {
        contract: PROJECT_DEVELOPMENT_VERSION,
        run_ref: run_ref.to_string(),
        observation_count: observations.len(),
        observations,
    };
    if json {
        return serde_json::to_string_pretty(&reading).map_err(CliError::from);
    }
    if observations.is_empty() {
        return Ok(format!(
            "{}\nRun: {}\nNo development observations recorded.",
            reading.contract, reading.run_ref
        ));
    }
    let lines = observations
        .iter()
        .map(|observation| {
            let owner = observation
                .owner_return
                .as_ref()
                .map(|proposal| {
                    format!(
                        " -> {} ({}, recognition required: {})",
                        proposal.owner_ref, proposal.proposal_ref, proposal.recognition_required
                    )
                })
                .unwrap_or_default();
            format!(
                "{}\t{:?}\t{}{}",
                observation.observation_ref, observation.kind, observation.statement, owner
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "{}\nRun: {}\nObservations: {}\n{}",
        reading.contract, reading.run_ref, reading.observation_count, lines
    ))
}

fn action_command(
    args: &[String],
    json: bool,
    stdin_override: Option<&str>,
) -> Result<String, CliError> {
    let operation = args
        .first()
        .ok_or_else(|| CliError::new("missing action operation"))?;
    let (state_path, selection) = selection_from_args(&args[1..])?;
    let mut provider = BuildStateDocument::open(&state_path, selection)?;

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
                .map_err(CliError::from_native)?;
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
        other => Err(CliError::new(format!("unknown action operation `{other}`"))),
    }
}

fn verify_command(args: &[String], json: bool) -> Result<String, CliError> {
    let provider_state_checked = if args.is_empty() {
        false
    } else {
        let (state_path, selection) = selection_from_args(args)?;
        BuildStateDocument::open(&state_path, selection)?.snapshot()?;
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
        return Err(CliError::new("expected <state> <project-ref> <run-ref>"));
    }
    let project_ref = ProjectRef::from_str(&args[1])
        .map_err(|error| CliError::new(format!("invalid project-ref: {error}")))?;
    let run_ref = RunRef::from_str(&args[2])
        .map_err(|error| CliError::new(format!("invalid run-ref: {error}")))?;
    Ok((
        args[0].clone(),
        FactoryBuildSelection {
            project_ref,
            run_ref,
        },
    ))
}

/// The state document behind the `build`, `action` and `verify` commands.
///
/// Two Factory-owned document kinds carry a Build projection: the Build
/// provider's own `factory.build-local-provider-state/v1` store, and the
/// developmental provider state `factory.developmental-local-provider/v1` —
/// the document `factory conformance developmental-state` writes — which
/// embeds the Build state of its single Project. Each kind is served through
/// its owning provider, so a developmental state is never rewritten as a
/// Build store and never loses its Journeys or correlations; any other
/// document kind is refused by name instead of surfacing as a deep serde
/// field error.
enum BuildStateDocument {
    Build(Box<FactoryBuildFileProvider>),
    Developmental {
        provider: Box<FactoryDevelopmentalFileProvider>,
        selection: FactoryBuildSelection,
    },
}

impl BuildStateDocument {
    fn open(path: &str, selection: FactoryBuildSelection) -> Result<Self, CliError> {
        let schema = document_schema(path)?;
        match schema.as_str() {
            FACTORY_BUILD_LOCAL_PROVIDER_STATE => Ok(Self::Build(Box::new(
                FactoryBuildFileProvider::open(path, selection)?,
            ))),
            FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER => {
                let provider = FactoryDevelopmentalFileProvider::open(path)
                    .map_err(CliError::from_native)?;
                if provider.project_ref() != &selection.project_ref {
                    return Err(CliError::new(format!(
                        "developmental state {} carries Project {}; the selection names {}",
                        path,
                        provider.project_ref(),
                        selection.project_ref
                    )));
                }
                Ok(Self::Developmental {
                    provider: Box::new(provider),
                    selection,
                })
            }
            other => Err(CliError::new(format!(
                "state document {path} declares schema `{other}`; expected `{FACTORY_BUILD_LOCAL_PROVIDER_STATE}` or `{FACTORY_DEVELOPMENTAL_LOCAL_PROVIDER}`"
            ))),
        }
    }

    fn snapshot(&self) -> Result<FactoryBuildSnapshot, CliError> {
        match self {
            Self::Build(provider) => Ok(provider.snapshot()?),
            Self::Developmental {
                provider,
                selection,
            } => Ok(provider.build_snapshot(&selection.run_ref)?),
        }
    }

    fn refresh(&mut self) -> Result<FactoryBuildSnapshot, CliError> {
        match self {
            Self::Build(provider) => provider.refresh().map_err(CliError::from),
            Self::Developmental {
                provider,
                selection,
            } => {
                let path = provider.path().to_string_lossy().into_owned();
                let selection = selection.clone();
                let reloaded = Self::open(&path, selection)?;
                let snapshot = reloaded.snapshot()?;
                *self = reloaded;
                Ok(snapshot)
            }
        }
    }
}

impl FactoryProjectedActionProvider for BuildStateDocument {
    type Error = FactoryActionProjectionError;

    fn execute_projected_action(
        &mut self,
        invocation: &FactoryActionInvocation,
        authority: &FactoryActionAuthority,
    ) -> Result<crate::build::FactoryActionReceipt, Self::Error> {
        match self {
            Self::Build(provider) => provider
                .execute_projected_action(invocation, authority)
                .map_err(|error| FactoryActionProjectionError::Provider(Box::new(error))),
            Self::Developmental {
                provider,
                selection,
            } => {
                // Parity with the Build provider: an invoked Action must land
                // on the Run named in the command's selection.
                if invocation.run_ref != selection.run_ref {
                    Err(FactoryActionProjectionError::Provider(Box::new(
                        CliError::new(format!(
                            "Action Run {} does not match provider-selected Run {}",
                            invocation.run_ref, selection.run_ref
                        )),
                    )))
                } else {
                    provider
                        .execute_projected_action(invocation, authority)
                        .map_err(|error| FactoryActionProjectionError::Provider(Box::new(error)))
                }
            }
        }
    }
}

/// Read only the top-level `schema` tag of a candidate state document, so a
/// wrong document kind is reported as itself rather than as a deep serde
/// field error (`missing field 'project'`) that names nothing.
fn document_schema(path: &str) -> Result<String, CliError> {
    let bytes = fs::read(path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        CliError::new(format!("state document {path} is not valid JSON: {error}"))
    })?;
    if let Some(schema) = value.get("schema").and_then(|schema| schema.as_str()) {
        return Ok(schema.to_owned());
    }
    if value.get("contract").and_then(|contract| contract.as_str())
        == Some(FACTORY_DEVELOPMENTAL_CONFORMANCE_MANIFEST)
    {
        return Err(CliError::new(format!(
            "state document {path} is a `{FACTORY_DEVELOPMENTAL_CONFORMANCE_MANIFEST}` locator (the stdout of `factory conformance developmental-state`), not a provider state document"
        )));
    }
    Err(CliError::new(format!(
        "state document {path} carries no top-level `schema`; it is not a Factory provider state document"
    )))
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

pub struct CliError {
    cause: Box<dyn Error + Send + Sync>,
    display_message: Option<String>,
    native_result: Option<serde_json::Value>,
}
impl std::fmt::Debug for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CliError").field("message",&self.to_string())
            .field("has_native_result",&self.native_result.is_some())
            .field("native_result",&"private evidence withheld").finish()
    }
}
#[derive(Debug)]
struct CliMessage(String);

impl Display for CliMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for CliMessage {}
impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::from_native(CliMessage(message.into()))
    }
    pub fn from_native(error: impl Error + Send + Sync + 'static) -> Self {
        Self::from_boxed(Box::new(error))
    }
    pub fn from_boxed(cause: Box<dyn Error + Send + Sync>) -> Self {
        Self {
            cause,
            display_message: None,
            native_result: None,
        }
    }
    /// Preserve compatibility text without replacing the original native cause.
    pub(crate) fn with_message(mut self, message: String) -> Self {
        self.display_message = Some(message);
        self
    }
    /// Retain only a result actually returned by this invocation after a typed
    /// uncertain publication or failed owner invocation. This transports evidence,
    /// never grants authority.
    pub(crate) fn with_native_result(mut self, result: serde_json::Value) -> Self {
        if crate::native_publication_uncertainty(&self).is_some()
            || self.cause.downcast_ref::<crate::attempt_owner_dispatch::AttemptOwnerError>().is_some()
        {
            self.native_result = Some(result);
        }
        self
    }
    /// Actual failed owner invocation result retained by the existing dispatcher.
    /// No result is reconstructed from private error bytes or historical prose.
    pub fn native_owner_failure_result(&self) -> Option<&serde_json::Value> {
        self.cause.downcast_ref::<crate::attempt_owner_dispatch::AttemptOwnerError>()?;
        self.native_result.as_ref()
    }
    pub fn native_publication_failure(&self) -> Option<serde_json::Value> {
        crate::native_publication_uncertainty(self)?;
        if let Some(owner) = self
            .cause
            .downcast_ref::<crate::configuration::ConfigError>()
        {
            if let Some(document) = owner.native_document() {
                return Some(document.clone());
            }
        }
        let mut failure = crate::native_publication_failure(self)?;
        if let Some(result) = &self.native_result {
            failure["error"]["details"]["native_result"] = result.clone();
        }
        Some(failure)
    }
}

impl Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(message) = &self.display_message {
            formatter.write_str(message)
        } else {
            Display::fmt(&self.cause, formatter)
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}
impl From<&str> for CliError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::from_native(error)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::from_native(error)
    }
}

impl From<crate::build_provider::FactoryBuildProviderError> for CliError {
    fn from(error: crate::build_provider::FactoryBuildProviderError) -> Self {
        Self::from_native(error)
    }
}

impl From<crate::developmental_read::FactoryDevelopmentalProviderError> for CliError {
    fn from(error: crate::developmental_read::FactoryDevelopmentalProviderError) -> Self {
        Self::from_native(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    fn publication_state() -> crate::build::FactoryBuildState {
        use crate::build::{CandidateRecord, FactoryBuildState};
        use crate::core::run::{Project, Run};
        let project: ProjectRef = "project:01ARZ3NDEKTSV4RRFFQ69G5FCA".parse().unwrap();
        let run: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FCB".parse().unwrap();
        let mut state = FactoryBuildState::new(
            Project::new(project.clone()),
            Run::new(
                run.clone(),
                project,
                "Native CLI publication uncertainty",
                "factory-test",
            )
            .unwrap(),
        )
        .unwrap();
        state
            .insert_candidate(CandidateRecord {
                run_ref: run,
                candidate_ref: "candidate:publication-proof".into(),
                revision: 1,
                label: "Actual owner action".into(),
                status: "ready".into(),
                producing_execution_refs: vec![],
                claim_refs: vec![],
                evidence_refs: vec![],
                artifact_refs: vec![],
                preview_ref: None,
                tradeoffs: vec![],
            })
            .unwrap();
        state
    }
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    fn assert_publication_envelope(
        error: &(dyn Error + 'static),
        path: &std::path::Path,
    ) -> serde_json::Value {
        let failure = crate::native_publication_failure(error)
            .expect("actual native cause survives the consumer chain");
        assert_eq!(failure["contract"], "factory.publication-failure/v1");
        assert_eq!(failure["ok"], false);
        assert_eq!(failure["error"]["code"], "factory.publication_uncertain");
        let details = &failure["error"]["details"];
        assert_eq!(details["source_path"], path.display().to_string());
        assert_eq!(details["published"], true);
        assert_eq!(details["outcome"], "unknown");
        assert_eq!(details["automatic_retry"], false);
        assert!(details.get("operation_ref").is_none());
        assert!(!details["cause"]["message"].as_str().unwrap().is_empty());
        failure
    }
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    #[test]
    fn actual_build_and_developmental_action_publication_failure_reaches_native_cli() {
        use crate::build::{
            REQUEST_MORE_EVIDENCE_ACTION_REF, REQUEST_MORE_EVIDENCE_CAPABILITY_REF,
        };
        use crate::developmental_read::FactoryDevelopmentalState;
        use std::os::unix::fs::PermissionsExt;
        for developmental in [false, true] {
            // Both a privacy drift and a genuine ENOENT occur after real rename;
            // no fake provider, fake error or string matching supplies the result.
            for move_after_publication in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let path = root.path().join("native-state.json");
                let retained = root.path().join("retained-committed-state.json");
                let state = publication_state();
                let before = state.revision().get();
                let selection = FactoryBuildSelection {
                    project_ref: state.project().reference().clone(),
                    run_ref: "run:01ARZ3NDEKTSV4RRFFQ69G5FCB".parse().unwrap(),
                };
                if developmental {
                    FactoryDevelopmentalFileProvider::create_new(
                        &path,
                        FactoryDevelopmentalState::new(state, vec![]).unwrap(),
                    )
                    .unwrap();
                } else {
                    FactoryBuildFileProvider::create(&path, selection.clone(), state).unwrap();
                }
                let physical = std::fs::canonicalize(&path).unwrap();
                let retained_for_observer = retained.clone();
                crate::native_file_transaction::observe_next_publication(move |published| {
                    if move_after_publication {
                        std::fs::rename(published, &retained_for_observer).unwrap();
                    } else {
                        std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777))
                            .unwrap();
                    }
                });
                let request = serde_json::json!({"contract":FACTORY_ACTION_PROJECTION_CONTRACT,
                    "projectionRef":"projection:actual-publication-proof",
                    "caller":{"callerRef":"agent:publication-proof","projectionKind":"situated-agent","lineage":["agent:publication-proof"]},
                    "actionRef":REQUEST_MORE_EVIDENCE_ACTION_REF,"subjectRef":"candidate:publication-proof","runRef":selection.run_ref,
                    "authority":{"authorityRef":"authority:publication-proof","nativeOwner":FACTORY_NATIVE_OWNER,
                        "capabilityRef":REQUEST_MORE_EVIDENCE_CAPABILITY_REF,"capabilityGranted":true,"actionAuthorised":true}});
                let args = vec![
                    "action".into(),
                    "invoke".into(),
                    path.display().to_string(),
                    selection.project_ref.to_string(),
                    selection.run_ref.to_string(),
                    "-".into(),
                    "--json".into(),
                ];
                let error =
                    crate::attempt_cli::execute(&args, Some(&request.to_string())).unwrap_err();
                let failure = assert_publication_envelope(&error, &physical);
                let actual_path = if move_after_publication {
                    &retained
                } else {
                    &path
                };
                if move_after_publication {
                    assert_eq!(
                        failure["error"]["details"]["cause"]["raw_os_error"],
                        libc::ENOENT
                    );
                    assert_eq!(
                        crate::native_publication_uncertainty(&error)
                            .unwrap()
                            .cause
                            .raw_os_error(),
                        Some(libc::ENOENT)
                    );
                    assert!(!path.exists());
                } else {
                    assert_eq!(failure["error"]["details"]["cause"]["kind"], "InvalidData");
                    assert_eq!(
                        std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
                        0o777
                    );
                }
                let after = if developmental {
                    FactoryDevelopmentalFileProvider::open(actual_path)
                        .unwrap()
                        .state()
                        .build
                        .revision()
                        .get()
                } else {
                    FactoryBuildFileProvider::open(actual_path, selection)
                        .unwrap()
                        .snapshot()
                        .unwrap()
                        .revision
                };
                assert!(
                    after > before,
                    "the native mutation really committed; reporting failure must not erase it"
                );
            }
        }
    }
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    #[test]
    fn actual_ledger_observation_publication_failure_keeps_result_and_typed_cli_cause() {
        use crate::project_development::ProjectDevelopmentLedger;
        use crate::project_development_store::ProjectDevelopmentStore;
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let run: RunRef = "run:01ARZ3NDEKTSV4RRFFQ69G5FCB".parse().unwrap();
        let store = FileProjectDevelopmentStore::new(root.path());
        store
            .save(&ProjectDevelopmentLedger::new(run.clone()))
            .unwrap();
        let path =
            std::fs::canonicalize(root.path().join(format!("{}.json", run.as_ref().id()))).unwrap();
        crate::native_file_transaction::observe_next_publication(|published| {
            std::fs::set_permissions(published, std::fs::Permissions::from_mode(0o777)).unwrap();
        });
        let request = serde_json::json!({"observation_ref":"observation:actual-publication-proof","kind":"insufficient-evidence",
            "statement":"Actual retained evidence after native rename","subject_refs":[run.to_string()],"evidence_refs":["evidence:actual-owner-readback"]});
        let args = vec![
            "development".into(),
            "observe".into(),
            root.path().display().to_string(),
            run.to_string(),
            "-".into(),
            "--json".into(),
        ];
        let error = crate::attempt_cli::execute(&args, Some(&request.to_string())).unwrap_err();
        assert_publication_envelope(&error, &path);
        let retained = store.load(&run).unwrap().unwrap();
        assert_eq!(retained.observations.len(), 1);
        assert_eq!(
            retained.observations[0].observation_ref,
            "observation:actual-publication-proof"
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o777
        );
    }
    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    #[test]
    fn actual_owner_reading_and_physical_cause_remain_separate_in_cli_failure() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("native-build.json");
        let retained = root.path().join("retained-native-build.json");
        let state = publication_state();
        let selection = FactoryBuildSelection {
            project_ref: state.project().reference().clone(),
            run_ref: "run:01ARZ3NDEKTSV4RRFFQ69G5FCB".parse().unwrap(),
        };
        FactoryBuildFileProvider::create(&path, selection.clone(), state).unwrap();
        let observed = retained.clone();
        crate::native_file_transaction::observe_next_publication(move |published| {
            std::fs::rename(published, observed).unwrap();
        });
        let request = serde_json::json!({"contract":FACTORY_ACTION_PROJECTION_CONTRACT,
            "projectionRef":"projection:actual-evidence-transport",
            "caller":{"callerRef":"agent:publication-proof","projectionKind":"situated-agent","lineage":["agent:publication-proof"]},
            "actionRef":crate::build::REQUEST_MORE_EVIDENCE_ACTION_REF,"subjectRef":"candidate:publication-proof","runRef":selection.run_ref,
            "authority":{"authorityRef":"authority:publication-proof","nativeOwner":FACTORY_NATIVE_OWNER,
                "capabilityRef":crate::build::REQUEST_MORE_EVIDENCE_CAPABILITY_REF,"capabilityGranted":true,"actionAuthorised":true}});
        let args = vec![
            "action".into(),
            "invoke".into(),
            path.display().to_string(),
            selection.project_ref.to_string(),
            selection.run_ref.to_string(),
            "-".into(),
            "--json".into(),
        ];
        let native_error = execute_cli(&args, Some(&request.to_string())).unwrap_err();
        // This tests existing evidence transport using an actual owner reading;
        // it does not stand in for Central submit or an executed worker Return.
        let actual_result = serde_json::to_value(
            FactoryBuildFileProvider::open(&retained, selection)
                .unwrap()
                .snapshot()
                .unwrap(),
        )
        .unwrap();
        let error = native_error.with_native_result(actual_result.clone());
        let failure = error.native_publication_failure().unwrap();
        assert_eq!(failure["error"]["details"]["native_result"], actual_result);
        assert_eq!(
            failure["error"]["details"]["cause"]["raw_os_error"],
            libc::ENOENT
        );
        assert_eq!(failure["error"]["details"]["published"], true);
        assert!(failure["error"]["details"].get("operation_ref").is_none());
        let before_effect =
            CliError::new("ordinary pre-effect refusal").with_native_result(actual_result);
        assert!(before_effect.native_publication_failure().is_none());
    }

    #[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
    #[test]
    fn actual_attempt_bootstrap_errno_is_preserved_through_store_runtime_and_cli() {
        use crate::attempt_runtime::{FactoryAttemptReading, FactoryAttemptSeed};
        use crate::core::run::Run;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("attempts.json");
        let retained = root.path().join("retained-actual-attempts.json");
        let seed = FactoryAttemptSeed {
            run: Run::new(
                "run:01ARZ3NDEKTSV4RRFFQ69G5FCB".parse().unwrap(),
                "project:01ARZ3NDEKTSV4RRFFQ69G5FCA".parse().unwrap(),
                "Real native bootstrap uncertainty",
                "factory-test",
            )
            .unwrap(),
            workflow_source: serde_json::from_str(include_str!(
                "../../contracts/factory/fixtures/agent-workflow-source.json"
            ))
            .unwrap(),
        };
        let physical = std::fs::canonicalize(root.path())
            .unwrap()
            .join("attempts.json");
        let observed = retained.clone();
        crate::native_file_transaction::observe_next_publication(move |published| {
            std::fs::rename(published, observed).unwrap();
        });
        let args = vec![
            "attempt".into(),
            "init".into(),
            path.display().to_string(),
            "-".into(),
            "--json".into(),
        ];
        let error =
            crate::attempt_cli::execute(&args, Some(&serde_json::to_string(&seed).unwrap()))
                .unwrap_err();
        let failure = assert_publication_envelope(&error, &physical);
        assert_eq!(
            failure["error"]["details"]["cause"]["raw_os_error"],
            libc::ENOENT
        );
        let reading: FactoryAttemptReading =
            crate::attempt_native_store::FileAttemptStore::open(&retained)
                .unwrap()
                .reading()
                .unwrap();
        assert!(
            reading.attempts.is_empty(),
            "bootstrap is not evidence a worker ran"
        );
        assert_eq!(reading.run_ref, seed.run.reference().clone());
        assert!(!path.exists());
    }

    #[test]
    fn version_and_help_are_native_and_stable() {
        let version = execute_cli(&["--version".into()], None).unwrap();
        assert!(
            version == format!("factory {}", env!("CARGO_PKG_VERSION"))
                || version.starts_with(&format!("factory {} (", env!("CARGO_PKG_VERSION"))),
            "version must be '<pkg-version>' or '<pkg-version> (<build-revision>)': {version:?}"
        );
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
