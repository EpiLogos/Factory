# Curated support-prompt map

Every one of the 32 support prompts has a distinct caller, operation, input/output and standing. Support prompts are never independently invoked.

## `skills/architect/references/design-red-flags.md`
- Purpose: pre-synthesis rejection screen
- Input: candidate design package
- Output: red-flag findings with evidence
- Caller: architect synthesis
- Standing: deferred-static: review aid, not an independent capability
- Evidence: lines 1–33, SHA-256 `905066f9bbac81c573c2b325be47751d2c0f9325e0a0772bd4384e82dafe9336`

## `skills/architect/references/rationale-template.md`
- Purpose: rationale package schema
- Input: problem, usage, shape and alternatives
- Output: completed rationale and open questions
- Caller: architect candidate / arena synthesis
- Standing: deferred-static: template constrains output only
- Evidence: lines 1–35, SHA-256 `6645a0e5f68c003298ec95b85a23262bdda0c79f998b926060169b54d9f23fbb`

## `skills/architect/references/runner-prompt.md`
- Purpose: parallel architecture candidate brief
- Input: task, grounding, worktree and output path
- Output: candidate design package
- Caller: architect orchestrator
- Standing: deferred-static: host runner prompt; no direct agency authority
- Evidence: lines 1–20, SHA-256 `3ef8c1452a0382a15b7601c7c94f2a16a00954170d705b7af8cc49ac78080ec9`

## `skills/create-verification-skill/references/feature-map-example/README.md`
- Purpose: verification map contract
- Input: app baseline, entry points and proof rules
- Output: feature map index and proof obligations
- Caller: create-verification-skill
- Standing: deferred-static: example domain; adapt only with commissioned target
- Evidence: lines 1–47, SHA-256 `cb7bd782cf89968a4ba3d58a5151db837430db92d19a6f52a906973b77b516ba`

## `skills/create-verification-skill/references/feature-map-example/create-note.md`
- Purpose: Notes create behavior recipe
- Input: disposable Notes instance and seeded state
- Output: browser/CLI action receipts and persistence proof
- Caller: feature-map README
- Standing: deferred-static: illustrative app procedure
- Evidence: lines 1–39, SHA-256 `644a44c74f35d38c2feb7cd05a0121fdebbb623e8dc376c185b565a581d1ccf7`

## `skills/create-verification-skill/references/feature-map-example/search.md`
- Purpose: Notes search behavior recipe
- Input: disposable Notes instance and query cases
- Output: match, empty, clear and CLI proof
- Caller: feature-map README
- Standing: deferred-static: illustrative app procedure
- Evidence: lines 1–45, SHA-256 `6e87b9e7f2791a7776ba1bb83f371cd285c306c67f19d245cc4dd6ca3015c823`

## `skills/how/references/critic-prompt.md`
- Purpose: architectural critique prompt
- Input: explanation, files and rubric
- Output: severity-weighted findings with evidence
- Caller: how orchestrator
- Standing: deferred-static: prompt carrier
- Evidence: lines 1–59, SHA-256 `aa77a15590814c814bd5cb074842029ab7b790c2e8379050525bdbc20739d1bb`

## `skills/how/references/critique-rubric.md`
- Purpose: critique evaluation rubric
- Input: candidate explanation and code evidence
- Output: structural/concern/observation judgment
- Caller: critic prompt
- Standing: deferred-static: rubric is caller context
- Evidence: lines 1–58, SHA-256 `5774ff2caf51796e54c4a61b63620a165de5801f23c8f47406e527abfbe9addc`

## `skills/how/references/explainer-prompt.md`
- Purpose: how explanation prompt
- Input: question, code anchors and findings
- Output: evidence-bounded explanation
- Caller: how orchestrator
- Standing: deferred-static: prompt carrier
- Evidence: lines 1–55, SHA-256 `04336086e7c415394459ca2f461717d920e15339517dbf5a56ca042f8b4fead3`

## `skills/how/references/explorer-prompt.md`
- Purpose: how investigation prompt
- Input: target and repository context
- Output: raw architectural observations
- Caller: how orchestrator
- Standing: deferred-static: prompt carrier
- Evidence: lines 1–52, SHA-256 `fd1218b8cf1f467c644d4ce6fa5e54e6089ede65a28fbbae61f6ee7a2d462f85`

