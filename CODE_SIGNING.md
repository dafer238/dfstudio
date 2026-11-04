# Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io),
certificate by [SignPath Foundation](https://signpath.org).

> **Status:** the application to the SignPath Foundation open-source program
> is in progress. Releases up to and including 1.0.1 are not signed. This
> page describes how Windows releases are signed once it is approved; the
> notes of each release say whether its files are signed.

The same policy is published at <https://deltafstudio.com/code-signing>.

## What is signed

Only files built from this repository by its public
[release workflow](.github/workflows/release.yml) on GitHub-hosted runners:

| File | Signed |
|---|---|
| `dfstudio.exe` (desktop app) and `dfs.exe` (command-line solver) | yes, before they are packed |
| `DeltaF_Studio-<version>-windows-x64-setup.exe` (installer) | yes |
| `DeltaF_Studio-<version>-windows-x64-portable.zip` | contains the signed `dfstudio.exe` and `dfs.exe` |
| `CoolProp.dll` | no: a third-party library ([CoolProp](https://github.com/CoolProp/CoolProp), MIT), shipped as published by its authors |

The macOS and Linux packages are not part of this program.

## How a release is signed

1. A maintainer pushes a version tag `vX.Y.Z`. The release workflow builds
   the program from that commit, without build caches, and runs the tests.
2. The workflow uploads the unsigned executables as a GitHub Actions artifact
   and submits a signing request to SignPath. SignPath checks that the
   artifact was produced by this repository's workflow (origin verification)
   and that the product name (`DeltaF Studio`) and version of every file match
   the release.
3. An approver reviews and approves the request in SignPath. Nothing is
   signed without this manual approval.
4. The installer is built from the signed executables and goes through the
   same steps 2 and 3.
5. The workflow publishes the signed files on the
   [GitHub releases page](https://github.com/dafer238/dfstudio/releases),
   with their SHA-256 checksums.

The signing key is kept by SignPath in a hardware security module; the
project's maintainers never hold it.

## Team roles

DeltaF Studio is maintained by one person, who holds all three roles:

| Role | Members |
|---|---|
| Committers and reviewers: may change the source code; review every pull request from other contributors before it is merged | [dafer238](https://github.com/dafer238) |
| Approvers: approve each signing request | [dafer238](https://github.com/dafer238) |

All team members use multi-factor authentication on GitHub and SignPath.
Contributions from anyone else reach the repository only through reviewed
pull requests ([CONTRIBUTING.md](CONTRIBUTING.md)).

## Privacy

This program will not transfer any information to other networked systems
unless specifically requested by the user or the person installing or
operating it. DeltaF Studio has no telemetry, no update check and no
account; it does not use the network. The only network access is opening a
web page in your browser when you click a link (for example in
*Help > Support DeltaF Studio*). The bundled CoolProp library computes fluid
properties locally and does not use the network either.

## Installation and removal

The installer says what it changes (install folder, Start menu entry, PATH,
file associations) and lets you turn each optional change off. It registers
an uninstaller under *Settings > Apps > Installed apps*, which removes the
program, its PATH entry and its file associations. The portable zip changes
nothing on the system: delete the folder to remove it.

## Reporting a problem

If you find a signed file that was not built from this repository, or any
other misuse of the certificate, open an
[issue](https://github.com/dafer238/dfstudio/issues) or write to SignPath
Foundation at <https://signpath.org>.
