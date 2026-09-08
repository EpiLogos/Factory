---
name: ql-html-account
description: Create clear, research-grounded, self-contained HTML accounts for explanation, analysis, design intent, review, reference, documentation, and interactive dissemination. Use a canonical QL account structure, adaptive depth, ASD-STE100 writing discipline, and task-appropriate diagrams, data views, mockups, images, links, and interactions.
argument-hint: Describe the subject, audience, sources, purpose, desired depth, and any required surfaces or interactions.
---

# QL HTML Account

Create HTML that lets a reader understand, inspect, navigate, and use a body of information.

The output is an **account**: one coherent representational object. It can contain one QL surface, several selected surfaces, or the complete six-surface QL account. The medium is flexible. The account can explain a concept, document a system, present research, specify design intent, compare alternatives, review evidence, expose a prototype, or combine these functions.

The default deliverable is a **standalone HTML file**. All selected QL surfaces live inside that file as switchable views. Each view inherits the same shell but can use a different internal composition when the subject requires it.

## 1. Governing principles

1. **Meaning determines form.** Choose prose, diagrams, tables, charts, mockups, controls, images, or links because they reveal the relation at hand.
2. **The QL account supplies wholeness.** Use QL as a semantic coordinate and recursive composition grammar, not as a six-box layout template.
3. **Depth follows the task.** A full account can contain sustained technical prose, dense evidence, large diagrams, and working prototypes. Do not compress important reasoning into slogan cards.
4. **Research precedes assertion.** Ground consequential factual claims in supplied material or high-quality external sources.
5. **Writing is part of the interface.** Apply ASD-STE100 discipline and direct editorial judgement before styling the prose.
6. **The shell serves the content.** Navigation and context rails must never reduce the main document to leftover space.
7. **Interaction is direct.** Sliders, toggles, selectors, filters, and editable fields update the view immediately when immediate feedback is meaningful.
8. **Standalone is the publication default.** Reuse primitives while authoring; inline the required CSS, JavaScript, SVG, and local images in the final HTML.

---

# 2. The canonical QL account

The complete account has six outer coordinates. The names below are practical semantic readings for document composition. They do not claim to exhaust the invariant QL positions.

| QL | Practical basin | Primary question | Typical informational work |
|---|---|---|---|
| `#0` | Ground / Source | Why is this object or question here? | givens, origin, scope, problem, source state, assumptions, provenance, motivating evidence |
| `#1` | Definition / Material | What is it? | terms, entities, boundaries, constituents, exact definitions, requirements, object model |
| `#2` | Operation / Dynamis | How does it work or change? | mechanisms, procedures, transformations, algorithms, flows, interactions, executable prototypes |
| `#3` | Pattern / Identity | What structure or relation makes it this kind of thing? | topology, architecture, hierarchy, graph relations, recurring patterns, classification, invariants |
| `#4` | Context / Horizon | Where, when, and under which conditions does it meet a field? | evidence, environment, scenarios, constraints, comparisons, timelines, people, interfaces, validation |
| `#5` | Synthesis / Integration | What does the whole amount to and what returns forward? | conclusions, implications, decisions, recognition, outputs, next ground, compact reference |

A task does not need equal content at every coordinate. The full account is canonical; the actual document can make one surface small and another very deep.

## 2.1 Output scope

Choose the smallest account that is whole for the user's purpose:

- **Single surface** — one QL coordinate is sufficient.
- **Selected surfaces** — only the coordinates with material work are rendered.
- **Full account** — all six coordinates are rendered as switchable surfaces.

When the user asks for the fullest treatment, prefer the full account. A complete account can exceed 10,000 words when the evidence and subject warrant it.

Do not invent content to equalize the six surfaces.

---

# 3. Nested QL inside a surface

A substantial surface recurses through the same sixfold relation by default. In a full account, derive a nested `.0–.5` plan for every outer surface. This is semantic recursion, not decorative numbering.

For a parent surface `#q`, derive six local functions:

| Inner coordinate | Local function | Question to answer inside the parent |
|---|---|---|
| `.0` | local ground | What must already be true, available, or understood for this surface to begin? |
| `.1` | local definition | What becomes determinate here? Which object, term, input, or boundary is fixed? |
| `.2` | local operation | What acts, changes, relates, or transforms here? |
| `.3` | local pattern | What structure, recurrence, identity, or organisation becomes visible? |
| `.4` | local context | Under what field, conditions, perspective, evidence, or situation does this hold? |
| `.5` | local synthesis | What result, recognition, decision, or integrated form leaves this surface? |

