//! Controlled state-machine evidence, not commercial-model or installed-world
//! proof. Every mutation and read below invokes the actual Factory binary in a
//! fresh process. Owner receipts are explicitly marked test-only.

use epilogos_factory::action_projection::{
    FactoryActionCaller, FactoryActionProjectionKind, ProjectedFactoryActionAuthority,
};
use epilogos_factory::attempt_runtime::*;
use epilogos_factory::core::run::{Run, RunRef};
use epilogos_factory::execution_intelligence::{
    accept_aikit_selection, AikitModelRosterSelection, ExecutionDemand, AIKIT_MODEL_ROSTER_VERSION,
};
use epilogos_factory::orchestration::{LegStatus, RetryGrant, ReturnedArtifact};
use epilogos_factory::workflow::{
    compile_workflow, workflow_source_digest, CompiledWorkflow, WorkflowSource,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};

const SOURCE: &str = include_str!("../../contracts/factory/fixtures/agent-workflow-source.json");
const RUN: &str = "run:01ARZ3NDEKTSV4RRFFQ69G5FBD";
const PROJECT: &str = "project:01ARZ3NDEKTSV4RRFFQ69G5FAW";
const FINGERPRINT_B: &str = "git-tree:dddddddddddddddddddddddddddddddddddddddd";
const FINGERPRINT_C: &str = "git-tree:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

fn delivery_plan(world: &World) -> epilogos_factory::attempt_delivery::DeliveryPlan {
    use epilogos_factory::attempt_delivery::DeliveryPlan;
    let unit = world.workflow.unit("inspect-source").unwrap();
    DeliveryPlan {
        intent_ref: "intent:controlled-delivery".into(),
        base_revision: "a".repeat(40),
        candidate_revision: "b".repeat(40),
        candidate_fingerprint: FINGERPRINT_B.into(),
        subject_ref: unit.subject_ref.to_string(),
        subject_revision: unit.basis_revision.clone(),
        verification_plan_ref: "verification-plan:controlled".into(),
        verification_plan_revision: "plan-r1".into(),
        source_basis_refs: set(["source:controlled-r1"]),
        repository: "EpiLogos/Factory".into(),
        branch: "feat/controlled-delivery".into(),
        pull_request: Some(280),
        publisher_ref: "caller:controlled-test".into(),
        expected_remote_head: Some("a".repeat(40)),
        required_checks: set(["factory-owner-check"]),
        cross_owner_verification: None,
        maximum_repairs: 2,
        maximum_environment_retries: 1,
        timeout_ms: 60_000,
        revalidate_repairs: true,
    }
}

fn delivery_request(
    world: &World,
    request_ref: &str,
    operation: epilogos_factory::attempt_delivery::DeliveryOperation,
) -> epilogos_factory::attempt_delivery::DeliveryRequest {
    use epilogos_factory::attempt_delivery::*;
    let basis = world.request(FactoryAttemptOperation::RequestCancellation {
        attempt_ref: "producer".into(),
    });
    DeliveryRequest {
        contract: DELIVERY_ACTION.into(),
        request_ref: request_ref.into(),
        projection_ref: basis.projection_ref,
        caller: basis.caller,
        run_ref: basis.run_ref,
        attempt_ref: "producer".into(),
        expected_revision: basis.expected_revision,
        authority: basis.authority,
        operation,
    }
}

fn delivery_invoke(
    world: &World,
    request: &epilogos_factory::attempt_delivery::DeliveryRequest,
) -> Output {
    command(
        &[
            "attempt",
            "delivery",
            world.state.to_str().unwrap(),
            "-",
            "--json",
        ],
        Some(&serde_json::to_string(request).unwrap()),
    )
}

fn delivery_ci(conclusion: Option<&str>) -> epilogos_factory::attempt_delivery::HostedObservation {
    use epilogos_factory::attempt_delivery::*;
    HostedObservation {
        candidate_revision: "b".repeat(40),
        candidate_fingerprint: None,
        remote_head: "b".repeat(40),
        enumerated: true,
        error: None,
        evidence_refs: set(["evidence:controlled-github-enumeration"]),
        cross_owner: None,
        checks: vec![HostedCheck {
            name: "factory-owner-check".into(),
            head_revision: "b".repeat(40),
            run_id: "controlled-run-1".into(),
            run_attempt: 1,
            status: if conclusion.is_some() {
                "completed"
            } else {
                "in_progress"
            }
            .into(),
            conclusion: conclusion.map(str::to_owned),
            evidence_ref: "evidence:controlled-check-run-1".into(),
        }],
    }
}

fn delivery_world() -> World {
    use epilogos_factory::attempt_delivery::*;
    let world = World::new();
    world.start("producer");
    world.bind("producer", OwnerOperationPhase::Returned);
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "select",
            DeliveryOperation::Select {
                plan: delivery_plan(&world),
            },
        ),
    ));
    world
}

fn delivery_retry_resolution(world: &World, previous: &str) -> ReresolutionRecord {
    let reading = world.reading();
    let retained = reading
        .attempts
        .iter()
        .find(|a| a.attempt_ref == previous)
        .unwrap();
    let mut evidence_refs = retained
        .observations
        .iter()
        .flat_map(|o| o.partial_effect_refs.iter().cloned())
        .collect::<BTreeSet<_>>();
    evidence_refs.extend(retained.failure_evidence_refs.iter().cloned());
    evidence_refs.insert("decision:controlled-retain-intent-review-correction".into());
    ReresolutionRecord { resolution_ref: format!("resolution:controlled-{previous}"), reason: "Retain published effects and current source, repair exact invariant, independently review fresh selected material".into(), source_revision: retained.disposition.participant.source_revision.clone(), evidence_refs, replacement_now_ref: None, replacement_material_ref: None, replacement_harness_ref: None }
}

fn delivery_cross_retry_resolution(
    world: &World,
    previous: &str,
    next: &epilogos_factory::attempt_delivery::DeliveryPlan,
) -> ReresolutionRecord {
    let mut resolution = delivery_retry_resolution(world, previous);
    let reading = world.reading();
    let prior = reading
        .attempts
        .iter()
        .find(|a| a.attempt_ref == previous)
        .unwrap();
    let repair = prior
        .observations
        .iter()
        .rev()
        .find(|r| r.payload["kind"] == "repair")
        .unwrap();
    resolution.evidence_refs.insert(repair.receipt_ref.clone());
    resolution
        .evidence_refs
        .insert(repair.payload["observationRef"].as_str().unwrap().into());
    resolution
        .evidence_refs
        .insert(epilogos_factory::attempt_delivery::cross_owner_material_ref(next).unwrap());
    resolution
}

const CROSS_JOB: &str = "Exact development cut / repertoire, collaboration and delivery";
fn delivery_cross_basis(
    candidate: char,
    run: Option<u64>,
) -> epilogos_factory::attempt_delivery::CrossOwnerVerification {
    use epilogos_factory::attempt_delivery::*;
    CrossOwnerVerification {
        repository: "EpiLogos/O-I".into(),
        workflow_revision: "f".repeat(40),
        run_id: run,
        run_attempt: run.map(|_| 1),
        required_jobs: set([CROSS_JOB]),
        aikit: SourceCommit {
            repository: "EpiLogos/ai-kit".into(),
            revision: "1".repeat(40),
            tree: "2".repeat(40),
        },
        factory: SourceCommit {
            repository: "EpiLogos/Factory".into(),
            revision: candidate.to_string().repeat(40),
            tree: (if candidate == 'c' { "e" } else { "d" }).repeat(40),
        },
        workcell: None,
    }
}

/// This controlled forge exercises the real bounded reader, pagination,
/// source-cut parser and immutable commit checks. It is not GitHub live proof.
#[cfg(unix)]
fn delivery_cross_reader(world: &World, provider: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let reader = world.dir.path().join("controlled-cross-owner-reader");
    let script = r#"#!/usr/bin/python3
import json,sys
v=json.load(open(PROVIDER))
e=sys.argv[-1]; s=v.get('candidate','b'*40); oi=v.get('oi','f'*40)
ai=v.get('aikit_revision','1'*40); ai_tree=v.get('aikit_tree','2'*40)
wc=v.get('workcell_revision','7'*40); wc_tree=v.get('workcell_tree','8'*40)
run=v.get('run',92 if s=='c'*40 else 91); attempt=v.get('attempt',1)
tree=('e' if s=='c'*40 else 'd')*40
job={'id':901,'run_id':run,'head_sha':oi,'name':JOB,'status':v.get('job_status','completed'),'conclusion':v.get('job_conclusion','success')}
cut={'schema':'oi.development-field-cut-execution/v1','oi_revision':oi,'run_id':str(run),'run_attempt':str(attempt),'provider_inference_observed':False,'physical_acceptance':False,'logs':['repertoire.log','handoff.log','team.log','delivery.log'],'owners':{}}
for owner,revision,owner_tree in [('ai-kit',ai,ai_tree),('factory',s,tree)]:
    cut['owners'][owner]={'requested_revision':revision,'observed_revision':revision,'tree':owner_tree,'lock_sha256':'3'*64,'binary_sha256':'4'*64}
if v.get('workcell',False):
    cut['owners']['workcell']={'requested_revision':wc,'observed_revision':wc,'tree':wc_tree,'lock_sha256':'3'*64,'binary_sha256':'4'*64,'binary_version':'workcell 0.1.0 ('+wc+')','binary_stamp_revision':wc,'source_revision':wc}
mode=v.get('mode','valid')
if 'workcell' in cut['owners']:
    w=cut['owners']['workcell']
    if mode=='missing-workcell':del cut['owners']['workcell']
    if mode=='wrong-workcell':w['observed_revision']='9'*40
    if mode=='wrong-workcell-tree':w['tree']='9'*40
    if mode=='missing-workcell-binary':w['binary_sha256']=None
    if mode=='wrong-workcell-lock':w['lock_sha256']=None
    if mode=='wrong-workcell-stamp':w['binary_stamp_revision']='9'*40
    if mode=='wrong-workcell-version':w['binary_version']='workcell 0.1.0 ('+'9'*40+')'
    if mode=='wrong-workcell-source':w['source_revision']='9'*40
    if mode=='missing-workcell-stamp':
        for field in ['binary_stamp_revision','binary_version','source_revision']:del w[field]
    if mode=='unbuilt-workcell':
        w['binary_sha256']=None
        for field in ['binary_stamp_revision','binary_version','source_revision']:del w[field]
if mode=='wrong-aikit':cut['owners']['ai-kit']['observed_revision']='9'*40
if mode=='wrong-factory':cut['owners']['factory']['tree']='9'*40
if mode=='missing-binary':cut['owners']['factory']['binary_sha256']=None
if mode=='missing-binaries':
    cut['owners']['factory']['binary_sha256']=None;cut['owners']['ai-kit']['binary_sha256']=None
if mode=='wrong-lock':cut['owners']['ai-kit']['lock_sha256']=None
if '/actions/jobs/901/logs' in e:
    marker='2026-10-07T12:00:00.000Z OI_DEVELOPMENT_FIELD_CUT_JSON='+json.dumps(cut)
    if mode=='missing-marker': print("2026-10-07T12:00:00.000Z print('OI_DEVELOPMENT_FIELD_CUT_JSON=' + json.dumps(reading))");sys.exit(0)
    if mode=='ambiguous-marker':print(marker)
    print("2026-10-07T12:00:00.000Z print('OI_DEVELOPMENT_FIELD_CUT_JSON=' + json.dumps(reading))")
    if mode=='move-after-log':
        v['head']='c'*40;json.dump(v,open(PROVIDER,'w'))
    print(marker);sys.exit(0)
if e.startswith('repos/EpiLogos/O-I/actions/runs/'):
    if '/jobs?' in e:
        jobs=[] if mode=='missing-job' else [job,job.copy()] if mode=='ambiguous-job' else [job]
        r={'total_count':2 if mode=='incomplete' else len(jobs),'jobs':jobs}
    else:r={'id':run,'head_sha':oi,'run_attempt':attempt,'path':'.github/workflows/other.yml' if mode=='wrong-workflow' else '.github/workflows/cross-product.yml','status':v.get('run_status','completed'),'conclusion':v.get('run_conclusion','success')}
elif e.startswith('repos/EpiLogos/ai-kit/commits/'):
    r={'sha':ai,'commit':{'tree':{'sha':'9'*40 if mode=='immutable-aikit' else ai_tree}}}
elif e.startswith('repos/EpiLogos/Workcell/commits/'):
    r={'sha':wc,'commit':{'tree':{'sha':'9'*40 if mode=='immutable-workcell' else wc_tree}}}
elif '/git/matching-refs/' in e:r=[{'ref':'refs/heads/feat/controlled-delivery','object':{'sha':v.get('head',s)}}]
elif '/pulls/' in e:r={'head':{'ref':'feat/controlled-delivery','sha':s}}
elif '/commits/' in e and '/check-runs?' not in e:r={'sha':s,'commit':{'tree':{'sha':tree}}}
elif '/check-runs?' in e:r={'total_count':1,'check_runs':[{'id':1,'name':'factory-owner-check','head_sha':s,'status':'completed','conclusion':v.get('conclusion','success'),'url':'controlled:check/1'}]}
else:raise RuntimeError(e)
print(json.dumps(r))
"#;
    fs::write(
        &reader,
        script
            .replace(
                "PROVIDER",
                &serde_json::to_string(provider.to_str().unwrap()).unwrap(),
            )
            .replace("JOB", &serde_json::to_string(CROSS_JOB).unwrap()),
    )
    .unwrap();
    fs::set_permissions(&reader, fs::Permissions::from_mode(0o700)).unwrap();
    reader
}

