; Inno Setup script for DF Studio (https://jrsoftware.org/isinfo.php).
; Built by build.ps1, which stages the files in ..\..\dist\windows\stage and
; passes the version:  ISCC /DAppVersion=0.1.0 dfstudio.iss
;
; Installing needs no administrator rights. The first page asks:
;   * "Install for all users"  (needs admin) -> C:\Program Files\DF Studio
;   * "Install for me only"                   -> %LOCALAPPDATA%\Programs\DF Studio
; From the command line: /ALLUSERS or /CURRENTUSER (add /VERYSILENT for an
; unattended install).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#define Stage "..\..\dist\windows\stage"

[Setup]
AppId={{9F99A851-0C78-4DD2-AE59-F58EBAD92123}
AppName=DF Studio
AppVersion={#AppVersion}
AppVerName=DF Studio {#AppVersion}
AppPublisher=DF Studio contributors
AppPublisherURL=https://github.com/dafer238/dfstudio
AppSupportURL=https://github.com/dafer238/dfstudio/issues
AppUpdatesURL=https://github.com/dafer238/dfstudio/releases
DefaultDirName={autopf}\DF Studio
DefaultGroupName=DF Studio
DisableProgramGroupPage=yes
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\icons\dfstudio.ico
UninstallDisplayIcon={app}\dfstudio.exe
UninstallDisplayName=DF Studio {#AppVersion}
OutputDir=..\..\dist
OutputBaseFilename=DF_Studio-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "assoc"; Description: "Open .df and .dfs files with DF Studio"; GroupDescription: "File associations:"

[Files]
Source: "{#Stage}\dfstudio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\dfs.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\CoolProp.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\dfstudio.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\examples\*"; DestDir: "{app}\examples"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\DF Studio"; Filename: "{app}\dfstudio.exe"; IconFilename: "{app}\dfstudio.ico"
Name: "{autoprograms}\DF Studio examples"; Filename: "{app}\examples"
Name: "{autodesktop}\DF Studio"; Filename: "{app}\dfstudio.exe"; IconFilename: "{app}\dfstudio.ico"; Tasks: desktopicon

[Registry]
; HKA = HKLM for an all-users install, HKCU for a per-user one.
Root: HKA; Subkey: "Software\Classes\.df"; ValueType: string; ValueName: ""; ValueData: "DFStudio.Project"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.dfs"; ValueType: string; ValueName: ""; ValueData: "DFStudio.Script"; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\DFStudio.Project"; ValueType: string; ValueName: ""; ValueData: "DF Studio project"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\DFStudio.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\dfstudio.ico"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\DFStudio.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\dfstudio.exe"" ""%1"""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\DFStudio.Script"; ValueType: string; ValueName: ""; ValueData: "DF Studio script"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\DFStudio.Script\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\dfstudio.ico"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\DFStudio.Script\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\dfstudio.exe"" ""%1"""; Tasks: assoc

[Run]
Filename: "{app}\dfstudio.exe"; Description: "{cm:LaunchProgram,DF Studio}"; Flags: nowait postinstall skipifsilent