### Nested naming rule

Derive **natural, task-specific titles** from these six functions before writing the surface.

For example, an outer `#2 Operation` surface about an API might internally become:

- `.0` Preconditions and state
- `.1` Request and response objects
- `.2` Execution path
- `.3` Failure and retry pattern
- `.4` Runtime conditions
- `.5` Observable result

The visible headings should normally use those natural titles. Keep `.0–.5` coordinates in the left contents tree for granular routing. Do not force coordinate numbers into every heading.

### Nested wholeness rule

A subnode is not complete because it has a heading. It must perform its QL function relative to the parent surface. The six inner functions remain part of the authoring plan even when two nodes share one visible block. Merge or hide a route only when the relation remains explicit in the account map.

---

# 4. Research and epistemic grounding

## 4.1 Source-first workflow

Before authoring factual, technical, medical, legal, scientific, historical, product, or current-state material:

1. Inspect all supplied sources.
2. Identify the claims the account must make.
3. Gather primary or high-trust sources for claims that need external grounding.
4. Separate source-supported statements from interpretation or inference.
5. Preserve unresolved disagreement rather than smoothing it away.
6. Keep a source ledger that can be projected into the HTML.

Prefer primary sources, official specifications, original papers, authoritative documentation, and the user's own canonical files.

## 4.2 Claim discipline

For consequential statements, be able to answer:

- What is the claim?
- Why is it present?
- Which source or evidence supports it?
- Is it observation, definition, inference, interpretation, recommendation, or decision?
- What would change its standing?

Use inline source links or citation markers where useful. Put a fuller source list in the relevant surface or provenance area.

## 4.3 External links

External links are first-class information components. Give them useful anchor text. When a source is important, include its title, publisher or domain, and why it matters.

Use `target="_blank" rel="noopener noreferrer"` for external links in generated standalone HTML.

---

# 5. Writing standard

Use **ASD-STE100 Simplified Technical English, Issue 9 (2025)** as the principal clarity discipline.

Official source: `https://www.asd-ste100.org/assets/files/ASD-STE100_ISSUE9.pdf`

The standard contains writing rules and a controlled dictionary. It permits subject-specific **technical nouns** and **technical verbs**, which are essential for precise specialist writing.

## 5.1 Default descriptive prose

Apply these rules unless a quotation, equation, code sample, proper name, or required technical form prevents it:

- Give information gradually.
- Use key words and key phrases to make the logical structure visible.
- Keep descriptive sentences to a maximum of **25 words**.
- Give each sentence one clear subject or idea where practical.
- Use paragraphs to group related information.
- Give each paragraph one topic.
- Keep a paragraph to a maximum of **six sentences**.
- Prefer active voice. Use passive voice when the agent is genuinely unknown or the passive form is technically more accurate.
- Use complete sentences. Do not remove necessary nouns or verbs to make prose shorter.
- Do not use contractions in formal explanatory text.
- Keep multi-word nouns to three words where practical. Define an official longer technical noun, then use a clear short form or approved abbreviation.
- Use the same term for the same thing. Do not rotate synonyms for style.
- Use technical terms when they carry necessary precision. Define them before relying on them.

## 5.2 Procedures and instructions

When a section tells the reader to do work:

- Keep procedural sentences to a maximum of **20 words**.
- Put one instruction in each sentence unless actions occur at the same time.
- Use the imperative form.
- Put a necessary condition before the command.
- Use notes for information only, not hidden instructions.

## 5.3 Controlled vocabulary and compliance

For ordinary accounts, apply the ASD-STE100 writing rules as a house discipline and use consistent subject-specific terminology.

Only describe the prose as **ASD-STE100 compliant** when the current official standard and dictionary were actually checked for the relevant text.

## 5.4 Editorial quality

After the STE pass, make a second editorial pass:

- Prefer concrete subjects and active verbs.
- Remove decorative intensifiers and empty transitions.
- Let evidence and explanation carry emphasis.
- Use metaphors only when they make a relation easier to understand.
- Keep rhetorical fragments rare.
- Use a claim card, pull quote, or large statement only when it performs a specific informational function.
- Keep the hero concise. Put depth in the body, not in repeated slogans.

