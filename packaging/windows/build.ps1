# Build the DF Studio installer and portable zip for Windows x64.
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1
# Needs Rust (MSVC toolchain) and Inno Setup 6 (ISCC.exe on PATH or in its
# default folder). Output in dist\:
#   DF_Studio-<version>-windows-x64-setup.exe
#   DF_Studio-<version>-windows-x64-portable.zip
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $root
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value

cargo build --release --locked -p dfapp -p dfcli
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$dll = Get-ChildItem -Path target\release\build -Recurse -Filter CoolProp.dll |
    Where-Object { $_.FullName -match 'coolprop-sys-windows' -and $_.FullName -match '\\out\\' } |
    Select-Object -First 1
if (-not $dll) { throw "CoolProp.dll not found under target\release\build" }

$stage = Join-Path $root "dist\windows\stage"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force (Join-Path $stage "examples") | Out-Null
Copy-Item target\release\dfstudio.exe, target\release\dfs.exe, $dll.FullName, packaging\icons\dfstudio.ico, THIRD_PARTY_NOTICES.md $stage
Copy-Item LICENSE (Join-Path $stage "LICENSE.txt")

# dfstudio.exe is a GUI-subsystem program: wait for it explicitly.
$p = Start-Process -FilePath (Join-Path $stage "dfstudio.exe") -ArgumentList "--write-examples", "`"$(Join-Path $stage 'examples')`"" -Wait -PassThru -NoNewWindow
if ($p.ExitCode -ne 0) { throw "writing the examples failed ($($p.ExitCode))" }
if (-not (Get-ChildItem (Join-Path $stage "examples") -Filter *.df)) { throw "no examples written" }

# Command-line smoke test of the staged files (CoolProp found next to the exe).
& (Join-Path $stage "dfs.exe") check examples\rankine.dfs
if ($LASTEXITCODE -ne 0) { throw "dfs smoke test failed" }

$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
& $iscc "/DAppVersion=$version" packaging\windows\dfstudio.iss
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }

$zip = Join-Path $root "dist\DF_Studio-$version-windows-x64-portable.zip"
$portable = Join-Path $root "dist\windows\DF Studio"
if (Test-Path $portable) { Remove-Item -Recurse -Force $portable }
Copy-Item -Recurse $stage $portable
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $portable -DestinationPath $zip
Write-Host "wrote dist\DF_Studio-$version-windows-x64-setup.exe and $zip"
