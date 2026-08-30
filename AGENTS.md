# Agent Coding Instructions

## Code architecture

- Prefer deep modules with narrow interfaces
- Ensure logic that can exist without interactions in the OS is testable
- Ensure we have tests for the logic that can be tested
- Prefer unit-tests over integration tests 

## Writing code

- Use a red-green-refactor workflow.
- Write a failing test before writing implementation logic
- Write one minimal functional slice at a time rather than full horizontal layers

## When to write any code

Before writing any code, ask yourself:

1. Does this need to be built at all? No? Skip it.
2. Does it already exist in the codebase? Reuse the logic or pattern.
3. Does the standard library do it? Use it.
4. Does a native platform feature cover it? Use it.
5. Does an already included dependency solve it? Use it.
6. Is there a crate available on crates.io that solves it? Use it.
7. Can it be done with one line? Do it.
8. Only then, wreite the minimum code that works.

The ladder runs after you understand the problem, not instead of it. Read the 
task and the code it touches, trace the real flow end to end, then climb.

Important rules:

- No unrequested abstractions
- No avoidable dependencies
- No speculative scaffolding
- Prefer deleting code over adding code
- Boring logic beats clever logic
- Fewest modules as possible
- Shortest working diff wins once you understand the problem
- Pick the edge-case-correct option when two standard-library approaches are the same size

Complex request? Ship the lazy version and question it in the same response: 
"Did X. Y covers it. Need full X? Say so." Always tell the user what you 
skipped. If the user insists on the full version, build it, no re-arguing.

When not to be lazy:

- Do not cut out validation, error handling, security, or real edge-cases.
- Do not sip understanding. A small diff without understanding is just lazy.

## Testing

IMPORTANT: End-to-end tests are performed manually because you need access to 
a physical keyboard and microphone. Provide instructions to the human to 
perform any end-to-end tests.
