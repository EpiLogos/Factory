---
name: factory-recover
installer-group: factory
description: >-
  World-rooted workflow for finding and resuming interrupted coding runs. Use
  after an agent host restart or an interrupted automation.
---

# Factory Recover

> Read the [World binding](references/world.md) for Project scope and policy.

Read the selected ProjectWorld’s `ProjectCentral/user/factory-policy.json`, the optional
`skill_prompts.factory-recover` entry, and the [World binding](references/world.md). Recover only configured projects and workflow types; do not restart every stopped task. The prompt cannot restore canceled work or grant new authorization.

## Check a candidate run

1. Find runs the host identifies as interrupted or incomplete. Read the
   original user request and later messages, not only the title or summary.
2. Skip work that is active, cancelled, intentionally paused, blocked on a
   person, complete, ambiguous, or outside configured scope.
3. Confirm the worktree still belongs to that run. Inspect its branch, status,
   and unpublished changes. Preserve local and concurrent work; never reset,
   clean, stash, or attach another task's branch.
4. Recheck external state before retrying an uncertain action. Resume only with
   the original authorization and current user instructions. Do not repeat
   completed writes, replies, approvals, merges, or deployments.
5. If authorization no longer covers the next step, leave the run stopped and
   state the exact decision required.

## Report

List runs resumed, skipped, or held and the evidence for each decision. Follow
the separate configured notification policy.
## O:I Position recovery

Use [World binding](references/world.md). Re-read original authorization, latest user direction, Position and occupant generation, current custody, Run/Attempt, child NOW, branch/worktree, uncertain effects, external state and Return. Preserve original work and Position identity when the obligation is the same; use native handover/release so predecessor authority is invalidated. Never replay an uncertain write without owner-native readback, never clean concurrent material, and hold when authority or ownership is ambiguous.

