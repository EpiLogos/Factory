#!/usr/bin/env python3
"""Validate Factory Commission contracts through the real native provider."""

import json
import subprocess
import tempfile
from pathlib import Path

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "contracts/factory"
FIXTURES = CONTRACT / "fixtures"

request_schema = json.loads((CONTRACT / "commission-request.schema.json").read_text())
commission_schema = json.loads((CONTRACT / "commission.schema.json").read_text())
mutation_schema = json.loads((CONTRACT / "developmental-mutation.schema.json").read_text())
workflow_schema = json.loads((CONTRACT / "agent-workflow-source.schema.json").read_text())
developmental_schema = json.loads((CONTRACT / "developmental-read.schema.json").read_text())
for schema in (request_schema, commission_schema, mutation_schema, workflow_schema, developmental_schema):
    Draft202012Validator.check_schema(schema)

registry = Registry()
for schema in (request_schema, commission_schema, mutation_schema, workflow_schema):
    registry = registry.with_resource(schema["$id"], Resource.from_contents(schema))
registry = registry.with_resource(
    "https://github.com/EpiLogos/agent-system-design/contracts/factory/commission-request.schema.json",
    Resource.from_contents(request_schema),
).with_resource(
    "https://github.com/EpiLogos/agent-system-design/contracts/factory/commission.schema.json",
    Resource.from_contents(commission_schema),
).with_resource(
    "https://github.com/EpiLogos/agent-system-design/contracts/factory/agent-workflow-source.schema.json",
    Resource.from_contents(workflow_schema),
)

request = json.loads((FIXTURES / "oi-self-hosting-commission-request.json").read_text())
expected_agents = {
    "agent/oi-guardian-anuttara", "agent/oi-guardian-paramasiva",
    "agent/oi-guardian-parasakti", "agent/oi-guardian-mahamaya",
    "agent/oi-guardian-nara", "agent/oi-guardian-epii", "agent:hermes",
}
assert set(request["centralComposition"]["resolvedAgentRefs"]) == expected_agents
assert request["centralComposition"]["developmentAgentSet"]["ref"] == "oi-development-agency"
assert request["centralComposition"]["guardianAgentSet"]["ref"] == "oi-product-guardians"
assert request["centralComposition"]["authorityStanding"] == "membership-non-authoritative"
assert request["rootAct"]["standing"] == "commissioned-not-executed"
expected_profiles = {
    "profile/oi-guardian-anuttara": "688b26a8b8f07de09af135d9c6abe90f3165795437693116949031620d7bb61f",
    "profile/oi-guardian-paramasiva": "ca27260d856d4c797e21cfc4c22da69ea16e6611d1643326ef25fcc550031ec7",
    "profile/oi-guardian-parasakti": "030068c24f6da573639f3de08fefb871f298721322aa9887a4a74dea9bf090d5",
    "profile/oi-guardian-mahamaya": "b058043e81d2e894991f5b5a4a2ebc1de9fd4ccdd44854ec67e06c47cecd30df",
    "profile/oi-guardian-nara": "43af85754d711339d4ed0d272e682e813e3b81b7c20b12fd161abac621ea54cd",
    "profile/oi-guardian-epii": "2db9aa5829152d8c39c5e3c895809b9f9a12af423f04f2b46d91e37eb9e4e65e",
}
assert {item["ref"]: item["sha256"] for item in request["centralComposition"]["agentProfileReceipts"]} == expected_profiles
assert not list(Draft202012Validator(request_schema).iter_errors(request))
invalid_request = json.loads(json.dumps(request))
invalid_request["rootAct"]["scopeRefs"][0] = "issue:201 bad"
assert list(Draft202012Validator(request_schema).iter_errors(invalid_request))

workflow = json.loads((FIXTURES / "oi-self-hosting-workflow-source.json").read_text())
mutation = json.loads((FIXTURES / "oi-self-hosting-workflow-mutation.json").read_text())
assert mutation["mutation"]["workflowSource"] == workflow
assert not list(Draft202012Validator(workflow_schema).iter_errors(workflow))
assert not list(Draft202012Validator(mutation_schema, registry=registry).iter_errors(mutation))
invalid_mutation = json.loads(json.dumps(mutation))
invalid_mutation["mutation"]["runRef"] = "run:not-a-factory-ref"
assert list(Draft202012Validator(mutation_schema, registry=registry).iter_errors(invalid_mutation))

binary = ROOT / "target/debug/factory"
assert binary.is_file(), "build the native Factory binary before validating Commission"
commission_validator = Draft202012Validator(commission_schema, registry=registry)
mutation_validator = Draft202012Validator(mutation_schema, registry=registry)
developmental_validator = Draft202012Validator(developmental_schema, registry=registry)

def invoke(*args):
    completed = subprocess.run([binary, *map(str, args), "--json"], check=True, capture_output=True, text=True)
    return json.loads(completed.stdout)

with tempfile.TemporaryDirectory() as directory:
    state = Path(directory) / "state.json"
    receipt = invoke("development", "commission", state, FIXTURES / "oi-self-hosting-commission-request.json")
    assert not list(commission_validator.iter_errors(receipt))
    replay = invoke("development", "commission", state, FIXTURES / "oi-self-hosting-commission-request.json")
    assert replay["status"] == "already-applied"
    mutation_receipt = invoke("development", "mutate", state, FIXTURES / "oi-self-hosting-workflow-mutation.json")
    assert not list(mutation_validator.iter_errors(mutation_receipt))
    rejected = subprocess.run(
        [binary, "development", "mutate", state, "-", "--json"],
        input=json.dumps(invalid_mutation), capture_output=True, text=True,
    )
    assert rejected.returncode != 0
    reading = invoke("development", "commission-read", state, request["requestRef"])
    assert not list(commission_validator.iter_errors(reading))
    assert not list(developmental_validator.iter_errors(reading))
    units = invoke("development", "workflow-units", state, receipt["commission"]["runRef"])
    assert {item["key"] for item in units["units"]} == {
        "factory-guardian-owner-surface", "aikit-guardian-consumer", "oi-snapshot-188"
    }
    stored = json.loads(state.read_text())
    run = stored["state"]["build"]["runs"]["runs"][receipt["commission"]["runRef"]]
    work_states = [node["state"] for node in run["map"]["nodes"].values() if node["kind"] == "work"]
    assert set(work_states) <= {"ready", "planned"} and len(work_states) == 3
    assert stored["state"]["build"]["agencies"] == {}
    assert stored["state"]["build"]["executions"] == {}
    assert stored == json.loads((FIXTURES / "oi-self-hosting-state.json").read_text())

print("Factory self-hosting Commission schemas/native state: PASS")
