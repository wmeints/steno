# Quality assurance

## Reducing complexity

Rust can be quite challenging to maintain if you're not too familiar with the
language. That's why I choose to configure the rust linter [clippy][clippy] to
allow a maximum complexity in the methods. 

The agent, oh-my-pi has a built-in LSP and can use clippy to verify code quality
and will receive feedback on method complexity after it edits code. This forces
the agent to write simple methods and move code into private helper methods as
needed.

## Writing tests before writing the code

The [instructions file][instructions_file] contains instructions for a general
test-driven approach where you write tests first and then add the implementation.

The agent is also instructed to prefer deep modules with narrow interfaces to
simplify the code structure. You can see evidence of this in how the modules
in the application are structured.

## Enforcing quality code

The agent is only allowed to commit code that passes the automated validation
checks. I have the following pre-commit hooks in place in the repository:

1. Run clippy without warnings/errors
2. Run unit-tests successfully
3. Run commitlint without errors

The final check ensures that the agent uses 
[the correct conventions][conventional_commits] when committing changes.

## Performing local code reviews

We use the built-in `/review` skill from oh-my-pi to review the code. This runs
multiple review agents in parallel to focus on different aspects of the code.

I've found that this produces higher quality code 
[than manual reviews][manual_review] can.

## Submitting pull requests

This project has the potential to do dangerous things to a computer because
it intercepts keyboard strokes and injects input via a virtual keyboard. I want
to have a level of control over this where I can review changes if they touch
anything that's potentially dangerous.

For this purpose I created a skill [submit-pr][submit_pr] that the agent must
use when submitting pull requests. It automatically summarizes the changes and
estimates the risk involved in the change.

[clippy]: https://doc.rust-lang.org/clippy/
[instructions_file]: ../../AGENTS.md
[conventional_commits]: https://www.conventionalcommits.org/en/v1.0.0/
[submit_pr]: ../../.agents/skills/submit-pr/
[manual_review]: https://www.beyondautocomplete.nl/code-review-was-never-really-about-finding-bugs/
