#!/usr/bin/env python3
"""Apply one actual ACP candidate with native Workcell and adverse basis checks.

Consumes verify_acp_bounded_work.py evidence. Never fabricates a candidate and
never repeats an already applied repair. Only its exclusive temporary commission
is withheld/changed for negative checks; original bytes are restored each time.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


def verify(args):
    output = args.evidence.resolve()
    acceptance = json.loads((output / "acceptance.json").read_text())
    assert acceptance["result"] == "passed"
    fixture = Path(acceptance["fixture_root"]).resolve(strict=True)
    candidate = Path(acceptance["candidate_json"]).resolve(strict=True)
    context = output / "required-context.json"
    digest = hashlib.sha256(context.read_bytes()).hexdigest()
    assert digest == acceptance["required_context_sha256"]
    sources = json.loads(context.read_text())["sources"]
    commission = Path(sources[0]["path"]).resolve(strict=True)
    commission.relative_to(fixture)
    original = commission.read_bytes()
    artifact = fixture / "pricing.py"
    before = artifact.read_bytes()
    proof = fixture / "workcell-proof.json"
    assert not proof.exists(), "Already applied: do not replay the material operation"
    command = [str(args.helper.resolve(strict=True)),
               "--fixture-root", str(fixture), "--candidate-json", str(candidate),
               "--authority-ref", acceptance["authority_ref"],
               "--python", str(Path(sys.executable).absolute()),
               "--context-json", str(context), "--context-sha256", digest]
    refusals = {}

    def refused(name, argv):
        result = subprocess.run(argv, capture_output=True, text=True, timeout=20)
        assert result.returncode != 0 and not proof.exists(), result.stdout
        assert artifact.read_bytes() == before, "Refused preflight changed artifact"
        refusals[name] = {"exit_code": result.returncode, "error": result.stderr.strip(),
                          "artifact_unchanged": True}

    refused("changed_manifest", [*command[:-1], "0" * 64])
    withheld = commission.with_name(commission.name + ".withheld-for-acceptance")
    assert not withheld.exists()
    commission.rename(withheld)
    try:
        refused("missing_required_source", command)
    finally:
        withheld.rename(commission)
    try:
        commission.write_bytes(original + b"\nChanged after ACP completion.\n")
        refused("changed_required_source", command)
    finally:
        commission.write_bytes(original)
    # A copied receipt cannot substitute for the mutable owner source. Exercise
    # the real primary source pins as well as the commission in this fixture.
    for source in sources:
        primary = Path(source["path"]).resolve(strict=True)
        if not primary.is_relative_to(fixture):
            continue
        if "profiles" not in primary.parts and primary.name not in ("actuation-model-bearing.json", "project.json"):
            continue
        source_bytes = primary.read_bytes()
        try:
            primary.write_bytes(source_bytes + b"\n")
            refused("changed_owner_source:" + str(primary.relative_to(fixture)), command)
        finally:
            primary.write_bytes(source_bytes)
    result = subprocess.run(command, capture_output=True, text=True, timeout=30)
    (output / "material-run.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(result.stderr)
    receipt = json.loads(proof.read_text())
    assert receipt["ok"] and receipt["effect_context"]["manifest_sha256"] == digest
    for key in ("denied_before_grant", "denied_expired", "denied_revoked", "denied_altered", "denied_replay"):
        assert receipt[key]
    shutil.copyfile(proof, output / "workcell-proof.json")
    report = {"result": "passed", "refusals": refusals,
              "material_receipt": str(output / "workcell-proof.json"),
              "helper_sha256": hashlib.sha256(args.helper.read_bytes()).hexdigest(),
              "scope": "Actual source preflight refusals and single fixed Workcell repair; no atomic source lock or durable grant claim."}
    (output / "material-acceptance.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--helper", type=Path, required=True)
    verify(parser.parse_args())
