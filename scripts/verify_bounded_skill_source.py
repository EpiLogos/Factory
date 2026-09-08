#!/usr/bin/env python3
"""Exercise the real AIKit source lifecycle on Factory's actual Skill package.

No global registry is used. This proves packaging, resolution and immutable
source behavior; model/harness fitness and authority need separate acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
NAMES = ("factory-development", "factory-operation", "factory-bounded-work")
IDS = [f"skill/factory-native/{name}" for name in NAMES]


def verify(binary: str) -> dict:
    executable = Path(shutil.which(binary) or binary).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="factory-skill-proof-") as directory:
        root = Path(directory)
        source = root / "source"
        cwd = root / "work"
        cwd.mkdir()
        for name in NAMES:
            shutil.copytree(ROOT / "skills" / name, source / name)
        env = dict(os.environ, AIKIT_HOME=str(root / "aikit"), AIKIT_ISOLATION="1")
        # A caller's context/session belongs to its world, not this test ground.
        env.pop("AIKIT_CONTEXT_ID", None)
        env.pop("AIKIT_SESSION_ID", None)

        def run(*args: str) -> dict:
            result = subprocess.run(
                [str(executable), "--json", *args], cwd=cwd, env=env,
                text=True, capture_output=True, timeout=60,
            )
            if result.returncode:
                raise RuntimeError(f"AIKit {args} failed: {result.stderr}")
            envelope = json.loads(result.stdout)
            assert envelope["ok"], envelope
            return envelope["data"]

        run("source", "add-directory", "factory-native", str(source))
        candidate = run("source", "sync", "factory-native")
        assert candidate["skills"] == 3 and candidate["active_snapshot"] is None
        run("source", "promote", "factory-native", "--trust")
        active = run("source", "show", "factory-native")
        registry = Path(active["active_registry"])
        registry.relative_to(root)
        for name in NAMES:
            payload = registry / "capsules/skill/factory-native" / name / "payload"
            for original in (source / name).rglob("*"):
                if original.is_file():
                    copied = payload / original.relative_to(source / name)
                    assert copied.read_bytes() == original.read_bytes(), copied

        run("set", "create", "factory-bounded-development", *IDS)
        unselected = run("set", "show", "factory-bounded-development")
        assert not unselected["complete"] and len(unselected["withheld"]) == 3
        for identity in IDS:
            run("enable", identity, "--scope", "global")
        selected = run("set", "show", "factory-bounded-development")
        assert selected["complete"] and set(selected["projected"]) == set(IDS)

        # Remove a real required member in the disposable source. Synchronizing
        # must not rewrite the active snapshot or pretend the new one is active.
        old = registry / "capsules/skill/factory-native/factory-operation/payload/SKILL.md"
        original_bytes = old.read_bytes()
        shutil.rmtree(source / "factory-operation")
        changed = run("source", "sync", "factory-native")
        assert changed["candidate_snapshot"] != active["active_snapshot"]
        assert changed["active_snapshot"] == active["active_snapshot"]
        assert old.read_bytes() == original_bytes
        run("source", "promote", "factory-native", "--trust")
        refused = subprocess.run(
            [str(executable), "--json", "set", "show", "factory-bounded-development"],
            cwd=cwd, env=env, text=True, capture_output=True, timeout=60,
        )
        # The native resolver may reject the enabled missing capsule before
        # it can construct a set reading. That is an honest refusal, not a
        # successful empty/partial composition.
        if refused.returncode:
            failure = json.loads(refused.stdout)
            assert failure["ok"] is False and IDS[1] in json.dumps(failure), failure
        else:
            incomplete = json.loads(refused.stdout)["data"]
            assert not incomplete["complete"]
            assert any(row["capability"] == IDS[1] for row in incomplete["withheld"])
        run("source", "rollback", "factory-native")
        restored = run("set", "show", "factory-bounded-development")
        assert restored["complete"] and set(restored["projected"]) == set(IDS)
        return {
            "result": "passed",
            "binary": str(executable),
            "binary_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            "snapshot": active["active_snapshot"],
            "members": IDS,
            "payload_and_license_byte_parity": True,
            "selection_distinct_from_membership": True,
            "missing_member_disclosed": True,
            "active_snapshot_immutable": True,
            "rollback_restores_resolution": True,
            "scope": "Actual isolated AIKit source/set lifecycle. Not harness loading, Method admission, runtime authority or human acceptance.",
        }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--aikit", default="aikit")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = json.dumps(verify(args.aikit), indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(report)
    print(report, end="")