#[cfg(unix)]
#[test]
fn delivery_selected_workcell_requires_exact_immutable_source_and_compiled_stamp() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    let mut selected = delivery_plan(&world);
    let original = delivery_cross_basis('b', Some(91));
    selected.cross_owner_verification = Some(original.clone());
    // Absence preserves the precise old fingerprint and serialized contract.
    let old_source = json!({"repository":original.repository,"workflowRevision":original.workflow_revision,
        "requiredJobs":original.required_jobs,"aikit":original.aikit,"factory":original.factory});
    assert_eq!(
        cross_owner_material_ref(&selected),
        Some(format!(
            "cross-owner-source-fingerprint:{}",
            blake3::hash(&serde_json::to_vec(&old_source).unwrap()).to_hex()
        ))
    );
    assert!(serde_json::to_value(&original)
        .unwrap()
        .get("workcell")
        .is_none());
    let old_fingerprint = cross_owner_material_ref(&selected);
    selected.cross_owner_verification.as_mut().unwrap().workcell = Some(SourceCommit {
        repository: "EpiLogos/Workcell".into(),
        revision: "7".repeat(40),
        tree: "8".repeat(40),
    });
    assert_ne!(cross_owner_material_ref(&selected), old_fingerprint);
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "enrol-material-owner",
            DeliveryOperation::Select {
                plan: selected.clone(),
            },
        ),
    ));
    let provider = world.dir.path().join("workcell-source-reader.json");
    let reader = delivery_cross_reader(&world, &provider);
    for mode in [
        "valid",
        "missing-workcell",
        "wrong-workcell",
        "wrong-workcell-tree",
        "immutable-workcell",
        "missing-workcell-binary",
        "wrong-workcell-lock",
        "missing-workcell-stamp",
        "wrong-workcell-stamp",
        "wrong-workcell-version",
        "wrong-workcell-source",
    ] {
        fs::write(&provider, json!({"mode":mode,"workcell":true}).to_string()).unwrap();
        let reading: Value = serde_json::from_slice(
            &success(delivery_invoke(
                &world,
                &delivery_request(
                    &world,
                    &format!("workcell-proof-{mode}"),
                    DeliveryOperation::ObserveGithub {
                        gh_binary: reader.to_str().unwrap().into(),
                    },
                ),
            ))
            .stdout,
        )
        .unwrap();
        assert_eq!(
            reading["crossOwnerVerification"]["workcell"]["revision"],
            "7".repeat(40)
        );
        if mode == "valid" {
            assert_eq!(reading["crossOwner"]["error"], Value::Null, "{reading}");
            assert_eq!(reading["crossOwner"]["enumerated"], true);
            assert!(reading["crossOwner"]["evidenceRefs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|reference| reference
                    .as_str()
                    .unwrap()
                    .contains("github:EpiLogos/Workcell:commit:")));
            let rendered = human(&readiness(&world.reading(), "producer").unwrap());
            assert!(rendered.contains(&format!(
                "Selected Workcell: EpiLogos/Workcell @ {} (git-tree:{})",
                "7".repeat(40),
                "8".repeat(40)
            )));
        } else {
            assert_eq!(reading["ready"], false, "{mode}: {reading}");
            assert!(
                reading["crossOwner"]["error"].is_string(),
                "{mode}: {reading}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn delivery_workcell_source_change_requires_retained_failure_repair_retry_and_fresh_evidence() {
    use epilogos_factory::attempt_delivery::*;
    for mode in ["no-repair", "stale-fingerprint", "remove-owner", "valid"] {
        let world = delivery_world();
        let mut selected = delivery_plan(&world);
        let mut basis = delivery_cross_basis('b', Some(91));
        basis.workcell = Some(SourceCommit {
            repository: "EpiLogos/Workcell".into(),
            revision: "7".repeat(40),
            tree: "8".repeat(40),
        });
        selected.cross_owner_verification = Some(basis);
        success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "enrol-workcell",
                DeliveryOperation::Select {
                    plan: selected.clone(),
                },
            ),
        ));
        let mut corrected = selected.clone();
        let cross = corrected.cross_owner_verification.as_mut().unwrap();
        cross.workcell.as_mut().unwrap().revision = "5".repeat(40);
        cross.workcell.as_mut().unwrap().tree = "6".repeat(40);
        cross.run_id = Some(92);
        if mode == "remove-owner" {
            cross.workcell = None;
        }
        let before = fs::read(&world.state).unwrap();
        assert!(!delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "ordinary-material-edit",
                DeliveryOperation::Select {
                    plan: corrected.clone()
                }
            )
        )
        .status
        .success());
        assert_eq!(before, fs::read(&world.state).unwrap());
        delivery_verified_return(
            &world,
            "producer",
            "inspect-source",
            &"b".repeat(40),
            FINGERPRINT_B,
        );
        let provider = world.dir.path().join("workcell-repair-reader.json");
        fs::write(
            &provider,
            json!({"workcell":true,"mode":"unbuilt-workcell",
            "job_conclusion":"failure","run_conclusion":"failure"})
            .to_string(),
        )
        .unwrap();
        let reader = delivery_cross_reader(&world, &provider);
        let failed: Value = serde_json::from_slice(
            &success(delivery_invoke(
                &world,
                &delivery_request(
                    &world,
                    "material-build-failure",
                    DeliveryOperation::ObserveGithub {
                        gh_binary: reader.to_str().unwrap().into(),
                    },
                ),
            ))
            .stdout,
        )
        .unwrap();
        assert_eq!(failed["ready"], false);
        assert_eq!(failed["crossOwner"]["error"], Value::Null, "{failed}");
        assert_eq!(failed["crossOwner"]["enumerated"], true);
        if mode != "no-repair" {
            success(delivery_invoke(
                &world,
                &delivery_request(
                    &world,
                    "material-repair",
                    DeliveryOperation::RequestRepair {
                        observation_ref: "delivery:material-build-failure".into(),
                        failure_class: "code".into(),
                        failure_logs: "controlled actual build fault".into(),
                        artifact_refs: set(["artifact:controlled-workcell-build-log"]),
                        prior_decision_refs: set(["decision:retain-material-owner"]),
                    },
                ),
            ));
        }
        let mut retry = world.retry("material-repair-producer");
        if mode != "no-repair" {
            if let FactoryAttemptOperation::Retry { reresolution, .. } = &mut retry {
                let mut resolution =
                    delivery_cross_retry_resolution(&world, "producer", &corrected);
                if mode == "stale-fingerprint" {
                    resolution
                        .evidence_refs
                        .remove(&cross_owner_material_ref(&corrected).unwrap());
                }
                *reresolution = Some(resolution);
            }
        }
        world.apply(retry);
        world.bind("material-repair-producer", OwnerOperationPhase::Returned);
        let before = fs::read(&world.state).unwrap();
        let mut request = delivery_request(
            &world,
            "corrected-material-source",
            DeliveryOperation::Select { plan: corrected },
        );
        request.attempt_ref = "material-repair-producer".into();
        let result = delivery_invoke(&world, &request);
        if mode != "valid" {
            assert!(!result.status.success(), "{mode}");
            assert_eq!(before, fs::read(&world.state).unwrap());
            continue;
        }
        success(result);
        request = delivery_request(
            &world,
            "fresh-material-proof",
            DeliveryOperation::ObserveGithub {
                gh_binary: reader.to_str().unwrap().into(),
            },
        );
        request.attempt_ref = "material-repair-producer".into();
        fs::write(
            &provider,
            json!({"workcell":true,"workcell_revision":"5".repeat(40),
            "workcell_tree":"6".repeat(40),"run":92})
            .to_string(),
        )
        .unwrap();
        let fresh: Value =
            serde_json::from_slice(&success(delivery_invoke(&world, &request)).stdout).unwrap();
        assert_eq!(fresh["crossOwner"]["error"], Value::Null, "{fresh}");
        assert!(
            fresh["failures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|failure| failure.as_str().unwrap().contains("verification")),
            "new source needs new verification: {fresh}"
        );
        delivery_verified_return(
            &world,
            "material-repair-producer",
            "inspect-source",
            &"b".repeat(40),
            FINGERPRINT_B,
        );
    }
}

#[cfg(unix)]
#[test]
fn delivery_cross_owner_proves_exact_executed_cuts_and_refuses_stale_or_ambiguous_evidence() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    let mut selected = delivery_plan(&world);
    selected.cross_owner_verification = Some(delivery_cross_basis('b', Some(91)));
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "enroll-cross",
            DeliveryOperation::Select { plan: selected },
        ),
    ));
    let provider = world.dir.path().join("cross-owner-state.json");
    fs::write(&provider, "{}").unwrap();
    let reader = delivery_cross_reader(&world, &provider);
    let result: Value = serde_json::from_slice(
        &success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "cross-valid",
                DeliveryOperation::ObserveGithub {
                    gh_binary: reader.to_str().unwrap().into(),
                },
            ),
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(result["crossOwner"]["enumerated"], true, "{result}");
    assert_eq!(result["crossOwner"]["error"], Value::Null);
    assert_eq!(
        result["crossOwner"]["checks"][0]["headRevision"],
        "f".repeat(40)
    );
    assert_eq!(result["candidateRevision"], "b".repeat(40));
    assert_eq!(
        result["crossOwner"]["sourceCut"]["owners"]["factory"]["tree"],
        "d".repeat(40)
    );
    let human = String::from_utf8(
        success(command(
            &[
                "attempt",
                "readiness",
                world.state.to_str().unwrap(),
                RUN,
                "producer",
            ],
            None,
        ))
        .stdout,
    )
    .unwrap();
    assert!(human.contains(&format!("Ready: {}", result["ready"].as_bool().unwrap())));
    assert!(human.contains(&format!(
        "EpiLogos/O-I @ {} (run 91, attempt 1)",
        "f".repeat(40)
    )));
    assert!(human.contains(CROSS_JOB));
    for mode in [
        "wrong-aikit",
        "wrong-factory",
        "missing-binary",
        "missing-marker",
        "ambiguous-marker",
        "missing-job",
        "ambiguous-job",
        "incomplete",
        "immutable-aikit",
        "wrong-workflow",
    ] {
        fs::write(&provider, json!({"mode":mode}).to_string()).unwrap();
        let result: Value = serde_json::from_slice(
            &success(delivery_invoke(
                &world,
                &delivery_request(
                    &world,
                    mode,
                    DeliveryOperation::ObserveGithub {
                        gh_binary: reader.to_str().unwrap().into(),
                    },
                ),
            ))
            .stdout,
        )
        .unwrap();
        assert_eq!(result["ready"], false, "{mode}: {result}");
        assert!(
            result["crossOwner"]["error"].is_string(),
            "{mode}: {result}"
        );
    }
    fs::write(&provider, json!({"mode":"move-after-log"}).to_string()).unwrap();
    let moved: Value = serde_json::from_slice(
        &success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "cross-log-aperture",
                DeliveryOperation::ObserveGithub {
                    gh_binary: reader.to_str().unwrap().into(),
                },
            ),
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(moved["ready"], false, "{moved}");
    assert!(
        moved["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str().unwrap().contains("remote moved while reading")),
        "{moved}"
    );
    for value in [
        json!({"attempt":2}),
        json!({"run_status":"in_progress"}),
        json!({"job_status":"in_progress","job_conclusion":null}),
    ] {
        fs::write(&provider, value.to_string()).unwrap();
        let result: Value = serde_json::from_slice(
            &success(command(
                &[
                    "attempt",
                    "readiness",
                    world.state.to_str().unwrap(),
                    RUN,
                    "producer",
                    "--json",
                ],
                None,
            ))
            .stdout,
        )
        .unwrap();
        assert_eq!(result["ready"], false, "{result}");
        assert!(
            result["failures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f.as_str().unwrap().contains("cross-owner")),
            "{result}"
        );
    }
}

#[cfg(unix)]
#[test]
fn delivery_failed_cross_owner_build_retains_source_proof_without_claiming_ready() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    let mut selected = delivery_plan(&world);
    selected.cross_owner_verification = Some(delivery_cross_basis('b', Some(91)));
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "enrol-negative-source",
            DeliveryOperation::Select { plan: selected },
        ),
    ));
    let provider = world.dir.path().join("negative-source-reader.json");
    let reader = delivery_cross_reader(&world, &provider);
    for (i, shape) in [
        json!({"mode":"wrong-aikit"}),
        json!({"mode":"wrong-factory"}),
        json!({"mode":"wrong-lock"}),
        json!({"mode":"missing-marker"}),
        json!({"mode":"ambiguous-marker"}),
        json!({"mode":"incomplete"}),
        json!({"attempt":2}),
        json!({"job_status":"in_progress","job_conclusion":null}),
        json!({"run_status":"in_progress"}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut shape = shape;
        shape["job_conclusion"] = shape
            .get("job_conclusion")
            .cloned()
            .unwrap_or(json!("failure"));
        shape["run_conclusion"] = json!("failure");
        fs::write(&provider, shape.to_string()).unwrap();
        let name = format!("invalid-negative-{i}");
        let result: Value = serde_json::from_slice(
            &success(delivery_invoke(
                &world,
                &delivery_request(
                    &world,
                    &name,
                    DeliveryOperation::ObserveGithub {
                        gh_binary: reader.to_str().unwrap().into(),
                    },
                ),
            ))
            .stdout,
        )
        .unwrap();
        assert_eq!(result["ready"], false, "{result}");
        let before = fs::read(&world.state).unwrap();
        let refused = delivery_invoke(
            &world,
            &delivery_request(
                &world,
                &format!("invalid-repair-{i}"),
                DeliveryOperation::RequestRepair {
                    observation_ref: format!("delivery:{name}"),
                    failure_class: "code".into(),
                    failure_logs: "controlled source-bound failed build".into(),
                    artifact_refs: set(["artifact:controlled-log"]),
                    prior_decision_refs: set(["decision:retain-source"]),
                },
            ),
        );
        assert!(!refused.status.success(), "{result}");
        assert_eq!(fs::read(&world.state).unwrap(), before);
    }
    fs::write(
        &provider,
        json!({"mode":"missing-binaries","job_conclusion":"failure","run_conclusion":"failure"})
            .to_string(),
    )
    .unwrap();
    let observed: Value = serde_json::from_slice(
        &success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "settled-negative-source",
                DeliveryOperation::ObserveGithub {
                    gh_binary: reader.to_str().unwrap().into(),
                },
            ),
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(observed["ready"], false, "{observed}");
    assert_eq!(observed["crossOwner"]["enumerated"], true, "{observed}");
    assert_eq!(observed["crossOwner"]["error"], Value::Null, "{observed}");
    assert_eq!(
        observed["crossOwner"]["sourceCut"]["owners"]["ai-kit"]["binary_sha256"],
        Value::Null
    );
    assert_eq!(
        observed["crossOwner"]["sourceCut"]["owners"]["factory"]["binary_sha256"],
        Value::Null
    );
    success(delivery_invoke(&world, &delivery_request(&world, "admit-source-repair", DeliveryOperation::RequestRepair {
        observation_ref: "delivery:settled-negative-source".into(), failure_class: "code".into(),
        failure_logs: "controlled real compile fault before binaries; immutable owner sources retained".into(),
        artifact_refs: set(["artifact:controlled-compile-log"]), prior_decision_refs: set(["decision:retain-source"]) })));
    let restarted = world.reading();
    assert!(restarted
        .attempts
        .iter()
        .find(|a| a.attempt_ref == "producer")
        .unwrap()
        .observations
        .iter()
        .any(|r| r.receipt_ref == "delivery:admit-source-repair"));
}