Complex thought can use plain syntax without becoming simplistic.

---

# 6. Depth and prose capacity

Long-form prose is a first-class component.

Use these as planning ranges, not quotas:

- **Compact surface:** 300–700 words.
- **Standard surface:** 700–1,500 words.
- **Deep surface:** 1,500–3,500+ words.
- **Full account:** 4,000–20,000+ words depending on the source material and task.

A surface can contain several substantial prose sections. Do not replace a necessary argument with six cards or one diagram.

### Reading typography

Default long-form values:

- body prose: `16px–17px`
- line height: `1.65–1.75`
- normal reading measure: `72ch–80ch`
- paragraph spacing: approximately `0.9em–1.1em`
- subsection gap: `64px–112px`
- heading-to-first-content gap: `24px–36px`

Large figures, mockups, tables, and graphs can break out beyond the reading measure.

---

# 7. Representation routing

Do not build a component zoo. Select the representation that makes the required relation easiest to inspect.

| Information or relation | Preferred forms |
|---|---|
| sustained reasoning, qualification, explanation | prose, footnotes, citations |
| sequence, workflow, state transition | flowchart, timeline, state diagram |
| hierarchy, decomposition, ontology | tree, mindmap, nested outline |
| network, dependency, influence | graph, node-edge map |
| architecture, topology, spatial relation | bounded SVG diagram, layer map, system schematic |
| quantities across categories | bar chart |
| change over an ordered axis | line chart |
| distribution | histogram, dot plot, box plot when suitable |
| part-to-whole | stacked bar or donut only when the part-to-whole relation is important |
| exact comparison | table, comparison matrix |
| source trail, provenance | source cards, evidence table, timeline |
| code, schema, formula | code block, syntax block, equation panel |
| visual evidence | image, annotated figure, gallery |
| page or component design intent | actual HTML mockup or prototype |
| parameter-sensitive behaviour | slider, toggle, selector, direct-manipulation simulator |
| decision or review | alternatives, criteria, evidence, notes, recognition controls |

## 7.1 Diagram rules

- A diagram must answer a visual question.
- Use inline SVG for generated standalone artifacts.
- Give every SVG a real `viewBox`.
- Bound the visual inside a figure container.
- Let a large diagram use wide or full width.
- Make labels readable without browser zoom.
- Use arrows only when direction matters.
- Distinguish nodes by role before using decorative color.
- Provide a caption that explains what to inspect.
- If Mermaid is useful during drafting, render it to inline SVG before final export. Do not require a Mermaid runtime in the standalone file.

## 7.2 Graphs and mindmaps

Use a graph when relations matter more than order. Use a mindmap when a central concept fans into semantic branches.

Do not use a mindmap merely because a topic has headings.

## 7.3 Real HTML mockups

When the account discusses a UI, component, page, or interaction design, include the actual HTML/CSS mockup when possible. A schematic diagram does not substitute for a view that the reader can inspect and operate.

---

# 8. Interaction design

Interaction must change understanding, reveal information, or perform a useful operation.

Preferred behaviours:

- update sliders on `input`
- update toggles immediately
- filter tables and graphs immediately
- switch views without an Apply button
- update dependent diagrams and readouts together
- save notes while the user types when persistence is useful
- use lightweight retrieval or quiz controls only when the task involves learning or assessment
- use forms only when the user can actually enter or review meaningful information

Use an explicit Apply, Run, Submit, or Commit action only when the operation has a real transactional boundary.

Persistence is best-effort. If `localStorage` is unavailable, the document must continue to navigate and function.

---

# 9. Canonical shell

The fullest account uses one shell with three regions.

## 9.1 Left rail — addressability

The left rail contains:

- the six outer QL surfaces
- a disclosure toggle for each surface
- the nested `.0–.5` routes for the surface
- active-route highlighting
- granular hash navigation

The reader can collapse the rail. The center must reclaim its width immediately.

## 9.2 Center — the account

The center is the primary artifact.

It owns the width required by the current surface. Each surface can set its own maximum width. Long prose stays within a readable measure. Diagrams and mockups can break out to wide or full spans.

Use CSS container queries for compositions that depend on the **actual available main width**, not only the browser width.

## 9.3 Right rail — situated context

The right rail is optional and section-relative. It can contain:

