//! CLI projection for the native Development Field technique.
//!
//! This extends the existing `factory development` doorway and delegates every
//! unrelated command to the established CLI.  It is deliberately CRUD-like over
//! Factory developmental truth; it does not introduce an orchestration grammar.

use crate::core::run::RunRef;
use crate::development_field::{
    AikitOperativeReferences, DevelopmentField, DevelopmentFieldReading, DevelopmentFieldReturn,
    DevelopmentMaterialBinding, DEVELOPMENT_FIELD_READING_CONTRACT,
};
use crate::project_development::ProjectDevelopmentLedger;
use crate::project_development_store::{FileProjectDevelopmentStore, ProjectDevelopmentStore};
use serde_json::Value;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{self, Read};
use std::process::ExitCode;
use std::str::FromStr;

const FIELD_COMMANDS: &[&str] = &[
    "development.field.read",
    "development.field.set",
    "development.field.operative",
    "development.field.bind-material",
    "development.field.return",
];

pub fn cli_main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match execute_extension(&args, None) {
        Ok(Some(output)) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Ok(None) => crate::cli::cli_main(),
        Err(error) => {
            eprintln!("factory: {error}");
            ExitCode::from(2)
        }
    }
}

/// Execute only the Development Field extension. `None` means the established
/// Factory CLI owns this command and should receive it unchanged.
pub fn execute_extension(
    args: &[String],
    stdin_override: Option<&str>,
) -> Result<Option<String>, DevelopmentFieldCliError> {
    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => {
            let base = crate::cli::execute_cli(args, None)
                .map_err(|error| DevelopmentFieldCliError(error.to_string()))?;
            Ok(Some(format!("{base}\n{}", field_help())))
        }
        Some("capabilities") => Ok(Some(extend_capabilities(args)?)),
        Some("development") if args.get(1).map(String::as_str) == Some("field") => {
            Ok(Some(field_command(&args[2..], stdin_override)?))
        }
        _ => Ok(None),
    }
}

fn field_help() -> &'static str {
    "Development Field:\n  factory development field read          <ledger-root> <run-ref> [--json]\n  factory development field set           <ledger-root> <run-ref> [request-file|-] [--json]\n  factory development field operative     <ledger-root> <run-ref> [request-file|-] [--json]\n  factory development field bind-material <ledger-root> <run-ref> [request-file|-] [--json]\n  factory development field return        <ledger-root> <run-ref> [request-file|-] [--json]\n\nThe Development Field retains Factory developmental meaning and references native owner state; it does not execute Git, AIKit, Workcell or Actuation operations itself."
}

fn extend_capabilities(args: &[String]) -> Result<String, DevelopmentFieldCliError> {
    let json = args.iter().any(|argument| argument == "--json");
    let base = crate::cli::execute_cli(args, None)
        .map_err(|error| DevelopmentFieldCliError(error.to_string()))?;
    if !json {
        return Ok(format!(
            "{base}\ndevelopment-field commands: {}\ndevelopment-field contract: {}",
            FIELD_COMMANDS.join(", "),
            DEVELOPMENT_FIELD_READING_CONTRACT
        ));
    }

    let mut value: Value = serde_json::from_str(&base)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| DevelopmentFieldCliError("invalid base capabilities document".into()))?;
    let commands = object
        .get_mut("commands")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| DevelopmentFieldCliError("base capabilities omit commands".into()))?;
    for command in FIELD_COMMANDS {
        commands.push(Value::String((*command).to_owned()));
    }
    let contracts = object
        .get_mut("nativeContracts")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| DevelopmentFieldCliError("base capabilities omit nativeContracts".into()))?;
    contracts.push(Value::String(DEVELOPMENT_FIELD_READING_CONTRACT.into()));
    serde_json::to_string_pretty(&value).map_err(Into::into)
}

