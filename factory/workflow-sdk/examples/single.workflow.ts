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
  "workflowKey": "single-development",
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
    }
  ],
  "barriers": [],
  "nesting": []
});
