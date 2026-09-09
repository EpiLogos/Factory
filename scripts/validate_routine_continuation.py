#!/usr/bin/env python3
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = ROOT / "contracts/factory/upstream/aikit.routine-invocation-evidence.v1.schema.json"
OWNER_FIXTURE = ROOT / "contracts/factory/fixtures/aikit-routine-invocation-evidence.json"
REQUEST_SCHEMA = ROOT / "contracts/factory/routine-continuation-request.schema.json"
REQUEST_FIXTURE = ROOT / "contracts/factory/fixtures/routine-continuation-request.json"
LOCK = ROOT / "contracts/factory/routine-continuation-source-lock.json"
OUTPUT_SCHEMA = ROOT / "contracts/factory/routine-continuation.schema.json"
DEVELOPMENTAL_SCHEMA = ROOT / "contracts/factory/developmental-read.schema.json"
COMMISSION_SCHEMA = ROOT / "contracts/factory/commission.schema.json"
COMMISSION_REQUEST_SCHEMA = ROOT / "contracts/factory/commission-request.schema.json"

upstream_bytes = UPSTREAM.read_bytes()
upstream = json.loads(upstream_bytes)
lock = json.loads(LOCK.read_text())
assert hashlib.sha256(upstream_bytes).hexdigest() == lock["schema_sha256"]
assert lock["revision"] == "7864b3c700b26e90e7b814295c9a2675aae8dbf7"
assert lock["standing"] == "accepted-owner-main"
Draft202012Validator.check_schema(upstream)
owner_errors = list(Draft202012Validator(upstream).iter_errors(json.loads(OWNER_FIXTURE.read_text())))
assert not owner_errors, "; ".join(error.message for error in owner_errors)

request_schema = json.loads(REQUEST_SCHEMA.read_text())
Draft202012Validator.check_schema(request_schema)
registry = Registry().with_resource(
    "https://github.com/EpiLogos/agent-system-design/contracts/factory/upstream/aikit.routine-invocation-evidence.v1.schema.json",
    Resource.from_contents(upstream),
)
registry = registry.with_resource(
    "https://github.com/EpiLogos/agent-system-design/contracts/factory/commission.schema.json",
    Resource.from_contents(json.loads(COMMISSION_SCHEMA.read_text())),
).with_resource(
    "https://github.com/EpiLogos/agent-system-design/contracts/factory/commission-request.schema.json",
    Resource.from_contents(json.loads(COMMISSION_REQUEST_SCHEMA.read_text())),
)
errors = list(
    Draft202012Validator(request_schema, registry=registry).iter_errors(
        json.loads(REQUEST_FIXTURE.read_text())
    )
)
assert not errors, "; ".join(error.message for error in errors)

output_schema = json.loads(OUTPUT_SCHEMA.read_text())
Draft202012Validator.check_schema(output_schema)
output_validator = Draft202012Validator(output_schema, registry=registry)
developmental_schema = json.loads(DEVELOPMENTAL_SCHEMA.read_text())
Draft202012Validator.check_schema(developmental_schema)
developmental_validator = Draft202012Validator(developmental_schema, registry=registry)
binary = ROOT / "target/debug/factory"
assert binary.is_file(), "build the native Factory binary before validating output contracts"
with tempfile.TemporaryDirectory() as directory:
    state = Path(directory) / "state.json"
    generated = subprocess.run(
        [binary, "conformance", "developmental-state", state, "--json"],
        check=True,
        capture_output=True,
        text=True,
    )
    manifest = json.loads(generated.stdout)
    errors = list(output_validator.iter_errors(manifest))
    assert not errors, "; ".join(error.message for error in errors)
    for operation, field in (
        ("project", "projectRef"),
        ("journey", "journeyRef"),
        ("run", "runRef"),
        ("workflow-unit", "workflowUnitRef"),
        ("execution-telemetry", "telemetryRef"),
    ):
        result = subprocess.run(
            [binary, "development", operation, state, manifest[field], "--json"],
            check=True,
            capture_output=True,
            text=True,
        )
        errors = list(developmental_validator.iter_errors(json.loads(result.stdout)))
        assert not errors, f"{operation}: " + "; ".join(error.message for error in errors)
    invocation_ref = manifest["invocationRef"]
    reading = subprocess.run(
        [binary, "development", "routine-continuation", state, invocation_ref, "--json"],
        check=True,
        capture_output=True,
        text=True,
    )
    errors = list(output_validator.iter_errors(json.loads(reading.stdout)))
    assert not errors, "; ".join(error.message for error in errors)
    admission = subprocess.run(
        [binary, "development", "admit-routine-continuation", state, REQUEST_FIXTURE, "--json"],
        check=True,
        capture_output=True,
        text=True,
    )
    errors = list(output_validator.iter_errors(json.loads(admission.stdout)))
    assert not errors, "; ".join(error.message for error in errors)
print("Factory Routine continuation schemas/fixtures: PASS")
