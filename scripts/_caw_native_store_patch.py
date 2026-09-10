"""Final bounded product edits for this PR; this helper is removed before handover."""
from pathlib import Path
root = Path(__file__).resolve().parents[1]
pending = {}
def once(text, old, new):
    assert text.count(old) == 1, f'Inspected source changed: {old[:100]}'
    return text.replace(old, new, 1)
path = 'factory/src/native_owner.rs'
s = (root / path).read_text()
if 'fn aikit_operation_identity(' not in s:
    s = once(s, '''    let operation_ref = delivery_ref
        .map(|delivery| format!("aikit-encounter:{action}:{session}:{delivery}"))
        .unwrap_or_else(|| format!("aikit-encounter:{action}:{session}"));
    let receipt_ref = delivery_ref
        .map(|delivery| format!("aikit-delivery:{delivery}"))
        .unwrap_or_else(|| stable_receipt_ref("aikit", &payload));''', '''    // Sending and later observing one delivery are the same owner operation.
    // Each changed observation needs its own receipt identity, not a reused
    // delivery ID that would discard the transition from submitted to returned.
    let operation_ref = aikit_operation_identity(action, session, delivery_ref);
    let receipt_ref = stable_receipt_ref("aikit-delivery-observation", &payload);''')
    s = once(s, '''    if let Some(authorization) = authorization {
        command.arg("--authorization").arg(authorization);
    }''', '''    configure_workcell_authorization(&mut command, authorization);''')
    s += r'''

fn aikit_operation_identity(action: &str, session: &str, delivery: Option<&str>) -> String {
    match delivery {
        Some(delivery) => format!("aikit-encounter-delivery:{session}:{delivery}"),
        None => format!("aikit-encounter:{action}:{session}"),
    }
}

fn configure_workcell_authorization(command: &mut Command, authorization: Option<&str>) {
    // WORKCELL_CONTROL_TOKEN is the published native authentication interface.
    // Absence preserves the operator's inherited environment. Neither the token
    // nor its value is copied into an operation receipt or process arguments.
    if let Some(authorization) = authorization {
        command.env("WORKCELL_CONTROL_TOKEN", authorization);
    }
}

#[cfg(test)]
mod native_adapter_regressions {
    use super::*;
    use serde_json::json;

    #[test]
    fn delivery_observations_keep_operation_identity_without_collapsing_receipts() {
        let sent = aikit_operation_identity("send", "session:controlled", Some("delivery:controlled"));
        let observed = aikit_operation_identity("delivery", "session:controlled", Some("delivery:controlled"));
        assert_eq!(sent, observed);
        assert_ne!(sent, aikit_operation_identity("delivery", "session:other", Some("delivery:controlled")));
        let submitted = json!({"delivery":{"delivery_ref":"delivery:controlled","phase":"submitted"}});
        let returned = json!({"delivery":{"delivery_ref":"delivery:controlled","phase":"returned"}});
        assert_ne!(stable_receipt_ref("aikit-delivery-observation", &submitted), stable_receipt_ref("aikit-delivery-observation", &returned));
        assert_eq!(parse_delivery_phase("submitted").unwrap(), OwnerOperationPhase::Submitted);
        assert!(parse_delivery_phase("made-up-success").is_err());
    }

    #[test]
    fn workcell_authorization_uses_environment_not_arguments() {
        let mut command = Command::new("not-executed-test-binary");
        configure_workcell_authorization(&mut command, Some("test-only-not-a-credential"));
        assert_eq!(command.get_args().count(), 0);
        let environment = command.get_envs().collect::<Vec<_>>();
        assert_eq!(environment.len(), 1);
        assert_eq!(environment[0].0, "WORKCELL_CONTROL_TOKEN");
        assert_eq!(environment[0].1.unwrap(), "test-only-not-a-credential");
        let mut inherited = Command::new("not-executed-test-binary");
        configure_workcell_authorization(&mut inherited, None);
        assert_eq!(inherited.get_envs().count(), 0);
    }

    #[test]
    fn unaccepted_owner_revision_is_not_silently_upgraded() {
        let invocation = NativeOwnerInvocation::AikitEncounter {
            binary: PathBuf::from("must-not-run"), cwd: PathBuf::from("/controlled-test"),
            contract_revision: "unverified-new-head".into(),
            request: json!({"action":"delivery","agent_session":"session:controlled","delivery_ref":"delivery:controlled"}),
        };
        assert!(matches!(invoke_native_owner(&invocation), Err(NativeOwnerError::ContractRevisionMismatch { .. })));
    }
}
'''
    pending[path] = s

path = 'factory/src/attempt_application.rs'
s = (root / path).read_text()
old = '            current_attempt(reading, attempt_ref)?;'
new = '''            let record = current_attempt(reading, attempt_ref)?;
            if matches!(operation, MarkQuiescent { .. }) && has_uncertain_operation(record) {
                return Err(invalid("unknown owner effects must be reconciled before quiescence releases the writer"));
            }'''
if 'before quiescence releases the writer' not in s:
    s = once(s, old, new)
    pending[path] = s

path = 'factory/tests/attempt_public_regressions.rs'
s = (root / path).read_text()
if 'uncertain_cancellation_cannot_release_writer_by_quiescing' not in s:
    s += r'''

#[test]
fn uncertain_cancellation_cannot_release_writer_by_quiescing() {
    let world = World::new();
    world.start("cancel-unknown");
    world.bind("cancel-unknown", OwnerOperationPhase::Uncertain);
    world.apply(FactoryAttemptOperation::RequestCancellation { attempt_ref: "cancel-unknown".into() });
    world.apply(FactoryAttemptOperation::AcceptCancellation { attempt_ref: "cancel-unknown".into() });
    world.apply(FactoryAttemptOperation::RecordProcessTermination { attempt_ref: "cancel-unknown".into() });
    assert!(world.refuses(FactoryAttemptOperation::MarkQuiescent { attempt_ref: "cancel-unknown".into() }).contains("unknown owner effects"));
}
'''
    pending[path] = s
for path, content in pending.items():
    (root / path).write_text(content)
    print('Updated', path)
