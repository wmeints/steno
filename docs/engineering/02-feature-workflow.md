# Feature workflow

This project uses [OpenSpec][OPSX] to write the specifications for the application. 
For the project we use the following workflow:

1. Create a new worktree + feature branch for the feature you want to work on.
   You can use `git worktree add -b feat/<feature-name> .worktrees/<feature-name>`.
   All work needs to happen on a clean work tree.

2. Start oh-my-pi with `/opsx-explore` to refine the idea you want to work on. 
   This leads to a somewhat unstructured approach to solve a specific problem.

3. Next, run `/opsx-propose <feature>` to create an official spec for the
   feature you want to implement in the project. Make sure to review this!

4. Then, run `/opsx-apply` to implement the spec in the codebase. This can take
   a long time on open-source models, so go do something else in the meantime.

5. After, run `/opsx-archive` to sync and archive the change. 

6. Finally, run `/skill:submit-pr` to create a pull request for the 
   new feature. You can also ask the agent to submit a PR.

## Clean up the context window

I've found that when you have worked on the specification for a new feature that
the context window is pretty messy. Sometimes you can fix this with `/compact` 
so that the agent has a cleaner context window, but often I find that I need
to start a new session with `/new` and then run the `/opsx-apply` command
to get the quality I need.

In short, it's highly recommended to start implementation of a task in a new
session. Otherwise you'll end up with broken code.

[OPSX]: https://openspec.dev/
