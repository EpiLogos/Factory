import { defineWorkflow } from "@epilogos/factory-workflow";

// Native identity references in this specimen are example inputs, not installed participants.
// Replace them with actual owner-resolved identities before attempting execution.
export default defineWorkflow({
  "source": {
    "ref": "workflow-source:01ARZ3NDEKTSV4RRFFQ69G5FAV",
    "revision": "source-revision-7",
    "temporalRef": "flow-time:2026-09-08",
    "flowRef": "flow:factory-197"
  },
  "workflowKey": "agent-first-implementation",
  "units": [
    {
      "key": "inspect-source",
      "developmentalConcern": "Recover the current Factory architecture and source obligations",
      "requiredDifference": "The implementation basis is explicit and evidenced",
      "returnContract": "Return the inspected seams and exact source basis",
      "subjectRef": "project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
      "basisRevision": "947ce7a",
      "agentRequirements": {
        "agentRefs": [
          "agent:01ARZ3NDEKTSV4RRFFQ69G5FAX"
        ],
        "agentSetRefs": [],
        "agencyRefs": []
      },
      "praxisRefs": [
        "skill:01ARZ3NDEKTSV4RRFFQ69G5FAY"
      ],
      "capabilityRefs": [
        "capability:01ARZ3NDEKTSV4RRFFQ69G5FAZ"
      ],
      "dependencies": [],
      "independenceFrom": [],
      "permittedEffects": [
        "read Factory source"
      ],
      "verificationObligations": [
        "cite exact files and revisions"
      ],
      "returnAddress": "return:01ARZ3NDEKTSV4RRFFQ69G5FB0",
      "stopConditions": "Stop if the requested base is unavailable",
      "escalationConditions": "Escalate contradictory source contracts"
    },
    {
      "key": "implement-compiler",
      "developmentalConcern": "Compile open Agent-first workflow source into stable Factory units",
      "requiredDifference": "A deterministic validated compiler feeds the existing RunMap path",
      "returnContract": "Return production code and functional verification evidence",
      "subjectRef": "project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
      "basisRevision": "947ce7a",
      "agentRequirements": {
        "agentRefs": [],
        "agentSetRefs": [
          "agent-set:01ARZ3NDEKTSV4RRFFQ69G5FB1"
        ],
        "agencyRefs": []
      },
      "praxisRefs": [
        "skill:01ARZ3NDEKTSV4RRFFQ69G5FB2"
      ],
      "capabilityRefs": [
        "capability:01ARZ3NDEKTSV4RRFFQ69G5FB3"
      ],
      "dependencies": [
        "inspect-source"
      ],
      "independenceFrom": [
        "review-adversarially"
      ],
      "permittedEffects": [
        "write Factory workflow compiler",
        "write direct contracts and tests"
      ],
      "verificationObligations": [
        "compile checked-in source",
        "preserve Run mutation authority"
      ],
      "returnAddress": "return:01ARZ3NDEKTSV4RRFFQ69G5FB4",
      "stopConditions": "Stop before creating a second execution runner",
      "escalationConditions": "Escalate unresolved ownership overlap"
    },
    {
      "key": "review-adversarially",
      "developmentalConcern": "Challenge identity and graph compilation under hostile inputs",
      "requiredDifference": "Malformed provenance and graph ambiguity fail closed",
      "returnContract": "Return adversarial findings and reproducible failures",
      "subjectRef": "project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
      "basisRevision": "947ce7a",
      "agentRequirements": {
        "agentRefs": [
          "agent:01ARZ3NDEKTSV4RRFFQ69G5FB5"
        ],
        "agentSetRefs": [],
        "agencyRefs": []
      },
      "praxisRefs": [
        "skill:01ARZ3NDEKTSV4RRFFQ69G5FB6"
      ],
      "capabilityRefs": [
        "capability:01ARZ3NDEKTSV4RRFFQ69G5FB7"
      ],
      "dependencies": [
        "inspect-source"
      ],
      "independenceFrom": [
        "implement-compiler"
      ],
      "permittedEffects": [
        "read implementation",
        "execute direct tests"
      ],
      "verificationObligations": [
        "exercise cycle and provenance attacks"
      ],
      "returnAddress": "return:01ARZ3NDEKTSV4RRFFQ69G5FB8",
      "stopConditions": "Stop before mutating implementation files",
      "escalationConditions": "Escalate any identity instability"
    },
    {
      "key": "integrate-return",
      "developmentalConcern": "Integrate implementation and independent review evidence",
      "requiredDifference": "One bounded return reports a verified integration order",
      "returnContract": "Return exact base, changes, checks, gaps, and integration order",
      "subjectRef": "project:01ARZ3NDEKTSV4RRFFQ69G5FAW",
      "basisRevision": "947ce7a",
      "agentRequirements": {
        "agentRefs": [],
        "agentSetRefs": [],
        "agencyRefs": [
          "agency:01ARZ3NDEKTSV4RRFFQ69G5FB9"
        ]
      },
      "praxisRefs": [
        "skill:01ARZ3NDEKTSV4RRFFQ69G5FBA"
      ],
      "capabilityRefs": [
        "capability:01ARZ3NDEKTSV4RRFFQ69G5FBB"
      ],
      "dependencies": [],
      "independenceFrom": [],
      "permittedEffects": [
        "publish bounded return"
      ],
      "verificationObligations": [
        "include exact test results"
      ],
      "returnAddress": "return:01ARZ3NDEKTSV4RRFFQ69G5FBC",
      "stopConditions": "Stop before merge, push, or pull request creation",
      "escalationConditions": "Escalate unresolved owner dependencies"
    }
  ],
  "barriers": [
    {
      "key": "implementation-reviewed",
      "waitsFor": [
        "implement-compiler",
        "review-adversarially"
      ],
      "releases": [
        "integrate-return"
      ]
    }
  ],
  "nesting": []
});
