//! What Factory admits from a desktop that acts on its attempts. A desktop
//! (the O:I shell) calls the attempt Actions as a person, not as an agent: it
//! needs the caller Factory expects and the authority the host's native action
//! contract must give, and until Factory said so the desk could only refuse
//! ("Factory has not disclosed a native desktop caller and authority grant").
//! This is a reading, not a grant: it names the caller identity and the native
//! action, scope and actor kind a grant must carry. Which principal of the
//! owner's `Control/user/native-action-authority.json` carries them is the
//! host's to resolve, with the host's own token, which never travels in JSON.
//!
//! The scope a grant must cover is the Run's own: Central authenticates an act where it happens (`control:root`, or
//! `project:<id>` for an act in a project), and a Run works in the project its Factory state stands in. Read for a state
//! (`factory attempt admission <state>`), each action names that project (`run_scope`); read without one, it names the rule
//! and no scope, and a desktop that needs the scope asks with the state.
use crate::attempt_runtime::{FACTORY_ATTEMPT_ACTION, FACTORY_ATTEMPT_CAPABILITY_REF};
use crate::build::FACTORY_NATIVE_OWNER;
use crate::cli::CliError;
use serde_json::{json, Value};
use std::path::Path;

pub const ADMISSION_READING: &str = "factory.attempt-admission/v1";
/// The caller a desktop acts as. Its lineage ends here, as every attempt Action
/// caller's must.
pub const DESKTOP_CALLER_REF: &str = "oi:desktop";
/// The contract that names the native action grants, and the form of the
/// authority ref a caller presents for one of its principals.
const GRANT_CONTRACT: &str = "central.native-action-authority/v1";
const GRANT_PATH: &str = "Control/user/native-action-authority.json";
const GRANT_REF_FORM: &str = "central:native-action-authority#<principal_ref>";
/// What choosing an attempt's Return does at Central: it is proposed to the
/// receiving field (`factory attempt receiving`).
const RECEIVING_NATIVE_ACTION: &str = "central.receiving.submit";
/// What retrying a unit is at the host: Factory's own `retry` attempt operation (`factory attempt action <state> -`), which the
/// host authenticates against the person's grant itself (Central lists it among the host-checked actions a project grant holds).
const RETRY_NATIVE_ACTION: &str = "factory.attempt.retry";
/// Where the scope comes from, said when no state is named.
const SCOPE_RULE: &str = "the project the Run's Factory state stands in: project:<id> of the ProjectCentral manifest beside its .factory, or control:root for a state at a Central ground's own root (factory attempt admission <state>)";

/// The world a Run's acts are authenticated in: the project its Factory state stands in. A state is kept in its project's
/// `.factory/` (crate::project_setup), so the project root is the folder above it; that root's ProjectCentral manifest names the
/// Central project (`project:<project_id>`), and a root that is a Central ground itself (it keeps `Control/`) is the root's
/// (`control:root`). Anything else names no scope, and the reading says why.
pub fn run_scope(state: &Path) -> Result<String, String> {
    let factory = state
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == ".factory"))
        .ok_or_else(|| {
            format!(
                "{} is no Factory state kept in a project's .factory folder, so it names no scope",
                state.display()
            )
        })?;
    let root = factory
        .parent()
        .ok_or("The Factory state's project has no folder, so it names no scope")?;
    let manifest = root.join("ProjectCentral/project.json");
    match std::fs::read(&manifest) {
        Ok(bytes) => {
            let source: Value = serde_json::from_slice(&bytes)
                .map_err(|error| format!("{} cannot be read: {error}", manifest.display()))?;
            source["project_id"]
                .as_str()
                .filter(|id| source["schema"] == "central.project/v1" && !id.trim().is_empty())
                .map(|id| format!("project:{id}"))
                .ok_or_else(|| {
                    format!(
                        "{} names no Central project, so the Run names no scope",
                        manifest.display()
                    )
                })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if root.join("Control").is_dir() {
                Ok("control:root".into())
            } else {
                Err(format!(
                    "{} keeps no ProjectCentral manifest and is no Central ground, so the Run names no scope",
                    root.display()
                ))
            }
        }
        Err(error) => Err(format!("{} cannot be read: {error}", manifest.display())),
    }
}

/// The disclosure, without a Run: each action names the scope rule and no scope.
pub fn reading() -> Value {
    reading_for(None)
}

