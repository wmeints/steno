---
name: submit-pr
description: "Create a pull request for the current feature branch on GitHub. Use when the user wants to open, submit, or push a PR for the current branch. Drafts a PR body with Why / Review Focus / Risks sections, classifies the change into a risk level, and applies the matching risk:high, risk:medium, or risk:low label."
---

# Submit PR

Create a pull request for the current feature branch. The body always carries three sections — **Why**, **Review focus**, **Risks** — and the PR is tagged with exactly one risk label: `risk:high`, `risk:medium`, or `risk:low`, chosen from what the change actually does.

## Tooling

Use the `gh` CLI (GitHub CLI) and `git`. Never hand-roll PR creation with `curl` or the GitHub API.

Verify `gh` is available and authenticated at the start:

```bash
gh auth status
```

If it reports unauthenticated, stop and tell the user to run `gh auth login`.

## Steps

### 1. Identify the branch and base

```bash
git rev-parse --abbrev-ref HEAD
git branch -vv
```

- **Head branch**: the current branch (`git rev-parse --abbrev-ref HEAD`).
- **Base branch**: infer the branch the head tracks or is relative to. Check `git branch -vv` for the upstream, then fall back to the repository default (`main`/`master`) via `gh repo view --json defaultBranchRef -q .defaultBranchRef.name`. Confirm the base with the user if it is ambiguous.
- **Guard against the default branch**: if the current branch is the base itself (e.g. you are on `main`), stop and ask — there is nothing to open a PR for.

### 2. Gather the diff and commit history

Collect what changed relative to the base:

```bash
git fetch origin
git diff origin/<base>...HEAD --stat
git diff origin/<base>...HEAD
git log origin/<base>..HEAD --pretty=format:'%h %s'
```

- If the branch is not yet pushed, use `git diff origin/<base>...HEAD` on the local refs and note the branch needs pushing in step 6.
- Read the full diff (`git diff origin/<base>...HEAD`) to classify risk accurately. The `--stat` view is for orientation; the full diff is the evidence for the three sections and the risk label.

### 3. Determine the risk level

Classify the change into exactly one of `risk:high`, `risk:medium`, `risk:low`. Apply the rules in priority order — **the highest applicable level wins**.

**`risk:high` — security-relevant change.** The change may cause security issues. Any single one of these in the diff forces `high`:

- Authentication, authorization, session, or permission logic; login, logout, token, or credential handling.
- Encryption, decryption, key management, signing, or secret material.
- Handling of external / untrusted input: parsing, deserialization, network, or command / shell / SQL construction.
- Anything that could introduce an injection (SQL, command, path traversal, template, LDAP, or similar) or an exposure (secrets, tokens, PII, private data).
- Dependency or lockfile changes that pull in new third-party code (supply-chain exposure).
- CI/CD, build, or release pipeline changes that execute code or grant permissions.
- Anything a reviewer would need to verify against an attack or exploit — when uncertain whether a change touches security, treat it as `high` and surface the doubt in **Review focus**.

**`risk:medium` — complex but not security-relevant.** No security trigger from `high`, and the change is complex enough that correctness is non-obvious:

- Non-trivial control flow, state machines, concurrency, or race conditions.
- Data-structure or algorithm changes; caching, batching, or performance-sensitive code.
- Changes spanning many files or multiple modules.
- Migration or schema changes; data transformation logic.
- Public API, interface, or public contract changes.
- Anything where a subtle bug is plausible and would need careful reading.

**`risk:low` — simple, no security concern.** No `high` trigger and no `medium` complexity:

- Documentation, comments, or config with no behavioral effect.
- Simple, isolated, self-evidently-correct changes: a few lines, a small refactor, a typo fix, a constant rename.
- Changes a reviewer can verify in seconds.

Record the chosen level and the reason. If the change sits on a boundary, choose the higher level and note the ambiguity in **Review focus**.

### 4. Write the PR body