#[cfg(unix)]
#[test]
fn delivery_cross_source_renewal_requires_actual_retry_selected_repair_and_full_material_evidence()
{
    use epilogos_factory::attempt_delivery::*;
    for mode in ["no-repair", "no-resolution", "stale-fingerprint", "valid"] {
        let world = delivery_world();
        let mut selected = delivery_plan(&world);
        selected.cross_owner_verification = Some(delivery_cross_basis('b', Some(91)));
        success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "enrol-cross",
                DeliveryOperation::Select {
                    plan: selected.clone(),
                },
            ),
        ));
        delivery_verified_return(
            &world,
            "producer",
            "inspect-source",
            &"b".repeat(40),
            FINGERPRINT_B,
        );
        let provider = world.dir.path().join("source-renewal-reader.json");
        fs::write(&provider, json!({"mode":"missing-binaries","job_conclusion":"failure","run_conclusion":"failure"}).to_string()).unwrap();
        let reader = delivery_cross_reader(&world, &provider);
        success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "failed-source",
                DeliveryOperation::ObserveGithub {
                    gh_binary: reader.to_str().unwrap().into(),
                },
            ),
        ));
        if mode != "no-repair" {
            success(delivery_invoke(
                &world,
                &delivery_request(
                    &world,
                    "source-repair",
                    DeliveryOperation::RequestRepair {
                        observation_ref: "delivery:failed-source".into(),
                        failure_class: "code".into(),
                        failure_logs: "controlled compile invariant".into(),
                        artifact_refs: set(["artifact:controlled-compile-log"]),
                        prior_decision_refs: set(["decision:preserve-intent"]),
                    },
                ),
            ));
        }
        let cross = selected.cross_owner_verification.as_mut().unwrap();
        cross.aikit.revision = "5".repeat(40);
        cross.aikit.tree = "6".repeat(40);
        cross.workflow_revision = "8".repeat(40);
        cross.run_id = Some(92);
        let mut retry = world.retry("repair-producer");
        if mode != "no-repair" && mode != "no-resolution" {
            if let FactoryAttemptOperation::Retry { reresolution, .. } = &mut retry {
                let mut resolution = delivery_cross_retry_resolution(&world, "producer", &selected);
                if mode == "stale-fingerprint" {
                    resolution
                        .evidence_refs
                        .remove(&cross_owner_material_ref(&selected).unwrap());
                }
                *reresolution = Some(resolution);
            }
        }
        world.apply(retry);
        world.bind("repair-producer", OwnerOperationPhase::Returned);
        let before = fs::read(&world.state).unwrap();
        let mut request = delivery_request(
            &world,
            "select-successor",
            DeliveryOperation::Select {
                plan: selected.clone(),
            },
        );
        request.attempt_ref = "repair-producer".into();
        let result = delivery_invoke(&world, &request);
        if mode != "valid" {
            assert!(!result.status.success(), "{mode}");
            assert_eq!(before, fs::read(&world.state).unwrap(), "{mode}");
            continue;
        }
        success(result);
        // A passing code verification for the same Factory tree, but the old
        // cross-source basis, cannot produce a Return after the AIKit correction.
        let stale_ref =
            cross_owner_material_ref(&delivery_plan_with_original_cross(&world)).unwrap();
        world.apply(FactoryAttemptOperation::RecordVerification {
            attempt_ref: "repair-producer".into(),
            verification: VerificationReceipt {
                verification_ref: "verification:stale-cross".into(),
                owner_ref: "factory-verifier:controlled".into(),
                source_revision: "b".repeat(40),
                outcome: VerificationOutcome::Passed,
                obligations: world
                    .workflow
                    .unit("inspect-source")
                    .unwrap()
                    .verification_obligations
                    .clone(),
                evidence_refs: BTreeSet::from([
                    format!("candidate-fingerprint:{FINGERPRINT_B}"),
                    "verification-plan:verification-plan:controlled@plan-r1".into(),
                    stale_ref,
                ]),
            },
        });
        let rejected = world.refuses(world.returned("repair-producer"));
        assert!(
            rejected.contains("verification") || rejected.contains("selected"),
            "{rejected}"
        );
        delivery_verified_return(
            &world,
            "repair-producer",
            "inspect-source",
            &"b".repeat(40),
            FINGERPRINT_B,
        );
        let before = world.reading();
        assert_eq!(
            before
                .attempts
                .iter()
                .find(|a| a.attempt_ref == "repair-producer")
                .unwrap()
                .reresolutions
                .len(),
            1
        );
    }
}
fn delivery_plan_with_original_cross(
    world: &World,
) -> epilogos_factory::attempt_delivery::DeliveryPlan {
    let mut plan = delivery_plan(world);
    plan.cross_owner_verification = Some(delivery_cross_basis('b', Some(91)));
    plan
}

#[cfg(unix)]
#[test]
fn delivery_environment_retry_renews_only_exact_same_sources_with_retained_failure_and_resolution()
{
    use epilogos_factory::attempt_delivery::*;
    for mode in [
        "no-repair",
        "no-resolution",
        "stale-fingerprint",
        "edited-source",
        "pending",
        "missing",
        "new-run",
        "new-attempt",
    ] {
        let world = delivery_world();
        let mut selected = delivery_plan(&world);
        selected.cross_owner_verification = Some(delivery_cross_basis('b', Some(91)));
        success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "enrol-environment-cross",
                DeliveryOperation::Select {
                    plan: selected.clone(),
                },
            ),
        ));
        delivery_verified_return(
            &world,
            "producer",
            "inspect-source",
            &"b".repeat(40),
            FINGERPRINT_B,
        );
        let mut renewed = selected.clone();
        let next = renewed.cross_owner_verification.as_mut().unwrap();
        if mode == "new-attempt" {
            next.run_attempt = 2;
        } else {
            next.run_id = Some(92);
        }
        if mode == "edited-source" {
            next.aikit.revision = "5".repeat(40);
            next.aikit.tree = "6".repeat(40);
        }
        let before = fs::read(&world.state).unwrap();
        assert!(
            !delivery_invoke(
                &world,
                &delivery_request(
                    &world,
                    "ordinary-run-rebinding",
                    DeliveryOperation::Select {
                        plan: renewed.clone()
                    }
                )
            )
            .status
            .success(),
            "{mode}"
        );
        assert_eq!(before, fs::read(&world.state).unwrap(), "{mode}");
        let provider = world.dir.path().join("environment-renewal-reader.json");
        let mut failed = json!({"job_conclusion":"failure", "run_conclusion":"failure"});
        if mode == "pending" {
            failed["job_status"] = json!("in_progress");
            failed["run_status"] = json!("in_progress");
        }
        if mode == "missing" {
            failed["mode"] = json!("missing-job");
        }
        fs::write(&provider, failed.to_string()).unwrap();
        let reader = delivery_cross_reader(&world, &provider);
        success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "failed-environment-cross",
                DeliveryOperation::ObserveGithub {
                    gh_binary: reader.to_str().unwrap().into(),
                },
            ),
        ));
        if mode != "no-repair" {
            let before = fs::read(&world.state).unwrap();
            let result = delivery_invoke(&world, &delivery_request(&world, "admit-environment-retry",
                DeliveryOperation::RequestRepair {
                    observation_ref: "delivery:failed-environment-cross".into(),
                    failure_class: "environment".into(), failure_logs: "controlled actual runner environment failure; source material unchanged".into(),
                    artifact_refs: set(["artifact:controlled-environment-log"]),
                    prior_decision_refs: set(["decision:retain-exact-sources"]),
                }));
            if mode == "pending" || mode == "missing" {
                assert!(!result.status.success(), "{mode}");
                assert_eq!(before, fs::read(&world.state).unwrap(), "{mode}");
                continue;
            }
            success(result);
        }
        let mut retry = world.retry("environment-retry-producer");
        if mode != "no-repair" && mode != "no-resolution" {
            if let FactoryAttemptOperation::Retry { reresolution, .. } = &mut retry {
                let mut resolution = delivery_cross_retry_resolution(&world, "producer", &renewed);
                if mode == "stale-fingerprint" {
                    resolution
                        .evidence_refs
                        .remove(&cross_owner_material_ref(&renewed).unwrap());
                }
                *reresolution = Some(resolution);
            }
        }
        world.apply(retry);
        world.bind("environment-retry-producer", OwnerOperationPhase::Returned);
        let before = fs::read(&world.state).unwrap();
        let mut request = delivery_request(
            &world,
            "select-environment-rerun",
            DeliveryOperation::Select {
                plan: renewed.clone(),
            },
        );
        request.attempt_ref = "environment-retry-producer".into();
        let result = delivery_invoke(&world, &request);
        if mode != "new-run" && mode != "new-attempt" {
            assert!(!result.status.success(), "{mode}");
            assert_eq!(before, fs::read(&world.state).unwrap(), "{mode}");
            continue;
        }
        success(result);
        assert_eq!(
            cross_owner_material_ref(&selected),
            cross_owner_material_ref(&renewed)
        );
        let before = fs::read(&world.state).unwrap();
        let mut repeated_renewal = renewed.clone();
        repeated_renewal
            .cross_owner_verification
            .as_mut()
            .unwrap()
            .run_id = Some(93);
        let mut extra = delivery_request(
            &world,
            "repeat-environment-renewal",
            DeliveryOperation::Select {
                plan: repeated_renewal,
            },
        );
        extra.attempt_ref = "environment-retry-producer".into();
        assert!(!delivery_invoke(&world, &extra).status.success());
        assert_eq!(before, fs::read(&world.state).unwrap());
        fs::write(
            &provider,
            json!({"run": if mode == "new-run" { 92 } else { 91 },
            "attempt": if mode == "new-attempt" { 2 } else { 1 }})
            .to_string(),
        )
        .unwrap();
        request = delivery_request(
            &world,
            "fresh-environment-rerun",
            DeliveryOperation::ObserveGithub {
                gh_binary: reader.to_str().unwrap().into(),
            },
        );
        request.attempt_ref = "environment-retry-producer".into();
        let fresh: Value =
            serde_json::from_slice(&success(delivery_invoke(&world, &request)).stdout).unwrap();
        assert_eq!(fresh["crossOwner"]["error"], Value::Null, "{fresh}");
        assert_eq!(fresh["crossOwner"]["enumerated"], true);
        assert_eq!(fresh["ready"], false);
        assert!(
            fresh["failures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|failure| failure.as_str().unwrap().contains("verification")),
            "fresh retry requires its own verification: {fresh}"
        );
        delivery_verified_return(
            &world,
            "environment-retry-producer",
            "inspect-source",
            &"b".repeat(40),
            FINGERPRINT_B,
        );
        let reading = readiness(&world.reading(), "environment-retry-producer").unwrap();
        assert!(
            !reading.ready,
            "new attempt still requires distinct selected reviewer"
        );
        assert_eq!(reading.environment_retries, 1);
        assert_eq!(reading.repairs, 0);
    }
}