## `skills/interrogate/references/code-quality-review.md`
- Purpose: code quality review lens
- Input: changed code and local standards
- Output: review findings and evidence
- Caller: interrogate lead
- Standing: deferred-static: rubric context
- Evidence: lines 1–47, SHA-256 `2462f1347b99b412b04fcb0577b96d8744c801f792f2651746f9409fd4465dc9`

## `skills/interrogate/references/lead-judgment.md`
- Purpose: review lead decision schema
- Input: reviewer findings and disagreement
- Output: accept/revise/escalate judgment
- Caller: interrogate lead
- Standing: deferred-static: decision aid; cannot grant merge authority
- Evidence: lines 1–58, SHA-256 `d2cea6cc308758201c6b8b82baf780947645f1ab752707ddf97fab374bf473f9`

## `skills/interrogate/references/reviewer-prompt.md`
- Purpose: independent code review brief
- Input: scope, diff and review question
- Output: attributed reviewer findings
- Caller: interrogate orchestrator
- Standing: deferred-static: prompt carrier
- Evidence: lines 1–72, SHA-256 `a397cc61102add709803d917fb23d726920525bf23e7c06dc4ed0b5cbeb00e54`

## `skills/interrogate/references/rubric.md`
- Purpose: interrogate review rubric
- Input: diff, tests and stated requirements
- Output: severity and evidence standard
- Caller: reviewer prompt
- Standing: deferred-static: caller context
- Evidence: lines 1–77, SHA-256 `a67bf02426f88714634ff481d667db821d4b2cea7b335bcd165fe3126e427fb5`

## `skills/poteto-mode/references/bugbot-triage.md`
- Purpose: bugbot finding triage lens
- Input: automated finding and code context
- Output: valid/invalid/defer classification
- Caller: bug-fix or review playbook
- Standing: deferred-static: external bot output requires human/owner authority
- Evidence: lines 1–142, SHA-256 `b2d146c770d8d3593e1ef3d77b668039b8e1f78093ea1a86f1d45d7f99614309`

## `skills/reflect/references/divergent-reviewer.md`
- Purpose: divergence review role
- Input: work result and intended outcome
- Output: mismatches and alternative readings
- Caller: reflect orchestrator
- Standing: deferred-static: reviewer prompt
- Evidence: lines 1–43, SHA-256 `823167d485c8fc424530ac4eff447f132a10e318a2db35380312c7e843f17683`

## `skills/reflect/references/judgment-reviewer.md`
- Purpose: judgment review role
- Input: decisions, evidence and constraints
- Output: quality and authority assessment
- Caller: reflect orchestrator
- Standing: deferred-static: reviewer prompt
- Evidence: lines 1–42, SHA-256 `8db7a315ccf9280486b4df08b1277ba0fe4c4ac533ad3719945aa1aab2ebddea`

## `skills/reflect/references/synthesizer.md`
- Purpose: reflection synthesis role
- Input: reviewer findings and original intent
- Output: attributed learning and unresolved questions
- Caller: reflect orchestrator
- Standing: deferred-static: synthesis prompt
- Evidence: lines 1–56, SHA-256 `52767b4566c37df0adc6350fff1c5c7a919a1178c2be539e0efbf920f2803702`

## `skills/reflect/references/tooling-reviewer.md`
- Purpose: tooling review role
- Input: commands, artifacts and receipts
- Output: tool/process deviations
- Caller: reflect orchestrator
- Standing: deferred-static: reviewer prompt
- Evidence: lines 1–57, SHA-256 `3a4dc9e87d0d74b46d0d0bc2c1af6acba10be7c95d1adafeab6ec5701e247933`

## `skills/typescript-best-practices/references/patterns.md`
- Purpose: TypeScript pattern reference
- Input: target code and language constraints
- Output: applicable pattern guidance
- Caller: typescript-best-practices
- Standing: deferred-static: language context only
- Evidence: lines 1–313, SHA-256 `eb73b5f3514c0c14d14bafca59ffb54b161422ea5fc5fe65e60c5b4d71c40178`

## `skills/why/references/epistemics.md`
- Purpose: why claim standing framework
- Input: investigator evidence and source provenance
- Output: Direct/Supported/Inferred/Speculative/Unknown classification
- Caller: why synthesizer
- Standing: deferred-static: epistemic rule set, no external effect
- Evidence: lines 1–144, SHA-256 `ba99dcdc4fbce91a83b50fc0c9b2c5769febd95540b1722c6b87fe8f900bb60a`

