# Contributing to `patch-hub`

Thank you for your interest in contributing to `patch-hub`! This document outlines the process for contributing to the project and provides guidelines to ensure your contributions align with the project's standards.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Getting Started](#getting-started)
3. [Development Workflow](#development-workflow)
4. [Coding Style](#coding-style)
5. [Commit Guidelines](#commit-guidelines)
6. [Pull Request Guidelines](#pull-request-guidelines)
7. [Issue Reporting](#issue-reporting)
8. [Communication](#communication)

## Code of Conduct

`patch-hub` adheres to the [Contributor Covenant of the Linux Kernel](https://docs.kernel.org/process/code-of-conduct.html) development community. In general, we expect all developers to:

- Be respectful and inclusive.
- Value diverse perspectives.
- Provide constructive feedback and gracefully accept constructive criticism.
- Focus on the technical merit of contributions and what benefits the community.

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/)
- [Git](https://git-scm.com/)
- [A GitHub account](https://github.com/signup)
- (Optional) [Pre-commit](https://pre-commit.com/)

### Setting Up Your Development Environment

1. [Fork](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/fork-a-repo) the repository on GitHub.
2. [Clone](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/fork-a-repo#cloning-your-forked-repository) your fork.
3. Add the [upstream repository](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/fork-a-repo#configuring-git-to-sync-your-fork-with-the-upstream-repository) as a remote:

## Development Cycle and Branches

### Branch Management

The development cycle relies on two branches, similar to [how kw does it](https://kworkflow.org/content/howtocontribute.html#development-cycle-and-branches).

1. `master`: The stable branch that contains the latest tested and approved code.
2. `unstable`: The main development branch where active work happens. Features and fixes are merged here before reaching the master branch.

From time to time, the `unstable` branch is merged into `master` to create a new version and release.

Always ensure your branch is up to date with `unstable` before starting to work on a contribution.

### Development Workflow

1. [**Keep your fork updated**](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/syncing-a-fork)
2. **Create a branch**:
   ```bash
   git checkout -b your-branch-name
   ```
   The branch names are mostly irrevelant; however, it is advisable to use clearly discernable and meaningful names. Prefixing with a label denoting the type of change (e.g., `refactor/apply-patchset` or `doc/add-readme`) you would like to make is also a good practice.

3. **Plan and identify your contribution**:
    - Clearly define what you are trying to achieve with your contribution.
    - If fixing an issue, ensure you fully understand the problem and check for any related discussions.
    - If adding a feature or refactoring, consider the overall design and any potential implications.
    - Discuss major changes with maintainers before implementation, if necessary.

4. **Implement your contribution**:
   - Add tests for new functionality (if any).
   - Update documentation as needed.

5. **Run tests and checks locally**:
   ```bash
   cargo test
   cargo lint
   cargo fmt --all
   ```
    Alternatively, you may skip this step, commit, and push your changes to your fork to let our GitHub CI run these for you. However, this would require you to rebase and amend your commits if CI fails, and it may result in slower feedback.

6. **Commit your changes** following the [Commit Guidelines](#commit-guidelines)
    ```bash
    git commit -s
    ```

7. **Push to your fork**:
   ```
   git push your-fork-name your-branch-name
   ```

8. [**Create a Pull Request**](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request) following the [Pull Request Guidelines](#pull-request-guidelines)

## Commit Guidelines

Your commit messages should be descriptive enough to explain **what** the commit changes and **why** it changes it. Additionally, succinctly describing **how** it changes is welcomed if convenient. You should leave deep discussions and the overarching context for the PR description. Commit messages should:

- Clearly state what the commit does.
- Follow the Conventional Commits style.
- Briefly explain why the change was necessary.
- If plausible, briefly explain how the change was done.

Commit contents, in other words, the changes the commit introduce should:
- Be **focused** and **atomic**, i.e, one logical change per commit. For example, if your changes include both bug fixes and feature additions, separate those into two or more commits. The point is that each commit should make an easily understood change without needing other commits to explain its existence.
- Go for the simplest implementation possible.
- Not destabilize the compilation, so `cargo build` shouldn't fail.
- Not fail in CI jobs.

### Use Conventional Commits

`patch-hub` follows the use of [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) wherein you indicate the type of change you're making at the start with a label, followed by a scope (optional, since the project is still small), and then the commit message after a colon.

Example Commits:
- `feat: allow provided config object to extend other configs`
- `docs: correct spelling of CHANGELOG`

Common prefixes:
- `feat`: A new feature.
- `fix`: A bug fix.
- `ui`: User interface changes
- `ci`: GitHub Actions changes
- `docs`: Documentation updates
- `refactor`: Code that neither fixes a bug nor adds a feature.
- `style`: Changes that do not affect the meaning of the code (white-space, formatting, missing semi-colons, etc).

### Scope and Breaking Changes

To specify which part of the project a commit affects, you can include a **scope** in paranthesis after the type. If the commit introduces a **breaking change**, add an exclamation mark `!` before the colon.

**Example:**

### Sign your work - the Developer's Certificate of Origin

`patch-hub` adopts the [Developer's Certificate of Origin](#developers-certificate-of-origin) practice from the Linux kernel. All commits must include the following line at the bottom to certify that you wrote it or otherwise have the right to pass it on as an open-source patch.
```
Signed-off-by: Your Name <your.name@example.com>
```

This line can be automatically added with the command `git commit -s`. Additionally, to make the review process more efficient, maintainers may make trivial changes to your commits instead of asking for changes after a review. It is important to note that:

1. The authorship is maintained,  i.e., the credits for the commit will still go to the original author.
2. Any such edits made will be catalogued at the end of the original commit message under `[Maintainer edits]`.

#### Developer's Certificate of Origin
```
Developer’s Certificate of Origin 1.1

By making a contribution to this project, I certify that:

a. The contribution was created in whole or in part by me and I have the right to submit it under the open source license indicated in the file; or

b. The contribution is based upon previous work that, to the best of my knowledge, is covered under an appropriate open source license and I have the right under that license to submit that work with modifications, whether created in whole or in part by me, under the same open source license (unless I am permitted to submit under a different license), as indicated in the file; or

c. The contribution was provided directly to me by some other person who certified (a), (b) or (c) and I have not modified it.

d. I understand and agree that this project and the contribution are public and that a record of the contribution (including all personal information I submit with it, including my sign-off) is maintained indefinitely and may be redistributed consistent with this project or the open source license(s) involved.
```

## Pull Request Guidelines

PRs should provide the big picture:

- What the PR does, i.e., the overarching context of all commits
- Why the change was necessary (including any revelant context)
- How it was implemented (if it's non-trivial)
- Potential alternatives
- Any follow-ups or future considerations

Your PR will be merged once it:
- Passes all automated checks
- Receives approval from at least one maintainer
- Meets the project's quality standards
- Aligns with the project's goals and roadmap

**Note**: PRs should always be opened using the branch `unstable` as a base.

## Issue Reporting

Use the preconfigured templates on GitHub to report issues and request features. If none of these fit your issue, you can use the "Blank" option.

## License

By contributing to `patch-hub`, you agree that your contributions will be licensed under the project's license (GPL-2.0, the same as kworkflow).

---

Thank you for contributing to `patch-hub`! Your efforts help improve tools for the Linux kernel development community.
