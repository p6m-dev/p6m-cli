; See https://jrsoftware.org/isinfo.php for more information on Inno Setup
[Setup]
AppName=Ybor CLI
AppVersion={#GetEnv('YBOR_VERSION')}
AppPublisher=Ybor Group
AppPublisherURL=https://ybor.ai
DefaultDirName={autopf}\YborCli
DefaultGroupName=YborCli
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
AllowNoIcons=yes
OutputBaseFilename=ybor-installer
Compression=zip
SolidCompression=no
WizardStyle=modern
SourceDir={#GetEnv('GITHUB_WORKSPACE')}
OutputDir=.

[Files]
Source: "{#GetEnv('YBOR_BINARY')}"; DestDir: "{app}"; Flags: ignoreversion

[Run]
Filename: "{cmd}"; Parameters: "/c ybor --version || setx PATH ""%PATH%;{app};"""; Flags: runhidden; StatusMsg: "Adding YborCli to PATH..."

