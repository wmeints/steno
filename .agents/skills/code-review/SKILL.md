---
name: code-review
description: "Perform a multi-lens code review of a feature branch and post the results to its GitHub PR. Runs several review agents in parallel - one per lens (architecture, correctness, test coverage, maintainability, security) - over the branch's code delta, using the Herdr terminal multiplexer to launch them as real agents in their own panes, then consolidates every lens's findings into a single report and turns the findings into detailed, line-anchored review comments posted to the PR. Use when the user wants a thorough, multi-perspective review of a branch and the results posted to GitHub."
---

# Code Review (Multi-Lens, PR Comments)

Review a feature branch from several distinct lenses **in parallel**, collect all
results into **one consolidated report**, then turn the findings into detailed
**line-anchored comments** posted to the PR on GitHub.

The review is only as good as the evidence behind it: every finding cites exact
`file:line`, and every comment posted to the PR is anchored to the exact lines
it concerns.

## Tooling

Use the `herdr` skill and its `herdr` CLI to spawn the review agents: each
lens runs as a real agent in its own Herdr pane, launched in parallel.
Coordinate with the `gh` CLI (GitHub CLI) and `git` to post the results.
Never hand-roll PR comment posting with `curl` or the raw GitHub API.

This skill is the *user* of the herdr skill; do not re-derive the `herdr`
CLI syntax from the examples here. Consult the herdr skill for command
detail, JSON response shapes, and safety rules, and treat the installed
`herdr` binary's `--help` as the authority for syntax.

### Verify Herdr is available

Before spawning any agent, confirm this agent is running inside a Herdr
pane:

```bash
test "${HERDR_ENV:-}" = 1
```

If the check fails, this skill cannot launch review agents. Stop and tell
the user the review must run inside Herdr (or run it via the `task`
tool as a fallback). If it passes, discover the current layout with:

```bash
herdr --help
herdr agent
herdr pane
herdr workspace list
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
herdr pane list --workspace "$HERDR_WORKSPACE_ID"
```

### Verify GitHub CLI is available

Verify `gh` is available and authenticated at the start:

```bash
gh auth status
```

If it reports unauthenticated, stop and tell the user to run `gh auth login`.

## The lenses

Each review lens is a distinct viewpoint. Run **all** of them so no dimension is
skipped; a single lens can miss what another catches. These five are the
standard set — keep them as the baseline and add a domain-specific lens only when
the diff clearly warrants it (e.g. a perf-heavy diff adds a performance lens).

| Lens | Question it asks | Herdr agent name |
| --- | --- | --- |
| **Architecture / design** | Is the design sound, and faithful to its stated intent? | `review-arch` |
| **Correctness** | Does the logic hold — edge cases, races, error paths? | `review-correctness` |
| **Test coverage** | Is the behavior actually verified, or only claimed? | `review-tests` |
| **Maintainability / style / deps** | Is the code clean, documented, and its footprint justified? | `review-maintain` |
| **Security** | Is untrusted input / trust boundaries handled safely? | `review-security` |

The `--kind` for each agent is the coding agent available in this Herdr
session (from the `herdr agent` kind list). The security lens must be a
**dedicated, independent** agent with its own verification (it re-reads the
code and the relevant third-party crate source); it must reason from the
code itself, not from a summary handed to it. Do not fold security into
another lens.

## Steps

### 1. Locate the PR and the diff

Identify the branch and the PR it targets, and collect the code delta relative
to the base.

```bash
git rev-parse --abbrev-ref HEAD
git branch -vv
git merge-base HEAD origin/<base>          # the common ancestor; the diff range start
gh pr list --head <head-branch> --state all --json number,base,head,url
```

- **Base branch**: from the PR (`base.ref`) if it exists, else the repository
  default via `gh repo view --json defaultBranchRef -q .defaultBranchRef.name`.
  Confirm with the user if ambiguous.
- **Merge-base** (`git merge-base HEAD origin/<base>`): the true diff start. The
  PR may have more commits than the code delta if the base moved; the merge-base
  is the range that actually represents this branch's change.
- **Code delta**: `git diff <merge-base> HEAD --stat` then the full diff. This is
  the evidence for every review. Focus the review on **code** (`.rs`, `.ts`,
  `.py`, `.go`, …), not doc/config churn — but note doc/spec changes when a
  finding is a deviation from a stated spec.
- Record the **PR number** (from `gh pr list`) — it is needed in step 6.

If no PR exists yet, review the branch against the base and post the consolidated
report as a comment once the PR is opened (or tell the user the PR does not exist
and ask how to proceed).

### 2. Establish ground truth before reviewing

