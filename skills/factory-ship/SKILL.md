---
name: factory-ship
installer-group: factory
description: >-
  World-rooted workflow for publishing and completing configured software
  delivery work. Use when the user or an enabled factory policy asks to ship.
---

# Factory Ship

> Read the [World binding](references/world.md) for Project scope and policy.

Read the selected ProjectWorld’s `ProjectCentral/user/factory-policy.json`, the optional
`skill_prompts.factory-ship` entry, and the repository's instructions before
publishing. See the [World binding](references/world.md). The prompt adds project guidance; editing code does not by itself authorize a PR update, approval, merge, or deployment.

## Delivery steps

1. **Confirm ownership.** Identify the target repository, task-owned worktree,
   branch, and complete task-related change set. Preserve unrelated changes.
   Use a clean automation-owned worktree when the scheduler provides one; never
   take over a peer's checkout.
2. **Verify changes.** Run the configured formatter, tests, and release checks.
   Report checks that were skipped or unavailable.
3. **Publish when enabled.** Open or update a PR only when authorized. Apply
   configured title, body, draft status, labels, and communication rules. Do
   not tag or message people unless enabled.
4. **Resolve feedback.** Compare comments with the code and current human
   direction. Make one coherent update, then rerun affected checks.
5. **Apply merge gates.** Merge only under its own enabled policy and while all
   criteria hold on the unchanged live head. Use a head-match guard when the
   host supports it. A head change invalidates tied evidence and resets the
   soak.
6. **Verify and close out.** Confirm the merge in the current base branch.
   Verify deployment only when configured; a merge is not live proof. Close
   issues, rotate worktrees, and notify only at their configured proof points.

## Stop conditions

Hold the work when ownership, authorization, a required check, or live host
state is missing or unclear. State the exact next step; do not turn an
inconclusive result into success.
## O:I delivery proof

Use [World binding](references/world.md). Bind delivery to the actual Factory Claim/VerificationPlan/Evidence/Closure and Return for the commissioned whole. Record changed code, local checks, published PR/head, merge commit/base, installed or deployed revision, and live behavior as separate facts with native refs. Apply issue closure and notification only at their own configured proof points. A passing subset does not establish whole-relative closure, and merge does not establish installed state or live resolution.

**Observed Gotcha (2026-09-24).** A successful installed `factory telemetry stats --day 2026-09-24 --json` once returned whole-provider state rather than the requested Day. Before citing Day telemetry as a ship, soak, or closure gate, check its returned window, exact policy revision and source coverage against the gate's requested basis. A zero exit code or accepted flag does not establish bounded evidence; hold the dependent gate and obtain a correctly bounded native reading if any basis is absent.
