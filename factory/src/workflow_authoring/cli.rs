//! Public TypeScript source entry; existing Factory operations own all execution.
use super::inspect::{inspect, locate, summary};
use super::*;
use crate::attempt_native_store::FileAttemptStore;
use crate::commission::FactoryCommissionRequest;
use crate::core::run::RunRef;
use crate::developmental_read::FactoryDevelopmentalFileProvider;
use crate::project_development_store::read_developmental_state;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const COMMANDS: &[&str] = &[
    "workflow.check",
    "workflow.compile",
    "workflow.commission",
    "workflow.inspect",
    "workflow.inputs",
    "workflow.locate",
    "workflow.source",
    "workflow.schema",
    "workflow.sdk",
];
pub const CONTRACTS: &[&str] = &[
    AUTHORING_CONTRACT,
    "factory.authored-workflow/v1",
    "factory.workflow-commission-receipt/v1",
    "factory.workflow-inspection/v1",
    "factory.workflow-input-candidates/v1",
    "factory.workflow-location/v1",
    "factory.workflow-check/v1",
    "factory.workflow-source-reading/v1",
    "factory.workflow-sdk-install/v1",
    "factory.workflow-diagnostic/v1",
];
pub const HELP: &str = "Typed native workflows:\n  factory workflow check|compile <source.workflow.ts> [--root <source-root>] [--json]\n  (registered domain type adapters, e.g. @epilogos/ql-vak, are listed as domainAdapters in help --json)\n  factory workflow commission <state> <request.json> <source.workflow.ts> [--root <source-root>] [--json]\n  factory workflow inspect <state> <run-ref> [--unit <key-or-ref>] [--attempt <ref>] [--limit 1..100] [--cursor <json>] [--json]\n  factory workflow inputs <state> <run-ref> --unit <key-or-ref> [--expected-revision <number>] [--json]\n  factory workflow locate <state> <exact-native-ref> [--limit 1..100] [--cursor <json>] [--json]\n  factory workflow source <state> <run-ref> <retained-module.ts> [--json]\n  factory workflow schema [--json]\n  factory workflow sdk <new-directory> [--json]\n\nCompilation never executes source. Commission stamps the exact basis before the existing attempt lifecycle can act. Normal factory attempt Actions own start/fork/dispatch/retry/cancel/Return. Inspect omits source bodies and transport payloads; source explicitly reads one retained module.";
fn failure(e: impl fmt::Display) -> Diagnostic {
    error("workflow.operation", e.to_string())
}
fn read_request(path: &str) -> Result<FactoryCommissionRequest, Diagnostic> {
    let before = fs::symlink_metadata(path).map_err(failure)?;
    if !before.is_file() {
        return Err(failure("Request must be a regular, non-symlink file"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let file = options.open(path).map_err(failure)?;
    if !file.metadata().map_err(failure)?.is_file() {
        return Err(failure("Request must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BUNDLE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(failure("Request exceeds 2 MiB"));
    }
    serde_json::from_slice(&bytes).map_err(failure)
}
fn take_option(args: &mut Vec<String>, key: &str) -> Result<Option<String>, Diagnostic> {
    let Some(index) = args.iter().position(|a| a == key) else {
        return Ok(None);
    };
    args.remove(index);
    if index >= args.len() || args[index].starts_with("--") {
        return Err(failure(format!("{key} needs one value")));
    }
    let value = args.remove(index);
    if args.iter().any(|a| a == key) {
        return Err(failure(format!("Duplicate {key}")));
    }
    Ok(Some(value))
}
fn require_len(args: &[String], n: usize) -> Result<(), Diagnostic> {
    if args.len() != n || args.iter().any(|s| s.starts_with("--")) {
        Err(failure(format!(
            "Unsupported arguments; use factory workflow help\n{HELP}"
        )))
    } else {
        Ok(())
    }
}
fn load(path: &str, root: Option<String>) -> Result<AuthoredWorkflow, Diagnostic> {
    let path = Path::new(path);
    match root {
        Some(root) => load_workflow(Path::new(&root), path),
        None => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let entry = path
                .file_name()
                .ok_or_else(|| failure("Missing source filename"))?;
            load_workflow(parent, Path::new(entry))
        }
    }
}
pub fn execute(raw: &[String]) -> Result<Value, Diagnostic> {
    let mut args = raw.to_vec();
    if args.first().map(String::as_str) == Some("workflow") {
        args.remove(0);
    }
    if args.iter().filter(|a| a.as_str() == "--json").count() > 1 {
        return Err(failure("Duplicate --json"));
    }
    args.retain(|a| a != "--json");
    let op = args
        .first()
        .map(String::as_str)
        .unwrap_or("help")
        .to_owned();
    if !args.is_empty() {
        args.remove(0);
    }
    match op.as_str() {
        "help" | "--help" | "-h" => {
            require_len(&args, 0)?;
            let adapters = super::domain::adapters()?
                .iter()
                .map(|a| json!({"specifier":a.specifier,"owner":a.owner,"typesContract":a.types_contract,
                    "unitField":a.unit_field,"lowering":a.lowering,"declarationsSha256":a.declarations_sha256,
                    "upstream":a.upstream}))
                .collect::<Vec<_>>();
            Ok(
                json!({"help":HELP,"commands":COMMANDS,"contracts":CONTRACTS,"domainAdapters":adapters}),
            )
        }
        "check" | "compile" => {
            let root = take_option(&mut args, "--root")?;
            require_len(&args, 1)?;
            let loaded = load(&args[0], root)?;
            if op == "compile" {
                serde_json::to_value(loaded).map_err(failure)
            } else {
                Ok(
                    json!({"contract":"factory.workflow-check/v1","valid":true,"source":summary(&loaded.source),
                "units":loaded.compiled.units.values().map(|u|{
                    let mut unit=json!({"key":u.key,"ref":u.reference});
                    if let Some(c)=&u.composition {
                        unit["composition"]=json!({"contract":c.contract,"actorRef":c.actor_ref,"frame":c.frame,
                            "thread":c.thread,"threadForm":c.thread_form().musical_role(),"sequence":c.sequence,
                            "direction":c.direction,"participation":c.participation,"content":c.content,"position":c.position,
                            "qlBindingRef":c.ql_binding_ref,"qlBindingRevision":c.ql_binding_revision,"wholeRef":c.whole_ref});
                    }
                    unit
                }).collect::<Vec<_>>(),"execution":"not-requested"}),
                )
            }
        }
        "commission" => {
            let root = take_option(&mut args, "--root")?;
            require_len(&args, 3)?;
            let request = read_request(&args[1])?;
            let loaded = load(&args[2], root)?;
            let receipt = FactoryDevelopmentalFileProvider::commission_workflow(
                &args[0],
                request,
                loaded.source.clone(),
            )
            .map_err(failure)?;
            let store = FileAttemptStore::attach(
                &args[0],
                receipt.commission.run_ref.clone(),
                &loaded.source.source.reference.to_string(),
            )
            .map_err(failure)?;
            let reading = store.reading().map_err(failure)?;
            Ok(
                json!({"contract":"factory.workflow-commission-receipt/v1","commission":receipt,"source":summary(&loaded.source),
                "attempts":reading,"execution":"not-requested","nextCommand":"factory attempt action"}),
            )
        }
        "inspect" => {
            let unit = take_option(&mut args, "--unit")?;
            let attempt = take_option(&mut args, "--attempt")?;
            let limit = take_option(&mut args, "--limit")?
                .map(|s| s.parse::<usize>().map_err(failure))
                .transpose()?
                .unwrap_or(25);
            let cursor = take_option(&mut args, "--cursor")?;
            require_len(&args, 2)?;
            inspect(
                Path::new(&args[0]),
                &args[1].parse().map_err(failure)?,
                unit,
                attempt,
                limit,
                cursor.as_deref(),
            )
        }
        "inputs" => {
            let unit = take_option(&mut args, "--unit")?
                .ok_or_else(|| failure("inputs requires --unit"))?;
            let expected = take_option(&mut args, "--expected-revision")?
                .map(|v| v.parse::<u64>().map_err(failure))
                .transpose()?;
            require_len(&args, 2)?;
            FileAttemptStore::open_run(&args[0], args[1].parse().map_err(failure)?)
                .map_err(failure)?
                .workflow_inputs(&unit, expected)
                .map_err(failure)
        }
        "locate" => {
            let limit = take_option(&mut args, "--limit")?
                .map(|s| s.parse::<usize>().map_err(failure))
                .transpose()?
                .unwrap_or(25);
            let cursor = take_option(&mut args, "--cursor")?;
            require_len(&args, 2)?;
            locate(Path::new(&args[0]), &args[1], limit, cursor.as_deref())
        }
        "source" => {
            require_len(&args, 3)?;
            let state = read_developmental_state(Path::new(&args[0])).map_err(failure)?;
            let run: RunRef = args[1].parse().map_err(failure)?;
            let source = state
                .attempt_states
                .get(&run)
                .ok_or_else(|| failure("Run has no retained workflow"))?
                .workflow_source();
            let basis = source.source.authoring.as_ref().ok_or_else(|| {
                failure("Historical source lacks a typed authoring stamp; no source bytes inferred")
            })?;
            basis.validate()?;
            let module = basis
                .modules
                .get(&args[2])
                .ok_or_else(|| failure("Module is not part of the original source basis"))?;
            Ok(
                json!({"contract":"factory.workflow-source-reading/v1","runRef":run,"sourceRef":source.source.reference,
                "sourceRevision":source.source.revision,"sourceDigest":source.source.digest,"authoredRevision":basis.revision,"module":module}),
            )
        }
        "schema" => {
            require_len(&args, 0)?;
            serde_json::from_str(include_str!("../../workflow-sdk/schema.json")).map_err(failure)
        }
        "sdk" => {
            require_len(&args, 1)?;
            install_sdk(Path::new(&args[0]))
        }
        _ => Err(failure(format!(
            "Unknown workflow operation {op}; use factory workflow help"
        ))),
    }
}
fn install_sdk(path: &Path) -> Result<Value, Diagnostic> {
    let root = PathBuf::from(path);
    fs::create_dir(&root).map_err(failure)?;
    fs::create_dir(root.join("examples")).map_err(failure)?;
    for (name, bytes) in [
        (
            "package.json",
            include_bytes!("../../workflow-sdk/package.json").as_slice(),
        ),
        (
            "index.d.ts",
            include_bytes!("../../workflow-sdk/index.d.ts").as_slice(),
        ),
        (
            "index.mjs",
            include_bytes!("../../workflow-sdk/index.mjs").as_slice(),
        ),
        (
            "schema.json",
            include_bytes!("../../workflow-sdk/schema.json").as_slice(),
        ),
        (
            "README.md",
            include_bytes!("../../workflow-sdk/README.md").as_slice(),
        ),
        (
            "test.mjs",
            include_bytes!("../../workflow-sdk/test.mjs").as_slice(),
        ),
        (
            "examples/single.workflow.ts",
            include_bytes!("../../workflow-sdk/examples/single.workflow.ts").as_slice(),
        ),
        (
            "examples/plural.workflow.ts",
            include_bytes!("../../workflow-sdk/examples/plural.workflow.ts").as_slice(),
        ),
        (
            "examples/commission.json",
            include_bytes!("../../workflow-sdk/examples/commission.json").as_slice(),
        ),
    ] {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .map_err(failure)?;
        f.write_all(bytes).map_err(failure)?;
        f.sync_all().map_err(failure)?;
    }
    Ok(
        json!({"contract":"factory.workflow-sdk-install/v1","path":root,"compiler":COMPILER,"compilerDigest":compiler_digest(),"installed":true}),
    )
}
pub fn main(args: &[String]) -> ExitCode {
    match execute(args) {
        Ok(value) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&value).expect("JSON value")
            );
            ExitCode::SUCCESS
        }
        Err(d) => {
            if args.iter().any(|a| a == "--json") {
                eprintln!(
                    "{}",
                    serde_json::to_string(
                        &json!({"contract":"factory.workflow-diagnostic/v1","error":d})
                    )
                    .expect("diagnostic")
                );
            } else {
                eprintln!("{d}");
            }
            ExitCode::from(2)
        }
    }
}
