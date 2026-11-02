# Contributing to Openbar

Openbar aims to stay small, native and readable. Contributions are accepted
under GPL-3.0-or-later unless stated otherwise.

## Getting started

On the Mac you need macOS 14 or newer, Apple Silicon and the Xcode Command Line Tools.
The Windows island lives in `windows/`; its [README](windows/README.md#build-it-yourself) covers that build.
The project builds with `build.sh`, without an Xcode project or external package
dependencies. `Package.swift` supports editor indexing; it does not assemble or
sign the app bundle.

```sh
git clone https://github.com/MF-DEEV/Openbar.git
cd Openbar
./build.sh --dev
./build/VorssaintDeveloper --selftest
./build.sh --test
```

To install and launch the separate Developer app, use `./build.sh --dev --install`.
The app and its binaries are still named Vorssaint, the project Openbar is built on.
The Developer build has its own preferences and permissions.
A plain `./build.sh` builds the optimized variant; its health check is
`./build/Vorssaint --selftest`.

### Stable signing

Developer and install builds automatically try to create a stable local signing
identity when none is available. You can also run `./Tools/setup-signing.sh`.
Keep that identity across rebuilds: ad-hoc signatures change with the binary and
can invalidate macOS permission grants. Check the build output for signing
failures. For stale grants, see [troubleshooting](docs/TROUBLESHOOTING.md#resetting-permissions).

Openbar ships no signed Mac builds yet; you build the app yourself, as above.

## Scope and implementation

Before a substantial feature, search existing issues and PRs and discuss the
direction in an issue. Explain the user need, required permissions, dependencies
and ongoing maintenance. Prefer existing macOS capabilities and repository code
over a new subsystem. Keep each PR focused on one independently useful change.

- App lifecycle lives in `Sources/Vorssaint/App`, shared catalogs and preferences
  in `Core`, behavior in `Services`, views in `UI`, and diagnostics in `Support`.
  Keep reusable decisions outside views where they can be tested.
- Check other callers and the upgrade path when fixing shared behavior.
- New features belong in the feature catalog, with availability and permission
  checks on every entry point. Optional background work should stop when disabled.
- User preferences must participate in settings backup. Machine-specific state
  and private content need an explicit exclusion.
- Update every locale in `AppLanguage.allCases`, including feature-specific
  string catalogs. A new language also needs its locale registration, localized
  permission prompts where applicable, and coverage tests.
- New source files retain the project's SPDX license and copyright headers.

Sensor changes should include a dump from `./build/Vorssaint --sensors` and the
Mac model. Selection logic lives in `TemperatureSensorSelector` and `SystemMonitor`.

## Validation

Documentation-only changes need a diff and link check. For code changes, run the
relevant tests; `./build.sh --list-tests` lists suites and
`./build.sh --test-suite=NAME` selects one. Behavior tests should fail without the
fix and preserve legitimate use, rather than pinning private implementation text.

The test executable compiles a selected set of sources, so passing tests does not
prove the app builds. Build the app and run its selftest for runtime, UI or bundled
resource changes. CI runs the tests only; the build and selftest are yours to run.
Builds should finish without warnings.

### Checks on pull requests

Every pull request runs [PR checks](.github/workflows/pr.yml) for the parts it
changes, each job capped at fifteen minutes, and **CI ok** sums them up as the one
check to require before merging into `main`. Run the same checks before you push:

- Mac: `./build.sh --test`
- Windows UI, in `windows/`: `bun run typecheck`, `bun run format:check`, `bun run test`
- Windows backend, in `windows/src-tauri/`, after `bun run build` in `windows/`:
  `cargo fmt --check`, `cargo xwin clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings`
  and `cargo test`. CI pins Rust 1.99.1 (`RUST_TOOLCHAIN` in the workflow), so a
  newer local toolchain can report lints CI does not.

The release build and its selftest are not part of CI; run
`./build.sh && ./build/Vorssaint --selftest` yourself for runtime, UI or bundled
resource changes.

For behavior involving windows, permissions or hardware, describe what you actually
verified and on which Mac and macOS version. Include display, Spaces or language
details when relevant, and state what remains untested. Compilation and static
inspection alone do not demonstrate an interaction on a real Mac.

## Pull requests

Explain the behavior before and after, the scope and validation. Use
`type(scope): lowercase imperative phrase` for the title and reuse an existing
scope where possible. Open unfinished work as a draft and name what is missing.
The [template](.github/PULL_REQUEST_TEMPLATE.md) is a prompt, not a form.

Leave version bumps and `CHANGELOG.md` to the maintainer. Describe the user-visible
effect in the PR so it can become a release note with your credit.

When an issue exists, include one reference per line:

```text
Refs #123
Refs #456
```

Avoid `Closes` or `Fixes` unless the change fully resolves the issue. If the PR
addresses only part of an issue, say exactly which part.

For agent-assisted work, see [contributing with an agent](docs/AI-CONTRIBUTIONS.md).
Bug reports and feature requests use the [issue forms](https://github.com/MF-DEEV/Openbar/issues/new/choose);
questions go to [Discussions](https://github.com/MF-DEEV/Openbar/discussions).
Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).
Report vulnerabilities through [private security reporting](SECURITY.md).

## Releases (maintainers)

Openbar publishes Windows installers as GitHub releases tagged `windows-vX.Y.Z`,
marked as pre-releases while the port is in beta. Mac users build from source.
Integrating into `main`, pushing a branch and opening a PR do not publish a release;
publishing is an explicit decision. Don't move published tags or replace published
artifacts.
