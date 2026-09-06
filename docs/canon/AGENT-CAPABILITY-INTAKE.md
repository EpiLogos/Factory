# Agent Capability Intake Law

Governs how the Factory receives composed agent capability (an Agent profile with attached SkillSets) into Commission/Run inputs. Implemented by `factory/src/agent_capability_intake.rs`; witnessed by `factory/tests/agent_capability_intake.rs`; bound to pinned owner revisions by `scripts/validate_agent_capability_intake.py` and `contracts/factory/fixtures/intake/`.

## The law

1. **Stable refs, not copies.** A Factory Commission/Run accepts composed agent capability only as typed stable refs — an authored Central AgentProfile ref plus an AIKit-resolved effective SkillSet-composition ref. No config blob, no Factory-local transcription of profile or resolution bodies. `AgentCapabilityRef` has no field in which a copy could be placed.

2. **Ref grammar follows the owner surfaces.**
   - Authored source: `central.agent-profile/<scope>/<profile-ref>@<revision-or-latest>` — `<scope>` is the Central `agent-profile.list`/`read` store selector (`personal`/`project`), `<profile-ref>` is the exact Central store key, `<revision>` is the exact Central revision (or `latest`, which binds to whatever revision the recorded authored evidence resolved to at intake).
   - Effective composition: `aikit.skillset/<set-name>@<generation-id>` — `<set-name>` is the exact `aikit set show <NAME>` identity; `<generation-id>` is the exact AIKit content-addressed `GenerationId` (`gen_…`).

3. **Three-way independent readability.** Authored source (Central) → effective composition (AIKit) → developmental use (Factory Run) is a W12-style witness relation: the same refs must resolve to the same authored evidence and the same effective evidence on every side, each side independently readable. The Factory proof runs on recorded fixture envelopes captured by executing the pinned owner revisions — never live calls in CI.

4. **Failure honesty (K2).** Unresolved (`not-in-catalog`), retired (`retired-standing` with the owner's reason) and otherwise withheld SkillSet members surface as explicit intake facts, in the resolver's own vocabulary and words. No member is silently dropped; the intake's member accounting must equal the composition's own reply.

5. **Binding to use.** An intake binds to a Run that already belongs to the commissioned Journey (`JourneyCommissionState::set_capability_intake`). Capability refs bind to developmental use, not to a floating intention.

6. **Pinned conformance, never live heads.** The conformance lane binds to the Central and ai-kit revisions pinned from O:I `suite/mainline.json` (`contracts/factory/fixtures/intake/conformance-pins.json`, with `observed_at`), and fails loudly on pin drift with re-record instructions.

## Legacy / deprecation

- The SSSF-era `AgentConfig`/`SSSFConfig` Pydantic models inside the research trees (`super-simple-software-factory/`, `inkwell-agent-sandboxes-and-software-factory/`) are legacy prior art that copy agent configuration locally. They are not the Factory implementation root and must never be recovered as a parallel agent-configuration truth.
- The loose per-field capability refs on `PraxisCondition` (`profile_ref`, `skill_refs`, `skill_set_refs`, …) remain readable for historical Run records, but new composed-agent-capability intake goes through the typed stable-refs contract above. No Factory-local agent-configuration format remains a parallel truth for composed capability.
