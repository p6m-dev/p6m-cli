; See https://jrsoftware.org/isinfo.php for more information on Inno Setup
[Setup]
AppName=p6m CLI
AppVersion={#GetEnv('P6M_VERSION')}
AppPublisher=P6m Dev
AppPublisherURL=https://p6m.dev
DefaultDirName={autopf}\P6mCli
DefaultGroupName=P6mCli
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
AllowNoIcons=yes
OutputBaseFilename=p6m-installer
Compression=zip
SolidCompression=no
WizardStyle=modern
SourceDir={#GetEnv('GITHUB_WORKSPACE')}
OutputDir=.

[Files]
Source: "{#GetEnv('P6M_BINARY')}"; DestDir: "{app}"; Flags: ignoreversion

[Run]
Filename: "{cmd}"; Parameters: "/c p6m --version || setx PATH ""%PATH%;{app};"""; Flags: runhidden; StatusMsg: "Adding Yp to PATH..."

