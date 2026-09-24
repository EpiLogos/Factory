---
name: factory-review-prs
installer-group: factory
description: >-
  World-rooted workflow for reviewing configured repositories' pull requests.
  Use for manual or scheduled PR triage, approval, or merge decisions.
---

# Factory Review PRs

> Read the [World binding](references/world.md) for Project scope and policy.

This skill reviews a filtered queue. For one long-lived, explicitly authorized
PR, use `factory-babysit-pr`. Read the filters and independent action policies
from `ProjectCentral/user/factory-policy.json`; see the [World binding](references/world.md).
Apply the optional `skill_prompts.factory-review-prs` entry as additional
project guidance; it does not replace this skill or authorize an action
disabled by policy.

## Review the queue

For each candidate PR:

1. Read live state from the configured host. Exclude drafts and PRs outside the
   configured filters. Skip a PR with a current review unless re-review is
   requested by policy.
2. Inspect the diff, linked issues, required checks, review threads, author
   eligibility, and exact head revision. Treat bot findings as leads and
   preserve human review direction unless source evidence disproves it.
3. Report actionable findings with file, line, impact, and a concrete fix. If
   none exist, record that outcome without inventing a comment.

Unavailable or partial state is unknown, never a clean result.

## Apply separate action gates

| Action | Proceed only when |
| --- | --- |
| Review | The PR matches configured filters and has not already had the required current review. |
| Reply or other PR write | That action is enabled and its conditions hold. Use configured wording; do not tag, assign, or message otherwise. |
| Approve | Approval is enabled and author, risk, ownership, current-head, check, and review conditions are verified. |
| Merge | Merge is separately enabled; every live merge condition and any soak hold on the unchanged head. |

Host-level mergeability, one green check, or a bot approval does not prove all
gates passed. Re-read the exact head and live state immediately before an
approval or merge. Restart a configured soak if the head or a gate changes.

## Report

For each PR, state the decision and evidence. List skipped, unavailable, and
held PRs with the reason. Keep review findings separate from approvals, replies,
and merge decisions.
## O:I review relation

Use [World binding](references/world.md). Resolve the PR to its ProjectWorld, originating signal and Factory work/Run/Return where present. Read full live diff, required checks, unresolved threads, reviews, owner, mergeability and exact head from the connected code host. Re-read after a head change and before any review write, approval, or merge. Keep each action's policy and actual host/Actuation authority separate; a review finding is not approval and approval is not merge.

