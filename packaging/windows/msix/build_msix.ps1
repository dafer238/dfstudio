# Build the Microsoft Store package (MSIX) of DeltaF Studio from the files that
# build.ps1 staged in dist\windows\stage (all but dfs.exe):
#   powershell -ExecutionPolicy Bypass -File packaging\windows\build.ps1
#   powershell -ExecutionPolicy Bypass -File packaging\windows\msix\build_msix.ps1 [-TestSign]
# Needs the Windows 10/11 SDK (makeappx.exe, makepri.exe, signtool.exe).
# Output: dist\DeltaF_Studio-<version>-windows-x64.msix, unsigned. Partner
# Center signs it when it is submitted to the Store; it cannot be installed
# before that.
# -TestSign also writes dist\windows\msix-test\DeltaF_Studio-<version>-windows-x64.msix,
# signed with a new self-signed certificate (exported next to it as
# test-cert.cer), to install and test the package locally or in CI. Installing
# it needs that certificate in Local Machine > Trusted People.
param([switch]$TestSign)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
Set-Location $root
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if ($version -notmatch '^(\d+)\.(\d+)\.(\d+)$') { throw "version $version is not X.Y.Z" }
$msixVersion = "$version.0"

$id = Get-Content (Join-Path $PSScriptRoot "store-identity.json") -Raw | ConvertFrom-Json
if ($id.Publisher -notmatch '^CN=') { throw "store-identity.json: Publisher must start with CN=" }
if ($id.IdentityName -eq "DeltaFStudio.Dev") {
    Write-Warning "store-identity.json has the placeholder identity: this package is for testing and cannot be submitted to the Store"
}

# Newest Windows SDK tools
$kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
$sdk = Get-ChildItem $kits -Directory | Where-Object { $_.Name -match '^10\.' -and (Test-Path (Join-Path $_.FullName "x64\makeappx.exe")) } |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { throw "Windows SDK (makeappx.exe) not found under $kits" }
$bin = Join-Path $sdk.FullName "x64"
Write-Host "Windows SDK tools: $bin"

$stage = Join-Path $root "dist\windows\stage"
if (-not (Test-Path (Join-Path $stage "dfstudio.exe"))) { throw "run build.ps1 first: $stage\dfstudio.exe not found" }
$work = Join-Path $root "dist\windows\msix"
$layout = Join-Path $work "layout"
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
New-Item -ItemType Directory -Force $layout | Out-Null
Copy-Item -Recurse (Join-Path $stage "*") $layout
# desktop app only: see the comment at the top of AppxManifest.xml
Remove-Item (Join-Path $layout "dfs.exe")
Copy-Item -Recurse (Join-Path $PSScriptRoot "Assets") $layout

$manifest = (Get-Content (Join-Path $PSScriptRoot "AppxManifest.xml") -Raw).
    Replace("@IdentityName@", [Security.SecurityElement]::Escape($id.IdentityName)).
    Replace("@Publisher@", [Security.SecurityElement]::Escape($id.Publisher)).
    Replace("@PublisherDisplayName@", [Security.SecurityElement]::Escape($id.PublisherDisplayName)).
    Replace("@Version@", $msixVersion)
if ($manifest -match '@\w+@') { throw "unfilled value in AppxManifest.xml: $($Matches[0])" }
[IO.File]::WriteAllText((Join-Path $layout "AppxManifest.xml"), $manifest, (New-Object Text.UTF8Encoding $false))

# resources.pri maps Assets\Square44x44Logo.png etc. to the scale-* and
# targetsize-* files, so Windows picks the right size for each place.
$priconfig = Join-Path $work "priconfig.xml"
& (Join-Path $bin "makepri.exe") createconfig /cf $priconfig /dq en-US /pv 10.0.0 /o
if ($LASTEXITCODE -ne 0) { throw "makepri createconfig failed" }
# One package, not a bundle: keep every scale in resources.pri instead of
# splitting resources.scale-200.pri out for a resource package.
[xml]$cfg = Get-Content $priconfig
$cfg.SelectNodes("//packaging/autoResourcePackage") | ForEach-Object { [void]$_.ParentNode.RemoveChild($_) }
$cfg.Save($priconfig)
& (Join-Path $bin "makepri.exe") new /pr $layout /cf $priconfig /mn (Join-Path $layout "AppxManifest.xml") /of (Join-Path $layout "resources.pri") /o
if ($LASTEXITCODE -ne 0) { throw "makepri new failed" }
$split = Get-ChildItem $layout -Filter "resources.*.pri"
if ($split) { throw "makepri split the resources: $($split.Name -join ', ')" }

$name = "DeltaF_Studio-$version-windows-x64.msix"
$msix = Join-Path $root "dist\$name"
& (Join-Path $bin "makeappx.exe") pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed" }
Write-Host "wrote dist\$name ($($id.IdentityName) $msixVersion)"

if ($TestSign) {
    $testDir = Join-Path $root "dist\windows\msix-test"
    New-Item -ItemType Directory -Force $testDir | Out-Null
    $cert = New-SelfSignedCertificate -Type Custom -Subject $id.Publisher -KeyUsage DigitalSignature `
        -FriendlyName "DeltaF Studio MSIX test" -CertStoreLocation Cert:\CurrentUser\My `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
    $test = Join-Path $testDir $name
    Copy-Item $msix $test -Force
    & (Join-Path $bin "signtool.exe") sign /fd SHA256 /sha1 $cert.Thumbprint $test
    if ($LASTEXITCODE -ne 0) { throw "signtool sign failed" }
    Export-Certificate -Cert $cert -FilePath (Join-Path $testDir "test-cert.cer") | Out-Null
    Write-Host "wrote test-signed $test and test-cert.cer"
}