/// The disclosure for one Run's state: each action names the scope its grant must cover.
pub fn reading_for(scope: Option<&str>) -> Value {
    json!({
        "contract": ADMISSION_READING,
        "nativeOwner": FACTORY_NATIVE_OWNER,
        "capabilityRef": FACTORY_ATTEMPT_CAPABILITY_REF,
        "desktopCaller": {
            "callerRef": DESKTOP_CALLER_REF,
            "projectionKind": "desktop-human",
            "lineage": [DESKTOP_CALLER_REF],
        },
        "authority": {
            "authorityRef": GRANT_REF_FORM,
            "nativeOwner": FACTORY_NATIVE_OWNER,
            "capabilityRef": FACTORY_ATTEMPT_CAPABILITY_REF,
            "grantContract": GRANT_CONTRACT,
            "grantPath": GRANT_PATH,
            "capabilityGranted": "true only once the host has resolved a grant that carries the action below",
            "actionAuthorised": "true only once the host has resolved a grant that carries the action below",
            "token": "the host's own CENTRAL_NATIVE_TOKEN; never part of a request",
        },
        "central": {
            "contractRevision": crate::attempt_receiving::CENTRAL_CONTRACT_REVISION,
            "binary": "the host's Central owner executable, an absolute path",
            "root": "the Central root of the Run's Flow (flowAssociations[].flow.location.root)",
        },
        "scopeRule": SCOPE_RULE,
        "actions": [{
            "actionRef": "attempt-receiving",
            "contract": crate::attempt_receiving::RECEIVING_ACTION,
            "command": "factory attempt receiving <state> <request-json|->",
            "nativeAction": RECEIVING_NATIVE_ACTION,
            "scopeRef": scope,
            "actorKind": "human",
            "meaning": "The person chooses an attempt: its retained Return is proposed to Central's receiving field. This is not Recognition, not inclusion of a document and not the Run's close.",
        }, {
            "actionRef": "attempt-retry",
            "contract": FACTORY_ATTEMPT_ACTION,
            "command": "factory attempt action <state> -",
            "operation": "retry",
            "nativeAction": RETRY_NATIVE_ACTION,
            "scopeRef": scope,
            "actorKind": "human",
            "meaning": "The person retries a unit: Factory's own retry operation starts a new attempt under the retry grant the unit's current attempt already consumes, with that attempt's task, parent Journey and disposition. Factory refuses a retry that needs re-resolution, in its own words.",
        }],
        "standing": "A reading of what Factory expects of a desktop caller. It grants nothing: the grant is the owner's, in the contract named here.",
    })
}