## `skills/why/references/investigator-prompt.md`
- Purpose: why evidence investigator brief
- Input: question, code anchor and assigned source
- Output: cited findings plus search gaps
- Caller: why orchestrator
- Standing: deferred-static: external-source prompt
- Evidence: lines 1–103, SHA-256 `e35d8eaea13f0a027dd35572d525e9f66e8f8918ca9e555237dc228f26278d5f`

## `skills/why/references/source-playbook.md`
- Purpose: why source search procedure
- Input: target symbols, dates and available connectors
- Output: source-specific evidence or explicit gap
- Caller: why investigator
- Standing: deferred-static: connector-dependent procedure
- Evidence: lines 1–17, SHA-256 `3cfb0245d5b2019eeeaa7a0e023579d390f2cdcdff6942d2fe009b9efac0eb11`

## `skills/why/references/sources/code-archaeology.md`
- Purpose: git history evidence lens
- Input: target files, symbols and dates
- Output: commits, diffs and author rationale
- Caller: why investigator
- Standing: deferred-static: evidence source guidance
- Evidence: lines 1–88, SHA-256 `8c14aa382f0de40829b2668d43e3f54814480bb93a486c9282ada4003f6b9dbd`

## `skills/why/references/sources/databricks.md`
- Purpose: analytics evidence lens
- Input: error/user event terms and time window
- Output: product impact correlation or gap
- Caller: why investigator
- Standing: deferred-static: external connector guidance
- Evidence: lines 1–70, SHA-256 `269a22032f027ee30a33c1b5e30443d13dd6d127ff20d8d1e8a20715b06a69cc`

## `skills/why/references/sources/datadog.md`
- Purpose: observability evidence lens
- Input: logs, metrics, traces and incident window
- Output: operational correlation or gap
- Caller: why investigator
- Standing: deferred-static: external connector guidance
- Evidence: lines 1–99, SHA-256 `db80803f1635a5015e3190f4753c4228083e6cebf222c84e24fc3c3919aa5574`

## `skills/why/references/sources/incident-postmortem.md`
- Purpose: incident motivation evidence lens
- Input: defensive code anchor and dates
- Output: postmortem/action-item correlation or gap
- Caller: why investigator
- Standing: deferred-static: external connector guidance
- Evidence: lines 1–15, SHA-256 `486132457d3782eea64292ce7a2d3bbe3b24b372b20ca00de4329e820f74cd42`

## `skills/why/references/sources/linear.md`
- Purpose: ticket motivation evidence lens
- Input: ticket IDs, terms and project context
- Output: rationale, scope and decision evidence or gap
- Caller: why investigator
- Standing: deferred-static: external connector guidance
- Evidence: lines 1–48, SHA-256 `7e1b72ba459089168efd701502f8c268d2481154aeeda1ddd84f840f7e974760`

## `skills/why/references/sources/notion.md`
- Purpose: long-form rationale evidence lens
- Input: feature terms, authors and dates
- Output: PRD/spec/ADR evidence or gap
- Caller: why investigator
- Standing: deferred-static: external connector guidance
- Evidence: lines 1–55, SHA-256 `2ce336dd3052967187a56ec33d308c7a69e7ef3b04ba7b17386a8b3914a0f000`

## `skills/why/references/sources/sentry.md`
- Purpose: error history evidence lens
- Input: exception, release and target symbols
- Output: temporal error/fix correlation or gap
- Caller: why investigator
- Standing: deferred-static: external connector guidance
- Evidence: lines 1–100, SHA-256 `0eef902cb0f835f7fa9bd85a8e19ba1505d05e59a0173dfe852d8d52d828266d`

## `skills/why/references/sources/slack.md`
- Purpose: conversation evidence lens
- Input: author, terms, channels and date window
- Output: thread decision evidence or retention/auth gap
- Caller: why investigator
- Standing: deferred-static: external connector guidance
- Evidence: lines 1–54, SHA-256 `60e4c8dbb941f563c86aec7267cde66443b1dc651422569c8d112bc9da7551f6`

## `skills/why/references/synthesizer-prompt.md`
- Purpose: confidence-weighted why synthesis
- Input: question, code anchor, findings and skipped sources
- Output: cited narrative with calibrated uncertainty
- Caller: why orchestrator
- Standing: deferred-static: output prompt; claims still require evidence
- Evidence: lines 1–135, SHA-256 `fcdc69ede22f94da9e22f510bb0c4a75a63820f0b49b8d5fd54a196b4010bf2a`
