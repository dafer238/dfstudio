# Publishing on the Microsoft Store and winget

DeltaF Studio for Windows is distributed in three ways:

| Channel | What users get | Signed by |
|---|---|---|
| GitHub releases | `DeltaF_Studio-<v>-windows-x64-setup.exe` (Inno Setup) and the portable `.zip` | not signed yet (SmartScreen warns on download) |
| winget | the same `setup.exe`, downloaded and installed by `winget install dafer238.DeltaFStudio` | as above |
| Microsoft Store ([9NSJN0W1J4D3](https://apps.microsoft.com/detail/9NSJN0W1J4D3)) | an MSIX package, `DeltaF_Studio-<v>-windows-x64.msix` | Microsoft, when the Store publishes it |

The README, the user guide and the website list them in this order: the
direct downloads (always the latest), winget (updated automatically with
each release), then the Store (updated by hand, after certification).

The Store re-signs every package it publishes with a Microsoft certificate,
so the Store version installs without a SmartScreen prompt. A browser
download of the `setup.exe` keeps the prompt until the file itself is
code-signed. winget installs from a terminal, which normally shows no
SmartScreen dialog, but the file is the same one.

## Microsoft Store

### One-time setup (Partner Center)

1. Create a developer account at <https://storedeveloper.microsoft.com>
   (individual account, free). It opens
   [Partner Center](https://partner.microsoft.com/dashboard/apps-and-games/overview).
2. *Apps and games > New product > MSIX or PWA app*, and reserve the name
   **DeltaF Studio**.
3. *Product management > Product identity* lists three values. Copy them
   into [`packaging/windows/msix/store-identity.json`](../packaging/windows/msix/store-identity.json):

   | Partner Center | `store-identity.json` |
   |---|---|
   | `Package/Identity/Name` | `IdentityName` |
   | `Package/Identity/Publisher` (`CN=...`) | `Publisher` |
   | `Package/Properties/PublisherDisplayName` | `PublisherDisplayName` |

   These values are public (they are in every package), not secrets. The
   placeholder `DeltaFStudio.Dev` identity builds a test package that
   cannot be submitted.

### Building the package

The `windows` job of the [Release workflow](../.github/workflows/release.yml)
runs [`build_msix.ps1`](../packaging/windows/msix/build_msix.ps1) after the
Inno Setup build. It packages the same staged files (`dfstudio.exe`,
`dfs.exe`, `CoolProp.dll`, the examples, licence and notices) with
[`AppxManifest.xml`](../packaging/windows/msix/AppxManifest.xml), which
declares:

* the Start menu entry (DeltaF Studio) and its icons (`Assets/`, generated
  from `packaging/icons/dfstudio.png` by `make_assets.py`);
* the `dfstudio` command (an app execution alias, the MSIX equivalent of
  adding the install folder to the PATH);
* `.df`, `.dfs` and `.ees` file associations;
* `runFullTrust`, required for a desktop (Win32) program.

The package holds the desktop app only, without the `dfs` command-line
solver: a second application without a Start menu entry needs a
HeadlessAppBypass waiver from Microsoft, and one application can have only
one execution alias. `dfs` comes with the installer and winget.

The package version is the `Cargo.toml` version with a fourth field 0
(1.0.1 becomes 1.0.1.0), as the Store requires. The workflow also signs a
copy with a throwaway certificate, installs it, runs
`dfstudio --write-examples` through its alias (which solves every built-in
template with the packaged CoolProp) and uninstalls it. The unsigned package is uploaded as the
`msix-store` artifact of the run; it is not attached to the GitHub release,
because it does not install until the Store signs it.

Locally, after `packaging\windows\build.ps1`:

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\msix\build_msix.ps1 -TestSign
```

### Submitting a version

1. Download the `msix-store` artifact from the Release run of the tag
   (*Actions > Release > the run > Artifacts*) and unzip it.
2. In Partner Center, *Start submission* (or *Update* for a later version)
   and fill in the sections:
   * **Pricing and availability**: free; markets.
   * **Properties**: category *Developer tools* (or *Education*); a privacy
     policy URL if asked (the app collects no personal data;
     <https://deltafstudio.com> can host the statement).
   * **Age ratings**: the questionnaire (no user interaction, no data
     sharing).
   * **Packages**: upload `DeltaF_Studio-<v>-windows-x64.msix`.
   * **Store listings**: description and at least one screenshot (the
     images in `docs/img/` are suitable).
   * **Submission options**: the justification for `runFullTrust`, e.g.
     "DeltaF Studio is a desktop (Win32) application packaged as MSIX; it
     needs full trust to run."
3. Submit. Certification usually takes from a few hours to three working
   days.

Every new submission needs a higher package version, which a new
`Cargo.toml` version gives. Once published, the Store version is also
available through winget's `msstore` source.

## winget

The manifest of each version lives in
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) under
`manifests/d/dafer238/DeltaFStudio/<version>/`. The package identifier
is `dafer238.DeltaFStudio`; it cannot be changed after the first
version is accepted.

### First version (by hand, once)

[`packaging/winget/manifests/`](../packaging/winget/manifests) holds the
manifest of 1.0.3, validated against the winget 1.10 schemas. To submit it:

1. Optional local test on Windows (administrator terminal):
   ```powershell
   winget settings --enable LocalManifestFiles
   winget validate --manifest packaging\winget\manifests\d\dafer238\DeltaFStudio\1.0.3
   winget install --manifest packaging\winget\manifests\d\dafer238\DeltaFStudio\1.0.3
   ```
2. Submit it with [wingetcreate](https://github.com/microsoft/winget-create)
   (`winget install wingetcreate`), which forks winget-pkgs and opens the
   pull request:
   ```powershell
   wingetcreate submit packaging\winget\manifests\d\dafer238\DeltaFStudio\1.0.3
   ```
   Without `--token`, wingetcreate signs in to GitHub in the browser and
   keeps the session; a token on the command line can end up in logs and
   the shell history.
   or copy the folder into a fork of winget-pkgs at the same path and open
   the pull request on GitHub.
3. The winget-pkgs bots validate and test-install the package; a moderator
   merges the pull request, usually within a few days.

### Later versions (automatic)

The `winget` job of the Release workflow runs after the GitHub release is
published. It uses [Komac](https://github.com/russellbanks/Komac) to write
the manifest of the new version from the release's `setup.exe` and opens
the pull request to winget-pkgs. It needs a repository secret
`WINGET_TOKEN`: a **classic** personal access token with the `public_repo`
scope (fine-grained tokens cannot open pull requests on other accounts'
repositories) of the account that owns the winget-pkgs fork. Without the
secret the job does nothing.

When the release workflow signs the installer, winget picks up the signed
file automatically, since it uses the release asset.