#[test]
fn delivery_cross_owner_binding_is_one_way_and_repair_requires_new_run_proof() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    let mut selected = delivery_plan(&world);
    selected.cross_owner_verification = Some(delivery_cross_basis('b', None));
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "cross-unbound",
            DeliveryOperation::Select {
                plan: selected.clone(),
            },
        ),
    ));
    selected.cross_owner_verification = Some(delivery_cross_basis('b', Some(91)));
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "cross-bound",
            DeliveryOperation::Select {
                plan: selected.clone(),
            },
        ),
    ));
    let before = fs::read(&world.state).unwrap();
    let mut changes = vec![
        selected.clone(),
        selected.clone(),
        selected.clone(),
        selected.clone(),
    ];
    changes[0].cross_owner_verification = None;
    changes[1].cross_owner_verification.as_mut().unwrap().run_id = Some(92);
    changes[2]
        .cross_owner_verification
        .as_mut()
        .unwrap()
        .aikit
        .tree = "9".repeat(40);
    changes[3].candidate_revision = "c".repeat(40);
    changes[3].candidate_fingerprint = FINGERPRINT_C.into();
    changes[3].cross_owner_verification = Some(delivery_cross_basis('c', Some(91)));
    for (i, plan) in changes.into_iter().enumerate() {
        assert!(!delivery_invoke(
            &world,
            &delivery_request(
                &world,
                &format!("cross-change-{i}"),
                DeliveryOperation::Select { plan }
            )
        )
        .status
        .success());
        assert_eq!(before, fs::read(&world.state).unwrap());
    }
    selected.candidate_revision = "c".repeat(40);
    selected.candidate_fingerprint = FINGERPRINT_C.into();
    selected.cross_owner_verification = Some(delivery_cross_basis('c', Some(92)));
    assert!(!delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "cross-correction",
            DeliveryOperation::Select { plan: selected },
        ),
    )
    .status
    .success());
}

#[test]
fn delivery_failed_ci_selects_bounded_repair_without_duplicate_execution_on_restart() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "ci-failure",
            DeliveryOperation::ObserveControlled {
                observation: delivery_ci(Some("failure")),
            },
        ),
    ));
    let operation = DeliveryOperation::RequestRepair {
        observation_ref: "delivery:ci-failure".into(),
        failure_class: "code".into(),
        failure_logs: "actual controlled invariant failed".into(),
        artifact_refs: set(["artifact:controlled-failure-log"]),
        prior_decision_refs: set(["decision:controlled-intent"]),
    };
    let mut oversized = operation.clone();
    if let DeliveryOperation::RequestRepair { failure_logs, .. } = &mut oversized {
        *failure_logs = "x".repeat(32 * 1024 + 1);
    }
    let before_invalid = fs::read(&world.state).unwrap();
    let refused = delivery_invoke(
        &world,
        &delivery_request(&world, "oversized-repair", oversized),
    );
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("bounded failure logs"));
    assert_eq!(before_invalid, fs::read(&world.state).unwrap());
    let request = delivery_request(&world, "repair", operation.clone());
    let first = success(delivery_invoke(&world, &request));
    let before = fs::read(&world.state).unwrap();
    let paused = world.reading();
    let retained = paused
        .attempts
        .iter()
        .find(|a| a.attempt_ref == "producer")
        .unwrap()
        .observations
        .iter()
        .find(|r| r.receipt_ref == "delivery:repair")
        .unwrap();
    assert_eq!(
        retained.payload["failureLogs"],
        "actual controlled invariant failed"
    );
    assert_eq!(retained.payload["observationRef"], "delivery:ci-failure");
    assert!(retained
        .evidence_refs
        .contains("artifact:controlled-failure-log"));
    let replay = success(delivery_invoke(&world, &request));
    assert_eq!(before, fs::read(&world.state).unwrap());
    assert_eq!(
        serde_json::from_slice::<Value>(&first.stdout).unwrap()["repairs"],
        1
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&replay.stdout).unwrap()["repairs"],
        1
    );
    let duplicate = delivery_invoke(&world, &delivery_request(&world, "repair-again", operation));
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("already has a retained repair"));
}

#[test]
fn delivery_initial_publication_can_precede_pr_and_exact_pr_binding_is_one_way() {
    use epilogos_factory::attempt_delivery::*;
    let world = World::new();
    world.start("producer");
    world.bind("producer", OwnerOperationPhase::Returned);
    let mut plan = delivery_plan(&world);
    plan.pull_request = None;
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "select-before-pr",
            DeliveryOperation::Select { plan: plan.clone() },
        ),
    ));
    let result: Value = serde_json::from_slice(
        &success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "observe-before-pr",
                DeliveryOperation::ObserveGithub {
                    gh_binary: "/missing/controlled-reader".into(),
                },
            ),
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(result["ready"], false);
    assert!(result["failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str().unwrap().contains("select exact PR")));
    plan.pull_request = Some(281);
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "bind-exact-pr",
            DeliveryOperation::Select { plan: plan.clone() },
        ),
    ));
    plan.pull_request = Some(282);
    assert!(!delivery_invoke(
        &world,
        &delivery_request(&world, "replace-pr", DeliveryOperation::Select { plan })
    )
    .status
    .success());
}

#[test]
fn delivery_pending_missing_stale_and_incomplete_checks_never_establish_readiness() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    for (index, mut ci) in [
        delivery_ci(None),
        delivery_ci(Some("success")),
        delivery_ci(Some("success")),
        delivery_ci(Some("success")),
    ]
    .into_iter()
    .enumerate()
    {
        match index {
            1 => ci.checks.clear(),
            2 => ci.remote_head = "a".repeat(40),
            3 => ci.enumerated = false,
            _ => {}
        }
        let output = success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                &format!("ci-{index}"),
                DeliveryOperation::ObserveControlled { observation: ci },
            ),
        ));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["ready"], false);
        let failures = value["failures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(failures.contains(
            [
                "required check unresolved",
                "required check missing",
                "differs from selected candidate",
                "enumeration incomplete"
            ][index]
        ));
    }
}

#[test]
fn delivery_controlled_green_is_not_live_provider_evidence_and_human_json_agree() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "ci-green",
            DeliveryOperation::ObserveControlled {
                observation: delivery_ci(Some("success")),
            },
        ),
    ));
    let args = [
        "attempt",
        "readiness",
        world.state.to_str().unwrap(),
        RUN,
        "producer",
    ];
    let human = success(command(&args, None));
    let mut json_args = args.to_vec();
    json_args.push("--json");
    let structured: Value =
        serde_json::from_slice(&success(command(&json_args, None)).stdout).unwrap();
    assert_eq!(structured["ready"], false);
    assert!(structured["failures"].as_array().unwrap().iter().any(|v| v
        .as_str()
        .unwrap()
        .contains("actual GitHub read remains required")));
    assert!(String::from_utf8_lossy(&human.stdout).contains("Ready: false"));
    assert!(String::from_utf8_lossy(&human.stdout).contains(&"b".repeat(40)));
}

#[test]
fn delivery_plan_repair_cannot_widen_authority_or_replenish_retry_budget() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    let mut changed = delivery_plan(&world);
    changed.maximum_repairs = 3;
    let before = fs::read(&world.state).unwrap();
    let output = delivery_invoke(
        &world,
        &delivery_request(&world, "widen", DeliveryOperation::Select { plan: changed }),
    );
    assert!(!output.status.success());
    assert_eq!(before, fs::read(&world.state).unwrap());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot widen"));
}

#[test]
fn delivery_enrolled_candidate_refuses_unbound_verification_and_foreign_return() {
    let world = delivery_world();
    world.verify("producer", VerificationOutcome::Passed);
    assert!(world
        .refuses(world.returned("producer"))
        .contains("later failed/unknown verification"));
    let plan = delivery_plan(&world);
    world.apply(FactoryAttemptOperation::RecordVerification {
        attempt_ref: "producer".into(),
        verification: VerificationReceipt {
            verification_ref: "verification:exact-controlled-candidate".into(),
            owner_ref: "factory-verifier:controlled".into(),
            source_revision: plan.candidate_revision,
            outcome: VerificationOutcome::Passed,
            obligations: world
                .workflow
                .unit("inspect-source")
                .unwrap()
                .verification_obligations
                .clone(),
            evidence_refs: set([
                "candidate-fingerprint:git-tree:dddddddddddddddddddddddddddddddddddddddd",
                "verification-plan:verification-plan:controlled@plan-r1",
            ]),
        },
    });
    assert!(world
        .refuses(world.returned("producer"))
        .contains("Return differs from enrolled exact candidate"));
    let mut returned = world.returned("producer");
    if let FactoryAttemptOperation::ReturnArtifact {
        artifact,
        readable_return,
        ..
    } = &mut returned
    {
        artifact.evidence_refs.insert(
            "candidate-fingerprint:git-tree:dddddddddddddddddddddddddddddddddddddddd".into(),
        );
        readable_return.evidence_refs.insert(
            "candidate-fingerprint:git-tree:dddddddddddddddddddddddddddddddddddddddd".into(),
        );
    }
    world.apply(returned);
}

#[test]
fn delivery_lower_level_action_cannot_bypass_independent_review_or_change_material() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    let receipt = OwnerOperationReceipt {
        owner_ref: "factory/delivery".into(),
        contract: DELIVERY_OBSERVATION.into(),
        operation_ref: "delivery:unsafe-push".into(),
        receipt_ref: "delivery:unsafe-push".into(),
        source_revision: "b".repeat(40),
        phase: OwnerOperationPhase::Dispatching,
        evidence_refs: set(["evidence:controlled"]),
        partial_effect_refs: BTreeSet::new(),
        payload: json!({"kind":"publication-intent","reviewerAttemptRef":"producer"}),
    };
    assert!(world
        .refuses(FactoryAttemptOperation::RecordObservation {
            attempt_ref: "producer".into(),
            receipt
        })
        .contains("current returned attempt"));
    let mut stale = delivery_plan(&world);
    stale.subject_revision = "foreign-head".into();
    assert!(!delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "stale-selection",
            DeliveryOperation::Select { plan: stale }
        )
    )
    .status
    .success());
}

#[test]
fn delivery_candidate_change_invalidates_native_return_and_requires_new_verification() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    delivery_verified_return(
        &world,
        "producer",
        "inspect-source",
        &"b".repeat(40),
        FINGERPRINT_B,
    );
    let mut changed = delivery_plan(&world);
    changed.candidate_revision = "c".repeat(40);
    changed.candidate_fingerprint = FINGERPRINT_C.into();
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "changed-candidate",
            DeliveryOperation::Select { plan: changed },
        ),
    ));
    let reading = world.reading();
    let unit = &world.workflow.unit("inspect-source").unwrap().reference;
    assert_eq!(reading.legs[unit].status, LegStatus::Failed);
    assert!(!reading.current_returned_units.contains(unit));
    let delivery = epilogos_factory::attempt_delivery::readiness(&reading, "producer").unwrap();
    assert!(!delivery.ready);
    assert!(delivery
        .failures
        .iter()
        .any(|f| f.contains("verification missing or stale")));
}

#[test]
fn delivery_lower_level_repair_cannot_forge_failed_run_identity() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "exact-failure",
            DeliveryOperation::ObserveControlled {
                observation: delivery_ci(Some("failure")),
            },
        ),
    ));
    let output=world.refuses(FactoryAttemptOperation::RecordObservation {
        attempt_ref:"producer".into(),
        receipt:OwnerOperationReceipt {
            owner_ref:"factory/delivery".into(), contract:DELIVERY_OBSERVATION.into(),
            operation_ref:"delivery:forged-repair".into(),receipt_ref:"delivery:forged-repair".into(),source_revision:"b".repeat(40),
            phase:OwnerOperationPhase::Observed,evidence_refs:set(["artifact:bounded-log"]),partial_effect_refs:BTreeSet::new(),
            payload:json!({"kind":"repair","observationRef":"delivery:exact-failure","failureClass":"code","failureLogs":"bounded failed invariant","failureIdentities":[]})
        }
    });
    assert!(
        output.contains("identities must match exact retained"),
        "{output}"
    );
}

