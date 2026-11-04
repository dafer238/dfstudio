; Inno Setup script for DeltaF Studio (https://jrsoftware.org/isinfo.php).
; Built by build.ps1, which stages the files in ..\..\dist\windows\stage and
; passes the version:  ISCC /DAppVersion=0.1.0 dfstudio.iss
;
; Installing needs no administrator rights. The first page asks:
;   * "Install for all users"  (needs admin) -> C:\Program Files\DeltaF Studio
;   * "Install for me only"                   -> %LOCALAPPDATA%\Programs\DeltaF Studio
; From the command line: /ALLUSERS or /CURRENTUSER (add /VERYSILENT for an
; unattended install).
;
; Optional tasks (all checked by default except the desktop icon; from the
; command line, e.g. /TASKS="startmenu,addtopath" or /MERGETASKS="!assocees"):
;   startmenu  Start menu entry
;   addtopath  install folder on the PATH (user PATH for a per-user install,
;              system PATH for an all-users install), removed on uninstall
;   assocdf    open .df and .dfs files with DeltaF Studio
;   assocees   open .ees files with DeltaF Studio

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
; numeric form for the version resource (build.ps1 strips -rc.1 and the like)
#ifndef AppNumVersion
  #define AppNumVersion AppVersion
#endif
#define Stage "..\..\dist\windows\stage"

[Setup]
AppId={{9F99A851-0C78-4DD2-AE59-F58EBAD92123}
AppName=DeltaF Studio
AppVersion={#AppVersion}
AppVerName=DeltaF Studio {#AppVersion}
AppPublisher=DeltaF Studio contributors
AppPublisherURL=https://github.com/dafer238/dfstudio
AppSupportURL=https://github.com/dafer238/dfstudio/issues
AppUpdatesURL=https://github.com/dafer238/dfstudio/releases
DefaultDirName={autopf}\DeltaF Studio
DefaultGroupName=DeltaF Studio
DisableProgramGroupPage=yes
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\icons\dfstudio.ico
UninstallDisplayIcon={app}\dfstudio.exe
UninstallDisplayName=DeltaF Studio {#AppVersion}
; Version resource of the setup .exe. Code signing checks the product name
; and version, so keep them equal to those of dfstudio.exe and dfs.exe.
VersionInfoVersion={#AppNumVersion}
VersionInfoProductName=DeltaF Studio
VersionInfoProductVersion={#AppNumVersion}
VersionInfoProductTextVersion={#AppVersion}
VersionInfoDescription=DeltaF Studio Setup
OutputDir=..\..\dist
OutputBaseFilename=DeltaF_Studio-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
ChangesEnvironment=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startmenu"; Description: "Add DeltaF Studio to the Start menu"; GroupDescription: "Shortcuts:"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "addtopath"; Description: "Add DeltaF Studio to the PATH (run dfstudio and dfs from any terminal)"; GroupDescription: "Command line:"
Name: "assocdf"; Description: "DeltaF Studio projects and scripts (.df, .dfs)"; GroupDescription: "Open these files with DeltaF Studio on double-click:"
Name: "assocees"; Description: "EES models (.ees)"; GroupDescription: "Open these files with DeltaF Studio on double-click:"

[Files]
Source: "{#Stage}\dfstudio.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\dfs.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\CoolProp.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\dfstudio.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\examples\*"; DestDir: "{app}\examples"; Flags: ignoreversion recursesubdirs

[InstallDelete]
; Shortcuts left by installers from before the rename to DeltaF Studio (<= 1.0.0).
Type: files; Name: "{autoprograms}\DF Studio.lnk"
Type: files; Name: "{autoprograms}\DF Studio examples.lnk"
Type: files; Name: "{autodesktop}\DF Studio.lnk"

[Icons]
Name: "{autoprograms}\DeltaF Studio"; Filename: "{app}\dfstudio.exe"; IconFilename: "{app}\dfstudio.ico"; Comment: "dfstudio equation solver"; Tasks: startmenu
Name: "{autoprograms}\DeltaF Studio examples"; Filename: "{app}\examples"; Tasks: startmenu
Name: "{autodesktop}\DeltaF Studio"; Filename: "{app}\dfstudio.exe"; IconFilename: "{app}\dfstudio.ico"; Tasks: desktopicon

[Registry]
; HKA = HKLM for an all-users install, HKCU for a per-user one.
Root: HKA; Subkey: "Software\Classes\.df"; ValueType: string; ValueName: ""; ValueData: "DFStudio.Project"; Flags: uninsdeletevalue; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\.dfs"; ValueType: string; ValueName: ""; ValueData: "DFStudio.Script"; Flags: uninsdeletevalue; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\DFStudio.Project"; ValueType: string; ValueName: ""; ValueData: "DeltaF Studio project"; Flags: uninsdeletekey; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\DFStudio.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\dfstudio.ico"; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\DFStudio.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\dfstudio.exe"" ""%1"""; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\DFStudio.Script"; ValueType: string; ValueName: ""; ValueData: "DeltaF Studio script"; Flags: uninsdeletekey; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\DFStudio.Script\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\dfstudio.ico"; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\DFStudio.Script\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\dfstudio.exe"" ""%1"""; Tasks: assocdf

; Also list DeltaF Studio under "Open with" for each extension, so it stays one
; click away even when Windows keeps another program as the user's default.
Root: HKA; Subkey: "Software\Classes\.df\OpenWithProgids"; ValueType: string; ValueName: "DFStudio.Project"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assocdf
Root: HKA; Subkey: "Software\Classes\.dfs\OpenWithProgids"; ValueType: string; ValueName: "DFStudio.Script"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assocdf
; .ees: EES models, imported when opened.
Root: HKA; Subkey: "Software\Classes\.ees"; ValueType: string; ValueName: ""; ValueData: "DFStudio.EES"; Flags: uninsdeletevalue; Tasks: assocees
Root: HKA; Subkey: "Software\Classes\.ees\OpenWithProgids"; ValueType: string; ValueName: "DFStudio.EES"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assocees
Root: HKA; Subkey: "Software\Classes\DFStudio.EES"; ValueType: string; ValueName: ""; ValueData: "EES model"; Flags: uninsdeletekey; Tasks: assocees
Root: HKA; Subkey: "Software\Classes\DFStudio.EES\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\dfstudio.ico"; Tasks: assocees
Root: HKA; Subkey: "Software\Classes\DFStudio.EES\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\dfstudio.exe"" ""%1"""; Tasks: assocees
; App Paths: lets Win+R (and Start search) run "dfstudio" by name.
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\dfstudio.exe"; ValueType: string; ValueName: ""; ValueData: "{app}\dfstudio.exe"; Flags: uninsdeletekey; Tasks: addtopath

[Run]
Filename: "{app}\dfstudio.exe"; Description: "{cm:LaunchProgram,DeltaF Studio}"; Flags: nowait postinstall skipifsilent

[Code]
{ PATH handling. The install folder is appended to the user PATH
  (HKCU\Environment) for a per-user install, or to the system PATH
  (HKLM\...\Session Manager\Environment) for an all-users install, and removed
  again on uninstall. Entries are compared case-insensitively, so running the
  installer twice does not add a duplicate. ChangesEnvironment=yes broadcasts
  the change, so terminals opened afterwards see it. }
const
  EnvKeyUser = 'Environment';
  EnvKeySystem = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';

function EnvRoot(): Integer;
begin
  if IsAdminInstallMode then Result := HKEY_LOCAL_MACHINE else Result := HKEY_CURRENT_USER;
end;

function EnvKey(): String;
begin
  if IsAdminInstallMode then Result := EnvKeySystem else Result := EnvKeyUser;
end;

procedure AddToPath(Dir: String);
var
  Paths: String;
begin
  if not RegQueryStringValue(EnvRoot(), EnvKey(), 'Path', Paths) then Paths := '';
  if Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') > 0 then Exit;
  if (Paths <> '') and (Copy(Paths, Length(Paths), 1) <> ';') then Paths := Paths + ';';
  if not RegWriteExpandStringValue(EnvRoot(), EnvKey(), 'Path', Paths + Dir) then
    Log('Could not add ' + Dir + ' to the PATH');
end;

procedure RemoveFromPath(Dir: String);
var
  Paths: String;
  P: Integer;
begin
  if not RegQueryStringValue(EnvRoot(), EnvKey(), 'Path', Paths) then Exit;
  Paths := ';' + Paths + ';';
  P := Pos(';' + Uppercase(Dir) + ';', Uppercase(Paths));
  if P = 0 then Exit;
  Delete(Paths, P, Length(Dir) + 1);
  Paths := Copy(Paths, 2, Length(Paths) - 2);
  RegWriteExpandStringValue(EnvRoot(), EnvKey(), 'Path', Paths);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then
    AddToPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}'));
end;
