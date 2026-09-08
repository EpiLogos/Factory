# Investigate and repair from evidence

Recover a concrete failing condition and the matching runtime surface. Drive available inspection yourself. When the target cannot be reached, report the specific access or instrumentation limit and retain investigation standing; do not claim the defect is proved fixed.

Read the affected source and relevant history to form competing explanations. Choose an observation that distinguishes them. Use runtime instrumentation when state is otherwise unclear, and discard changes motivated by a refuted explanation. Confirm the mechanism before designing a wider repair: a plausible cause does not become true through agreement.

Define the smallest repair justified by that evidence and the whole it must preserve. Use design/review capability when coupling, uncertainty or consequence warrants it; crossing a function boundary alone does not require a panel. Delegate only a bounded subject with its inputs, authority and expected evidence.

Keep before/after proof. Where a cheap real discriminator is available, observe it fail before the repair and pass afterward. Otherwise retain the actual matching-surface reproduction and its conditions. A wrong-surface or inconclusive check remains unresolved. Do not manufacture a unit test whose passing substitutes for an unavailable integration proof.

Inspect the diff and rerun affected obligations. Return the defect, causal evidence, actual fix, exact verification and remaining limits. Preserve unrelated work; do not rebase, commit or open a PR merely to enact a narrative sequence.
