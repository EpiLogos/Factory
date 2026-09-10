use super::*;

fn observation(phase: OwnerOperationPhase) -> OwnerOperationReceipt {
    OwnerOperationReceipt {
        owner_ref: "aikit/session-space".into(),
        contract: "aikit.encounter-delivery/v1".into(),
        operation_ref: "delivery:material-regression".into(),
        receipt_ref: format!("receipt:material-regression:{phase:?}"),
        source_revision: "controlled-test-owner-revision".into(),
        phase,
        evidence_refs: BTreeSet::from(["evidence:material-regression".into()]),
        partial_effect_refs: BTreeSet::new(),
        payload: json!({"testOnly":true}),
    }
}

#[test]
fn exact_material_replay_survives_later_execution_binding() {
    let world = World::new();
    let request = world.request("before:binding", WorkcellWorldOperation::Inspect);
    success(world.invoke(&request));
    success(world.action(FactoryAttemptOperation::BindDispatch {
        attempt_ref: ATTEMPT.into(),
        execution_ref: "execution:bound-after-material-read".into(),
        receipt: observation(OwnerOperationPhase::Submitted),
    }));
    let before = fs::read(world.state()).unwrap();
    assert_eq!(success(world.invoke(&request))["replayed"], true);
    assert_eq!(fs::read(world.state()).unwrap(), before);
    assert_eq!(world.calls(), 1);
    let fresh = world.request("new:wrong-execution", WorkcellWorldOperation::Inspect);
    refused(world.invoke(&fresh), "execution does not belong");
}

#[test]
fn fresh_read_reconciles_the_original_unknown_read_without_repeating_an_effect() {
    let world = World::new();
    world.mode("foreign");
    let original = world.request("read:unknown", WorkcellWorldOperation::Observe);
    assert_eq!(success(world.invoke(&original))["needsReconciliation"], true);
    world.mode("ok");
    let refreshed = success(world.invoke(&world.request("read:fresh", WorkcellWorldOperation::Observe)));
    assert_eq!(refreshed["needsReconciliation"], false);
    let replayed = success(world.invoke(&original));
    assert_eq!(replayed["needsReconciliation"], false);
    assert_eq!(replayed["transportObservation"]["payload"]["detail"]["reconciledBy"], "read:fresh");
    assert_eq!(world.calls(), 2);
    assert!(world.read().attempts[0].observations.iter().any(|receipt| {
        receipt.phase == OwnerOperationPhase::Uncertain
            && receipt.payload.pointer("/detail/ownerReceipt/payload/world_ref")
                .and_then(Value::as_str) == Some("world:foreign")
    }), "original uncertain response must remain in history");
}

#[test]
fn a_healthy_inspection_does_not_claim_that_an_unknown_recovery_had_no_effect() {
    let world = World::new();
    world.mode("lost");
    let original = world.request("recover:uncertain", WorkcellWorldOperation::Recover);
    success(world.invoke(&original));
    world.mode("ok");
    let fresh = success(world.invoke(&world.request("inspect:later", WorkcellWorldOperation::Inspect)));
    assert_eq!(fresh["needsReconciliation"], true);
    assert_eq!(success(world.invoke(&original))["transportObservation"]["phase"], "uncertain");
    assert_eq!(world.calls(), 2);
}

#[test]
fn an_unknown_delivery_after_quiescence_still_blocks_material_release() {
    let world = World::new();
    world.quiesce();
    success(world.action(FactoryAttemptOperation::RecordObservation {
        attempt_ref: ATTEMPT.into(),
        receipt: observation(OwnerOperationPhase::Uncertain),
    }));
    refused(
        world.invoke(&world.request("release:late-unknown", WorkcellWorldOperation::Release)),
        "unknown owner effects",
    );
    assert_eq!(world.calls(), 0);
}