fn field_command(
    raw_args: &[String],
    stdin_override: Option<&str>,
) -> Result<String, DevelopmentFieldCliError> {
    let mut args = raw_args.to_vec();
    let json = remove_flag(&mut args, "--json");
    let operation = args
        .first()
        .ok_or_else(|| DevelopmentFieldCliError("missing Development Field operation".into()))?;
    let ledger_root = args
        .get(1)
        .ok_or_else(|| DevelopmentFieldCliError("missing development ledger root".into()))?;
    let run_ref = args
        .get(2)
        .ok_or_else(|| DevelopmentFieldCliError("missing run-ref".into()))?;
    let run_ref = RunRef::from_str(run_ref)
        .map_err(|error| DevelopmentFieldCliError(format!("invalid run-ref: {error}")))?;
    let store = FileProjectDevelopmentStore::new(ledger_root);

    let reading = match operation.as_str() {
        "read" => load_required(&store, &run_ref)?.development_field_reading()?,
        "set" => {
            let mut ledger = load_or_new(&store, &run_ref)?;
            let request_path = args.get(3).map(String::as_str).unwrap_or("-");
            let field: DevelopmentField =
                serde_json::from_str(&read_input(request_path, stdin_override)?)?;
            ledger.set_development_field(field)?;
            store.save(&ledger)?;
            ledger.development_field_reading()?
        }
        "operative" => {
            let mut ledger = load_required(&store, &run_ref)?;
            let request_path = args.get(3).map(String::as_str).unwrap_or("-");
            let operative: AikitOperativeReferences =
                serde_json::from_str(&read_input(request_path, stdin_override)?)?;
            ledger.set_development_field_operative(operative)?;
            store.save(&ledger)?;
            ledger.development_field_reading()?
        }
        "bind-material" => {
            let mut ledger = load_required(&store, &run_ref)?;
            let request_path = args.get(3).map(String::as_str).unwrap_or("-");
            let binding: DevelopmentMaterialBinding =
                serde_json::from_str(&read_input(request_path, stdin_override)?)?;
            ledger.add_development_field_material(binding)?;
            store.save(&ledger)?;
            ledger.development_field_reading()?
        }
        "return" => {
            let mut ledger = load_required(&store, &run_ref)?;
            let request_path = args.get(3).map(String::as_str).unwrap_or("-");
            let returned: DevelopmentFieldReturn =
                serde_json::from_str(&read_input(request_path, stdin_override)?)?;
            ledger.add_development_field_return(returned)?;
            store.save(&ledger)?;
            ledger.development_field_reading()?
        }
        other => {
            return Err(DevelopmentFieldCliError(format!(
                "unknown Development Field operation `{other}`"
            )))
        }
    };

    render_reading(reading, json)
}

fn load_required(
    store: &FileProjectDevelopmentStore,
    run_ref: &RunRef,
) -> Result<ProjectDevelopmentLedger, DevelopmentFieldCliError> {
    store
        .load(run_ref)?
        .ok_or_else(|| DevelopmentFieldCliError(format!("development ledger not found for {run_ref}")))
}

fn load_or_new(
    store: &FileProjectDevelopmentStore,
    run_ref: &RunRef,
) -> Result<ProjectDevelopmentLedger, DevelopmentFieldCliError> {
    Ok(store
        .load(run_ref)?
        .unwrap_or_else(|| ProjectDevelopmentLedger::new(run_ref.clone())))
}

