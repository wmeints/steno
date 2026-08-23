# Agent Watchdog Instructions

## Scope & Boundaries
- Do not modify or refactor files outside the explicitly requested task.
- Do not alter public APIs, exported functions, or interfaces without explicit user sign-off.
- Treat backward compatibility as a strict constraint.

## Reasoning & Loop Control

- Limit internal reasoning/thinking token expenditure when instructions are already specific.
- If a task requires >3 sequential tool-call failures or a repeating generation loop, halt and output a clarification request to the user.
- Avoid over-engineering; implement the most direct, minimal solution that passes verification.

## Testing & Verification

- Every new function or bug fix must include a corresponding unit or integration test.
- Run local linters/test suites before marking any task step as complete.