pub fn execute_cli(args: &[String]) -> Result<String, CliError> {
    if args
        .iter()
        .any(|argument| matches!(argument.as_str(), "help" | "--help" | "-h"))
    {
        return Ok(format!(
            "factory attempt admission [<state>] [--json]\nReads {ADMISSION_READING}: the desktop caller Factory expects and the native action, scope and actor kind the host's grant must carry, the scope being the project the Run's state stands in. It grants nothing."
        ));
    }
    let states: Vec<&String> = args
        .iter()
        .filter(|argument| *argument != "--json")
        .collect();
    let reading = match states.as_slice() {
        [] => reading(),
        [state] if !state.starts_with('-') => reading_for(Some(
            &run_scope(Path::new(state.as_str())).map_err(CliError::new)?,
        )),
        _ => {
            return Err(
                "factory attempt admission takes at most one Factory state and --json".into(),
            )
        }
    };
    serde_json::to_string_pretty(&reading).map_err(CliError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_projection::{FactoryActionCaller, ProjectedFactoryActionAuthority};
    use crate::attempt_runtime::{
        validate_action_request, FactoryAttemptActionRequest, FactoryAttemptOperation,
        OwnerOperationPhase, OwnerOperationReceipt, FACTORY_ATTEMPT_ACTION,
    };
    use std::collections::BTreeSet;

    /// The disclosed caller and authority, as a desktop would present them
    /// once the host has resolved a grant, are what the attempt Actions admit.
    #[test]
    fn the_disclosed_caller_and_authority_are_what_attempt_actions_admit() {
        let disclosed = reading();
        let caller: FactoryActionCaller =
            serde_json::from_value(disclosed["desktopCaller"].clone()).unwrap();
        let authority = ProjectedFactoryActionAuthority {
            authority_ref: GRANT_REF_FORM.replace(
                "<principal_ref>",
                "central:source:control:root:Control/user/identity",
            ),
            native_owner: disclosed["authority"]["nativeOwner"]
                .as_str()
                .unwrap()
                .into(),
            capability_ref: disclosed["authority"]["capabilityRef"]
                .as_str()
                .map(str::to_owned),
            capability_granted: true,
            action_authorised: true,
        };
        let request = FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: "projection:admission-test".into(),
            caller,
            run_ref: "run:01ARZ3NDEKTSV4RRFFQ69G5FBD".parse().unwrap(),
            expected_revision: 1,
            authority,
            operation: FactoryAttemptOperation::RecordObservation {
                attempt_ref: "attempt:admission-test".into(),
                receipt: OwnerOperationReceipt {
                    owner_ref: "factory".into(),
                    contract: "factory.admission-test/v1".into(),
                    operation_ref: "operation:admission-test".into(),
                    receipt_ref: "receipt:admission-test".into(),
                    source_revision: "factory-state:1".into(),
                    phase: OwnerOperationPhase::Observed,
                    evidence_refs: BTreeSet::new(),
                    partial_effect_refs: BTreeSet::new(),
                    payload: Value::Null,
                },
            },
        };
        validate_action_request(&request).expect("the disclosed identity is admitted");
        assert_eq!(request.caller.caller_ref, DESKTOP_CALLER_REF);
        assert_eq!(
            serde_json::to_value(request.caller.projection_kind).unwrap(),
            "desktop-human"
        );
    }

    #[test]
    fn the_central_revision_is_the_one_the_receiving_path_demands() {
        assert_eq!(
            reading()["central"]["contractRevision"],
            crate::attempt_receiving::CENTRAL_CONTRACT_REVISION
        );
    }

    #[test]
    fn the_action_names_the_native_action_the_receiving_path_actually_calls() {
        let disclosed = reading();
        let action = &disclosed["actions"][0];
        assert_eq!(action["actionRef"], "attempt-receiving");
        assert_eq!(
            action["contract"],
            crate::attempt_receiving::RECEIVING_ACTION
        );
        assert_eq!(action["nativeAction"], "central.receiving.submit");
        assert_eq!(action["scopeRef"], Value::Null, "no Run, no scope");
        assert_eq!(
            reading_for(Some("project:field-notes"))["actions"][0]["scopeRef"],
            "project:field-notes"
        );
        assert_eq!(action["actorKind"], "human");
        // The grant is named, never carried: no token, principal or hash.
        let text = disclosed.to_string();
        for secret in ["token_sha256", "377a9803", "Control/user/identity"] {
            assert!(!text.contains(secret), "{secret} must not be disclosed");
        }
    }

    #[test]
    fn the_command_reads_the_same_disclosure_and_takes_nothing_else() {
        let text = execute_cli(&["--json".into()]).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), reading());
        assert!(
            execute_cli(&["state.json".into()]).is_err(),
            "no project's state"
        );
        assert!(execute_cli(&["a/.factory/s.json".into(), "b/.factory/s.json".into()]).is_err());
        assert!(execute_cli(&["--unknown".into()]).is_err());
        assert!(execute_cli(&["--help".into()])
            .unwrap()
            .contains("grants nothing"));
    }

    /// A ground with a Central project at `Work/<member>`, and a Factory state in each place a Run can stand.
    fn ground(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "factory-admission-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Control/user")).unwrap();
        let member = root.join("Work/field-notes/ProjectCentral");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(
            member.join("project.json"),
            r#"{"schema":"central.project/v1","project_id":"field-notes"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join("Work/loose")).unwrap();
        root
    }

    #[test]
    fn each_action_names_the_project_the_run_s_state_stands_in() {
        let root = ground("scope");
        let state = |at: &str| root.join(at).join(".factory/development-state.json");
        assert_eq!(
            run_scope(&state("Work/field-notes")).as_deref(),
            Ok("project:field-notes")
        );
        assert_eq!(
            run_scope(&state("")).as_deref(),
            Ok("control:root"),
            "a state at the ground's own root is the root's"
        );
        assert!(run_scope(&state("Work/loose"))
            .unwrap_err()
            .contains("keeps no ProjectCentral manifest"));
        assert!(run_scope(&root.join("Work/field-notes/state.json"))
            .unwrap_err()
            .contains("no Factory state kept in a project's .factory folder"));
        // The command, read for a Run's state: both actions name its project; no other scope is named.
        let text = execute_cli(&[
            state("Work/field-notes").display().to_string(),
            "--json".into(),
        ])
        .unwrap();
        let read: Value = serde_json::from_str(&text).unwrap();
        for action in read["actions"].as_array().unwrap() {
            assert_eq!(action["scopeRef"], "project:field-notes", "{action}");
        }
        assert!(!text.contains("project:Factory"));
        assert!(execute_cli(&[state("Work/loose").display().to_string()]).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_retry_is_admitted_as_factory_s_own_retry_operation_by_the_person() {
        let disclosed = reading_for(Some("project:field-notes"));
        let retry = disclosed["actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["actionRef"] == "attempt-retry")
            .expect("a desktop's retry is admitted");
        assert_eq!(retry["contract"], FACTORY_ATTEMPT_ACTION);
        assert_eq!(retry["nativeAction"], RETRY_NATIVE_ACTION);
        assert_eq!(retry["actorKind"], "human");
        assert_eq!(retry["scopeRef"], "project:field-notes");
        // The operation it names is Factory's own: the attempt action's `retry` tag.
        let tag = serde_json::to_value(
            crate::attempt_runtime::FactoryAttemptOperation::MarkQuiescent {
                attempt_ref: "attempt:x".into(),
            },
        )
        .unwrap();
        assert_eq!(
            tag["operation"], "mark-quiescent",
            "the operation tag is kebab-case"
        );
        let _named = |operation: &crate::attempt_runtime::FactoryAttemptOperation| {
            matches!(
                operation,
                crate::attempt_runtime::FactoryAttemptOperation::Retry { .. }
            )
        };
        assert_eq!(retry["operation"], "retry");
    }
}
