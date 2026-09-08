#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OPERATOR = ROOT / "skills/factory-operation/SKILL.md"
DEVELOPER = ROOT / "skills/factory-development/SKILL.md"
BOUNDED = ROOT / "skills/factory-bounded-work/SKILL.md"
BOUNDED_COORDINATION = ROOT / "skills/factory-bounded-work/references/coordination.md"
FACTORY_PROFILE = ROOT / "agents/factory-mode/profile.json"
PRODUCT = ROOT / "skills/fixtures/product-improvement.json"
SKILL = ROOT / "skills/fixtures/skill-revision.json"


def require_text(path: Path, needles: list[str]) -> str:
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        raise SystemExit(f"{path}: missing Agent Skill frontmatter")
    for needle in needles:
        if needle not in text:
            raise SystemExit(f"{path}: missing {needle!r}")
    return text


require_text(
    OPERATOR,
    [
        "factory:operator",
        "FactoryBuildViewProvider::snapshot",
        "FactoryActionExecutor::execute",
        "RunMutationAuthority",
        "Capability granted != Action authorised",
        "factory.build-view/v1",
        "plausible Claim != evidenced Closure",
        "partial evidence != whole satisfaction",
        "Whole-relative verification / plausibility barrier",
        "Claim -> operative Whole/opening condition -> VerificationRequirement/VerificationPlan -> Evidence/Assessment -> Closure/Gate",
    ],
)
require_text(
    DEVELOPER,
    [
        "factory:developer",
        "native-owner review / Recognition",
        "edited harness projection",
        "automatic backlog",
        "Proof over plausibility",
        "whole-relative verification ledger",
        "Discharge verification obligations",
        "A passing subset remains a passing subset.",
    ],
)
bounded_text = require_text(
    BOUNDED,
    [
        'description: "METHOD:',
        "skill/factory-native/factory-bounded-work",
        "There is no second Method source",
        "UsageOverlay",
        "method list",
    ],
)
coordination_text = BOUNDED_COORDINATION.read_text(encoding="utf-8")
coordination_normalized = " ".join(coordination_text.split())
for law in (
    "factory.bounded-coordination/v1",
    "Semantic | Live | Trajectory",
    "Fork != automatic parallelism",
    "one writer per shared mutable subject",
    "barrier",
    "synthesis",
    "cancellation",
    "Late child output",
    "retry",
    "Process liveness != execution ownership",
    "Restart != replenished authority",
):
    if law not in coordination_normalized:
        raise SystemExit(f"{BOUNDED_COORDINATION}: missing coordination law {law!r}")

profile = json.loads(FACTORY_PROFILE.read_text(encoding="utf-8"))
assert profile["schema"] == "central.agent-profile/v1"
assert profile["skill_refs"][-1] == "skill/factory-native/factory-bounded-work"
assert profile["skill_set_refs"], "Factory profile must retain situated SkillSet projection"
assert "method_refs" not in profile
assert all("method.json" not in reference for reference in profile["provenance_refs"])
assert not (ROOT / "skills/factory-bounded-work/method.json").exists()

product = json.loads(PRODUCT.read_text(encoding="utf-8"))
assert product["schema"] == "factory.skill-workflow/v1"
assert product["kind"] == "product-improvement"
assert product["ownerSkillRef"] == "workcell:operator"
assert product["directForeignPrivateMutation"] is False
assert product["factoryRunImpliesRepositoryAuthority"] is False
assert product["automaticPromotion"] is False
assert product["stages"][-2:] == ["owner-recognition", "attributable-return"]

skill = json.loads(SKILL.read_text(encoding="utf-8"))
assert skill["schema"] == "factory.skill-workflow/v1"
assert skill["kind"] == "skill-revision"
assert skill["source"]["revision"]
assert skill["projectedCopyAuthoritative"] is False
assert skill["successfulUsePromotesAutomatically"] is False
assert skill["rollbackPointRequired"] is True
assert skill["stages"][-2:] == ["native-owner-recognition", "explicit-promotion-or-rejection"]

print("Factory native Skills and self-improvement fixtures: OK")
