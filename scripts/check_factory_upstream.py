#!/usr/bin/env python3
"""Read-only Builder.io Factory source and adaptation drift check."""
from __future__ import annotations

import argparse
import difflib
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "skills/factory/provenance/upstream.json"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(upstream: Path, remote: bool) -> dict:
    manifest = json.loads(MANIFEST.read_text())
    actual = subprocess.run(["git", "-C", str(upstream), "rev-parse", "HEAD"],
                            capture_output=True, text=True, check=True).stdout.strip()
    if actual != manifest["revision"]:
        raise ValueError(f"upstream checkout is {actual}; recorded revision is {manifest['revision']}")
    checked = []
    differences = []
    for row in manifest["source_documents"]:
        path = upstream / row["path"]
        if digest(path) != row["sha256"]:
            raise ValueError(f"upstream document changed: {path}")
        checked.append(row["path"])
    for row in manifest["skills"]:
        source = upstream / row["upstream_path"]
        target = ROOT / "skills" / row["name"] / "SKILL.md"
        license_copy = target.parent / "provenance/LICENSE.builderio"
        if digest(source) != row["upstream_sha256"]:
            raise ValueError(f"upstream skill changed: {source}")
        if digest(target) != row["adapted_sha256"]:
            raise ValueError(f"adapted skill changed without updating provenance: {target}")
        if digest(license_copy) != manifest["license_sha256"]:
            raise ValueError(f"missing or changed MIT text: {license_copy}")
        world = target.parent / "references/world.md"
        if digest(world) != row["world_reference_sha256"]:
            raise ValueError(f"world binding changed without updating provenance: {world}")
        differences.extend(difflib.unified_diff(
            source.read_text().splitlines(keepends=True),
            target.read_text().splitlines(keepends=True),
            fromfile=f"BuilderIO/skills/{row['upstream_path']}",
            tofile=f"Factory/skills/{row['name']}/SKILL.md"))
        checked.append(row["upstream_path"])
    diff_path = ROOT / manifest["upstream_diff_path"]
    if diff_path.read_text() != "".join(differences) or digest(diff_path) != manifest["upstream_diff_sha256"]:
        raise ValueError("recorded exact upstream difference is stale")
    head = None
    if remote:
        result = subprocess.run(["git", "-C", str(upstream), "ls-remote", "origin", "HEAD"],
                                capture_output=True, text=True, check=True, timeout=30)
        head = result.stdout.split()[0]
    return {"result": "passed", "recorded_revision": actual,
            "remote_head": head, "remote_has_moved": head is not None and head != actual,
            "checked_paths": checked, "adapted_skill_count": len(manifest["skills"]),
            "scope": "Read-only upstream/adaptation byte parity; does not establish operational fitness."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--upstream", type=Path, required=True)
    parser.add_argument("--check-remote", action="store_true")
    args = parser.parse_args()
    print(json.dumps(check(args.upstream, args.check_remote), indent=2))
