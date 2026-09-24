// Development and knowledge: ordinary software development carried by Anima's
// voices, then Aletheia returns what was learned through the NOW/wiki door.
//
// A Factory workflow source (restricted TypeScript data, never executed).
// Check it with: factory workflow check workflows/development-and-knowledge.workflow.ts --json
// The subject is this repository; commission a successor with the exact
// subject and basis revision of the change being made.
import { defineWorkflow, unit } from "@epilogos/factory-workflow";
import type { CPrime } from "@epilogos/ql-vak";

const subject = "project:quaternal-logic";
const basis = "c624c52";
const whole = "central:source:project:quaternal-logic:ProjectCentral/now";
const returns = "central:source:project:quaternal-logic:ProjectCentral/now";
const cprime = { ref: "ql/interpretation/c-prime", revision: "09f7d29ad6262f85bc2858f7c468f22d0bd398f3" };
const agents = "central:source:project:quaternal-logic:AGENTS.md";
const verification = "central:source:control:root:Control/agents/governance/engineering/verification.md";
const placement = "central:source:control:root:Control/user/placement.json";
const returnDoor = "central:source:control:root:Control/agents/governance/field-and-now/session-work-placement.md";
const wikiLaw = "central:source:control:root:Control/agents/governance/field-and-now/wiki-field-law.md";
const anima = ["agent-set/anima"];