fn render_reading(
    reading: DevelopmentFieldReading,
    json: bool,
) -> Result<String, DevelopmentFieldCliError> {
    if json {
        return serde_json::to_string_pretty(&reading).map_err(Into::into);
    }
    let field = &reading.field;
    let latest_result = field
        .returns
        .last()
        .map(|returned| returned.result_revision.as_str())
        .unwrap_or("not-returned");
    let grade_list = |grades: &std::collections::BTreeSet<_>| {
        grades
            .iter()
            .map(|grade| format!("{grade:?}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    Ok(format!(
        "{}\nField: {}\nProject: {}\nJourney: {}\nRun: {}\nCommission: {}\nDifference: {}\nPlan: {}\nUX: {}\nCapabilities: {}\nGit: {} @ {} -> {}\nOperative resolution: {}\nMaterial bindings: {}\nActuality correlations: {}\nProof required: {}\nProof satisfied: {}\nProof remaining: {}\nLatest return: {}",
        reading.contract,
        field.field_ref,
        field.project_ref,
        field.journey_ref,
        field.run_ref,
        field.commission_ref,
        field.required_difference,
        field.targets.plan_ref,
        field.targets.ux_refs.join(", "),
        field.targets.capability_refs.join(", "),
        field.git_basis.repository_ref,
        field.git_basis.base_revision,
        latest_result,
        field
            .aikit_operative
            .as_ref()
            .map(|operative| operative.resolution_ref.as_str())
            .unwrap_or("unresolved"),
        field
            .material_bindings
            .iter()
            .map(|binding| binding.binding_ref.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        field
            .returns
            .iter()
            .flat_map(|returned| returned.actualities.iter())
            .map(|actuality| actuality.execution_correlation_ref.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        grade_list(&field.required_proof),
        grade_list(&reading.satisfied_proof),
        grade_list(&reading.remaining_proof),
        reading.latest_return_ref.as_deref().unwrap_or("none")
    ))
}

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|argument| argument == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn read_input(
    path: &str,
    stdin_override: Option<&str>,
) -> Result<String, DevelopmentFieldCliError> {
    if path != "-" {
        return std::fs::read_to_string(path).map_err(Into::into);
    }
    if let Some(input) = stdin_override {
        return Ok(input.to_owned());
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(input)
}

#[derive(Debug)]
pub struct DevelopmentFieldCliError(String);

impl Display for DevelopmentFieldCliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for DevelopmentFieldCliError {}

impl From<std::io::Error> for DevelopmentFieldCliError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for DevelopmentFieldCliError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<crate::project_development::ProjectDevelopmentError> for DevelopmentFieldCliError {
    fn from(error: crate::project_development::ProjectDevelopmentError) -> Self {
        Self(error.to_string())
    }
}

impl From<crate::project_development_store::ProjectDevelopmentStoreError>
    for DevelopmentFieldCliError
{
    fn from(error: crate::project_development_store::ProjectDevelopmentStoreError) -> Self {
        Self(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::run::{ProjectRef, WorkflowUnitRef};
    use crate::development_field::{
        DevelopmentEvidenceGrade, DevelopmentFieldTargets, DevelopmentGitBasis,
    };
    use crate::journey::JourneyRef;
    use std::collections::BTreeSet;
    use tempfile::tempdir;

    fn field(run_ref: RunRef) -> DevelopmentField {
        let project_ref =
            ProjectRef::from_str("project:01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        DevelopmentField::new(
            "development-field:cli",
            project_ref.clone(),
            run_ref.clone(),
            JourneyRef::from_str("journey:01ARZ3NDEKTSV4RRFFQ69G5FAX").unwrap(),
            "commission:cli",
            vec![WorkflowUnitRef::from_str(
                "workflow-unit:01ARZ3NDEKTSV4RRFFQ69G5FAY",
            )
            .unwrap()],
            "Return the intended software difference with exact provenance",
            DevelopmentFieldTargets {
                plan_ref: "plan:cli".into(),
                ux_refs: vec!["ux:cli".into()],
                capability_refs: vec!["capability/factory/development-field".into()],
                source_refs: vec!["source:issue-215".into()],
                self_description_refs: Vec::new(),
            },
            BTreeSet::from([DevelopmentEvidenceGrade::D, DevelopmentEvidenceGrade::H]),
            DevelopmentGitBasis {
                git_development_ref: "git-development:cli".into(),
                project_ref,
                run_ref,
                repository_ref: "repo:agent-system-design".into(),
                base_revision: "revision:base".into(),
                initial_worktree_clean: true,
                dirty_snapshot_ref: None,
                initial_worktree_ref: Some("git-worktree:cli".into()),
                source_basis_refs: vec!["source:issue-215".into()],
                structural_ground_refs: Vec::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn cli_persists_and_reads_run_scoped_field() {
        let root = tempdir().unwrap();
        let run_ref = RunRef::from_str("run:01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap();
        let encoded = serde_json::to_string(&field(run_ref.clone())).unwrap();
        let set_args = vec![
            "development".into(),
            "field".into(),
            "set".into(),
            root.path().display().to_string(),
            run_ref.to_string(),
            "-".into(),
            "--json".into(),
        ];
        let set = execute_extension(&set_args, Some(&encoded))
            .unwrap()
            .expect("field command handled");
        let set: DevelopmentFieldReading = serde_json::from_str(&set).unwrap();
        assert_eq!(set.field.targets.plan_ref, "plan:cli");
        assert_eq!(
            set.remaining_proof,
            BTreeSet::from([DevelopmentEvidenceGrade::D, DevelopmentEvidenceGrade::H])
        );

        let read_args = vec![
            "development".into(),
            "field".into(),
            "read".into(),
            root.path().display().to_string(),
            run_ref.to_string(),
            "--json".into(),
        ];
        let read = execute_extension(&read_args, None)
            .unwrap()
            .expect("field command handled");
        let read: DevelopmentFieldReading = serde_json::from_str(&read).unwrap();
        assert_eq!(read.field.run_ref, run_ref);
        assert_eq!(read.contract, DEVELOPMENT_FIELD_READING_CONTRACT);
    }

    #[test]
    fn help_and_capabilities_expose_the_native_surface() {
        let help = execute_extension(&["help".into()], None)
            .unwrap()
            .expect("help handled");
        assert!(help.contains("factory development field read"));

        let capabilities = execute_extension(&["capabilities".into(), "--json".into()], None)
            .unwrap()
            .expect("capabilities handled");
        let capabilities: Value = serde_json::from_str(&capabilities).unwrap();
        assert!(capabilities["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "development.field.return"));
    }
}
