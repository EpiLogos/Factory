# Factory capability and relation matrix

Standing: Agent-recovered inference · draft-for-review · 6 September 2026.

This companion connects the [Factory account](factory.html#whole) to purpose, native operations and inspectable evidence. The default view relates its six seed questions to the O:I whole and peer products.

The 18 capability families below are functional descriptions with stable document identities, not an Action registry. The native CLI integration suite was executed on 6 September 2026; its narrow results are called out only for the command, Build-reading and Action-projection capabilities. Historical issue references retain their original repository name, agent-system-design.

## Native CLI command catalogue

Machine discovery runs `target/debug/factory capabilities --json`; its `commands` array is the maintained inventory. The executable reports these exact command IDs. The legacy `factory-action-headless` path is intentionally absent because it is not emitted by this inventory.

<!-- cli-catalog:start -->
| CLI identity | Capability |
| --- | --- |
| `action.invoke` | [cap.factory.action-projection](#cap-factory-action-projection) · [cap.factory.native-cli](#cap-factory-native-cli) |
| `action.list` | [cap.factory.action-projection](#cap-factory-action-projection) · [cap.factory.build-reading](#cap-factory-build-reading) · [cap.factory.native-cli](#cap-factory-native-cli) |
| `build.refresh` | [cap.factory.build-reading](#cap-factory-build-reading) · [cap.factory.native-cli](#cap-factory-native-cli) |
| `build.snapshot` | [cap.factory.build-reading](#cap-factory-build-reading) · [cap.factory.native-cli](#cap-factory-native-cli) |
| `verify` | [cap.factory.native-cli](#cap-factory-native-cli) |
<!-- cli-catalog:end -->

`cargo test --test cli --test native_cli` passed all seven native CLI integration tests on 2026-09-06. They create persisted state, invoke the executable, verify snapshot/refresh/listing, receipt persistence and native failure semantics. This does not establish owner-world or desktop acceptance.


## Seed × field contribution

[View declarations](capability-matrix.json) · [Editable CSV](capability-matrix.csv). Select a populated cell for its source and capability links. Unassessed cells carry no assertion.

| Seed | O:I whole | Central | Actuation | AIKit | Workcell | Quaternal Logic |
| --- | --- | --- | --- | --- | --- | --- |
| [Why?](factory.html#whole/why) | [1 capabilities](#field-q0-S) | Unassessed | Unassessed | Unassessed | Unassessed | Unassessed |
| [What?](factory.html#whole/what) | [5 capabilities](#field-q1-S) | Unassessed | Unassessed | Unassessed | Unassessed | Unassessed |
| [How?](factory.html#whole/how) | [6 capabilities](#field-q2-S) | Unassessed | Unassessed | [3 capabilities](#field-q2-S2) | Unassessed | Unassessed |
| [Who / Whereby?](factory.html#whole/whereby) | [4 capabilities](#field-q3-S) | Unassessed | [1 capabilities](#field-q3-S1) | Unassessed | [1 capabilities](#field-q3-S4) | [1 capabilities](#field-q3-S5) |
| [Where / When?](factory.html#whole/context) | [2 capabilities](#field-q4-S) | [1 capabilities](#field-q4-S0) | Unassessed | Unassessed | Unassessed | Unassessed |
| [Why-For?](factory.html#whole/purpose) | [relation](#field-q5-S) | Unassessed | Unassessed | Unassessed | Unassessed | Unassessed |

<details>
<summary>Read the field contributions and their capability links</summary>

<a id="field-q0-S"></a>

### Why? → O:I whole

Agentic development can produce working changes while losing the purpose and experience that made them worth asking for. Factory exists to carry human intention through development and returned reality, freeing the person from repeated orientation and mechanical supervision while preserving consequential judgment.

[structural ground](#cap-factory-structural-ground)

[Source account passage](factory.html#whole/why) · placement: agent-inference.

<a id="field-q1-S"></a>

### What? → O:I whole

Software Factory is a native development system with a Rust semantic core, a factory command surface and a Build presentation. It relates Projects, continuing Journeys, bounded Runs, Claims, Evidence, Decisions and Candidates so an intended transformation remains inspectable across sessions, tools and material environments.

[native cli](#cap-factory-native-cli) · [build reading](#cap-factory-build-reading) · [journey continuity](#cap-factory-journey-continuity) · [run map](#cap-factory-run-map) · [build evidence](#cap-factory-build-evidence)

[Source account passage](factory.html#whole/what) · placement: agent-inference.

<a id="field-q2-S"></a>

### How? → O:I whole

Start from the Project’s recognised purpose and a concrete intended difference. Recover the required source and conditions, carry out bounded development, make the resulting Candidate and Evidence encounterable, and return through Recognition or revision. Keep the exact intent, context, execution and evidence relations throughout.

[journey commission](#cap-factory-journey-commission) · [bounded intent](#cap-factory-bounded-intent) · [git development](#cap-factory-git-development) · [intent return](#cap-factory-intent-return) · [request evidence](#cap-factory-request-evidence) · [owner return](#cap-factory-owner-return)

[Source account passage](factory.html#whole/how) · placement: agent-inference.

<a id="field-q2-S2"></a>

### How? → AIKit

The entering Agent reads the relevant intent source, success conditions and exact AIKit ContextResolution reference.

[bounded intent](#cap-factory-bounded-intent) · [git development](#cap-factory-git-development) · [intent return](#cap-factory-intent-return)

[Source account passage](factory.html#q2/implement) · placement: agent-inference.

<a id="field-q3-S"></a>

### Who / Whereby? → O:I whole

The person or authorised governing locus determines consequential direction and recognises outcomes. Agents and AgentSets carry Journeys through bounded Runs. Factory supplies developmental continuity; Actuation supplies authority, AIKit the operative context and body, Workcell material execution, and Central durable source ground.

[cognitive reading](#cap-factory-cognitive-reading) · [praxis condition](#cap-factory-praxis-condition) · [action projection](#cap-factory-action-projection) · [run thought](#cap-factory-run-thought)

[Source account passage](factory.html#whole/whereby) · placement: agent-inference.

<a id="field-q3-S1"></a>

### Who / Whereby? → Actuation

AIKit resolves the operative context, knowledge and praxis; Actuation determines situated Agency and authority; Workcell supplies material execution and lifecycle.

[praxis condition](#cap-factory-praxis-condition)

[Source account passage](factory.html#q3/owners) · placement: agent-inference.

<a id="field-q3-S4"></a>

### Who / Whereby? → Workcell

AIKit resolves the operative context, knowledge and praxis; Actuation determines situated Agency and authority; Workcell supplies material execution and lifecycle.

[praxis condition](#cap-factory-praxis-condition)

[Source account passage](factory.html#q3/owners) · placement: agent-inference.

<a id="field-q3-S5"></a>

### Who / Whereby? → Quaternal Logic

QL-MEF supplies optional formal orientation and refraction.

[praxis condition](#cap-factory-praxis-condition)

[Source account passage](factory.html#q3/owners) · placement: agent-inference.

<a id="field-q4-S"></a>

### Where / When? → O:I whole

Factory operates where software development needs continuity beyond a terminal or conversation. A Journey can span Runs and absences; a Run keeps its identity across execution changes. Native CLI, Build and external projections read the same developmental subjects. Ordinary projects remain valid without a special ontology or QL runtime.

[development persistence](#cap-factory-development-persistence) · [praxis fitness](#cap-factory-praxis-fitness)

[Source account passage](factory.html#whole/context) · placement: agent-inference.

<a id="field-q4-S0"></a>

### Where / When? → Central

AIKit owns operative Method/Routine resolution and provider handoff; Central owns saved profile source; Workcell supplies material scheduling conditions.

[praxis fitness](#cap-factory-praxis-fitness)

[Source account passage](factory.html#q4/routines) · placement: agent-inference.

<a id="field-q5-S"></a>

### Why-For? → O:I whole

Success means a person can commission meaningful work, leave routine development to capable collaborators, and return to a clear result or consequential decision. The Project retains why its reality changed and what it learned. Evidence can improve implementation, methods or intention through the owner responsible for each.



[Source account passage](factory.html#whole/purpose) · placement: agent-inference.

</details>


| Capability | Governing account | Current reading |
|---|---|---|
| [cap.factory.native-cli](#cap-factory-native-cli) | [Account](factory.html#q1/product) | source-inspected; native CLI integration executed |
| [cap.factory.build-reading](#cap-factory-build-reading) | [Account](factory.html#q1/product) | source-inspected; native CLI integration executed |
| [cap.factory.journey-continuity](#cap-factory-journey-continuity) | [Account](factory.html#q1/continuity) | source-inspected |
| [cap.factory.run-map](#cap-factory-run-map) | [Account](factory.html#q1/continuity) | source-inspected |
| [cap.factory.build-evidence](#cap-factory-build-evidence) | [Account](factory.html#q1/epistemics) | source-inspected |
| [cap.factory.structural-ground](#cap-factory-structural-ground) | [Account](factory.html#q0/ground) | source-inspected |
| [cap.factory.journey-commission](#cap-factory-journey-commission) | [Account](factory.html#q2/commission) | source-inspected |
| [cap.factory.bounded-intent](#cap-factory-bounded-intent) | [Account](factory.html#q2/implement) | source-inspected |
| [cap.factory.git-development](#cap-factory-git-development) | [Account](factory.html#q2/implement) | source-inspected |
| [cap.factory.intent-return](#cap-factory-intent-return) | [Account](factory.html#q2/implement) | source-inspected |
| [cap.factory.request-evidence](#cap-factory-request-evidence) | [Account](factory.html#q2/encounter) | source-inspected |
| [cap.factory.cognitive-reading](#cap-factory-cognitive-reading) | [Account](factory.html#q3/thought) | source-inspected |
| [cap.factory.owner-return](#cap-factory-owner-return) | [Account](factory.html#q2/return) | source-inspected |
| [cap.factory.development-persistence](#cap-factory-development-persistence) | [Account](factory.html#q4/reentry) | source-inspected |
| [cap.factory.praxis-condition](#cap-factory-praxis-condition) | [Account](factory.html#q3/owners) | source-inspected |
| [cap.factory.action-projection](#cap-factory-action-projection) | [Account](factory.html#q3/contracts) | source-inspected; native CLI integration executed |
| [cap.factory.run-thought](#cap-factory-run-thought) | [Account](factory.html#q3/thought) | source-inspected |
| [cap.factory.praxis-fitness](#cap-factory-praxis-fitness) | [Account](factory.html#q4/routines) | source-inspected |

<a id="cap-factory-native-cli"></a>



## cap.factory.native-cli

**Need:** A developer or Agent needs a stable way to inspect and operate Factory.

**Operation:** factory capabilities, build snapshot/refresh and action list/invoke use native provider handlers.

**Outcome:** Native snapshots and receipts remain usable from shell and machine clients.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [181.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/181.json) · [181.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/181.comments.json)

**Code refs:** [cli.rs](../../factory/src/cli.rs)

**Test refs:** [native_cli.rs](../../factory/tests/native_cli.rs)

**Account ref:** [Governing account](factory.html#q1/product)


<a id="cap-factory-build-reading"></a>

## cap.factory.build-reading

**Need:** A person needs the selected Project and Run’s current developmental state.

**Operation:** FactoryBuildFileProvider opens persistent state and produces or refreshes a snapshot.

**Outcome:** A revisioned Build reading exposes frontier, native subjects and available Actions.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [143.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/143.json) · [143.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/143.comments.json)

**Code refs:** [build_provider.rs](../../factory/src/build_provider.rs)

**Test refs:** [build_file_provider.rs](../../factory/tests/build_file_provider.rs)

**Account ref:** [Governing account](factory.html#q1/product)


<a id="cap-factory-journey-continuity"></a>

## cap.factory.journey-continuity

**Need:** A continuing outcome needs to outlive one bounded Run or session.

**Operation:** Journey records participants, linked Runs, frontier, activity, Return and Recognition.

**Outcome:** One continuing identity relates distinct transformations and preserves returned evidence.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [168.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/168.json) · [168.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/168.comments.json)

**Code refs:** [journey.rs](../../factory/src/journey.rs)

**Test refs:** [journey.rs](../../factory/src/journey.rs)

**Account ref:** [Governing account](factory.html#q1/continuity)


<a id="cap-factory-run-map"></a>

## cap.factory.run-map

**Need:** A developer needs an inspectable transformation whose identity survives execution changes.

**Operation:** Run accepts revisioned topology mutations and derives one canonical RunMap.

**Outcome:** Atomic topology updates preserve identity and distinguish return edges from dependencies.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [4.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/4.json) · [4.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/4.comments.json)

**Code refs:** [model.rs](../../factory/src/core/run/model.rs) · [topology.rs](../../factory/src/core/run/topology.rs)

**Test refs:** [run_map.rs](../../factory/tests/run_map.rs)

**Account ref:** [Governing account](factory.html#q1/continuity)


<a id="cap-factory-build-evidence"></a>

## cap.factory.build-evidence

**Need:** A reviewer needs the reasons bearing upon a proposed result.

**Operation:** FactoryBuildState projects Candidate, Claim, Evidence and HumanRequest relations.

**Outcome:** The Build consumer can follow native epistemic subjects without substituting trace detail.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [143.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/143.json) · [143.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/143.comments.json)

**Code refs:** [build.rs](../../factory/src/build.rs)

**Test refs:** [build_live.rs](../../factory/tests/build_live.rs)

**Account ref:** [Governing account](factory.html#q1/epistemics)


<a id="cap-factory-structural-ground"></a>

## cap.factory.structural-ground

**Need:** A target with authored relational identity needs development faithful to that ground.

**Operation:** StructuralGround validates source-owned identities, relations and provider evidence.

**Outcome:** Missing or stale structure can be disclosed before feature-local decomposition replaces its source.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [156.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/156.json) · [156.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/156.comments.json)

**Code refs:** [structural_ground.rs](../../factory/src/structural_ground.rs)

**Test refs:** [source_ground.rs](../../factory/tests/source_ground.rs) · [epi_holographic_structural_ground.rs](../../factory/tests/epi_holographic_structural_ground.rs)

**Account ref:** [Governing account](factory.html#q0/ground)


<a id="cap-factory-journey-commission"></a>

## cap.factory.journey-commission

**Need:** A person needs a named collaborator to carry a continuing developmental responsibility.

**Operation:** JourneyCommissionState records accountable subjects, state, handoffs and Recognition attention.

**Outcome:** The continuing responsibility stays readable apart from session lifecycle and invocation authority.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [176.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/176.json) · [176.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/176.comments.json)

**Code refs:** [journey_commission.rs](../../factory/src/journey_commission.rs)

**Test refs:** [journey_commission.rs](../../factory/src/journey_commission.rs)

**Account ref:** [Governing account](factory.html#q2/commission)


<a id="cap-factory-bounded-intent"></a>

## cap.factory.bounded-intent

**Need:** A Run needs the exact intended difference and operative context it answers to.

**Operation:** ProjectDevelopmentLedger.set_intent retains native source, success criteria and ContextResolution references.

**Outcome:** Development has an attributable basis and replacing the intent clears its previous return.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [174.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/174.json) · [174.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/174.comments.json)

**Code refs:** [project_development.rs](../../factory/src/project_development.rs)

**Test refs:** [project_context_intent.rs](../../factory/tests/project_context_intent.rs)

**Account ref:** [Governing account](factory.html#q2/implement)


<a id="cap-factory-git-development"></a>

## cap.factory.git-development

**Need:** A change needs an exact source base and isolated material correlation.

**Operation:** GitDevelopmentRegistry pins a base, binds or rebinds material worktrees and records returned differences.

**Outcome:** Run and Candidate lineage survive material changes while returned evidence identifies its base.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [170.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/170.json) · [170.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/170.comments.json)

**Code refs:** [git_development.rs](../../factory/src/git_development.rs)

**Test refs:** [git_development.rs](../../factory/src/git_development.rs)

**Account ref:** [Governing account](factory.html#q2/implement)


<a id="cap-factory-intent-return"></a>

## cap.factory.intent-return

**Need:** A reviewer needs to know whether the stated criteria were met.

**Operation:** set_intent_return validates criterion/source/context identity and derives satisfied, unsatisfied or indeterminate.

**Outcome:** Conclusive criteria require evidence and incomplete coverage stays indeterminate.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [174.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/174.json) · [174.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/174.comments.json)

**Code refs:** [project_development.rs](../../factory/src/project_development.rs)

**Test refs:** [project_context_intent.rs](../../factory/tests/project_context_intent.rs)

**Account ref:** [Governing account](factory.html#q2/implement)


<a id="cap-factory-request-evidence"></a>

## cap.factory.request-evidence

**Need:** A reviewer needs additional support for a developmental decision.

**Operation:** The canonical request-evidence Action checks capability and authority then creates a HumanRequest.

**Outcome:** The result advances persistent native state; failed authority does not mutate it.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [143.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/143.json) · [143.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/143.comments.json)

**Code refs:** [build.rs](../../factory/src/build.rs)

**Test refs:** [build_live.rs](../../factory/tests/build_live.rs) · [build_file_provider.rs](../../factory/tests/build_file_provider.rs)

**Account ref:** [Governing account](factory.html#q2/encounter)


<a id="cap-factory-cognitive-reading"></a>

## cap.factory.cognitive-reading

**Need:** A collaborator needs relevant retained thought without reading every session.

**Operation:** FactoryBuildCognitiveFocus filters by subject, producer, anchor, related reference and lifecycle.

**Outcome:** A source-backed cognitive reading remains related to the Run and native thought identities.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [163.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/163.json) · [163.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/163.comments.json)

**Code refs:** [build_cognitive.rs](../../factory/src/build_cognitive.rs)

**Test refs:** [build_cognitive.rs](../../factory/tests/build_cognitive.rs)

**Account ref:** [Governing account](factory.html#q3/thought)


<a id="cap-factory-owner-return"></a>

## cap.factory.owner-return

**Need:** An implementation finding needs to pressure the proper source or praxis owner.

**Operation:** The development ledger retains observations, reflection anchors and owner-return proposals.

**Outcome:** The review can follow code back to source and route a proposed change without silently mutating foreign ground.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [155.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/155.json) · [155.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/155.comments.json)

**Code refs:** [project_development.rs](../../factory/src/project_development.rs)

**Test refs:** [project_development.rs](../../factory/tests/project_development.rs)

**Account ref:** [Governing account](factory.html#q2/return)


<a id="cap-factory-development-persistence"></a>

## cap.factory.development-persistence

**Need:** A later process needs the actual Run context and findings.

**Operation:** FileProjectDevelopmentStore saves and reloads the existing ledger with identity and version validation.

**Outcome:** Developmental state survives reopening; a missing record stays absent.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [174.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/174.json) · [174.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/174.comments.json)

**Code refs:** [project_development_store.rs](../../factory/src/project_development_store.rs)

**Test refs:** [project_development_persistence.rs](../../factory/tests/project_development_persistence.rs)

**Account ref:** [Governing account](factory.html#q4/reentry)


<a id="cap-factory-praxis-condition"></a>

## cap.factory.praxis-condition

**Need:** Development needs an attributable account of how its Agent actually worked.

**Operation:** The development ledger retains external Method, Skill, body and context conditions and capability rows.

**Outcome:** Evidence can qualify the actual operative condition without making Factory the resolver.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [155.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/155.json) · [155.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/155.comments.json)

**Code refs:** [project_development.rs](../../factory/src/project_development.rs)

**Test refs:** [project_development.rs](../../factory/tests/project_development.rs)

**Account ref:** [Governing account](factory.html#q3/owners)


<a id="cap-factory-action-projection"></a>

## cap.factory.action-projection

**Need:** Different clients must operate the same developmental subject under the same authority.

**Operation:** execute_projected_factory_action validates projection lineage and invokes the native handler.

**Outcome:** Headless and native CLI paths retain subject, caller and authority in the returned receipt.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [181.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/181.json) · [181.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/181.comments.json)

**Code refs:** [action_projection.rs](../../factory/src/action_projection.rs)

**Test refs:** [action_projection.rs](../../factory/tests/action_projection.rs) · [native_cli.rs](../../factory/tests/native_cli.rs)

**Account ref:** [Governing account](factory.html#q3/contracts)


<a id="cap-factory-run-thought"></a>

## cap.factory.run-thought

**Need:** Developmental reasoning must remain attributable beyond a single session.

**Operation:** Run retains typed source-backed thought with producer, passage anchors and Run-local identity.

**Outcome:** Multiple sessions contribute without cross-Run leakage or duplicate retention.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [163.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/163.json) · [163.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/163.comments.json)

**Code refs:** [thought.rs](../../factory/src/core/run/thought.rs)

**Test refs:** [run_thought.rs](../../factory/tests/run_thought.rs) · [build_cognitive_write.rs](../../factory/tests/build_cognitive_write.rs)

**Account ref:** [Governing account](factory.html#q3/thought)


<a id="cap-factory-praxis-fitness"></a>

## cap.factory.praxis-fitness

**Need:** A continuing responsibility needs evidence before its method becomes recurring unattended work.

**Operation:** JourneyPraxisContext relates profile selection, Method proof, praxis return and Routine observations.

**Outcome:** Changed method/proof relations disclose revalidation pressure while native owners retain scheduling and resolution.

**Implementation status:** source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.

**Source refs:** [176.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/176.json) · [176.comments.json](../../../github-recovery-mirror/mirror/agent-system-design/issues/176.comments.json)

**Code refs:** [journey_praxis.rs](../../factory/src/journey_praxis.rs)

**Test refs:** [journey_praxis.rs](../../factory/src/journey_praxis.rs)

**Account ref:** [Governing account](factory.html#q4/routines)

## Full lossless carrier

```csv
id,record_type,view_id,row_id,column_id,capability_refs,need,operation,outcome,implementation_status,standing,source_refs,code_refs,test_refs,account_ref,relation,coverage,extensions,question
cap.factory.native-cli,capability,,,,[],A developer or Agent needs a stable way to inspect and operate Factory.,"factory capabilities declares public contracts; build snapshot/refresh, action list/invoke, and verify operate or validate the native provider.",Native snapshots and receipts remain usable from shell and machine clients.,source-inspected; native CLI integration evidence executed 2026-09-06 (cargo test --test cli --test native_cli: 7 passed). Owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/181.json;../github-recovery-mirror/mirror/agent-system-design/issues/181.comments.json,factory/src/cli.rs,factory/tests/native_cli.rs,factory.html#q1/product,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [""build.snapshot"", ""build.refresh"", ""action.list"", ""action.invoke"", ""verify""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The native factory executable declares and handles these command families.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/181.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/cli.rs"": ""054034cf91ed94ed7ce9f9f2433e53e75149949f0ec9fad1788f3e3db9ac2175""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.build-reading,capability,,,,[],A person needs the selected Project and Run’s current developmental state.,FactoryBuildFileProvider opens persistent state and produces or refreshes a snapshot.,"A revisioned Build reading exposes frontier, native subjects and available Actions.","source-inspected; native CLI integration evidence executed 2026-09-06 (cargo test --test cli --test native_cli: 7 passed) covers persisted snapshot, refresh and Action listing. Owner-world and desktop acceptance are not established.",agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/143.json;../github-recovery-mirror/mirror/agent-system-design/issues/143.comments.json,factory/src/build_provider.rs,factory/tests/build_file_provider.rs,factory.html#q1/product,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [""build.snapshot"", ""build.refresh"", ""action.list""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The native command directly returns the selected provider snapshot or its Action inventory.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/143.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/build_provider.rs"": ""527ce717e1f48a394385ddd9cd489b8b5b5c318c621162492df8ce72da83eab5""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.journey-continuity,capability,,,,[],A continuing outcome needs to outlive one bounded Run or session.,"Journey records participants, linked Runs, frontier, activity, Return and Recognition.",One continuing identity relates distinct transformations and preserves returned evidence.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/168.json;../github-recovery-mirror/mirror/agent-system-design/issues/168.comments.json,factory/src/journey.rs,factory/src/journey.rs,factory.html#q1/continuity,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Journey continuity has native domain code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/168.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/journey.rs"": ""e91d5e20d9cf61fa47466c2122fd5d1f2a7c2eb7bff70e4e65a2968c55d248c2""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.run-map,capability,,,,[],A developer needs an inspectable transformation whose identity survives execution changes.,Run accepts revisioned topology mutations and derives one canonical RunMap.,Atomic topology updates preserve identity and distinguish return edges from dependencies.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/4.json;../github-recovery-mirror/mirror/agent-system-design/issues/4.comments.json,factory/src/core/run/model.rs;factory/src/core/run/topology.rs,factory/tests/run_map.rs,factory.html#q1/continuity,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Run-map topology has native domain code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/4.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/core/run/model.rs"": ""a1f4606fbfa0ef4c253ef5580e18a54dcdecf17f203eb47b6a9550a27d64ba1c"", ""factory/src/core/run/topology.rs"": ""0750ed154f2472c9b88e90bd35052e85d7db2c0eb6a4437307afd7d7013647a1""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.build-evidence,capability,,,,[],A reviewer needs the reasons bearing upon a proposed result.,"FactoryBuildState projects Candidate, Claim, Evidence and HumanRequest relations.",The Build consumer can follow native epistemic subjects without substituting trace detail.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/143.json;../github-recovery-mirror/mirror/agent-system-design/issues/143.comments.json,factory/src/build.rs,factory/tests/build_live.rs,factory.html#q1/epistemics,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""composed"", ""reason"": ""Build snapshots may contain native evidence subjects, but the CLI declares no evidence-specific command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/143.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/build.rs"": ""8cd331654bd08b6584baba8c5d497841bdf8237abccb1dbf5ad664f928cdda4a""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.structural-ground,capability,,,,[],A target with authored relational identity needs development faithful to that ground.,"StructuralGround validates source-owned identities, relations and provider evidence.",Missing or stale structure can be disclosed before feature-local decomposition replaces its source.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/156.json;../github-recovery-mirror/mirror/agent-system-design/issues/156.comments.json,factory/src/structural_ground.rs,factory/tests/source_ground.rs;factory/tests/epi_holographic_structural_ground.rs,factory.html#q0/ground,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Structural-ground validation has native code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/156.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/structural_ground.rs"": ""37ff1583b95e7fdbe42c9c35c8692422bb762a4e4a012d9191d6f6f27e4fb060""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.journey-commission,capability,,,,[],A person needs a named collaborator to carry a continuing developmental responsibility.,"JourneyCommissionState records accountable subjects, state, handoffs and Recognition attention.",The continuing responsibility stays readable apart from session lifecycle and invocation authority.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/176.json;../github-recovery-mirror/mirror/agent-system-design/issues/176.comments.json,factory/src/journey_commission.rs,factory/src/journey_commission.rs,factory.html#q2/commission,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Journey commission has native code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/176.json"", ""EpiLogos/agent-system-design#188"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/journey_commission.rs"": ""a424c3be7b85a1616106dc8cfc38c8a3e65ae762657e6bb82a2b9660f5b74b05""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.bounded-intent,capability,,,,[],A Run needs the exact intended difference and operative context it answers to.,"ProjectDevelopmentLedger.set_intent retains native source, success criteria and ContextResolution references.",Development has an attributable basis and replacing the intent clears its previous return.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/174.json;../github-recovery-mirror/mirror/agent-system-design/issues/174.comments.json,factory/src/project_development.rs,factory/tests/project_context_intent.rs,factory.html#q2/implement,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Bounded intent has native ledger code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/174.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/project_development.rs"": ""27acb008950f925aabb9ca2c56eee705e9fe5c818931672996aa6c8099ca02a6""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.git-development,capability,,,,[],A change needs an exact source base and isolated material correlation.,"GitDevelopmentRegistry pins a base, binds or rebinds material worktrees and records returned differences.",Run and Candidate lineage survive material changes while returned evidence identifies its base.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/170.json;../github-recovery-mirror/mirror/agent-system-design/issues/170.comments.json,factory/src/git_development.rs,factory/src/git_development.rs,factory.html#q2/implement,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Git-development registration has native code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/170.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/git_development.rs"": ""2ebc9473dea88891bdccee7ce8ca140d18120573184c29a6331fc188a41da64c""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.intent-return,capability,,,,[],A reviewer needs to know whether the stated criteria were met.,"set_intent_return validates criterion/source/context identity and derives satisfied, unsatisfied or indeterminate.",Conclusive criteria require evidence and incomplete coverage stays indeterminate.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/174.json;../github-recovery-mirror/mirror/agent-system-design/issues/174.comments.json,factory/src/project_development.rs,factory/tests/project_context_intent.rs,factory.html#q2/implement,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Intent return has native ledger code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/174.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/project_development.rs"": ""27acb008950f925aabb9ca2c56eee705e9fe5c818931672996aa6c8099ca02a6""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.request-evidence,capability,,,,[],A reviewer needs additional support for a developmental decision.,The canonical request-evidence Action checks capability and authority then creates a HumanRequest.,The result advances persistent native state; failed authority does not mutate it.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/143.json;../github-recovery-mirror/mirror/agent-system-design/issues/143.comments.json,factory/src/build.rs,factory/tests/build_live.rs;factory/tests/build_file_provider.rs,factory.html#q2/encounter,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""composed"", ""reason"": ""The currently projected request-evidence Action can be invoked through action.invoke only when native state exposes and authorises it; it is not a declared command ID.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/143.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/build.rs"": ""8cd331654bd08b6584baba8c5d497841bdf8237abccb1dbf5ad664f928cdda4a""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.cognitive-reading,capability,,,,[],A collaborator needs relevant retained thought without reading every session.,"FactoryBuildCognitiveFocus filters by subject, producer, anchor, related reference and lifecycle.",A source-backed cognitive reading remains related to the Run and native thought identities.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/163.json;../github-recovery-mirror/mirror/agent-system-design/issues/163.comments.json,factory/src/build_cognitive.rs,factory/tests/build_cognitive.rs,factory.html#q3/thought,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Cognitive reading has native code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/163.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/build_cognitive.rs"": ""47d1298f7b9710bd5e216c206435d23a9543c5b9e612fd6e18d7a90ca0eb42d2""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.owner-return,capability,,,,[],An implementation finding needs to pressure the proper source or praxis owner.,"The development ledger retains observations, reflection anchors and owner-return proposals.",The review can follow code back to source and route a proposed change without silently mutating foreign ground.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/155.json;../github-recovery-mirror/mirror/agent-system-design/issues/155.comments.json,factory/src/project_development.rs,factory/tests/project_development.rs,factory.html#q2/return,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Owner-return proposals have native ledger code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/155.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/project_development.rs"": ""27acb008950f925aabb9ca2c56eee705e9fe5c818931672996aa6c8099ca02a6""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.development-persistence,capability,,,,[],A later process needs the actual Run context and findings.,FileProjectDevelopmentStore saves and reloads the existing ledger with identity and version validation.,Developmental state survives reopening; a missing record stays absent.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/174.json;../github-recovery-mirror/mirror/agent-system-design/issues/174.comments.json,factory/src/project_development_store.rs,factory/tests/project_development_persistence.rs,factory.html#q4/reentry,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Development persistence has native store code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/174.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/project_development_store.rs"": ""59ed443133d058af04e9207aa20da8cbd40f38e939a36d8ab649c0aa4d3d34e6""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.praxis-condition,capability,,,,[],Development needs an attributable account of how its Agent actually worked.,"The development ledger retains external Method, Skill, body and context conditions and capability rows.",Evidence can qualify the actual operative condition without making Factory the resolver.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/155.json;../github-recovery-mirror/mirror/agent-system-design/issues/155.comments.json,factory/src/project_development.rs,factory/tests/project_development.rs,factory.html#q3/owners,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Praxis conditions have native ledger code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/155.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/project_development.rs"": ""27acb008950f925aabb9ca2c56eee705e9fe5c818931672996aa6c8099ca02a6""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.action-projection,capability,,,,[],Different clients must operate the same developmental subject under the same authority.,execute_projected_factory_action validates projection lineage and invokes the native handler.,"Headless and native CLI paths retain subject, caller and authority in the returned receipt.","source-inspected; native CLI integration evidence executed 2026-09-06 (cargo test --test cli --test native_cli: 7 passed) covers projected invocation, rejection and persisted receipt parity. Owner-world and desktop acceptance are not established.",agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/181.json;../github-recovery-mirror/mirror/agent-system-design/issues/181.comments.json,factory/src/action_projection.rs,factory/tests/action_projection.rs;factory/tests/native_cli.rs,factory.html#q3/contracts,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [""action.list"", ""action.invoke""], ""cli_exposure"": {""kind"": ""direct"", ""reason"": ""The native command lists selected Actions and invokes a projected Action through the canonical provider.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/181.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/action_projection.rs"": ""6a05b689bbdc16eaba36afff86f4609d3fb350f954b3a1ad122a20a701fcd3d2""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.run-thought,capability,,,,[],Developmental reasoning must remain attributable beyond a single session.,"Run retains typed source-backed thought with producer, passage anchors and Run-local identity.",Multiple sessions contribute without cross-Run leakage or duplicate retention.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/163.json;../github-recovery-mirror/mirror/agent-system-design/issues/163.comments.json,factory/src/core/run/thought.rs,factory/tests/run_thought.rs;factory/tests/build_cognitive_write.rs,factory.html#q3/thought,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Run thought has native domain code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/163.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/core/run/thought.rs"": ""d142a69299b63cf0dd9f9699217fd7fc5302a782e192cee7987ac7174225882f""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
cap.factory.praxis-fitness,capability,,,,[],A continuing responsibility needs evidence before its method becomes recurring unattended work.,"JourneyPraxisContext relates profile selection, Method proof, praxis return and Routine observations.",Changed method/proof relations disclose revalidation pressure while native owners retain scheduling and resolution.,source-inspected; Native source and test definitions inspected 2026-09-06. Tests were not executed in this documentation pass; owner-world and desktop acceptance are not established.,agent-inference,../github-recovery-mirror/mirror/agent-system-design/issues/176.json;../github-recovery-mirror/mirror/agent-system-design/issues/176.comments.json,factory/src/journey_praxis.rs,factory/src/journey_praxis.rs,factory.html#q4/routines,,,"{""basis"": ""Source-recovered capability account; original verification limits retained in Markdown."", ""converted_from_sha256"": ""99e4e8737c1c481bcd25a49a0a02f21cf3c34d78041d709e58fc566a5da0bf20"", ""cli_commands"": [], ""cli_exposure"": {""kind"": ""library"", ""reason"": ""Journey praxis fitness has native code but no declared CLI command.""}, ""maintenance"": {""updated_at"": ""2026-09-08"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""../github-recovery-mirror/mirror/agent-system-design/issues/176.json"", ""reconcile:factory-pr-194""], ""code_basis"": {""factory/src/journey_praxis.rs"": ""3df4a870058ff174b69fdb08d735a181ee5491bfcaa771fe7bea25c22f628426""}}, ""last_reconciled_at"": ""2026-09-08T11:52:07.186089+00:00""}",
H0->H3,relation,suite-relations,H0,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H0->A3,relation,suite-relations,H0,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H1->H3,relation,suite-relations,H1,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Actuation"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H1->A3,relation,suite-relations,H1,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Actuation"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H2->H3,relation,suite-relations,H2,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""AIKit"", ""dst_product"": ""Factory"", ""ql"": ""A2|C3"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H2->A3,relation,suite-relations,H2,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""AIKit"", ""dst_product"": ""Factory"", ""ql"": ""D2-transform|D2-complete"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->H0,relation,suite-relations,H3,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->H1,relation,suite-relations,H3,H1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Factory"", ""dst_product"": ""Actuation"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->H2,relation,suite-relations,H3,H2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""Factory"", ""dst_product"": ""AIKit"", ""ql"": ""A2|C3"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->H3,relation,suite-relations,H3,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,self:Factory,I,"{""src_product"": ""Factory"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""self:Factory"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#131(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->H4,relation,suite-relations,H3,H4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Factory"", ""dst_product"": ""Workcell"", ""ql"": ""B2"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->H5,relation,suite-relations,H3,H5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""Factory"", ""dst_product"": ""QL"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->A0,relation,suite-relations,H3,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->A1,relation,suite-relations,H3,A1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Factory"", ""dst_product"": ""Actuation"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->A2,relation,suite-relations,H3,A2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""Factory"", ""dst_product"": ""AIKit"", ""ql"": ""D2-require|D2-complete"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->A3,relation,suite-relations,H3,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,conjugation:Factory,H,"{""src_product"": ""Factory"", ""dst_product"": ""Factory"", ""ql"": ""D1"", ""cf_view"": ""CF4"", ""seam"": ""conjugation:Factory"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#131(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->A4,relation,suite-relations,H3,A4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Factory"", ""dst_product"": ""Workcell"", ""ql"": ""D2-transform"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H3->A5,relation,suite-relations,H3,A5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""Factory"", ""dst_product"": ""QL"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H4->H3,relation,suite-relations,H4,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Workcell"", ""dst_product"": ""Factory"", ""ql"": ""B2"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H4->A3,relation,suite-relations,H4,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Workcell"", ""dst_product"": ""Factory"", ""ql"": ""D2-require"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H5->H3,relation,suite-relations,H5,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""QL"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
H5->A3,relation,suite-relations,H5,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""QL"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A0->H3,relation,suite-relations,A0,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A0->A3,relation,suite-relations,A0,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Central"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A1->H3,relation,suite-relations,A1,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Actuation"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A1->A3,relation,suite-relations,A1,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Actuation"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A2->H3,relation,suite-relations,A2,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""AIKit"", ""dst_product"": ""Factory"", ""ql"": ""D2-require.inverse|D2-complete.inverse"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A2->A3,relation,suite-relations,A2,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""AIKit"", ""dst_product"": ""Factory"", ""ql"": ""D3:A2|D3:C3"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->H0,relation,suite-relations,A3,H0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->H1,relation,suite-relations,A3,H1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Factory"", ""dst_product"": ""Actuation"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->H2,relation,suite-relations,A3,H2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""Factory"", ""dst_product"": ""AIKit"", ""ql"": ""D2-transform.inverse|D2-complete.inverse"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->H3,relation,suite-relations,A3,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,conjugation:Factory,H,"{""src_product"": ""Factory"", ""dst_product"": ""Factory"", ""ql"": ""D1.inverse"", ""cf_view"": ""CF4"", ""seam"": ""conjugation:Factory"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#131(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->H4,relation,suite-relations,A3,H4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Factory"", ""dst_product"": ""Workcell"", ""ql"": ""D2-require.inverse"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->H5,relation,suite-relations,A3,H5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""Factory"", ""dst_product"": ""QL"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->A0,relation,suite-relations,A3,A0,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,03:ground-development,L,"{""src_product"": ""Factory"", ""dst_product"": ""Central"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""03:ground-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Central#24(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->A1,relation,suite-relations,A3,A1,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,13:agency-development,S,"{""src_product"": ""Factory"", ""dst_product"": ""Actuation"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""13:agency-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/Actuation#1;EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->A2,relation,suite-relations,A3,A2,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,23:possibility-development,H,"{""src_product"": ""Factory"", ""dst_product"": ""AIKit"", ""ql"": ""D3:A2|D3:C3"", ""cf_view"": ""CF4"", ""seam"": ""23:possibility-development"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/ai-kit#58(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->A3,relation,suite-relations,A3,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,self:Factory,I,"{""src_product"": ""Factory"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": ""CF4"", ""seam"": ""self:Factory"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#131(PR);EpiLogos/agent-system-design#142(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->A4,relation,suite-relations,A3,A4,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Factory"", ""dst_product"": ""Workcell"", ""ql"": ""D3:B2"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A3->A5,relation,suite-relations,A3,A5,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""Factory"", ""dst_product"": ""QL"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A4->H3,relation,suite-relations,A4,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Workcell"", ""dst_product"": ""Factory"", ""ql"": ""D2-transform.inverse"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A4->A3,relation,suite-relations,A4,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,34:development-materialisation,H,"{""src_product"": ""Workcell"", ""dst_product"": ""Factory"", ""ql"": ""D3:B2"", ""cf_view"": ""CF5-field"", ""seam"": ""34:development-materialisation"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#142(PR);EpiLogos/Workcell#18(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A5->H3,relation,suite-relations,A5,H3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""QL"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
A5->A3,relation,suite-relations,A5,A3,[],,,,,agent-inference,O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR),,,,35:development-ql,S,"{""src_product"": ""QL"", ""dst_product"": ""Factory"", ""ql"": """", ""cf_view"": """", ""seam"": ""35:development-ql"", ""defined_in"": ""O-I:docs/CANONICAL-PRODUCT-FIELD.md|QL-MEF#19(PR)"", ""tracked_by"": ""EpiLogos/O-I#29;EpiLogos/agent-system-design#132(PR);EpiLogos/QL-MEF#19(PR)"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
rel.factory.q0.S,relation,product-field,q0,S,"[""cap.factory.structural-ground""]",,,,,agent-inference,ProjectCentral/user/factory.html#whole-why,,,factory.html#whole/why,"Agentic development can produce working changes while losing the purpose and experience that made them worth asking for. Factory exists to carry human intention through development and returned reality, freeing the person from repeated orientation and mechanical supervision while preserving consequential judgment.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""factory:seed:q0"", ""source_unit"": ""whole-why"", ""seed_sha256"": ""c5fc7b2673bf4788d30cf51e972372ca6e078910483cf73fa755b81a3f2f12b2"", ""source_standing"": ""agent-inference"", ""reconciled_change_ref"": ""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",Why?
rel.factory.q1.S,relation,product-field,q1,S,"[""cap.factory.native-cli"", ""cap.factory.build-reading"", ""cap.factory.journey-continuity"", ""cap.factory.run-map"", ""cap.factory.build-evidence""]",,,,,agent-inference,ProjectCentral/user/factory.html#whole-what,,,factory.html#whole/what,"Software Factory is a native development system with a Rust semantic core, a factory command surface and a Build presentation. It relates Projects, continuing Journeys, bounded Runs, Claims, Evidence, Decisions and Candidates so an intended transformation remains inspectable across sessions, tools and material environments.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""factory:seed:q1"", ""source_unit"": ""whole-what"", ""seed_sha256"": ""2adb57515414fc8f670ae6cf8b9df2f3abeadc4b8a49c3af36a6236679605e88"", ""source_standing"": ""agent-inference"", ""reconciled_change_ref"": ""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",What?
rel.factory.q2.S,relation,product-field,q2,S,"[""cap.factory.journey-commission"", ""cap.factory.bounded-intent"", ""cap.factory.git-development"", ""cap.factory.intent-return"", ""cap.factory.request-evidence"", ""cap.factory.owner-return""]",,,,,agent-inference,ProjectCentral/user/factory.html#whole-how,,,factory.html#whole/how,"Start from the Project’s recognised purpose and a concrete intended difference. Recover the required source and conditions, carry out bounded development, make the resulting Candidate and Evidence encounterable, and return through Recognition or revision. Keep the exact intent, context, execution and evidence relations throughout.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""factory:seed:q2"", ""source_unit"": ""whole-how"", ""seed_sha256"": ""57f98a054ee00a34b1404d916c651f9a6fb9c1de4b9e323801d58bd3c6377491"", ""source_standing"": ""agent-inference"", ""reconciled_change_ref"": ""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",How?
rel.factory.q3.S,relation,product-field,q3,S,"[""cap.factory.cognitive-reading"", ""cap.factory.praxis-condition"", ""cap.factory.action-projection"", ""cap.factory.run-thought""]",,,,,agent-inference,ProjectCentral/user/factory.html#whole-whereby,,,factory.html#whole/whereby,"The person or authorised governing locus determines consequential direction and recognises outcomes. Agents and AgentSets carry Journeys through bounded Runs. Factory supplies developmental continuity; Actuation supplies authority, AIKit the operative context and body, Workcell material execution, and Central durable source ground.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""factory:seed:q3"", ""source_unit"": ""whole-whereby"", ""seed_sha256"": ""52ca82a7c392196d52e9edf26966ba7751cd894be01bc5a8f13d58793eba4ac2"", ""source_standing"": ""agent-inference"", ""reconciled_change_ref"": ""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",Who / Whereby?
rel.factory.q4.S,relation,product-field,q4,S,"[""cap.factory.development-persistence"", ""cap.factory.praxis-fitness""]",,,,,agent-inference,ProjectCentral/user/factory.html#whole-context,,,factory.html#whole/context,"Factory operates where software development needs continuity beyond a terminal or conversation. A Journey can span Runs and absences; a Run keeps its identity across execution changes. Native CLI, Build and external projections read the same developmental subjects. Ordinary projects remain valid without a special ontology or QL runtime.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""factory:seed:q4"", ""source_unit"": ""whole-context"", ""seed_sha256"": ""d11fa2c433bf3c28eb4aba45f33627584afaf5cba3c87ad0bf5146ee492f0467"", ""source_standing"": ""agent-inference"", ""reconciled_change_ref"": ""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",Where / When?
rel.factory.q5.S,relation,product-field,q5,S,[],,,,,agent-inference,ProjectCentral/user/factory.html#whole-purpose,,,factory.html#whole/purpose,"Success means a person can commission meaningful work, leave routine development to capable collaborators, and return to a clear result or consequential decision. The Project retains why its reality changed and what it learned. Evidence can improve implementation, methods or intention through the owner responsible for each.",,"{""basis"": ""Exact overview seed text. Capability links follow their existing governing expanded account units; cell placement is editorial inference."", ""seed_ref"": ""factory:seed:q5"", ""source_unit"": ""whole-purpose"", ""seed_sha256"": ""521f80aaca94d8145ab65b945cc6cce65817a9a5f88045953fc356eebe563476"", ""source_standing"": ""agent-inference"", ""reconciled_change_ref"": ""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",Why-For?
rel.factory.q4.S0,relation,product-field,q4,S0,"[""cap.factory.praxis-fitness""]",,,,,agent-inference,ProjectCentral/user/factory.html#q4-routines,,,factory.html#q4/routines,AIKit owns operative Method/Routine resolution and provider handoff; Central owns saved profile source; Workcell supplies material scheduling conditions.,,"{""basis"": ""Exact account sentence; cell placement is editorial inference."", ""source_unit"": ""q4-routines"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
rel.factory.q3.S1,relation,product-field,q3,S1,"[""cap.factory.praxis-condition""]",,,,,agent-inference,ProjectCentral/user/factory.html#q3-owners,,,factory.html#q3/owners,"AIKit resolves the operative context, knowledge and praxis; Actuation determines situated Agency and authority; Workcell supplies material execution and lifecycle.",,"{""basis"": ""Exact account sentence; cell placement is editorial inference."", ""source_unit"": ""q3-owners"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
rel.factory.q2.S2,relation,product-field,q2,S2,"[""cap.factory.bounded-intent"", ""cap.factory.git-development"", ""cap.factory.intent-return""]",,,,,agent-inference,ProjectCentral/user/factory.html#q2-implement,,,factory.html#q2/implement,"The entering Agent reads the relevant intent source, success conditions and exact AIKit ContextResolution reference.",,"{""basis"": ""Exact account sentence; cell placement is editorial inference."", ""source_unit"": ""q2-implement"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
rel.factory.q3.S4,relation,product-field,q3,S4,"[""cap.factory.praxis-condition""]",,,,,agent-inference,ProjectCentral/user/factory.html#q3-owners,,,factory.html#q3/owners,"AIKit resolves the operative context, knowledge and praxis; Actuation determines situated Agency and authority; Workcell supplies material execution and lifecycle.",,"{""basis"": ""Exact account sentence; cell placement is editorial inference."", ""source_unit"": ""q3-owners"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
rel.factory.q3.S5,relation,product-field,q3,S5,"[""cap.factory.praxis-condition""]",,,,,agent-inference,ProjectCentral/user/factory.html#q3-owners,,,factory.html#q3/owners,QL-MEF supplies optional formal orientation and refraction.,,"{""basis"": ""Exact account sentence; cell placement is editorial inference."", ""source_unit"": ""q3-owners"", ""maintenance"": {""updated_at"": ""2026-09-06"", ""change_refs"": [""codex:thread:01a07608-d2ec-7b10-9713-74c445adf8a5""]}, ""last_reconciled_at"": ""2026-09-06T14:30:24.173993+00:00""}",
```