Before spawning agents, gather the facts the lenses will rely on so they reason
from evidence, not guesses:

- Read the changed files in full (the lens-specific ranges).
- **Build / clippy / test status**: run the project's build, linter, and test
  suite once (e.g. `cargo build`, `cargo clippy`, `cargo test`). Report the
  outcome. Agents do **not** re-run these — they reason from your result plus the
  code they read.
- For any finding that depends on a **third-party API**, verify it against the
  crate/package source rather than from memory (e.g. read the dependency's source
  under `~/.cargo/registry/src/…`). A finding about an API contract must cite the
  actual signature/behavior.

Record these facts (build result, the exact line numbers, any API facts) — they
are seeded into the agent `context` in step 3, so agents do not re-verify.

### 3. Spawn the parallel review agents

Launch **one agent per lens in its own Herdr pane**, so the lenses run
concurrently — this is real parallelism, not a queue. Follow the herdr
skill for command detail; the exact syntax below is a guide, and the
installed `herdr` CLI is the authority.

For each lens:

1. **Split a sibling pane** in the current tab, preserving the caller's
   working directory and keeping user focus in the calling pane (use
   `--no-focus`). Split a wide pane `right` and a narrow/tall pane `down`;
   read the new pane id from `.result.pane.pane_id`:

   ```bash
   herdr pane split --current --direction right --cwd "$PWD" --no-focus
   ```

2. **Start the agent** in that pane with a unique name (match the lens,
   e.g. `review-arch`, `review-correctness`, … from the lens table) and the
   available `--kind`. `agent start` returns only after the agent is ready
   for input:

   ```bash
   herdr agent start review-arch --kind <kind> --pane <pane-id>
   ```

3. **Prompt** the agent with the shared context and the lens-specific
   instructions, and wait for it to settle:

   ```bash
   herdr agent prompt review-arch "<prompt>" --wait --timeout 120000
   ```

The `<prompt>` for every agent carries:

- **The shared context** (the goal, the constraints, the output contract, and
  the **verified facts** from step 2: PR number, base/head, merge-base, the
  code-delta file list, build/clippy/test results, key third-party API
  signatures, and the exact line-number map of the changed files). Agents are
  blank — give them everything they need so they never re-verify.
- **This lens**: what it must examine specifically.
- **The output path** it writes to, e.g. `local://review-<lens>.md`.
- **The acceptance criterion**: the file written in the shape below.
- **Skip project-wide build/lint/test** — reason from the seeded facts.
- The security lens is read-only and verifies itself (it re-reads the code and
  the relevant dependency source); never hand it a summary in place of that.

The output shape every agent must follow (state it in the prompt):

- Line 1: `# Review: <lens name>`
- 2-4 sections from `## Findings`, `## Suggestions`, `## Praise`
- Each finding: **bold** one-line title, severity in parentheses
  `(critical|major|minor|nit)`, then 1-3 sentences of evidence with **exact
  `file:line`**. Distinguish what is substantiated from the code vs. `[INFERENCE]`
  (things the agent could not verify). **Do not invent line numbers.**
- End with `## Verdict` — one sentence: approve / approve-with-nits / request-changes.

### 4. Wait for all agents and read their output

Do not post until every lens has delivered. `agent prompt … --wait` (step 3)
already waits for each agent to reach a settled `idle` / `done` / `blocked`
state. Then read each agent's output file, e.g.:

```bash
herdr agent read review-arch --source recent-unwrapped --lines 120
```

If a prompt returned `blocked` instead of settling, inspect the agent before
deciding what to do:

```bash
herdr agent get review-arch
herdr agent read review-arch --source recent-unwrapped --lines 120
```

Verify each output file exists and is well-formed before moving on. If a lens
failed or produced an empty file, re-prompt just that lens — do not skip it and
do not fabricate its findings.

### 5. Consolidate into a single report

Merge the five lens outputs into **one** consolidated report. This is the report
the user reads; it is **not** posted to the PR yet.

Write the consolidated report to a file (e.g. `local://review-consolidated.md`)
using `review-consolidated.md` (read it via `skill://code-review/review-consolidated.md`). It must contain:

- **Overview** — one paragraph: the branch, the base, the code-delta summary
  (which files changed), and the build/clippy/test status.
- **Findings by lens** — a subsection per lens, each with its findings (title,
  severity, `file:line`, evidence) and its verdict.
- **Cross-cutting / highest-priority** — the issues that appear from more than
  one lens, or that are the highest severity, pulled out as the "fix these first"
  list. These are the ones the inline comments (step 6) will emphasize.
