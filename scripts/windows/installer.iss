#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef NumericVersion
  #error NumericVersion is required
#endif
#ifndef SourceDirectory
  #error SourceDirectory is required
#endif
#ifndef DestinationDirectory
  #error DestinationDirectory is required
#endif

[Setup]
; This ID must stay the same across releases so Setup upgrades the existing app.
AppId={{5A204178-F373-4278-9A9E-D1F69F835152}
AppName=Scry
AppVersion={#AppVersion}
VersionInfoVersion={#NumericVersion}
DefaultDirName={localappdata}\Programs\Scry
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UsePreviousAppDir=yes
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\scry.exe
OutputDir={#DestinationDirectory}
OutputBaseFilename=Scry-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
SetupMutex=ScrySetup

[Files]
Source: "{#SourceDirectory}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Scry"; Filename: "{app}\scry.exe"; WorkingDir: "{app}"

[Run]
Filename: "{app}\scry.exe"; Description: "Open Scry"; Flags: nowait postinstall skipifsilent
