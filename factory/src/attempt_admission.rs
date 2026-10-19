//! What Factory admits from a desktop that acts on its attempts. A desktop
//! (the O:I shell) calls the attempt Actions as a person, not as an agent: it
//! needs the caller Factory expects and the authority the host's native action
//! contract must give, and until Factory said so the desk could only refuse
//! ("Factory has not disclosed a native desktop caller and authority grant").
//! This is a reading, not a grant: it names the caller identity and the native
//! action, scope and actor kind a grant must carry. Which principal of the
//! owner's `Control/user/native-action-authority.json` carries them is the
//! host's to resolve, with the host's own token, which never travels in JSON.
use crate::attempt_runtime::FACTORY_ATTEMPT_CAPABILITY_REF;
use crate::build::FACTORY_NATIVE_OWNER;
use crate::cli::CliError;
use serde_json::{json, Value};

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
const FACTORY_SCOPE: &str = "project:Factory";

pub fn reading() -> Value {
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
        "actions": [{
            "actionRef": "attempt-receiving",
            "contract": crate::attempt_receiving::RECEIVING_ACTION,
            "command": "factory attempt receiving <state> <request-json|->",
            "nativeAction": RECEIVING_NATIVE_ACTION,
            "scopeRef": FACTORY_SCOPE,
            "actorKind": "human",
            "meaning": "The person chooses an attempt: its retained Return is proposed to Central's receiving field. This is not Recognition, not inclusion of a document and not the Run's close.",
        }],
        "standing": "A reading of what Factory expects of a desktop caller. It grants nothing: the grant is the owner's, in the contract named here.",
    })
}

pub fn execute_cli(args: &[String]) -> Result<String, CliError> {
    if args
        .iter()
        .any(|argument| !matches!(argument.as_str(), "--json" | "help" | "--help" | "-h"))
    {
        return Err("factory attempt admission takes no arguments beyond --json".into());
    }
    if args
        .iter()
        .any(|argument| matches!(argument.as_str(), "help" | "--help" | "-h"))
    {
        return Ok(format!(
            "factory attempt admission [--json]\nReads {ADMISSION_READING}: the desktop caller Factory expects and the native action, scope and actor kind the host's grant must carry. It grants nothing."
        ));
    }
    serde_json::to_string_pretty(&reading()).map_err(CliError::from)
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
        assert_eq!(action["scopeRef"], "project:Factory");
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
        assert!(execute_cli(&["state.json".into()]).is_err());
        assert!(execute_cli(&["--help".into()])
            .unwrap()
            .contains("grants nothing"));
    }
}