Build the body from the diff evidence using the template in `template.md` (read it via `skill://submit-pr/template.md`). Do not rename or omit a section, keep each section concise and specific (reference concrete files, symbols, and behaviors from the diff, never generic filler), and replace every `<...>` placeholder with real content — never leave a placeholder or a section blank. The template contains:

- **Why** — the motivation and the outcome delivered.
- **Review focus** — what a human reviewer must look at.
- **Risks** — performance, security, and complexity, each `none`-justified when genuinely untouched.
- A final **Risk level** line stating the label chosen in step 3 and its justification.

Guidance:

- **Why** must be distinct from a title restatement — explain the motivation and the delivered outcome.
- **Review focus** is for a human, not a diff summary. If there is genuinely nothing a reviewer must look at, say so, but prefer pointing at the riskiest or most subtle code.
- **Risks** must cover all three axes (performance, security, complexity). When an axis is genuinely untouched, write `none` and briefly justify it — never leave an axis blank.
- The final **Risk level** line states the label chosen in step 3 and why.

### 5. Create the risk label if it does not exist

The PR must carry exactly one of `risk:high`, `risk:medium`, `risk:low`. Check whether the label exists; create it with a sensible color if missing (so the label survives across repos and does not fail the create):

```bash
RISK_LABEL="risk:<level>"
if ! gh label list --json name | grep -q "\"$RISK_LABEL\""; then
  # color: high=red, medium=orange, low=green
  gh label create "$RISK_LABEL" --color "c0542e" --description "Change introduces security-relevant risk" || true
fi
```

Map the level to a color and description:

| Level | Color | Description |
| --- | --- | --- |
| `risk:high` | `c0542e` (red) | Change introduces security-relevant risk |
| `risk:medium` | `e08a2f` (orange) | Change is complex but not security-relevant |
| `risk:low` | `0ca678` (green) | Change is simple and not security-relevant |

If `gh label create` fails (for example, insufficient permission to create labels), do not block: skip label creation and continue to step 6, then report that the label could not be created and that the user should add it manually.

### 6. Create the pull request

Write the body to a temp file and pass it via `--body-file` (avoids shell-quoting issues and preserves formatting). Push the branch, create the PR, and apply the risk label:

```bash
BODY=$(mktemp)
cat > "$BODY" <<'EOF'
<the body from step 4>
EOF

gh pr create \
  --base <base> \
  --head <head> \
  --title "<concise title describing the change>" \
  --body-file "$BODY" \
  --label "$RISK_LABEL"

rm -f "$BODY"
```

- **Title**: concise, imperative, specific to the change (e.g. "Add rate limiting to the auth endpoint"). Do not invent a generic title.
- If the branch is not yet pushed, push it first: `git push -u origin <head>`.
- If `--base` cannot be determined confidently, ask the user before creating the PR.
- After creation, `gh pr create` prints the PR URL.

### 7. Verify and report

Confirm the PR was created with the intended label:

```bash
gh pr view <number> --json number,url,labels,headRefName,baseRefName
```

- Verify the `risk:*` label is present and matches the chosen level.
- Report the PR URL, the base/head, the chosen risk level and its justification, and a one-line summary of each section.

## Guardrails

- **Exactly one risk label.** Every PR gets one of `risk:high`, `risk:medium`, `risk:low` — no more, no less. Do not apply a second risk label.
- **Highest level wins.** When a change could fit multiple levels, apply the higher one. When a change is ambiguous between `low` and `medium`, choose `medium`; when ambiguous between `medium` and `high`, choose `high`.
- **Security is high.** Any security-relevant change is `risk:high`, even if it also looks complex. Complexity alone is `risk:medium`.
- **Never fabricate.** Ground every section and the risk classification in the actual diff. If the diff is empty, stop and ask — do not open a PR with no changes.
- **Do not silently skip the label.** If label creation or application fails, create the PR anyway, then report the failure and ask the user to add the label manually.
- **Confirm before force-pushing or rebasing.** If the branch needs history rewriting, ask the user first.
- **Three sections, always.** The body must contain `## Why`, `## Review focus`, and `## Risks`. Do not drop a section even if its content is `none`.