export default defineWorkflow({
  source: {
    ref: "workflow-source:01M399B6B0R8CJY3Y2RTG5EPWS",
    revision: "development-and-knowledge-v1",
  },
  workflowKey: "development-and-knowledge",
  units: [
    unit({
      key: "logos-scope-plan",
      developmentalConcern: "Scope the change and write the plan it will be built from",
      requiredDifference: "A bounded plan: files, behaviour, tests and the checks the repository already runs",
      returnContract: "Return the plan with its scope boundary and the exact verification it will need",
      subjectRef: subject,
      basisRevision: basis,
      agentRequirements: { agentRefs: ["agent/anima-logos"], agentSetRefs: anima },
      praxisRefs: ["skill/personal/brainstorming", "skill/personal/writing-plans"],
      capabilityRefs: ["capability/source-read"],
      permittedEffects: ["read the repository and its governing documents"],
      verificationObligations: ["the plan cites the repository's own gates"],
      returnAddress: returns,
      stopConditions: "Stop when the request needs an owner decision before it can be scoped",
      escalationConditions: "Escalate scope that crosses another repository's ownership",
      composition: {
        CPF: "dialogical", CT: "CT1", CP: "4.1", CF: "CF2", CFP: "CFP2", CS: "CS2", direction: "forward",
        actor: "agent/anima-logos", interpretation: cprime, whole: whole,
        resolvePath: "resolve-scoped-path:development-and-knowledge/logos",
        contextResolution: "context-resolution:development-and-knowledge/logos",
        sources: [agents, verification],
      } satisfies CPrime,
    }),
    unit({
      key: "techne-implement",
      developmentalConcern: "Implement the plan test-first in the repository",
      requiredDifference: "The planned behaviour exists with tests that fail without it",
      returnContract: "Return the branch, the changed files and the test evidence",
      subjectRef: subject,
      basisRevision: basis,
      agentRequirements: { agentRefs: ["agent/anima-techne-helper"], agentSetRefs: anima },
      praxisRefs: ["skill/personal/test-driven-development", "skill/personal/executing-plans", "skill/personal/subagent-driven-development"],
      capabilityRefs: ["capability/source-read", "capability/source-write", "capability/test-execution"],
      dependencies: ["logos-scope-plan"],
      inputs: [{ predecessor: "logos-scope-plan", receivingContextRef: "context:development-and-knowledge/implement-from-plan" }],
      permittedEffects: ["write source and tests on a feature branch", "run the repository's local checks"],
      verificationObligations: ["run the tests named in the plan and quote their results"],
      returnAddress: returns,
      stopConditions: "Stop when the plan cannot be implemented without widening scope",
      escalationConditions: "Escalate any change to protected or authored ground",
      composition: {
        CPF: "authorised-undertaking", authority: placement,
        CT: "CT2", CP: "4.2", CF: "CF5", CFP: "CFP2", CS: "CS2", direction: "forward",
        actor: "agent/anima-techne-helper", interpretation: cprime, whole: whole,
        resolvePath: "resolve-scoped-path:development-and-knowledge/techne",
        contextResolution: "context-resolution:development-and-knowledge/techne",
        sources: [agents, verification],
      } satisfies CPrime,
    }),
    unit({
      key: "eros-verify",
      developmentalConcern: "Verify the implementation against the plan with the repository's real gates",
      requiredDifference: "Executed evidence says whether the change does what the plan required",
      returnContract: "Return the commands run, their results and every failure found",
      subjectRef: subject,
      basisRevision: basis,
      agentRequirements: { agentRefs: ["agent/anima-eros"], agentSetRefs: anima },
      praxisRefs: ["skill/personal/verification-before-completion", "skill/ql/vak-evaluate"],
      capabilityRefs: ["capability/source-read", "capability/test-execution"],
      dependencies: ["techne-implement"],
      inputs: [{ predecessor: "techne-implement", receivingContextRef: "context:development-and-knowledge/verify-from-implement" }],
      permittedEffects: ["run tests, format and lint checks", "read the branch"],
      verificationObligations: ["quote executed results; never infer a pass from a name or a fixture"],
      returnAddress: returns,
      stopConditions: "Stop when the branch cannot be built",
      escalationConditions: "Escalate a gate that cannot be run in the available environment",
      composition: {
        CPF: "dialogical", CT: "CT2", CP: "4.2", CF: "CF3", CFP: "CFP2", CS: "CS2", direction: "returning",
        actor: "agent/anima-eros", interpretation: cprime, whole: whole,
        resolvePath: "resolve-scoped-path:development-and-knowledge/eros",
        contextResolution: "context-resolution:development-and-knowledge/eros",
        sources: [verification],
      } satisfies CPrime,
    }),
    unit({
      key: "mythos-debug",
      developmentalConcern: "Find the organising cause of any verification failure and repair it at its owner",
      requiredDifference: "Each failure has a located cause and a repair proven at the level it failed",
      returnContract: "Return the causes, the repairs and the re-run evidence, or state that verification found no defect",
      subjectRef: subject,
      basisRevision: basis,
      agentRequirements: { agentRefs: ["agent/anima-mythos"], agentSetRefs: anima },
      praxisRefs: ["skill/personal/systematic-debugging", "skill/personal/test-driven-development"],
      capabilityRefs: ["capability/source-read", "capability/source-write", "capability/test-execution"],
      dependencies: ["eros-verify"],
      inputs: [{ predecessor: "eros-verify", receivingContextRef: "context:development-and-knowledge/debug-from-verify" }],
      permittedEffects: ["write repairs and regression tests on the feature branch"],
      verificationObligations: ["never weaken an assertion to turn a failure green"],
      returnAddress: returns,
      stopConditions: "Stop at once when verification returned no failure; return that finding with its evidence",
      escalationConditions: "Escalate a defect whose repair belongs to another owner",
      composition: {
        CPF: "authorised-undertaking", authority: placement,
        CT: "CT3", CP: "4.3", CF: "CF4", CFP: "CFP2", CS: "CS3", direction: "returning",
        actor: "agent/anima-mythos", interpretation: cprime, whole: whole,
        resolvePath: "resolve-scoped-path:development-and-knowledge/mythos",
        contextResolution: "context-resolution:development-and-knowledge/mythos",
        sources: [verification],
      } satisfies CPrime,
    }),
    unit({
      key: "sophia-finish",
      developmentalConcern: "Finish the branch: final checks, commit and a pull request ready for review",
      requiredDifference: "A reviewable pull request with its checks and an honest description",
      returnContract: "Return the pull request ref, the final check results and any open item with its reason",
      subjectRef: subject,
      basisRevision: basis,
      agentRequirements: { agentRefs: ["agent/anima-sophia"], agentSetRefs: anima },
      praxisRefs: ["skill/personal/finishing-a-development-branch", "skill/personal/verification-before-completion"],
      capabilityRefs: ["capability/source-read", "capability/git-branch", "capability/pull-request"],
      dependencies: ["mythos-debug"],
      inputs: [{ predecessor: "mythos-debug", receivingContextRef: "context:development-and-knowledge/finish-from-debug" }],
      permittedEffects: ["commit on the feature branch", "open a pull request; never merge"],
      verificationObligations: ["the final check results are from the pushed head"],
      returnAddress: returns,
      stopConditions: "Stop before merging; merge is the owner's review decision",
      escalationConditions: "Escalate a protected-branch or permission refusal",
      composition: {
        CPF: "authorised-undertaking", authority: placement,
        CT: "CT5", CP: "4.5", CF: "CF7", CFP: "CFP2", CS: "CS5", direction: "returning",
        actor: "agent/anima-sophia", interpretation: cprime, whole: whole,
        resolvePath: "resolve-scoped-path:development-and-knowledge/sophia",
        contextResolution: "context-resolution:development-and-knowledge/sophia",
        sources: [agents, verification],
      } satisfies CPrime,
    }),
    unit({
      key: "aletheia-knowledge-return",
      developmentalConcern: "Return what the development taught through the NOW field and the wiki return door",
      requiredDifference: "Durable learning leaves the session as an attributed Return pointing at its evidence",
      returnContract: "Return the NOW Return ref and any wiki promotion it proposes, each with its lineage",
      subjectRef: subject,
      basisRevision: basis,
      agentRequirements: { agentRefs: ["agent/aletheia"], agentSetRefs: ["agent-set/aletheia"] },
      praxisRefs: ["skill/ql/aletheia-stack-traverse", "skill/ql/thought-distil"],
      capabilityRefs: ["capability/now-return", "capability/wiki-return"],
      dependencies: ["sophia-finish"],
      inputs: [{ predecessor: "sophia-finish", receivingContextRef: "context:development-and-knowledge/return-from-finish" }],
      permittedEffects: ["write one attributed Return to the project NOW field", "propose a wiki promotion through NOW"],
      verificationObligations: ["the Return points at the pull request and its evidence, not a paraphrase"],
      returnAddress: returns,
      stopConditions: "Stop when nothing durable was learned; a routine record is enough",
      escalationConditions: "Never edit wiki.json directly; escalate promotion decisions to the wiki owner",
      composition: {
        CPF: "authorised-undertaking", authority: returnDoor,
        CT: "CT4b'", CP: "4.5", CF: "CF7", CFP: "CFP0", CS: "CS5", direction: "returning",
        actor: "agent/aletheia", interpretation: cprime, whole: whole,
        resolvePath: "resolve-scoped-path:development-and-knowledge/aletheia",
        contextResolution: "context-resolution:development-and-knowledge/aletheia",
        sources: [returnDoor, wikiLaw],
      } satisfies CPrime,
    }),
  ],
});