- notes or comments
- form fields
- current coordinate and section context
- source links
- review controls
- provenance
- confidence or status controls

Do not show an empty right rail. The center must reclaim the space when the rail is closed or absent.

## 9.4 Responsive behaviour

Recommended default:

- wide desktop: both rails can dock
- medium desktop: left rail can dock; right rail becomes an overlay
- tablet and narrow embedded viewers: both rails become overlays
- mobile: center uses the full viewport; rails become drawers

Test the **main container width after rail allocation**. Do not assume that browser width equals content width.

---

# 10. Theme and visual language

Use semantic tokens rather than hard-coded decorative colors.

### Light theme

- clean white or very pale cool gray paper
- dark blue-gray text
- structural blue
- restrained teal accent
- quiet gray rules and panels

### Dark theme

- midnight blue paper
- pale blue-gray text
- blue and teal structural accents
- restrained burnt or dark orange for warm emphasis

Preserve the same hierarchy across themes.

### Typography

Use a readable serif for sustained prose and a restrained system sans for navigation, metadata, controls, tables, and labels.

Do not depend on remote fonts in the default standalone output.

---

# 11. Component grammar

These are composable primitives, not required sections:

- long prose
- lede
- definition block
- claim or key distinction
- note / warning / caveat
- evidence block
- source card
- table
- comparison matrix
- timeline
- flowchart
- architecture diagram
- relation graph
- mindmap
- quantitative chart
- equation
- code or schema
- image / figure / gallery
- UI mockup
- slider
- toggle
- selector
- filter
- editable field
- section-relative notes
- review or decision controls

Use as few primitives as the account needs.

---

# 12. Provenance and filing

Every generated account should carry a machine-readable provenance envelope.

Recommended fields:

```json
{
  "uid": "account-...",
  "kind": "html-account",
  "title": "...",
  "project": "...",
  "session": "...",
  "status": "draft|review|recognised",
  "ql": [0, 1, 2, 3, 4, 5],
  "created": "ISO-8601 timestamp",
  "updated": "ISO-8601 timestamp",
  "source_basis": [],
  "relations": [],
  "generator": "..."
}
```

Embed this as `application/json` in the HTML. Show only the useful subset in the visible shell.

If a session identifier is known, include it. Use an explicit timezone in timestamps.

---

# 13. Standalone export contract

A default generated HTML file must work when it is opened by itself.

Therefore:

- inline all required CSS
- inline all required JavaScript
- use inline SVG for generated diagrams
- embed required local raster images as data URIs
- do not depend on sibling CSS, JavaScript, fonts, or images
- do not depend on a local server
- allow ordinary external links to sources and related documents

If the user explicitly requests a deployable multi-file package, a shared-assets build can also be produced. Each requested inspectable HTML should still have a standalone build unless the user says otherwise.

---

# 14. Accessibility and semantics

- Use landmarks: `header`, `nav`, `main`, `aside`, `section`, `figure`.
- Use real buttons for controls.
- Preserve keyboard operation.
- Show visible focus states.
- Use `aria-expanded` for disclosure controls.
- Use `aria-current` for active routes.
- Give images useful `alt` text.
- Give diagrams captions.
- Do not use color as the only carrier of meaning.
- Respect `prefers-reduced-motion`.
- Keep text selectable and copyable.

---

# 15. Navigation behaviour

The fullest single-file account uses hash routes such as:

```text
#q0
#q2/execution-path
#q4/evidence
```

Requirements:

- browser back/forward must work
- changing outer surface must update the URL hash
- nested route clicks must switch to the correct surface and scroll to the correct section
- active section tracking must remain correct at the bottom of a surface
- expanding a surface in the left rail must not automatically change the active surface unless the user selects it
- surface disclosure state can persist independently from the active route

---

# 16. Authoring workflow

## Step 1 — Resolve the purpose

Identify:

- audience
- intended use
- required depth
- source basis
- current-state vs historical material
- whether the artifact is explanatory, analytical, procedural, design-oriented, review-oriented, or mixed

## Step 2 — Research before layout

Gather the material needed to make the account true and useful. Do not use the HTML prototype as a substitute for research.

## Step 3 — Make the outer QL account map

For each `#0–#5`, write one sentence that states the surface's actual job for this subject.

Do not start writing sections until these six jobs are coherent.

