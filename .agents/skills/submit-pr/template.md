# PR Body Template

Fill every section from the diff evidence. Use the exact section names below; do not rename or omit a section. Keep each section concise and specific — reference concrete files, symbols, and behaviors from the diff, never generic filler. Replace each `<...>` placeholder with real content; never leave a placeholder or a section blank.

## Why

<Why this PR is created: the problem or motivation it addresses, and the outcome it delivers. Ground it in the actual change, not a restatement of the title.>

## Review focus

<What needs a human reviewer's attention: the parts that are hard to verify by tests alone, subtle or non-obvious logic, assumptions to confirm, edge cases, and anything the author is unsure about. Point the reviewer at the specific files and decisions that matter most.>

## Risks

Risks related to **performance**, **security**, and **complexity** this change introduces or could cause.

- **Performance:** <regressions or hot-path impact, or "none" if genuinely none — say why.>
- **Security:** <exposure or trust boundary, or "none" — and if "none", confirm no security surface is touched.>
- **Complexity:** <new abstraction, state, coupling, or maintenance cost, or "none".>

**Risk level: risk:<high|medium|low>** — <one-line justification matching the classification in step 3.>
