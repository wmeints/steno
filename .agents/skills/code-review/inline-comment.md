# Line-anchored review comment (template)

One detailed comment posted to the PR, **anchored to the exact line(s)** of its
finding. Write the body to a file, then post it per the skill (step 6) with
`path`/`line`/`start_line`/`side=RIGHT`. One comment per distinct finding.

Replace every `<...>` placeholder. Never leave a placeholder or a section blank.
Every comment cites exact `file:line`; mark anything unverifiable `[INFERENCE]`.

---

**<finding title>** (severity: critical|major|minor|nit)

<1-2 sentences: the problem — what is wrong and why it matters. Reference the
exact line(s), e.g. "At `model.rs:161-167` the size check compares the streamed
byte count to the server-declared size, so a same-size tampered file passes.">

**Evidence.** <the concrete evidence: the code at the line, the API behavior, or
the test gap — with `file:line`. Cite the third-party source where an API
contract is the evidence (e.g. `hf-hub download.rs:198`).>

**Suggested fix.** <a concrete change: the code, the line, and what it should
become. If the fix is a larger decision, name the options and the trade-off.>

<Optional, when the finding recurs: **Also seen in <lens>.** Note the other lens
that found the same issue, so the reader knows it is cross-confirmed.>

---

## Posting reference

Anchor to the new (right) side of the diff. A single-line finding:

```
path  = "<file as it appears in the diff>"
line  = <line number>
side  = "RIGHT"
body  = <the comment above, from the body file>
```

A multi-line finding (anchor to a range):

```
path      = "<file>"
start_line = <first line>
line       = <last line>
side       = "RIGHT"
body       = <the comment>
```

Only post findings with a **concrete line in the diff**. A finding with no anchor
(e.g. a cross-cutting design observation) goes into the overall review comment
instead — do not post it as a line comment with a guessed line.