## Step 4 — Derive nested QL where needed

For each substantial surface, derive six task-native inner functions from `.0–.5`.

Do not copy generic section titles into the page.

## Step 5 — Choose representation per relation

For every planned block, ask:

> What does the reader need to see, understand, compare, manipulate, or verify here?

Choose the medium from that question.

## Step 6 — Draft the prose at full required depth

Write the explanation before reducing it into labels and components. Apply the ASD-STE100 pass after the technical content is correct.

## Step 7 — Compose the standalone HTML

Start from `full-account-template.html`. Remove unused surfaces or components rather than adding artificial content.

## Step 8 — Validate

Run the validation checklist below.

---

# 17. Validation checklist

## Content and QL

- [ ] The account's purpose is explicit.
- [ ] Each rendered outer QL surface has a distinct semantic job.
- [ ] Nested `.0–.5` nodes perform their local QL functions.
- [ ] Visible nested headings use task-native language.
- [ ] The account has enough prose depth for the requested level.
- [ ] Cards and large statements do not replace necessary explanation.
- [ ] Every visual answers a real question.
- [ ] Consequential factual claims have source support.
- [ ] Inference and interpretation are distinguishable from sourced fact.

## Writing

- [ ] Descriptive sentences normally stay within 25 words.
- [ ] Procedural sentences stay within 20 words.
- [ ] Paragraphs contain one topic and no more than six sentences.
- [ ] Terminology is consistent.
- [ ] Technical terms are defined when necessary.
- [ ] Active voice is the default.
- [ ] Prose uses complete sentences and no contractions in formal descriptive text.
- [ ] Decorative slogans and empty transitions have been removed.

## Layout

Test at minimum:

- [ ] `1600px` wide
- [ ] `1180px` wide
- [ ] `900px` wide
- [ ] `720px` wide
- [ ] `390px` wide

At each size:

- [ ] no page-level horizontal overflow
- [ ] main width responds to actual docked rail state
- [ ] closing a rail immediately gives its width back to the center
- [ ] diagrams remain bounded
- [ ] prose remains readable
- [ ] heading-to-component spacing remains clear

## Interaction

- [ ] no JavaScript errors
- [ ] theme switch works
- [ ] rail toggles work
- [ ] browser back/forward routing works
- [ ] bottom-of-surface section tracking selects the final section correctly
- [ ] sliders and toggles update on direct input
- [ ] blocked `localStorage` does not break core navigation
- [ ] keyboard focus is visible

## Standalone integrity

- [ ] no required local stylesheet dependency
- [ ] no required local script dependency
- [ ] no required local font dependency
- [ ] no required local image dependency
- [ ] every inline SVG has a `viewBox`
- [ ] all internal fragment routes resolve

---

# 18. Template use

Use `full-account-template.html` as the maximal base.

It contains:

- the complete six-surface QL shell
- dynamically generated left-hand contents and nested toggles
- section-relative right-rail context
- light and dark themes
- robust hash routing
- best-effort note persistence
- adaptive docked/overlay rails
- long-form prose typography
- wide and full-width breakout regions
- a hidden component library with reusable HTML patterns
- print behaviour
- machine-readable provenance

Keep the account architecture stable. Change the actual content, nested headings, diagrams, controls, and proportions according to the task.


---

# 19. Design lineage

This skill generalises several useful properties of Matt Pocock's `/teach` skill without making the account learning-specific:

- HTML is a durable artifact, not disposable chat formatting.
- High-trust sources should ground consequential content.
- A consistent visual system should support many outputs.
- Reusable components are better than repeated one-off code.
- Reference surfaces should be attractive, readable, and easy to return to.

Teaching-specific mechanisms such as missions, retrieval practice, quizzes, or learning records are optional components. Use them only when the task is actually educational.

Primary reference: `https://github.com/mattpocock/skills/blob/main/skills/productivity/teach/SKILL.md`

## QL source basis

The QL composition in this skill follows the supplied Epi-Logos / QL corpus:

- `epi_logos_coordinate_system.md`
- `mef-12-lenses-sublens-reference.md`
- `QL-SOFTWARE-FACTORY-ARCHITECTURE-SPEC.md`

The practical document labels `Ground / Definition / Operation / Pattern / Context / Synthesis` are semantic readings used for composition. The deeper QL positions remain prior to any one document vocabulary.
