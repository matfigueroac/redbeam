#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef AppBinary
  #error AppBinary is required
#endif
#ifndef ThirdPartyNotices
  #error ThirdPartyNotices is required
#endif
#ifndef LlvmLicense
  #error LlvmLicense is required
#endif
#ifndef LlvmRuntime
  #error LlvmRuntime is required
#endif

[Setup]
AppId={{C5034EAE-B272-4175-92B5-F6284A1187A1}
AppName=RedBIM
AppVersion={#AppVersion}
AppPublisher=RedBIM
AppPublisherURL=https://github.com/matfigueroac/redbim
AppSupportURL=https://github.com/matfigueroac/redbim/issues
AppUpdatesURL=https://github.com/matfigueroac/redbim/releases/latest
DefaultDirName={autopf}\RedBIM
DefaultGroupName=RedBIM
DisableProgramGroupPage=yes
AllowNoIcons=yes
PrivilegesRequired=lowest
MinVersion=10.0.19045
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\redbim.exe
SetupIconFile=..\assets\redbim.ico
OutputBaseFilename=RedBIM-Setup-x64
VersionInfoVersion={#AppVersion}
VersionInfoProductTextVersion={#AppVersion}
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Files]
Source: "{#AppBinary}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#LlvmRuntime}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestName: "Licencia-RedBIM.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ThirdPartyNotices}"; DestName: "Licencias-de-terceros.html"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#LlvmLicense}"; DestName: "Licencia-LLVM.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\RedBIM"; Filename: "{app}\redbim.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\RedBIM"; Filename: "{app}\redbim.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\redbim.exe"; Description: "{cm:LaunchProgram,RedBIM}"; Flags: nowait postinstall skipifsilent
