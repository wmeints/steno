# Consolidated code review — <PR number / title>

The single report that merges every lens. Built from the per-lens outputs
(`local://review-<lens>.md`). Deduplicate findings that appear in more than one
lens into one entry (note the lenses that found it). Keep every `file:line`
reference intact. Do not post this file to the PR as-is; turn its findings into
line-anchored comments per the skill.

## Overview

One paragraph: the branch (`<head>`), its base (`<base>`), the code-delta
summary (which files changed and roughly what), and the build / clippy / test
status the main agent captured. Example: "Reviewed `feat/x` against `main`
(merge-base `<sha>`). The code delta is 3 files: `a.rs` (new), `b.rs` (+16/-1),
`Cargo.toml` (+3 deps). `cargo build`, `cargo clippy`, and `cargo test` all
pass."

## Findings by lens

Repeat one subsection per lens, in the order: Architecture / design, Correctness,
Test coverage, Maintainability / style / deps, Security.

### Architecture / design — verdict: <approve | approve-with-nits | request-changes>

- **<finding title>** (severity) — `file:line`. <1-3 sentences of evidence.
  Mark `[INFERENCE]` where unverifiable.>
- ...

### Correctness — verdict: <...>

- ...

### Test coverage — verdict: <...>

- ...

### Maintainability / style / deps — verdict: <...>

- ...

### Security — verdict: <...>

- ...

## Cross-cutting / highest priority

The issues that recur across lenses or are the highest severity, pulled out as
the "fix these first" list. Each item: the finding, the lenses that flagged it,
the exact `file:line`, and why it matters. These are the comments the skill will
emphasize in step 6.

1. **<issue>** (severity) — `file:line`. Flagged by <lens, lens>. <why it matters.>
2. ...

## Verdict

One overall line — approve / approve-with-nits / request-changes — with the
blocking reasons. Example: "Request changes: the size-only integrity check
(`model.rs:161-167`) has no test and leaves a supply-chain gap; fix before merge."
