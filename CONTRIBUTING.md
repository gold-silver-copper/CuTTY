# Contributing to CuTTY

Thank you for your interest in contributing to CuTTY!

Table of Contents:

1. [Feature Requests](#feature-requests)
2. [Bug Reports](#bug-reports)
3. [Patches / Pull Requests](#patches--pull-requests)
    1. [Testing](#testing)
    2. [Performance](#performance)
    3. [Documentation](#documentation)
    4. [Style](#style)
4. [Release Process](#release-process)
5. [Contact](#contact)

## Feature Requests

Feature requests should be tracked in the project's issue tracker. To reduce
the number of duplicates, please make sure to check existing enhancement and
missing-feature reports first.

## Bug Reports

Bug reports should be filed in the project's issue tracker.

If a bug was not present in a previous version of CuTTY, providing the exact commit which
introduced the regression helps out a lot.

## Patches / Pull Requests

All patches should be sent as pull requests through the repository host used
for the project.

If you are looking for a place to start contributing to CuTTY, take a look at
issues marked as help wanted or easy in the project's tracker.

You can find the minimum supported Rust version in CuTTY's manifest file
(`cat cutty/Cargo.toml | grep "rust-version"`). CuTTY **must** always
build with the MSRV and bumping it should be avoided if possible.

Since `cutty_terminal`'s version always tracks the next release, make sure that the version is
bumped according to semver when necessary.

### Testing

To make sure no regressions were introduced, all tests should be run before sending a pull request.
The following command can be run to test CuTTY:

```
cargo test
```

Additionally if there's any functionality included which would lend itself to additional testing,
new tests should be added. These can either be in the form of Rust tests using the `#[test]`
annotation, or CuTTY's ref tests.

To record a new ref test, a release version of the patched binary should be created and run with the
`--ref-test` flag. After closing the CuTTY window, or killing it (`exit` and `^D` do not work),
some new files should have been generated in the working directory. Those can then be copied to the
`./tests/ref/NEW_TEST_NAME` directory and the test can be enabled by editing the `ref_tests!` macro
in the `./tests/ref.rs` file. When fixing a bug, it should be checked that the ref test does not
complete correctly with the unpatched version, to make sure the test case is covered properly.

### Performance

If changes could affect throughput or latency of CuTTY, these aspects should be benchmarked to
prevent potential regressions. Since there are often big performance differences between Rust's
nightly releases, it's advised to perform these tests on the latest Rust stable release.

CuTTY mainly uses the [vtebench](https://github.com/cutty/vtebench) tool for testing CuTTY's
performance. Instructions on how to use it can be found in its
[README](https://github.com/cutty/vtebench/blob/master/README.md).

Latency is another important factor for CuTTY. On X11, Windows, and macOS the
[typometer](https://github.com/pavelfatin/typometer) tool allows measuring keyboard latency.

### Documentation

Code should be documented where appropriate. The existing code can be used as a guidance here and
the general `rustfmt` rules can be followed for formatting.

If any change has been made to the `config.rs` file, it should also be documented in the man pages.

Changes compared to the latest CuTTY release which have a direct effect on the user (opposed to
things like code refactorings or documentation/tests) additionally need to be documented in the
`CHANGELOG.md`. When a notable change is made to `cutty_terminal`, it should be documented in
`cutty_terminal/CHANGELOG.md` as well. The existing entries should be used as a style guideline.
The change log should be used to document changes from a user-perspective, instead of explaining the
technical background (like commit messages) More information about CuTTY's change log format can
be found [here](https://keepachangelog.com).

### Style

All CuTTY changes are automatically verified by CI to conform to its rustfmt guidelines. If a CI
build is failing because of formatting issues, you can install rustfmt using `rustup component add
rustfmt` and then format all code using `cargo fmt`.

Unless otherwise specified, CuTTY follows the Rust compiler's style guidelines:

https://rust-lang.github.io/api-guidelines

All comments should be fully punctuated with a trailing period. This applies both to regular and
documentation comments.

# Release Process

CuTTY's release process aims to provide stable and well tested releases without having to hold
back new features during the testing period.

To achieve these goals, a new branch is created for every major release. Both the release candidates
and the final version are only committed and tagged in this branch. The master branch only tracks
development versions, allowing us to keep the branches completely separate without merging releases
back into master.

The exact steps for an exemplary `0.2.0` release might look like this:
  1. Initially, the version on the latest master is `0.2.0-dev`
  2. A new `v0.2` branch is created for the release
  3. In the branch, the version is bumped to `0.2.0-rc1`
  4. The new commit in the branch is tagged as `v0.2.0-rc1`
  5. The pre-release versions are published to crates.io
  6. A GitHub release is created for the `v0.2.0-rc1` tag
  7. The changelog since the last release (stable or RC) is added to the GitHub release description
  8. On master, the version is bumped to `0.3.0-dev`
     and the `-dev` suffix is removed from the `0.2.0-dev` changelog
  9. Bug fixes are cherry-picked from master into the branch and steps 3-7 are repeated until no
     major issues are found in the release candidates
 10. In the branch, the version is bumped to `0.2.0`
 11. The new commit in the branch is tagged as `v0.2.0`
 12. The new version is published to crates.io
 13. A GitHub release is created for the `v0.2.0` tag
 14. The changelog since the last stable release (**not** RC) is added to the GitHub release
     description

On master and with new planned releases, only the minor version is bumped. This makes it possible to
create bug fix releases by incrementing the patch version of a previous minor release, without
having to adjust the next planned release's version number.

The exact steps for an exemplary `0.2.3` release might look like this:
 1. Initially, the version on the latest master is `0.3.0-dev` and the latest release was `0.2.2`
 2. The `v0.2` branch is checked out
 3. All bug fixes are cherry-picked from master into the `v0.2` branch
 4. The version is bumped to `v0.2.3-rc1` and the changelog is updated to include all fixes
 5. Follow Steps 4-13 of the regular release's example
 6. The release's changelog is ported back to master, removing fixes from the `0.2.3` release

The `cutty_terminal` crate is released in synchronization with `cutty`, keeping the `-dev`
and `-rcX` version suffix identical across the two crates. As soon as the new CuTTY stable
release is made, releases are tagged as `cutty_terminal_vX.Y.Z` and pushed to crates.io. During
a release, only the patch version is bumped on master, since there haven't been any changes since
the last release yet.

# Contact

If there are any outstanding questions about contributing to CuTTY, they can be
asked in the project's issue tracker.

As a more immediate and direct form of communication, the CuTTY IRC channel (`#cutty` on
Libera.Chat) can be used to contact many of the CuTTY contributors.
