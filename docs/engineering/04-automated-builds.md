# Automated builds

To prevent issues with code quality even more than what is already covered
in [quality assurance][qa_doc] I've made a few workflows.

## Continuous integration

The [CI workflow][ci_workflow] automatically runs the unit-tests and verifies
code quality as an extra validation for pull requests submitted in the 
repository. It is triggered on pull request events and after a push on the 
`main` branch in the repository.

## Release build

The [release workflow][release_workflow] is triggered when a new tag is pushed.
This workflow compiles the sources and produces a package that users can install
on their machine. 

[qa_workflow]: ../../.github/workflows/ci.yml
[release_workflow]: ../../.github/workflows/release.yml
