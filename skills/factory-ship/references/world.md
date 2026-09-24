# Factory World binding

This reference binds the Builder.io Factory practice to the installed O:I owners. The source practice is retained in the entrypoint; this reference supplies owner and evidence rules.

## Policy and authority

For a situated ProjectWorld, read the authored `ProjectCentral/user/factory-policy.json` within that Project root. Its native schema is `factory.sensing-policy/v1`, `version` is `1`, and `project_world_ref` is `control:root` or the exact `project:<Central ID>` of the selected World. It retains Builder’s `sources`, `repositories`, `workflows`, and `skill_prompts` organization. Each source needs a real `source_ref`, provider and scope. Resolve that ref and the Project root before any source query. Never search a shared host for a policy by basename. If only `.agent-factory/config.yaml` exists, treat it as legacy human input for a reviewed conversion that accounts for every explicit choice. The current native JSON schema rejects unknown fields, so retain unsupported Builder fields as named migration holds instead of silently dropping them or claiming that the native reader parses YAML. Missing, stale, unreadable, or conflicting policy means hold the dependent action. Do not write credentials into policy. An overlay adds guidance and cannot override current instructions, native grants, or a separate action gate.

Policy describes allowed scope and desired cadence; it grants no Agency, Factory mutation, provider, GitHub, or host authority. Resolve the actual Central ProjectWorld, Actuation Position/occupant generation, Factory Run/WorkflowUnit/Attempt and work custody, and current user commission before taking an effect. A human-authored Day and a machine-derived Day telemetry reading remain distinct. NOW is a live temporal index; owner-native source refs and revisions remain canonical. Use Project scope in every shared AIKit, Redis, code-index, or provider query. A shared process does not authorize cross-Project retrieval.

## Current native reads

The existing `factory telemetry status|inspect|search|stats|watch|compare|export|doctor` family reads Factory developmental evidence from its state. `factory development custody list` and `factory development current-work` expose actual custody and Position work. Confirm installed CLI help and result shape before invocation. New `factory telemetry collect`, `signals|signal`, `digest|lookback|day|field` commands must be used only when the installed build advertises them; never substitute a made-up command. Central owns Project NOW/DAY and policy source; AIKit owns ContextSource/provider reads, Skill/SkillSet resolution and Routines; Actuation owns Position/authority/Return; Workcell owns material placement.

## Evidence and action gates

For each configured source, preserve provider identity, exact Project scope, query window/cursor, pagination coverage, source ref/link, occurrence and observation times, and useful version/environment. `empty`, `unavailable`, and `truncated` are distinct; event counts and affected people/sessions require separate denominators. A source read may establish a signal, not a verified defect or permission to fix. Keep implementation, reply, close, review, approval, publish, merge, deploy, recover, and notify independent. Re-read owner-native live state before consequential action; bind PR checks/reviews to the exact head. Factory custody carries authorised developmental work to the correct World Position and child NOW, with signal ancestry retained through Return. A green check, merge, or deployment alone is not live-resolution proof.

## Gotchas

- Redis field or Agent summary can be stale; re-read the native owner before writing.
- A vacant Position or quiet thread is not evidence that work may be stolen.
- Source access shared across projects must still be restricted by ProjectWorld scope.
- Do not use a successful `telemetry stats --day` as a ship gate until its returned Day window, policy revision, and source coverage match the requested basis. The 2026-09-24 installed CLI returned whole-provider data; native regression: `factory/src/telemetry_cli.rs` `public_cli_day_stats_exclude_untimed_native_records`.
