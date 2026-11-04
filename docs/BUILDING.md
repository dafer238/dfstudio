# Building and releasing DeltaF Studio

## Building from source

Requirements: [Rust](https://rustup.rs) 1.85 or newer (on Windows, the MSVC
toolchain). The CoolProp library is not compiled: the `coolprop-sys` crates
download nothing at build time, they carry the prebuilt library for Windows,
macOS and Linux (x86-64 and AArch64).

```bash
cargo build --release --locked          # target/release/dfstudio and dfs
cargo test --workspace --locked         # full test suite
cargo run --release -p dfapp -- examples/brayton.dfs
```

A development build finds CoolProp in the Cargo build folder. A copied or
installed executable looks for it, in order, in `$DFSTUDIO_COOLPROP` (a full
path to the library), next to the executable, in `../lib`, `../Frameworks`
and `../Resources` (see [ARCHITECTURE.md](ARCHITECTURE.md) §6).

## Packages

All scripts write to `dist/` and must be run from a clone of the repository.

| Platform | Command | Output |
|---|---|---|
| Linux | `packaging/linux/build_appimage.sh` | `DeltaF_Studio-<v>-linux-<arch>.AppImage` |
| macOS | `packaging/macos/build_app.sh` | `DeltaF Studio.app`, `DeltaF_Studio-<v>-macos-universal.dmg` |
| Windows | `powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1` | `DeltaF_Studio-<v>-windows-x64-setup.exe`, `...-portable.zip` |
| Windows (Microsoft Store) | then `powershell -ExecutionPolicy Bypass -File packaging\windows\msix\build_msix.ps1` | `DeltaF_Studio-<v>-windows-x64.msix` |

Each package contains `dfstudio`, `dfs`, the CoolProp library and the
example projects, which the app writes itself during packaging
(`dfstudio --write-examples DIR` solves each built-in template and saves it
as a `.df` project with its cycle diagram).

* **Linux**: needs `curl`; [appimagetool](https://github.com/AppImage/appimagetool)
  is downloaded when it is not on `PATH`. The prebuilt CoolProp library of
  `coolprop-sys` needs glibc 2.38 or newer, so the build machine and the
  user's system need it too (the release is built on Ubuntu 24.04).
* **macOS**: install both Rust targets for a universal build:
  `rustup target add aarch64-apple-darwin x86_64-apple-darwin`. The app is
  signed ad hoc. For distribution without the Gatekeeper warning, sign it
  with a Developer ID certificate and notarise it (`codesign`,
  `xcrun notarytool`).
* **Windows**: needs [Inno Setup 6](https://jrsoftware.org/isinfo.php). The
  installer (`packaging/windows/dfstudio.iss`) asks whether to install for
  all users (administrator rights, `C:\Program Files\DeltaF Studio`) or for the
  current user only (no administrator rights,
  `%LOCALAPPDATA%\Programs\DeltaF Studio`). Optional tasks, checked by default
  unless noted: `startmenu` (Start menu entry), `desktopicon` (unchecked),
  `addtopath` (install folder appended to the user or system PATH by the
  `[Code]` section and removed on uninstall, plus an App Paths entry),
  `assocdf` (`.df`, `.dfs`) and `assocees` (`.ees`). Unattended:
  `DeltaF_Studio-<v>-windows-x64-setup.exe /VERYSILENT /CURRENTUSER` (or
  `/ALLUSERS`); deselect tasks with e.g. `/MERGETASKS="!addtopath,!assocees"`. A local build is not code-signed, so SmartScreen may
  ask for confirmation on first run; release builds are signed as described
  below.
  The MSIX package for the Microsoft Store and the winget manifests are
  described in [STORE_AND_WINGET.md](STORE_AND_WINGET.md).

## Making a release

1. Update `version` in `Cargo.toml` (workspace) and `python/pyproject.toml`,
   and move the *Unreleased* entries of `CHANGELOG.md` under a new
   `## [X.Y.Z] - date` heading.
2. Merge to `main` and wait for CI to pass.
3. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.

The [Release workflow](../.github/workflows/release.yml) then checks that
the tag matches `Cargo.toml`, runs the tests on the three platforms, builds
the three packages, smoke-tests the bundled `dfs` against an example, and
publishes a GitHub release with the changelog section, the packages and
`SHA256SUMS.txt`. Running the workflow by hand (*Actions > Release > Run
workflow*) builds the packages as downloadable artifacts without
publishing.

After the release, the workflow submits the new version to winget when the
`WINGET_TOKEN` secret is set. The Store package (`msix-store` artifact) is
uploaded to Partner Center by hand ([STORE_AND_WINGET.md](STORE_AND_WINGET.md)).

## Windows code signing

Windows releases are signed through the SignPath Foundation open-source
program ([CODE_SIGNING.md](../CODE_SIGNING.md) is the policy). The `windows`
job of the release workflow runs `build.ps1 -StageOnly`, has SignPath sign
`dfstudio.exe` and `dfs.exe`, runs `build.ps1 -PackageOnly` to build the
installer and portable zip from the signed files, and has SignPath sign the
installer. Each of the two signing requests waits (up to an hour) for an
approver to approve it at <https://app.signpath.io>. Rust build caches are
not used in a signed build.

Signing runs for a tag push, or for a manual run with a *signing policy*
other than `none`, and only when these are set in the repository settings
(*Settings > Secrets and variables > Actions*):

| Name | Kind | Value |
|---|---|---|
| `SIGNPATH_API_TOKEN` | secret | API token of the SignPath CI user (submitter) |
| `SIGNPATH_ORGANIZATION_ID` | variable | SignPath organization ID |
| `SIGNPATH_PROJECT_SLUG` | variable, optional | project slug, default `dfstudio` |
| `SIGNPATH_SIGNING_POLICY_SLUG` | variable, optional | policy for tag builds, default `release-signing` |

Without them, or for pull requests, the workflow builds unsigned packages.
The SignPath project needs two artifact configurations, `executables` and
`installer`; copies are kept in
[`.signpath/artifact-configurations/`](../.signpath/artifact-configurations).
`dfstudio.exe`, `dfs.exe` and the installer all carry the product name
`DeltaF Studio` and the version from `Cargo.toml` (set by `build.rs` and
`dfstudio.iss`), which those configurations check.

## Third-party notices

`THIRD_PARTY_NOTICES.md` lists every crate in `Cargo.lock` with its licence.
After changing dependencies, regenerate the table with:

```bash
cargo metadata --format-version 1 --locked | python3 -c '
import json, sys
m = json.load(sys.stdin)
for n, v, l in sorted((p["name"], p["version"], p["license"] or "?") for p in m["packages"] if p["source"]):
    print(f"| {n} | {v} | {l} |")'
```
