# Feature workflow

This project uses [OpenSpec][OPSX] to write the specifications for the application. 
For the project we use the following workflow:

1. Create a new worktree + feature branch for the feature you want to work on.
   You can use `git worktree add -b feat/<feature-name> .worktrees/<feature-name>`.
   All work needs to happen on a clean work tree.

1. Start OMP with `/opsx-explore` to refine the idea you want to work on. This
   leads to a somewhat unstructured approach to solve a specific problem.

2. Next, run `/opsx-propose <feature>` to create an official spec for the
   feature you want to implement in the project. Make sure to review this!

3. Then, run `/opsx-apply` to implement the spec in the codebase. This can take
   a long time on open-source models, so go do something else in the meantime.

4. After, run `/opsx-archive` to sync and archive the change. 

5. Finally, run `/skill:submit-pr` to create a pull request for the 
   new feature.

[OPSX]: https://openspec.dev/
