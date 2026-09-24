#!/usr/bin/env python3
"""Exercise all Builder-derived Factory skills through the real isolated AIKit source lifecycle."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
NAMES = ("factory", "factory-collect", "factory-lookback", "factory-human-digest",
         "factory-review-prs", "factory-babysit-pr", "factory-ship", "factory-watchdog",
         "factory-recover", "agent-watchdog")
EXISTING = ("factory-development", "factory-operation", "factory-bounded-work")
IDS = [f"skill/factory-native/{name}" for name in NAMES]


def verify(binary: str) -> dict:
    executable = Path(shutil.which(binary) or binary).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="factory-sensing-skill-proof-") as directory:
        root = Path(directory)
        source = root / "source"
        work = root / "work"
        work.mkdir()
        for name in (*EXISTING, *NAMES):
            shutil.copytree(ROOT / "skills" / name, source / name)
        env = dict(os.environ, AIKIT_HOME=str(root / "aikit"), AIKIT_ISOLATION="1")
        env.pop("AIKIT_CONTEXT_ID", None)
        env.pop("AIKIT_SESSION_ID", None)

        def run(*args: str) -> dict:
            process = subprocess.run([str(executable), "--json", *args], cwd=work,
                                     env=env, capture_output=True, text=True, timeout=60)
            if process.returncode:
                raise RuntimeError(f"aikit {args}: {process.stdout}\n{process.stderr}")
            envelope = json.loads(process.stdout)
            if not envelope["ok"]:
                raise RuntimeError(f"aikit {args}: {envelope}")
            return envelope["data"]

        run("source", "add-directory", "factory-native", str(source))
        candidate = run("source", "sync", "factory-native")
        assert candidate["skills"] == len(EXISTING) + len(NAMES) and candidate["active_snapshot"] is None
        run("source", "promote", "factory-native", "--trust")
        active = run("source", "show", "factory-native")
        registry = Path(active["active_registry"])
        registry.relative_to(root)
        for name in (*EXISTING, *NAMES):
            payload = registry / "capsules/skill/factory-native" / name / "payload"
            for original in (source / name).rglob("*"):
                if original.is_file():
                    copy = payload / original.relative_to(source / name)
                    assert copy.read_bytes() == original.read_bytes(), copy
        run("set", "create", "factory-sensing", *IDS)
        before = run("set", "show", "factory-sensing")
        assert not before["complete"] and len(before["withheld"]) == len(NAMES)
        for identity in IDS:
            run("enable", identity, "--scope", "global")
        after = run("set", "show", "factory-sensing")
        assert after["complete"] and set(after["projected"]) == set(IDS)
        discovered = run("search", "factory-lookback")
        assert IDS[2] in json.dumps(discovered), discovered
        # A source change remains a candidate until promotion. Even a promoted
        # missing member must refuse or disclose incomplete SkillSet selection.
        old = registry / "capsules/skill/factory-native/factory-collect/payload/SKILL.md"
        old_bytes = old.read_bytes()
        shutil.rmtree(source / "factory-collect")
        changed = run("source", "sync", "factory-native")
        assert changed["candidate_snapshot"] != active["active_snapshot"]
        assert changed["active_snapshot"] == active["active_snapshot"]
        assert old.read_bytes() == old_bytes
        run("source", "promote", "factory-native", "--trust")
        denied = subprocess.run([str(executable), "--json", "set", "show", "factory-sensing"],
                                cwd=work, env=env, capture_output=True, text=True, timeout=60)
        envelope = json.loads(denied.stdout)
        if denied.returncode:
            assert not envelope["ok"] and IDS[1] in json.dumps(envelope), envelope
        else:
            selection = envelope["data"]
            assert not selection["complete"] and any(
                row["capability"] == IDS[1] for row in selection["withheld"]), selection
        run("source", "rollback", "factory-native")
        restored = run("set", "show", "factory-sensing")
        assert restored["complete"] and set(restored["projected"]) == set(IDS)
        return {"result": "passed", "aikit": str(executable),
                "binary_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
                "snapshot": active["active_snapshot"], "skill_refs": IDS,
                "payload_and_license_byte_parity": True,
                "selection_distinct_from_membership": True,
                "native_search_discovery": True,
                "missing_member_refusal": True, "snapshot_immutable_until_promotion": True,
                "rollback_restores_selection": True,
                "scope": "Real isolated AIKit source/SkillSet lifecycle; not live installation or harness loading."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--aikit", default="aikit")
    args = parser.parse_args()
    print(json.dumps(verify(args.aikit), indent=2))
