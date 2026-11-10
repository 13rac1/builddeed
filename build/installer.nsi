; NSIS installer for BuildDeed Checksum. It installs for the current user
; only, so it does not need administrator rights.
Unicode true
!ifndef VERSION
  !error "Give /DVERSION=x.y.z"
!endif
!ifndef EXE
  !error "Give /DEXE=path"
!endif
!ifndef OUT
  !error "Give /DOUT=path"
!endif

Name "BuildDeed Checksum ${VERSION}"
OutFile "${OUT}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\BuildDeed Checksum"
SetCompressor /SOLID lzma
; File times in the installer come from the source file. The build script
; sets them to SOURCE_DATE_EPOCH.
SetDateSave on

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  File /oname=builddeed-checksum.exe "${EXE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\BuildDeed Checksum.lnk" "$INSTDIR\builddeed-checksum.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BuildDeedChecksum" "DisplayName" "BuildDeed Checksum"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BuildDeedChecksum" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BuildDeedChecksum" "Publisher" "Hardened Minds"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BuildDeedChecksum" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\builddeed-checksum.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\BuildDeed Checksum.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\BuildDeedChecksum"
SectionEnd