fn delivery_verified_return(
    world: &World,
    reference: &str,
    key: &str,
    candidate: &str,
    fingerprint: &str,
) {
    let unit = world.workflow.unit(key).unwrap();
    let mut evidence = BTreeSet::from([
        format!("candidate-fingerprint:{fingerprint}"),
        "verification-plan:verification-plan:controlled@plan-r1".into(),
    ]);
    let reading = world.reading();
    let producer = &reading.legs[&world.workflow.unit("inspect-source").unwrap().reference];
    if let Some(selected) = reading
        .attempts
        .iter()
        .find(|a| a.execution_ref.as_deref() == Some(producer.execution_ref.as_str()))
        .and_then(|a| {
            a.observations
                .iter()
                .rev()
                .find(|o| o.payload["kind"] == "selection")
        })
    {
        let plan: epilogos_factory::attempt_delivery::DeliveryPlan =
            serde_json::from_value(selected.payload["plan"].clone()).unwrap();
        if let Some(reference) = epilogos_factory::attempt_delivery::cross_owner_material_ref(&plan)
        {
            evidence.insert(reference);
        }
    }
    world.apply(FactoryAttemptOperation::RecordVerification {
        attempt_ref: reference.into(),
        verification: VerificationReceipt {
            verification_ref: format!("verification:{reference}:{}", world.reading().revision),
            owner_ref: "factory-verifier:controlled".into(),
            source_revision: candidate.into(),
            outcome: VerificationOutcome::Passed,
            obligations: unit.verification_obligations.clone(),
            evidence_refs: evidence.clone(),
        },
    });
    world.apply(FactoryAttemptOperation::ReturnArtifact {
        attempt_ref: reference.into(),
        artifact: ReturnedArtifact {
            artifact_ref: format!("artifact:{reference}"),
            subject_ref: unit.subject_ref.to_string(),
            subject_revision: unit.basis_revision.clone(),
            producing_execution_ref: format!("execution:{reference}"),
            evidence_refs: evidence.clone(),
            semantic_difference: "controlled native exact candidate return".into(),
        },
        readable_return: ReadableReturn {
            return_ref: format!("return:{reference}"),
            summary: "controlled backend evidence; no real provider claim".into(),
            artifact_refs: BTreeSet::from([format!("artifact:{reference}")]),
            evidence_refs: evidence,
            receiving_ref: None,
            receiving_source_revision: None,
            archive_refs: BTreeSet::new(),
            regression_observation_refs: BTreeSet::new(),
        },
    });
}

#[test]
fn delivery_hosted_failure_reopens_native_attempt_and_retry_keeps_exact_plan() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    delivery_verified_return(
        &world,
        "producer",
        "inspect-source",
        &"b".repeat(40),
        FINGERPRINT_B,
    );
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "ci-failed-after-return",
            DeliveryOperation::ObserveControlled {
                observation: delivery_ci(Some("failure")),
            },
        ),
    ));
    assert_eq!(
        world.reading().legs[&world.workflow.unit("inspect-source").unwrap().reference].status,
        LegStatus::Failed
    );
    world.apply(world.retry("repair-producer"));
    world.bind("repair-producer", OwnerOperationPhase::Returned);
    assert!(world
        .refuses(world.returned("repair-producer"))
        .contains("later failed/unknown verification"));
    let inherited = world
        .reading()
        .attempts
        .into_iter()
        .find(|r| r.attempt_ref == "repair-producer")
        .unwrap();
    assert!(inherited
        .observations
        .iter()
        .any(|r| r.payload["kind"] == "selection"));
    assert!(world
        .reading()
        .attempts
        .iter()
        .find(|r| r.attempt_ref == "producer")
        .unwrap()
        .failure_evidence_refs
        .contains("delivery:ci-failed-after-return"));
}

#[test]
fn delivery_polling_same_failed_run_cannot_duplicate_repair() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    for (poll, repair) in [
        ("ci-failure-first", "first-repair"),
        ("ci-failure-again", "second-repair"),
    ] {
        success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                poll,
                DeliveryOperation::ObserveControlled {
                    observation: delivery_ci(Some("failure")),
                },
            ),
        ));
        let request = delivery_request(
            &world,
            repair,
            DeliveryOperation::RequestRepair {
                observation_ref: format!("delivery:{poll}"),
                failure_class: "code".into(),
                failure_logs: "controlled failed invariant".into(),
                artifact_refs: set(["artifact:failure-log"]),
                prior_decision_refs: BTreeSet::new(),
            },
        );
        let output = delivery_invoke(&world, &request);
        if repair == "first-repair" {
            success(output);
        } else {
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("same hosted run/attempt"));
        }
    }
}

#[test]
fn delivery_unavailable_hosted_reader_retains_exact_open_live_obligation() {
    use epilogos_factory::attempt_delivery::*;
    let world = delivery_world();
    let result: Value = serde_json::from_slice(
        &success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "missing-github-reader",
                DeliveryOperation::ObserveGithub {
                    gh_binary: "/not-installed/gh".into(),
                },
            ),
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(result["ready"], false);
    assert!(result["failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("/not-installed/gh")));
    let reading = world.reading();
    let observed = reading
        .attempts
        .iter()
        .find(|r| r.attempt_ref == "producer")
        .unwrap()
        .observations
        .last()
        .unwrap();
    assert_eq!(observed.payload["observation"]["enumerated"], false);
    assert!(observed.payload["observation"]["error"].is_string());
    assert_eq!(observed.payload["standing"], "actual-github-read");
}

#[cfg(unix)]
#[test]
fn delivery_hosted_read_deadline_stops_owned_reader_and_retains_failure() {
    use epilogos_factory::attempt_delivery::*;
    use std::os::unix::fs::PermissionsExt;
    let world = World::new();
    world.start("producer");
    world.bind("producer", OwnerOperationPhase::Returned);
    let mut plan = delivery_plan(&world);
    plan.timeout_ms = 40;
    success(delivery_invoke(
        &world,
        &delivery_request(&world, "select", DeliveryOperation::Select { plan }),
    ));
    let reader = world.dir.path().join("slow-owned-reader");
    let pid = world.dir.path().join("reader.pid");
    fs::write(&reader,format!("#!/usr/bin/python3\nimport os,time\nopen({:?},'w').write(str(os.getpid()))\ntime.sleep(30)\n",pid.to_str().unwrap())).unwrap();
    fs::set_permissions(&reader, fs::Permissions::from_mode(0o700)).unwrap();
    let output = success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "deadline",
            DeliveryOperation::ObserveGithub {
                gh_binary: reader.to_str().unwrap().into(),
            },
        ),
    ));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ready"], false);
    assert!(value["failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("timed out")));
    if pid.exists() {
        let pid: i32 = fs::read_to_string(pid).unwrap().parse().unwrap();
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "owned reader residual pid={pid}"
        );
    }
}