- **Verdict** — one overall line: approve / approve-with-nits / request-changes,
  with the blocking reasons.

Consolidate, do not just concatenate: deduplicate a finding reported by two lenses
into one (noting it came from multiple lenses), and order by severity. Keep every
`file:line` reference intact.

### 6. Turn findings into line-anchored comments on the PR

Turn the consolidated findings into **detailed review comments**, each **anchored
to the exact lines** it concerns, and post them to the PR.

- **Inline (line-anchored) comments** — the primary output. For each finding that
  has a concrete location, post a comment anchored to that line range of the
  PR's diff:

  ```bash
  gh pr review <pr-number> --repo <owner>/<repo> --comment --comment-body-file <comment-file>
  # or, per-line, the review API with a line position:
  gh pr review <pr-number> --repo <owner>/<repo> --json ...
  ```

  Anchor each comment to the precise `file` + line(s) of the finding. A finding
  may anchor to several line ranges if it spans them. The comment is **detailed**:
  the problem, the evidence, the exact line(s), and the suggested fix — not a
  one-liner. Prefer the GitHub API (see below) when you need explicit
  `path`/`line`/`start_line` positions; `gh pr review --comment-body-file`
  posts a review comment anchored to the diff.

  To post an inline comment at an explicit position, write a JSON payload and
  submit it:

  ```bash
  gh api repos/<owner>/<repo>/pulls/<pr-number>/comments \
    -X POST -f commit=$(gh pr view <pr-number> --repo <owner>/<repo> --json headRefOid -q .headRefOid) \
    -f path="<file>" -f line=<line> -f start_line=<start_line> -f side=RIGHT \
    -f body="$(cat <comment-file>)"
  ```

  - `path`: the file, as it appears in the diff.
  - `line` / `start_line` / `side=RIGHT`: the line range in the new (right) side
    of the diff. Use `start_line`+`line` for a range; a single line is
    `line` with no `start_line`.
  - `body`: the detailed comment, referencing the exact line(s).
  - Only post comments for findings with a **concrete line** in the diff. If a
    finding has no anchor (e.g. a cross-cutting observation about a decision),
    fold it into the overall review comment (below) instead.

- **Overall review comment** — one summary comment on the PR (not per-line): the
  consolidated verdict, the highest-priority issues, and a short index of what
  each lens found. This gives a reader the whole picture; the inline comments
  carry the line-level detail. Post it as the review body:

  ```bash
  gh pr review <pr-number> --repo <owner>/<repo> --body-file <report-file> --comment
  ```

- **One comment per distinct finding, anchored to its lines** — do not dump all
  findings into a single giant comment; the value is the line-level anchoring.
  Group closely related lines, but never drop a line anchor.

### 7. Verify and report

Confirm the comments landed and report back.

```bash
gh pr view <pr-number> --repo <owner>/<repo> --json number,url
gh api repos/<owner>/<repo>/pulls/<pr-number>/comments -q '.[].id'   # inline comments
```

- Report the PR URL, the number of inline comments posted, and the overall
  verdict from the consolidated report.
- If any comment failed to post, report the failure and the finding it belonged
  to — do not silently drop it.

## Guardrails

- **All lenses, in parallel, via Herdr.** Run every lens as its own agent in its
  own Herdr pane, launched concurrently. Do not serialize them into one agent or
  skip a lens. The security lens is a dedicated, independent agent.
- **Consolidate before posting.** Post to the PR only after every lens has
  delivered and the single consolidated report is written. Never post from a
  partial batch.
- **Anchor to lines.** Every inline comment is anchored to the exact `file:line`
  of its finding. A finding with no concrete line goes into the overall comment,
  not posted as a line comment with a guessed line.
- **Exact line numbers, never fabricated.** Every finding and comment cites
  exact `file:line` from the actual code (the line map from step 2). Mark
  anything unverifiable `[INFERENCE]` rather than guessing a number.
- **Evidence from the code.** Ground every finding in the code the agent read and
  (for API-dependent findings) the third-party source verified in step 2. Do not
  reason from a summary handed to an agent — the security agent verifies itself.
- **Build/lint/test run once, by the main agent.** Seed their result into the
  agent `context`. Agents do not re-run the suites — it blocks them on each
  other's edits.
- **Never fabricate output.** A lens that fails is re-run, not invented. If the
  diff is empty, stop and ask — do not review nothing.
- **Do not force-merge or alter the branch.** This skill reviews and comments; it
  does not edit the branch, merge, or change labels.
- **Confirm before posting if the PR is the wrong target** (ambiguous base, or no
  PR found): ask the user rather than posting to an arbitrary PR.