#[test]
fn another_workcell_with_the_same_world_label_is_not_a_shared_material_user() {
    let world = World::new();
    let mut disposition = world.disposition("peer-read");
    disposition.body.workcell_ref = Some("workcell:other".into());
    success(world.action(FactoryAttemptOperation::StartSerial {
        attempt_ref: "attempt:other-workcell".into(),
        task_ref: "task:other-workcell".into(),
        parent_journey_ref: "journey:test".into(),
        workflow_unit_ref: world.workflow.unit("peer-read").unwrap().reference.clone(),
        disposition,
        retry_grant: None,
        tracking: vec![],
    }));
    world.quiesce();
    assert_eq!(success(world.invoke(&world.request("release:scoped", WorkcellWorldOperation::Release)))["needsReconciliation"], false);
}

#[test]
fn recovery_cannot_replace_material_while_a_peer_attempt_is_still_active() {
    let world = World::new();
    success(world.start("peer-read", "attempt:active-peer"));
    refused(
        world.invoke(&world.request("recover:shared", WorkcellWorldOperation::Recover)),
        "another active",
    );
    assert_eq!(world.calls(), 0);
}

#[test]
fn native_transaction_refuses_a_competing_material_intent_and_released_reuse() {
    let world = World::new();
    world.quiesce();
    let receipt = OwnerOperationReceipt {
        owner_ref: "factory".into(),
        contract: "factory.attempt-material-call/v1".into(),
        operation_ref: "factory-material:test-first".into(),
        receipt_ref: "factory-material:test-first-intent".into(),
        source_revision: "factory-state:test".into(),
        phase: OwnerOperationPhase::Dispatching,
        evidence_refs: BTreeSet::new(),
        partial_effect_refs: BTreeSet::new(),
        payload: json!({"worldRef":MATERIAL,"workcellRef":WORKCELL,"operation":"release"}),
    };
    success(world.action(FactoryAttemptOperation::RecordObservation {
        attempt_ref: ATTEMPT.into(),
        receipt: receipt.clone(),
    }));
    let mut other = receipt.clone();
    other.operation_ref = "factory-material:test-competing".into();
    other.receipt_ref = "factory-material:test-competing-intent".into();
    other.payload["operation"] = json!("recover");
    refused(world.action(FactoryAttemptOperation::RecordObservation {
        attempt_ref: ATTEMPT.into(), receipt: other.clone(),
    }), "material lifecycle");
    let mut released = receipt;
    released.phase = OwnerOperationPhase::Released;
    released.receipt_ref = "factory-material:test-released".into();
    released.payload["detail"] = json!({"validated":true});
    success(world.action(FactoryAttemptOperation::RecordObservation {
        attempt_ref: ATTEMPT.into(), receipt: released,
    }));
    refused(world.action(FactoryAttemptOperation::RecordObservation {
        attempt_ref: ATTEMPT.into(), receipt: other,
    }), "material lifecycle");
    assert_eq!(world.calls(), 0);
}

#[test]
fn a_swapped_world_receipt_is_rejected_before_the_first_owner_call() {
    let world = World::new();
    let mut receipt: Value = serde_json::from_slice(&fs::read(world.receipt()).unwrap()).unwrap();
    receipt["world_ref"] = json!("world:unrelated");
    fs::write(world.receipt(), receipt.to_string()).unwrap();
    let before = fs::read(world.state()).unwrap();
    refused(world.invoke(&world.request("inspect:foreign", WorkcellWorldOperation::Inspect)), "not bound");
    assert_eq!(world.calls(), 0);
    assert_eq!(fs::read(world.state()).unwrap(), before);
}

#[test]
fn post_effect_retention_failure_returns_actual_owner_output_not_a_stale_reading() {
    let world = World::new();
    let script = fs::read_to_string(world.owner()).unwrap();
    fs::write(world.owner(), script.replace("value['ok'] = True", "(root/'state.json').write_text('interrupted unrelated publication')\nvalue['ok'] = True")).unwrap();
    let response = success(world.invoke(&world.request("inspect:retention-failed", WorkcellWorldOperation::Inspect)));
    assert_eq!(response["ownerReceipt"]["payload"]["world_ref"], MATERIAL);
    assert_eq!(response["needsReconciliation"], true);
    assert!(response["reading"].is_null());
    assert!(response["retentionError"].is_string());
    assert_eq!(world.calls(), 1);
}
