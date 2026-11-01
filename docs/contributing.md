# Contributing Code

## Help wanted

If you'd like to contribute but don't know what to work on, please take a look
at the [issues tab](https://github.com/ankitects/anki/issues) of the Anki repo
on GitHub.

## Larger changes

Before starting work on larger changes, especially ones that aren't listed on the
issue tracker, please reach out on the forums before you begin work, so we can let
you know whether they're likely to be accepted or not. When you spent a bunch of time
on a PR that ends up getting rejected, it's no fun for either you or us.

## Pull Request Description

When opening a pull request, fill in the provided template
(`.github/pull_request_template.md`). It covers the required fields: linked
issue, summary/motivation, how to test, and UI evidence for visual changes.

## Linked Issues

Every pull request, except hotfixes and dependency updates, must be linked
to an existing open issue. If no issue exists, please open one to describe
the problem before submitting a PR. This helps us ensure we're solving the
right problems and prevents wasted effort on both sides.

PRs without a linked issue may be automatically closed after a short period.

## Dependency updates

Dependabot PRs do not need a linked issue. Maintainers should follow
[Dependabot updates](dependabot.md) when reviewing or landing them.

## Refactoring

Please avoid PRs that focus on refactoring. Every PR has a cost to review, and a chance
of introducing accidental regressions, and often these costs are not worth it for
slightly more elegant code.

That's not to say there's no value in refactoring. But such changes are usually better done
in a PR that happens to be working in the same area - for example, making small changes
to the code as part of fixing a bug, or a larger refactor when introducing a new feature.

## Translations

For information on adding new translatable strings to Anki, please see
[the translation guide](https://anki.mintlify.app/translators/anki/developers).

For information on translating the documentation, please see
[translating the manual](https://anki.mintlify.app/translators/anki/manual).

## Tests Must Pass

Please make sure 'ninja check' completes successfully before submitting code.

New or changed behavior must be covered by tests. Exceptions where tests are not
required: version bumps, documentation-only changes, translation updates, dependency
updates, and CI/build housekeeping (chore).

## Do One Thing

A patch or pull request should be the minimum necessary to address one issue.
Please don't make a pull request for a bunch of unrelated changes, as they are
difficult to review and will be rejected - split them up into separate
requests instead.

## AI-Assisted Contributions

Using AI tools to help write or review code is permitted. However, you must
understand every change you submit. If you cannot explain what your changes
do and how they interact with the rest of the system, your PR will be closed.

Please review AI-generated code carefully before submitting. PRs that appear
to have been submitted without human review — e.g., irrelevant code, duplicate
logic, or comments that don't match the implementation — may be closed without
further discussion.

## License

Please add yourself to the [CONTRIBUTORS](https://github.com/ankitects/anki/blob/main/CONTRIBUTORS) file in your first pull request.