#[cfg(unix)]
#[test]
fn delivery_owner_capture_bounds_inherited_pipes_and_nonreading_stdin() {
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let closed_pid = dir.path().join("closed-pipe-child.pid");
    let script = format!("import subprocess,json; p=subprocess.Popen(['/bin/sleep','30'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL); open({:?},'w').write(str(p.pid)); print(json.dumps({{'complete':True}}))", closed_pid.to_str().unwrap());
    let clock = Instant::now();
    let output = epilogos_factory::sensing_sources::owner_json_with_timeout(
        "/usr/bin/python3",
        &["-c".into(), script],
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(output["complete"], true);
    assert!(clock.elapsed() < Duration::from_secs(10));
    let pid = fs::read_to_string(closed_pid).unwrap();
    assert!(pid.trim().parse::<u32>().unwrap() > 0);
    let residual = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", pid.trim()])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&residual.stdout).trim().is_empty(),
        "successful reader left owned child: {}",
        String::from_utf8_lossy(&residual.stdout)
    );
    for exits_early in [false, true] {
        let pids = dir.path().join(format!("pids-{exits_early}"));
        let script = format!("import os,subprocess,time; p=subprocess.Popen(['/bin/sleep','30']); open({:?},'w').write(str(os.getpid())+' '+str(p.pid)); {}", pids.to_str().unwrap(), if exits_early { "raise SystemExit(0)" } else { "time.sleep(30)" });
        let clock = Instant::now();
        let error = epilogos_factory::sensing_sources::owner_json_with_timeout(
            "/usr/bin/python3",
            &["-c".into(), script],
            Some(&json!({"request":"x".repeat(1024*1024-64)})),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(error.contains("timed out"), "{error}");
        assert!(
            clock.elapsed() < Duration::from_secs(10),
            "owned capture exceeded wall budget: {:?}",
            clock.elapsed()
        );
        let observed_pids = fs::read_to_string(pids)
            .expect("reader must start and record its actual descendant before timeout");
        let observed_pids = observed_pids.split_whitespace().collect::<Vec<_>>();
        assert_eq!(
            observed_pids.len(),
            2,
            "reader and descendant must be observed"
        );
        for pid in observed_pids {
            assert!(pid.parse::<u32>().unwrap() > 0);
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                let output = Command::new("/bin/ps")
                    .args(["-o", "stat=", "-p", pid])
                    .output()
                    .unwrap();
                let status = String::from_utf8_lossy(&output.stdout);
                // A reaped client or an observed retired orphan is no live
                // owned process. Keep the actual status observable in failure.
                if status.trim().is_empty() || status.trim().starts_with('Z') {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "owned reader residual {pid}: {status}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn delivery_hosted_rerun_cannot_relabel_old_green_check_as_current_attempt() {
    use epilogos_factory::attempt_delivery::*;
    use std::os::unix::fs::PermissionsExt;
    let world = delivery_world();
    let state = world.dir.path().join("controlled-rerun.json");
    fs::write(&state, "1").unwrap();
    let reader = world.dir.path().join("controlled-rerun-reader");
    fs::write(&reader,format!("#!/usr/bin/python3\nimport json,sys\ni=json.load(open({:?}));e=sys.argv[-1];s='b'*40\nif '/git/matching-refs/' in e:r=[{{'ref':'refs/heads/feat/controlled-delivery','object':{{'sha':s}}}}]\nelif '/pulls/' in e:r={{'head':{{'ref':'feat/controlled-delivery','sha':s}}}}\nelif '/commits/' in e and '/check-runs?' not in e:r={{'sha':s,'commit':{{'tree':{{'sha':('e' if s=='c'*40 else 'd')*40}}}}}}\nelif '/check-runs?' in e:r={{'total_count':1,'check_runs':[{{'id':i,'name':'factory-owner-check','head_sha':s,'status':'completed','conclusion':'success','url':'controlled:check/'+str(i),'details_url':'https://github.com/EpiLogos/Factory/actions/runs/2/job/9'}}]}}\nelif '/attempts/2/jobs?' in e:r={{'total_count':1,'jobs':[{{'check_run_url':'controlled:check-runs/2'}}]}}\nelif '/actions/runs/2' in e:r={{'head_sha':s,'run_attempt':2}}\nelse:raise RuntimeError(e)\nprint(json.dumps(r))\n",state.to_str().unwrap())).unwrap();
    fs::set_permissions(&reader, fs::Permissions::from_mode(0o700)).unwrap();
    let observed: Value = serde_json::from_slice(
        &success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "old-green",
                DeliveryOperation::ObserveGithub {
                    gh_binary: reader.to_str().unwrap().into(),
                },
            ),
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(observed["ready"], false);
    assert!(observed["failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str().unwrap().contains("superseded workflow attempt")));
    fs::write(&state, "2").unwrap();
    let current: Value = serde_json::from_slice(
        &success(delivery_invoke(
            &world,
            &delivery_request(
                &world,
                "current-green",
                DeliveryOperation::ObserveGithub {
                    gh_binary: reader.to_str().unwrap().into(),
                },
            ),
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(current["checks"][0]["runAttempt"], 2);
    assert_eq!(current["checks"][0]["runId"], "2");
    assert!(!current["failures"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f.as_str().unwrap().contains("superseded workflow attempt")));
}

#[test]
fn delivery_native_review_publication_interruption_and_fresh_readiness() {
    use epilogos_factory::attempt_delivery::*;
    use epilogos_factory::workflow_inputs::{SelectedWorkflowInput, WorkflowInputSource};
    let mut source: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
    source
        .units
        .iter_mut()
        .find(|u| u.key == "review-adversarially")
        .unwrap()
        .inputs = vec![WorkflowInputSource {
        predecessor: "inspect-source".into(),
        receiving_context_ref: "context:controlled-review-material".into(),
    }];
    source.source.digest = workflow_source_digest(&source).unwrap();
    let world = World::with_source(source);
    world.start("producer");
    world.bind("producer", OwnerOperationPhase::Returned);
    let mut joined_plan = delivery_plan(&world);
    joined_plan.cross_owner_verification = Some(delivery_cross_basis('b', Some(91)));
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "select",
            DeliveryOperation::Select { plan: joined_plan },
        ),
    ));
    delivery_verified_return(
        &world,
        "producer",
        "inspect-source",
        &"b".repeat(40),
        FINGERPRINT_B,
    );
    let mut start = world.start_operation("reviewer", "review-adversarially");
    if let FactoryAttemptOperation::StartSerial { disposition, .. } = &mut start {
        disposition.participant.agency_ref = "agency:controlled-distinct-reviewer".into();
        disposition.selection.demand.agency_ref = Some(disposition.participant.agency_ref.clone());
        disposition
            .context_refs
            .insert("context:controlled-review-material".into());
        let leg = &world.reading().legs[&world.workflow.unit("inspect-source").unwrap().reference];
        disposition
            .selection
            .demand
            .independence_from
            .insert(leg.execution_ref.clone());
        disposition.selected_inputs = vec![SelectedWorkflowInput {
            predecessor: world
                .workflow
                .unit("inspect-source")
                .unwrap()
                .reference
                .clone(),
            execution_ref: leg.execution_ref.clone(),
            receiving_context_ref: "context:controlled-review-material".into(),
            artifacts: leg.artifacts.clone(),
        }];
    }
    world.apply(start);
    // Pause before the reviewer body is bound. Every native read starts a new
    // process, so this readback exercises restart with the selected material.
    let paused = world.reading();
    let selected = paused
        .attempts
        .iter()
        .find(|a| a.attempt_ref == "reviewer")
        .unwrap()
        .disposition
        .selected_inputs
        .clone();
    assert_eq!(selected.len(), 1);
    assert!(selected[0].artifacts.iter().any(|a| a
        .evidence_refs
        .contains(&format!("candidate-fingerprint:{FINGERPRINT_B}"))));
    assert_eq!(
        selected,
        world
            .reading()
            .attempts
            .iter()
            .find(|a| a.attempt_ref == "reviewer")
            .unwrap()
            .disposition
            .selected_inputs
    );
    world.bind("reviewer", OwnerOperationPhase::Returned);
    delivery_verified_return(
        &world,
        "reviewer",
        "review-adversarially",
        &"b".repeat(40),
        FINGERPRINT_B,
    );
    world.apply(FactoryAttemptOperation::RegisterIndependentReview {
        attempt_ref: "reviewer".into(),
        review_of: BTreeSet::from([world
            .workflow
            .unit("inspect-source")
            .unwrap()
            .reference
            .clone()]),
    });
    let provider = world.dir.path().join("controlled-forge.json");
    fs::write(
        &provider,
        json!({"head":"a".repeat(40),"conclusion":"success"}).to_string(),
    )
    .unwrap();
    let gh = delivery_cross_reader(&world, &provider);
    let request = delivery_request(
        &world,
        "publish-once",
        DeliveryOperation::ReservePublication {
            reviewer_attempt_ref: "reviewer".into(),
            gh_binary: gh.to_str().unwrap().into(),
        },
    );
    success(delivery_invoke(&world, &request));
    let before_uncertain_retry = fs::read(&world.state).unwrap();
    let retry_request = world.request(world.retry("premature-repair"));
    let rejected_retry = world.invoke(&retry_request);
    assert!(!rejected_retry.status.success());
    let rejection = String::from_utf8_lossy(&rejected_retry.stderr);
    assert!(
        rejection.contains("reconcile retained publication")
            || rejection.contains("unreconciled owner outcome"),
        "{rejection}"
    );
    assert_eq!(before_uncertain_retry, fs::read(&world.state).unwrap());
    let duplicate = delivery_invoke(
        &world,
        &delivery_request(&world, "another-publish", request.operation.clone()),
    );
    assert!(!duplicate.status.success());
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("uncertain publication"));
    // Model a publisher's remote effect followed by a process crash before its
    // response. Recovery performs only real native readback, never another effect.
    fs::write(
        &provider,
        json!({"head":"b".repeat(40),"conclusion":"success"}).to_string(),
    )
    .unwrap();
    let reconcile = delivery_request(
        &world,
        "recover-publication",
        DeliveryOperation::ReconcilePublication {
            gh_binary: gh.to_str().unwrap().into(),
            publication_ref: "delivery:publish-once".into(),
        },
    );
    let result: Value =
        serde_json::from_slice(&success(delivery_invoke(&world, &reconcile)).stdout).unwrap();
    assert_eq!(result["ready"], true, "{result}");
    assert_eq!(result["crossOwner"]["basis"]["runId"], 91);
    assert_eq!(
        result["crossOwner"]["sourceCut"]["owners"]["factory"]["observed_revision"],
        "b".repeat(40)
    );
    let before = fs::read(&world.state).unwrap();
    success(delivery_invoke(&world, &reconcile));
    assert_eq!(before, fs::read(&world.state).unwrap());
    assert_eq!(
        world
            .reading()
            .attempts
            .iter()
            .find(|r| r.attempt_ref == "producer")
            .unwrap()
            .observations
            .iter()
            .filter(|r| r.payload["kind"] == "publication-intent")
            .count(),
        1
    );
    fs::write(
        &provider,
        json!({"head":"c".repeat(40),"conclusion":"success"}).to_string(),
    )
    .unwrap();
    let stale: Value = serde_json::from_slice(
        &success(command(
            &[
                "attempt",
                "readiness",
                world.state.to_str().unwrap(),
                RUN,
                "producer",
                "--json",
            ],
            None,
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(
        stale["ready"], false,
        "remote movement cannot inherit prior green: {stale}"
    );
    // A real native hosted failure after publication must reopen both the
    // candidate and its actual selected-material review, retaining old outputs.
    fs::write(
        &provider,
        json!({"head":"b".repeat(40),"job_conclusion":"failure","run_conclusion":"failure","mode":"missing-binaries"}).to_string(),
    )
    .unwrap();
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "published-ci-failure",
            DeliveryOperation::ObserveGithub {
                gh_binary: gh.to_str().unwrap().into(),
            },
        ),
    ));
    let failed = world.reading();
    assert_eq!(
        failed.legs[&world.workflow.unit("inspect-source").unwrap().reference].status,
        LegStatus::Failed
    );
    assert_eq!(
        failed.legs[&world
            .workflow
            .unit("review-adversarially")
            .unwrap()
            .reference]
            .status,
        LegStatus::Failed
    );
    assert!(failed
        .attempts
        .iter()
        .find(|a| a.attempt_ref == "reviewer")
        .unwrap()
        .failure_evidence_refs
        .contains("delivery:published-ci-failure"));
    success(delivery_invoke(
        &world,
        &delivery_request(
            &world,
            "bounded-live-shape-repair",
            DeliveryOperation::RequestRepair {
                observation_ref: "delivery:published-ci-failure".into(),
                failure_class: "code".into(),
                failure_logs: "controlled exact failed invariant".into(),
                artifact_refs: set(["artifact:bounded-failure-log"]),
                prior_decision_refs: set(["decision:preserve-admitted-delivery"]),
            },
        ),
    ));
    let mut repaired = delivery_plan(&world);
    repaired.candidate_revision = "c".repeat(40);
    repaired.candidate_fingerprint = FINGERPRINT_C.into();
    repaired.expected_remote_head = Some("b".repeat(40));
    repaired.cross_owner_verification = Some(delivery_cross_basis('c', Some(92)));
    let successor = repaired.cross_owner_verification.as_mut().unwrap();
    successor.aikit.revision = "5".repeat(40);
    successor.aikit.tree = "6".repeat(40);
    successor.workflow_revision = "8".repeat(40);
    let mut repair = world.retry("repair-producer");
    if let FactoryAttemptOperation::Retry { reresolution, .. } = &mut repair {
        *reresolution = Some(delivery_cross_retry_resolution(
            &world, "producer", &repaired,
        ));
    }
    world.apply(repair);
    world.bind("repair-producer", OwnerOperationPhase::Returned);
    let mut request = delivery_request(
        &world,
        "select-correction",
        DeliveryOperation::Select { plan: repaired },
    );
    request.attempt_ref = "repair-producer".into();
    success(delivery_invoke(&world, &request));
    delivery_verified_return(
        &world,
        "repair-producer",
        "inspect-source",
        &"c".repeat(40),
        FINGERPRINT_C,
    );
    let mut disposition = world.disposition("review-adversarially");
    disposition.participant.agency_ref = "agency:controlled-distinct-reviewer".into();
    disposition.selection.demand.agency_ref = Some(disposition.participant.agency_ref.clone());
    disposition
        .context_refs
        .insert("context:controlled-review-material".into());
    let reading = world.reading();
    let producer = &reading.legs[&world.workflow.unit("inspect-source").unwrap().reference];
    disposition
        .selection
        .demand
        .independence_from
        .insert(producer.execution_ref.clone());
    disposition.selected_inputs = vec![SelectedWorkflowInput {
        predecessor: world
            .workflow
            .unit("inspect-source")
            .unwrap()
            .reference
            .clone(),
        execution_ref: producer.execution_ref.clone(),
        receiving_context_ref: "context:controlled-review-material".into(),
        artifacts: producer.artifacts.clone(),
    }];
    world.apply(FactoryAttemptOperation::Retry {
        attempt_ref: "correction-reviewer".into(),
        task_ref: "task:review-adversarially".into(),
        parent_journey_ref: "journey:controlled-test".into(),
        workflow_unit_ref: world
            .workflow
            .unit("review-adversarially")
            .unwrap()
            .reference
            .clone(),
        grant_ref: "grant:review-adversarially".into(),
        disposition,
        tracking: vec![],
        reresolution: Some(delivery_retry_resolution(&world, "reviewer")),
        place_grant: None,
    });
    world.bind("correction-reviewer", OwnerOperationPhase::Returned);
    delivery_verified_return(
        &world,
        "correction-reviewer",
        "review-adversarially",
        &"c".repeat(40),
        FINGERPRINT_C,
    );
    world.apply(FactoryAttemptOperation::RegisterIndependentReview {
        attempt_ref: "correction-reviewer".into(),
        review_of: BTreeSet::from([world
            .workflow
            .unit("inspect-source")
            .unwrap()
            .reference
            .clone()]),
    });
    let mut reserve = delivery_request(
        &world,
        "publish-correction",
        DeliveryOperation::ReservePublication {
            reviewer_attempt_ref: "correction-reviewer".into(),
            gh_binary: gh.to_str().unwrap().into(),
        },
    );
    reserve.attempt_ref = "repair-producer".into();
    success(delivery_invoke(&world, &reserve));
    fs::write(
        &provider,
        json!({"head":"c".repeat(40),"candidate":"c".repeat(40),"conclusion":"success", "aikit_revision":"5".repeat(40),"aikit_tree":"6".repeat(40),"oi":"8".repeat(40)})
            .to_string(),
    )
    .unwrap();
    let mut recovered = delivery_request(
        &world,
        "readback-correction",
        DeliveryOperation::ReconcilePublication {
            gh_binary: gh.to_str().unwrap().into(),
            publication_ref: "delivery:publish-correction".into(),
        },
    );
    recovered.attempt_ref = "repair-producer".into();
    let ready: Value =
        serde_json::from_slice(&success(delivery_invoke(&world, &recovered)).stdout).unwrap();
    assert_eq!(ready["ready"], true, "{ready}");
    assert_eq!(ready["candidateRevision"], "c".repeat(40));
    assert_eq!(ready["repairs"], 1);
    assert!(ready["failureEvidenceRefs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "delivery:published-ci-failure"));
    assert_eq!(
        world
            .reading()
            .attempts
            .iter()
            .flat_map(|a| &a.observations)
            .filter(|r| r.payload["kind"] == "repair")
            .count(),
        1
    );
}

#[path = "support/praxis_receipt_fixture.rs"]
mod praxis_receipt_fixture;

struct World {
    dir: tempfile::TempDir,
    state: PathBuf,
    workflow: CompiledWorkflow,
}

impl World {
    fn new() -> Self {
        Self::with_source(serde_json::from_str(SOURCE).unwrap())
    }

    fn with_source(source: WorkflowSource) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for name in ["NOW", "work", "Control"] {
            fs::create_dir(dir.path().join(name)).unwrap();
        }
        let state = dir.path().join("attempts.json");
        let workflow = compile_workflow(source.clone()).unwrap();
        let seed = FactoryAttemptSeed {
            run: Run::new(
                RUN.parse().unwrap(),
                PROJECT.parse().unwrap(),
                "public attempt regressions",
                "factory-test",
            )
            .unwrap(),
            workflow_source: source,
        };
        success(command(
            &["attempt", "init", state.to_str().unwrap(), "-", "--json"],
            Some(&serde_json::to_string(&seed).unwrap()),
        ));
        Self {
            dir,
            state,
            workflow,
        }
    }

    fn reading(&self) -> FactoryAttemptReading {
        serde_json::from_slice(
            &success(command(
                &["attempt", "read", self.state.to_str().unwrap(), "--json"],
                None,
            ))
            .stdout,
        )
        .unwrap()
    }

    fn request(&self, operation: FactoryAttemptOperation) -> FactoryAttemptActionRequest {
        let revision = self.reading().revision;
        FactoryAttemptActionRequest {
            contract: FACTORY_ATTEMPT_ACTION.into(),
            projection_ref: format!("projection:controlled-{revision}"),
            caller: FactoryActionCaller {
                caller_ref: "caller:controlled-test".into(),
                projection_kind: FactoryActionProjectionKind::Headless,
                lineage: vec!["caller:controlled-test".into()],
            },
            run_ref: RUN.parse::<RunRef>().unwrap(),
            expected_revision: revision,
            authority: ProjectedFactoryActionAuthority {
                authority_ref: "authority:controlled-test".into(),
                native_owner: "factory".into(),
                capability_ref: Some(FACTORY_ATTEMPT_CAPABILITY_REF.into()),
                capability_granted: true,
                action_authorised: true,
            },
            operation,
        }
    }

    fn invoke(&self, request: &FactoryAttemptActionRequest) -> Output {
        command(
            &[
                "attempt",
                "action",
                self.state.to_str().unwrap(),
                "-",
                "--json",
            ],
            Some(&serde_json::to_string(request).unwrap()),
        )
    }

    fn apply(&self, operation: FactoryAttemptOperation) {
        success(self.invoke(&self.request(operation)));
    }

    fn refuses(&self, operation: FactoryAttemptOperation) -> String {
        let request = self.request(operation);
        let bytes = fs::read(&self.state).unwrap();
        let result = self.invoke(&request);
        assert!(
            !result.status.success(),
            "unexpected success: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(
            bytes,
            fs::read(&self.state).unwrap(),
            "refusal mutated durable state"
        );
        String::from_utf8(result.stderr).unwrap()
    }

    fn disposition(&self, key: &str) -> SituatedExecutionDisposition {
        let unit = self.workflow.unit(key).unwrap();
        let agent = unit
            .agent_requirements
            .agent_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agent:controlled-test".into());
        let agency = unit
            .agent_requirements
            .agency_refs
            .iter()
            .next()
            .map(ToString::to_string)
            .unwrap_or_else(|| "agency:controlled-test".into());
        let demand = ExecutionDemand {
            project_ref: PROJECT.into(),
            run_ref: RUN.into(),
            workflow_unit_ref: Some(unit.reference.to_string()),
            agency_ref: Some(agency.clone()),
            profile_ref: None,
            use_type: "controlled-state-test".into(),
            required_capabilities: unit.capability_refs.clone(),
            required_modalities: BTreeSet::new(),
            required_actions: BTreeSet::new(),
            required_tools: BTreeSet::new(),
            context_characteristics: BTreeSet::new(),
            independence_from: BTreeSet::new(),
            cost_ceiling_usd: None,
            latency_preference_ms: None,
            requires_local_materialisation: false,
        };
        let selection = accept_aikit_selection(
            demand,
            AikitModelRosterSelection {
                roster_version: AIKIT_MODEL_ROSTER_VERSION.into(),
                model_ref: "model:controlled-test".into(),
                provider_ref: "provider:controlled-test".into(),
                ranking_policy: "controlled-test".into(),
                ranking_explanation: json!({"testOnly":true}),
                provenance: vec!["selection:controlled-test".into()],
            },
            "2026-09-10T20:00:00Z",
        )
        .unwrap();
        SituatedExecutionDisposition {
            selected_inputs: Vec::new(),
            selection,
            participant: SituatedParticipant {
                agent_ref: agent,
                agency_ref: agency,
                world_binding_ref: "world-binding:controlled-test".into(),
                profile_ref: None,
                position_ref: None,
                source_ref: "source:controlled-test".into(),
                source_revision: "fixture-v1".into(),
                source_digest: format!("blake3:{}", blake3::hash(b"controlled test only").to_hex()),
            },
            context_refs: set(["context:controlled-test"]),
            praxis_refs: unit.praxis_refs.clone(),
            capability_refs: unit.capability_refs.clone(),
            praxis_receipt: Some(praxis_receipt_fixture::resolved_receipt(
                &unit.reference.to_string(),
                &unit.praxis_refs,
            )),
            now_context_ref: (!unit.praxis_refs.is_empty()).then(|| "now:fixture".into()),
            body: ExecutionBody {
                model_ref: "model:controlled-test".into(),
                provider_ref: "provider:controlled-test".into(),
                route_ref: "route:controlled-test".into(),
                harness_ref: "harness:controlled-test".into(),
                harness_composition_ref: "composition:controlled-test".into(),
                agent_session_ref: format!("agent-session:controlled-{key}"),
                session_space_ref: "session-space:controlled-test".into(),
                material_world_ref: Some("material-world:controlled-test".into()),
                workcell_ref: Some("workcell:controlled-test".into()),
            },
            placement: Some(PlacementProtection {
                now_ref: "now:controlled-test".into(),
                now_path: self.dir.path().join("NOW").to_str().unwrap().into(),
                policy_ref: "policy:controlled-test".into(),
                policy_revision: "fixture-policy-v1".into(),
                authority_ref: "authority:controlled-test".into(),
                writable_paths: BTreeSet::from([
                    self.dir.path().join("NOW").to_str().unwrap().to_owned(),
                    self.dir.path().join("work").to_str().unwrap().to_owned(),
                ]),
                protected_paths: BTreeSet::from([self
                    .dir
                    .path()
                    .join("Control")
                    .to_str()
                    .unwrap()
                    .to_owned()]),
                required_coverage: set(["file-content"]),
                effective_coverage: set(["file-content"]),
                write_boundary_ref: Some("boundary:controlled-fixture-not-kernel-proof".into()),
                material_receipt_ref: Some("material-receipt:controlled-test".into()),
            }),
            permitted_effects: unit.permitted_effects.clone(),
            verification_obligations: unit.verification_obligations.clone(),
            return_address: unit.return_address.clone(),
            stop_conditions: unit.stop_conditions.clone(),
            escalation_conditions: unit.escalation_conditions.clone(),
            budget: ExecutionBudget {
                cost_ceiling_usd: None,
                latency_preference_ms: None,
                wall_clock_timeout_ms: Some(5000),
                retry_grant_ref: Some(format!("grant:{key}")),
                maximum_attempts: Some(2),
            },
        }
    }

    fn start_operation(&self, reference: &str, key: &str) -> FactoryAttemptOperation {
        FactoryAttemptOperation::StartSerial {
            attempt_ref: reference.into(),
            task_ref: format!("task:{key}"),
            parent_journey_ref: "journey:controlled-test".into(),
            workflow_unit_ref: self.workflow.unit(key).unwrap().reference.clone(),
            disposition: self.disposition(key),
            retry_grant: Some(RetryGrant::new(format!("grant:{key}"), 2).unwrap()),
            tracking: vec![],
            place_grant: None,
        }
    }

    fn start(&self, reference: &str) {
        self.apply(self.start_operation(reference, "inspect-source"));
    }

    fn bind(&self, reference: &str, phase: OwnerOperationPhase) {
        self.apply(FactoryAttemptOperation::BindDispatch {
            attempt_ref: reference.into(),
            execution_ref: format!("execution:{reference}"),
            receipt: owner_receipt(reference, "dispatch", phase),
        });
    }

    fn verify(&self, reference: &str, outcome: VerificationOutcome) {
        self.apply(FactoryAttemptOperation::RecordVerification {
            attempt_ref: reference.into(),
            verification: VerificationReceipt {
                verification_ref: format!("verification:{reference}:{}", self.reading().revision),
                owner_ref: "factory-verifier:controlled-test".into(),
                source_revision: "controlled-verifier-v1".into(),
                outcome,
                obligations: self
                    .workflow
                    .unit("inspect-source")
                    .unwrap()
                    .verification_obligations
                    .clone(),
                evidence_refs: set(["evidence:controlled-verification"]),
            },
        });
    }

    fn fail(&self, reference: &str) {
        self.apply(FactoryAttemptOperation::Fail {
            attempt_ref: reference.into(),
            reason: "controlled failure".into(),
            evidence_refs: set(["evidence:controlled-failure"]),
        });
    }

    fn retry(&self, reference: &str) -> FactoryAttemptOperation {
        FactoryAttemptOperation::Retry {
            attempt_ref: reference.into(),
            task_ref: "task:inspect-source".into(),
            parent_journey_ref: "journey:controlled-test".into(),
            workflow_unit_ref: self
                .workflow
                .unit("inspect-source")
                .unwrap()
                .reference
                .clone(),
            grant_ref: "grant:inspect-source".into(),
            disposition: self.disposition("inspect-source"),
            tracking: vec![],
            reresolution: None,
            place_grant: None,
        }
    }

    fn returned(&self, reference: &str) -> FactoryAttemptOperation {
        let unit = self.workflow.unit("inspect-source").unwrap();
        let artifact_ref = format!("artifact:{reference}");
        let evidence = set(["evidence:controlled-verification"]);
        FactoryAttemptOperation::ReturnArtifact {
            attempt_ref: reference.into(),
            artifact: ReturnedArtifact {
                artifact_ref: artifact_ref.clone(),
                subject_ref: unit.subject_ref.to_string(),
                subject_revision: unit.basis_revision.clone(),
                producing_execution_ref: format!("execution:{reference}"),
                evidence_refs: evidence.clone(),
                semantic_difference: "Controlled test artifact; no provider proof".into(),
            },
            readable_return: ReadableReturn {
                return_ref: format!("return:{reference}"),
                summary: "Controlled native state transition, not live model execution".into(),
                artifact_refs: BTreeSet::from([artifact_ref]),
                evidence_refs: evidence,
                receiving_ref: None,
                receiving_source_revision: None,
                archive_refs: BTreeSet::new(),
                regression_observation_refs: BTreeSet::new(),
            },
        }
    }
}

fn set<const N: usize>(items: [&str; N]) -> BTreeSet<String> {
    items.into_iter().map(str::to_owned).collect()
}

fn owner_receipt(attempt: &str, suffix: &str, phase: OwnerOperationPhase) -> OwnerOperationReceipt {
    OwnerOperationReceipt {
        owner_ref: "aikit".into(),
        contract: "controlled-owner-fixture/v1".into(),
        operation_ref: format!("delivery:{attempt}"),
        receipt_ref: format!("receipt:{attempt}:{suffix}"),
        source_revision: "controlled-owner-fixture".into(),
        phase,
        evidence_refs: set(["evidence:controlled-owner"]),
        partial_effect_refs: BTreeSet::new(),
        payload: json!({"testOnly":true}),
    }
}

fn spawn(args: &[&str], input: Option<&str>) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_factory"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    child
}
fn command(args: &[&str], input: Option<&str>) -> Output {
    spawn(args, input).wait_with_output().unwrap()
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "Factory command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn native_restart_readback_and_stale_revision_are_process_safe() {
    let world = World::new();
    let stale = world.request(world.start_operation("stale", "inspect-source"));
    world.start("first");
    let before = fs::read(&world.state).unwrap();
    assert!(!world.invoke(&stale).status.success());
    assert_eq!(before, fs::read(&world.state).unwrap());
    let reading = world.reading();
    assert_eq!(reading.attempts[0].attempt_ref, "first");
    assert_eq!(reading.run_ref.to_string(), RUN);
    let stored: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        stored["state"]["build"]["runs"]["runs"][RUN]["writeAuthority"],
        json!({"owner":"factory-test","epoch":1})
    );
    let alias: FactoryAttemptReading = serde_json::from_slice(
        &success(command(
            &[
                "development",
                "attempt",
                "read",
                world.state.to_str().unwrap(),
                "--json",
            ],
            None,
        ))
        .stdout,
    )
    .unwrap();
    assert_eq!(reading, alias);
}

#[test]
fn historical_cancellation_cannot_hit_retry_and_late_return_stays_historical() {
    let world = World::new();
    world.start("z-first");
    world.bind("z-first", OwnerOperationPhase::Returned);
    world.verify("z-first", VerificationOutcome::Passed);
    world.fail("z-first");
    world.apply(world.retry("a-second"));
    assert!(world
        .refuses(FactoryAttemptOperation::RequestCancellation {
            attempt_ref: "z-first".into()
        })
        .contains("historical"));
    assert!(world
        .refuses(FactoryAttemptOperation::Fail {
            attempt_ref: "z-first".into(),
            reason: "late failure".into(),
            evidence_refs: set(["evidence:late"])
        })
        .contains("historical"));
    world.apply(world.returned("z-first"));
    let reading = world.reading();
    let leg = &reading.legs[&world.workflow.unit("inspect-source").unwrap().reference];
    assert_eq!(leg.status, LegStatus::Active);
    assert_eq!(leg.execution_ref, "factory-attempt:a-second");
    assert_eq!(leg.attempts[0].late_artifacts.len(), 1);
    assert!(leg.artifacts.is_empty());
    world.fail("a-second");
    assert!(world
        .refuses(world.retry("third"))
        .contains("RetryExhausted"));
}

#[test]
fn unknown_effects_do_not_release_writer_or_become_a_return() {
    let world = World::new();
    world.start("unknown");
    world.bind("unknown", OwnerOperationPhase::Uncertain);
    world.verify("unknown", VerificationOutcome::Passed);
    assert!(world
        .refuses(FactoryAttemptOperation::Fail {
            attempt_ref: "unknown".into(),
            reason: "transport lost".into(),
            evidence_refs: set(["evidence:lost"])
        })
        .contains("uncertain"));
    assert!(world
        .refuses(world.returned("unknown"))
        .contains("uncertain"));
    let reading = world.reading();
    assert_eq!(
        reading.legs.values().next().unwrap().status,
        LegStatus::Active
    );
    assert_eq!(
        reading.attempts[0].dispatch.as_ref().unwrap().phase,
        OwnerOperationPhase::Uncertain
    );
}

#[test]
fn submitted_ack_is_not_work_but_observed_return_and_verification_can_return() {
    let world = World::new();
    world.start("ack");
    world.bind("ack", OwnerOperationPhase::Submitted);
    world.verify("ack", VerificationOutcome::Passed);
    assert!(world
        .refuses(world.returned("ack"))
        .contains("not completed"));
    world.apply(FactoryAttemptOperation::RecordObservation {
        attempt_ref: "ack".into(),
        receipt: owner_receipt("ack", "completed", OwnerOperationPhase::Returned),
    });
    world.apply(world.returned("ack"));
    let reading = world.reading();
    assert_eq!(
        reading.legs.values().next().unwrap().status,
        LegStatus::Returned
    );
    assert!(reading.attempts[0]
        .readable_return
        .as_ref()
        .unwrap()
        .receiving_ref
        .is_none());
    let prose = success(command(
        &["attempt", "read", world.state.to_str().unwrap()],
        None,
    ));
    assert!(String::from_utf8(prose.stdout)
        .unwrap()
        .contains("Controlled native state transition"));
}

#[test]
fn later_failed_verification_supersedes_an_earlier_pass() {
    let world = World::new();
    world.start("verified");
    world.bind("verified", OwnerOperationPhase::Returned);
    world.verify("verified", VerificationOutcome::Passed);
    world.verify("verified", VerificationOutcome::Failed);
    assert!(world
        .refuses(world.returned("verified"))
        .contains("supersedes"));
}

#[test]
fn attribution_preserves_distinct_usage_owners_and_exact_source_revisions() {
    let world = World::new();
    world.start("trace");
    let fact = AttemptTrackingFact {
        fact_ref: "fact:model-usage".into(),
        kind: "model-usage".into(),
        owner_ref: "workcell".into(),
        subject_ref: "usage:controlled".into(),
        source_revision: "usage-revision-exact".into(),
        evidence_refs: set(["evidence:owner-usage"]),
    };
    assert!(world
        .refuses(FactoryAttemptOperation::RecordTracking {
            attempt_ref: "trace".into(),
            fact: fact.clone()
        })
        .contains("misattributes"));
    world.apply(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "trace".into(),
        fact: AttemptTrackingFact {
            owner_ref: "actuation".into(),
            ..fact
        },
    });
    world.apply(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "trace".into(),
        fact: AttemptTrackingFact {
            fact_ref: "fact:source".into(),
            kind: "source-change".into(),
            owner_ref: "central".into(),
            subject_ref: "source:controlled".into(),
            source_revision: "source-revision-exact".into(),
            evidence_refs: set(["evidence:source-change"]),
        },
    });
    let reading = world.reading();
    assert_eq!(
        reading.attempts[0].tracking[0].source_revision,
        "usage-revision-exact"
    );
    assert_eq!(
        reading.attempts[0].tracking[1].source_revision,
        "source-revision-exact"
    );
}

#[test]
fn persisted_retry_spend_and_source_tampering_are_refused_on_public_read() {
    let world = World::new();
    world.start("corrupt");
    let bytes = fs::read(&world.state).unwrap();
    let mut state: Value = serde_json::from_slice(&bytes).unwrap();
    state["state"]["attemptStates"][RUN]["snapshot"]["retryGrants"]["grant:inspect-source"]
        ["attemptsSpent"] = json!(0);
    fs::write(&world.state, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(!command(
        &["attempt", "read", world.state.to_str().unwrap(), "--json"],
        None
    )
    .status
    .success());
    let mut state: Value = serde_json::from_slice(&bytes).unwrap();
    state["state"]["attemptStates"][RUN]["workflowSource"]["source"]["revision"] =
        json!("different-source");
    fs::write(&world.state, serde_json::to_vec(&state).unwrap()).unwrap();
    assert!(!command(
        &["attempt", "read", world.state.to_str().unwrap(), "--json"],
        None
    )
    .status
    .success());
}

#[test]
fn parallel_public_writers_cannot_both_commit_the_same_revision() {
    let world = World::new();
    let first =
        serde_json::to_string(&world.request(world.start_operation("one", "inspect-source")))
            .unwrap();
    let second =
        serde_json::to_string(&world.request(world.start_operation("two", "inspect-source")))
            .unwrap();
    let args = [
        "attempt",
        "action",
        world.state.to_str().unwrap(),
        "-",
        "--json",
    ];
    let first = spawn(&args, Some(&first));
    let second = spawn(&args, Some(&second));
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert_ne!(first.status.success(), second.status.success());
    assert_eq!(world.reading().attempts.len(), 1);
}

fn fork_world(shared_writer: bool) -> World {
    let mut value: Value = serde_json::from_str(SOURCE).unwrap();
    value["units"][1]["dependencies"] = json!([]);
    value["units"][2]["dependencies"] = json!([]);
    if shared_writer {
        value["units"][2]["permittedEffects"] = json!(["write Factory review"]);
    }
    let mut source: WorkflowSource = serde_json::from_value(value).unwrap();
    source.source.digest = workflow_source_digest(&source).unwrap();
    World::with_source(source)
}

fn fork(world: &World) -> FactoryAttemptOperation {
    FactoryAttemptOperation::StartFork {
        parent_journey_ref: "journey:controlled-test".into(),
        attempts: ["implement-compiler", "review-adversarially"]
            .into_iter()
            .map(|key| AttemptStart {
                attempt_ref: format!("fork:{key}"),
                task_ref: format!("task:{key}"),
                workflow_unit_ref: world.workflow.unit(key).unwrap().reference.clone(),
                disposition: world.disposition(key),
                retry_grant: Some(RetryGrant::new(format!("grant:{key}"), 2).unwrap()),
                tracking: vec![],
                place_grant: None,
            })
            .collect(),
    }
}

#[test]
fn native_fork_refuses_shared_writer_and_failed_branch_does_not_release_barrier() {
    let conflict = fork_world(true);
    assert!(conflict
        .refuses(fork(&conflict))
        .contains("SharedWriterConflict"));
    let world = fork_world(false);
    world.apply(fork(&world));
    world.fail("fork:review-adversarially");
    assert!(world
        .refuses(world.start_operation("integrate", "integrate-return"))
        .contains("BarrierNotSatisfied"));
    let reading = world.reading();
    assert_eq!(reading.attempts.len(), 2);
}

#[test]
fn wrong_project_and_missing_action_authority_never_publish_attempts() {
    let world = World::new();
    let mut operation = world.start_operation("wrong", "inspect-source");
    if let FactoryAttemptOperation::StartSerial { disposition, .. } = &mut operation {
        disposition.selection.demand.project_ref = "project:01ARZ3NDEKTSV4RRFFQ69G5FAA".into();
    }
    assert!(world.refuses(operation).contains("another Project"));
    let mut request = world.request(world.start_operation("unauthorised", "inspect-source"));
    request.authority.action_authorised = false;
    let before = fs::read(&world.state).unwrap();
    assert!(!world.invoke(&request).status.success());
    assert_eq!(before, fs::read(&world.state).unwrap());
}

#[test]
fn native_projection_reuse_and_exact_action_replay_are_distinct() {
    let world = World::new();
    let first = world.request(world.start_operation("projection-reuse", "inspect-source"));
    success(world.invoke(&first));
    let committed = fs::read(&world.state).unwrap();
    success(world.invoke(&first));
    assert_eq!(committed, fs::read(&world.state).unwrap());
    let mut second = world.request(FactoryAttemptOperation::RecordTracking {
        attempt_ref: "projection-reuse".into(),
        fact: AttemptTrackingFact {
            fact_ref: "fact:second-action-same-projection".into(),
            kind: "source-change".into(),
            owner_ref: "central".into(),
            subject_ref: "source:controlled-test".into(),
            source_revision: "source:revision-two".into(),
            evidence_refs: set(["evidence:source-two"]),
        },
    });
    second.projection_ref = first.projection_ref.clone();
    success(world.invoke(&second));
    let committed = fs::read(&world.state).unwrap();
    success(world.invoke(&second));
    assert_eq!(committed, fs::read(&world.state).unwrap());
    assert_eq!(world.reading().attempts[0].tracking.len(), 1);
}

#[test]
fn native_project_and_attempt_reads_share_one_canonical_run() {
    let world = World::new();
    world.start("canonical");
    let attempts = world.reading();
    let native =
        epilogos_factory::project_development_store::read_developmental_state(&world.state)
            .unwrap();
    let run = native.build.run(&RUN.parse().unwrap()).unwrap();
    assert_eq!(run.revision().get(), attempts.run_revision);
    assert_eq!(
        run.map().topology_revision().get(),
        attempts.topology_revision
    );
    assert_eq!(native.build.revision().get(), attempts.revision);
    let result = success(command(
        &[
            "development",
            "run",
            world.state.to_str().unwrap(),
            RUN,
            "--json",
        ],
        None,
    ));
    assert!(String::from_utf8(result.stdout).unwrap().contains(RUN));
    let bytes: Value = serde_json::from_slice(&fs::read(&world.state).unwrap()).unwrap();
    assert!(bytes["state"]["attemptStates"][RUN].get("run").is_none());
    assert_eq!(
        bytes["state"]["build"]["runs"]["runs"]
            .as_object()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn changed_current_source_stays_readable_but_cannot_start_effects_from_old_basis() {
    let world = World::new();
    world.start("history");
    let before = world.reading();
    epilogos_factory::project_development_store::transact_developmental_state(
        &world.state,
        |native| {
            native.workflow_sources[0].source.revision = "source-revision-new-current".into();
            native.workflow_sources[0].source.digest =
                workflow_source_digest(&native.workflow_sources[0]).unwrap();
            Ok(())
        },
    )
    .unwrap();
    let after = world.reading();
    assert!(!after.source_current);
    assert_eq!(
        before.workflow_source_revision,
        after.workflow_source_revision
    );
    assert_eq!(before.attempts, after.attempts);
    let blocked = world.refuses(FactoryAttemptOperation::BindDispatch {
        attempt_ref: "history".into(),
        execution_ref: "execution:new".into(),
        receipt: owner_receipt(
            "history",
            "cannot-dispatch-stale-source",
            OwnerOperationPhase::Submitted,
        ),
    });
    assert!(blocked.contains("explicit re-resolution"));
    world.apply(FactoryAttemptOperation::RecordObservation {
        attempt_ref: "history".into(),
        receipt: owner_receipt(
            "history",
            "historical-observation",
            OwnerOperationPhase::Observed,
        ),
    });
    assert!(!world.reading().source_current);
}

#[test]
fn attaching_an_existing_native_run_does_not_create_a_parallel_run_store() {
    use epilogos_factory::build::FactoryBuildState;
    use epilogos_factory::core::run::Project;
    use epilogos_factory::developmental_read::{
        FactoryDevelopmentalFileProvider, FactoryDevelopmentalState,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("native.json");
    let source: WorkflowSource = serde_json::from_str(SOURCE).unwrap();
    let source_ref = source.source.reference.to_string();
    let run = Run::new(
        RUN.parse().unwrap(),
        PROJECT.parse().unwrap(),
        "existing canonical Run",
        "retained-writer",
    )
    .unwrap();
    let original_authority = run.write_authority().clone();
    let state = FactoryDevelopmentalState::new(
        FactoryBuildState::new(Project::new(PROJECT.parse().unwrap()), run).unwrap(),
        vec![],
    )
    .unwrap()
    .with_workflow_sources(vec![source])
    .unwrap();
    FactoryDevelopmentalFileProvider::create(&path, state).unwrap();
    let args = [
        "attempt",
        "attach",
        path.to_str().unwrap(),
        RUN,
        &source_ref,
        "--json",
    ];
    success(command(&args, None));
    let committed = fs::read(&path).unwrap();
    success(command(&args, None));
    assert_eq!(committed, fs::read(&path).unwrap());
    let native =
        epilogos_factory::project_development_store::read_developmental_state(&path).unwrap();
    let run = native.build.run(&RUN.parse().unwrap()).unwrap();
    assert_eq!(run.write_authority(), &original_authority);
    assert_eq!(run.destination(), "existing canonical Run");
    assert_eq!(native.attempt_states.len(), 1);
}

#[test]
fn uncertain_cancellation_cannot_release_writer_by_quiescing() {
    let world = World::new();
    world.start("cancel-unknown");
    world.bind("cancel-unknown", OwnerOperationPhase::Uncertain);
    world.apply(FactoryAttemptOperation::RequestCancellation {
        attempt_ref: "cancel-unknown".into(),
    });
    world.apply(FactoryAttemptOperation::AcceptCancellation {
        attempt_ref: "cancel-unknown".into(),
    });
    world.apply(FactoryAttemptOperation::RecordProcessTermination {
        attempt_ref: "cancel-unknown".into(),
    });
    assert!(world
        .refuses(FactoryAttemptOperation::MarkQuiescent {
            attempt_ref: "cancel-unknown".into()
        })
        .contains("unknown owner effects"));
}
