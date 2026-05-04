;============================================
;  CC Desktop Switch - NSIS installer script
;============================================
;  Prerequisites:
;     1. Install NSIS 3.0+
;     2. Run from repository root: makensis /DROOT_DIR=%CD% windows\installer.nsi
;     3. Output: CC-Desktop-Switch-Setup-1.1.0.exe
;============================================

!ifndef ROOT_DIR
  !define ROOT_DIR "${__FILEDIR__}\.."
!endif

!define PRODUCT_NAME "CC Desktop Switch"
!define PRODUCT_VERSION "1.1.0"
!define PRODUCT_PUBLISHER "CC Desktop Switch"
!define PRODUCT_DIR "$LOCALAPPDATA\Programs\CC-Desktop-Switch"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "CC-Desktop-Switch-Setup-${PRODUCT_VERSION}.exe"
InstallDir "${PRODUCT_DIR}"
InstallDirRegKey HKCU "${PRODUCT_UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define MUI_ABORTWARNING

!ifdef ICON_FILE
  !define MUI_ICON "${ICON_FILE}"
  !define MUI_UNICON "${ICON_FILE}"
!endif

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${ROOT_DIR}\LICENSE.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\CC-Desktop-Switch.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${PRODUCT_NAME}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Function .onInit
    ReadRegStr $R1 HKCU "${PRODUCT_UNINST_KEY}" "InstallLocation"
    ReadRegStr $R0 HKCU "${PRODUCT_UNINST_KEY}" "UninstallString"
    ${If} $R1 == ""
    ${AndIf} $R0 != ""
        ${GetParent} $R0 $R1
    ${EndIf}
    ${If} $R1 != ""
        StrCpy $INSTDIR $R1
    ${EndIf}

    Call CloseRunningApp

    ${If} $R0 != ""
        DetailPrint "Existing version detected. The installer will uninstall it first."
        ExecWait '"$R0" /S'
    ${EndIf}
FunctionEnd

Function CloseRunningApp
    DetailPrint "Closing running ${PRODUCT_NAME} process if needed..."
    nsExec::ExecToStack 'taskkill /IM "CC-Desktop-Switch.exe" /T /F'
    Pop $0
    Pop $1
FunctionEnd

Section "Main" SEC01
    SetOutPath "$INSTDIR"
    SetOverwrite ifnewer

    File /r "${ROOT_DIR}\dist\CC-Desktop-Switch\*.*"

    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\CC-Desktop-Switch.exe"
    CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\uninst.exe"

    CreateShortCut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\CC-Desktop-Switch.exe"

    WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "UninstallString" "$INSTDIR\uninst.exe"
    WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\CC-Desktop-Switch.exe"
    WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
    WriteRegStr HKCU "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
    WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "NoRepair" 1

    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKCU "${PRODUCT_UNINST_KEY}" "EstimatedSize" "$0"

    WriteUninstaller "$INSTDIR\uninst.exe"
SectionEnd

Section "Uninstall"
    Call un.CloseRunningApp
    Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
    RMDir /r "$SMPROGRAMS\${PRODUCT_NAME}"
    RMDir /r "$INSTDIR"
    DeleteRegKey HKCU "${PRODUCT_UNINST_KEY}"
SectionEnd

Function un.CloseRunningApp
    nsExec::ExecToStack 'taskkill /IM "CC-Desktop-Switch.exe" /T /F'
    Pop $0
    Pop $1
FunctionEnd
