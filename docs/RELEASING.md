# Releasing

One command builds, signs and publishes a release, then points Homebrew and winget at it:

```sh
Tools/release.sh --mac --notes-mac notes-mac.md
Tools/release.sh --windows --notes-windows notes-windows.md
```

Both flags can go together. Bump the versions in a PR first (`Resources/Info.plist`, `Tests/SwitcherModelFeatureTests.swift`, and for Windows `windows/package.json`, `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`), merge it, and run the script from that commit. `--dry-run` builds and signs into `dist/` and stops before tagging.

## What gets published

| Platform | Tag | Files |
| --- | --- | --- |
| Mac | `v<version>` | `Openbar-<version>.dmg`, `Openbar-<version>.dmg.sig` |
| Windows | `windows-v<version>` | `Openbar_<version>_{x64,arm64}-setup.exe`, their `.sig` files, `latest.json` |

A version with a hyphen (`3.6.0-beta.1`) is a beta and so is every Windows release below 1.0; they go out as GitHub pre-releases and reach only people with beta updates on.

## Update keys

Installed copies refuse an update that isn't signed with these keys. Losing one means those copies can't update themselves any more, so keep a backup of both.

- **Mac:** an Ed25519 key in the login keychain (generic password, service `openbar-update-signing`). Its public half is `UpdateSignature.publicKeyBase64`. `swift Tools/sign-update.swift --public-key` prints it; `--generate` makes a new one on a new machine (and then the app must ship the new public key before releases signed with it can install).
- **Windows:** the Tauri updater key at `~/.openbar-release/tauri-updater.key`, its password in the keychain (service `openbar-tauri-updater`). Its public half is `plugins.updater.pubkey` in `windows/src-tauri/tauri.conf.json`.

The Mac installer also requires the new app to be signed by the same certificate as the running one (`Openbar Signing`, or later an Apple Developer ID), so privacy permissions carry over.

## Package managers

`Tools/bump-packages.sh mac|windows <version>` runs at the end of a release and can be re-run by hand:

- **Homebrew:** rewrites `Casks/openbar.rb` in [MF-DEEV/homebrew-openbar](https://github.com/MF-DEEV/homebrew-openbar) from `packaging/homebrew/openbar.rb`. The official homebrew/cask repository needs a notarized app; move there after notarization.
- **winget:** fills `packaging/winget/` and opens a PR to microsoft/winget-pkgs with [komac](https://github.com/russellbanks/Komac) (`brew install komac`) using `gh auth token`. Microsoft reviews it, which takes a day or more.
