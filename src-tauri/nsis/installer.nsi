; Custom NSIS template for Handy with portable mode support.
; Based on tauri-apps/tauri@tauri-v2.9.1 crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi
; Portable changes are marked with "; --- PORTABLE MODE ---" comments.
;
; When upgrading Tauri, diff this file against the new upstream template and
; merge changes while preserving the portable sections.

Unicode true
ManifestDPIAware true
; Add in `dpiAwareness` `PerMonitorV2` to manifest for Windows 10 1607+ (note this should not affect lower versions since they should be able to ignore this and pick up `dpiAware` `true` set by `ManifestDPIAware true`)
; Currently undocumented on NSIS's website but is in the Docs folder of source tree, see
; https://github.com/kichik/nsis/blob/5fc0b87b819a9eec006df4967d08e522ddd651c9/Docs/src/attributes.but#L286-L300
; https://github.com/tauri-apps/tauri/pull/10106
ManifestDPIAwareness PerMonitorV2

!if "{{compression}}" == "none"
  SetCompress off
!else
  ; Set the compression algorithm. We default to LZMA.
  SetCompressor /SOLID "{{compression}}"
!endif

!include MUI2.nsh
!include FileFunc.nsh
!include x64.nsh
!include WordFunc.nsh
!include "utils.nsh"
!include "FileAssociation.nsh"
!include "Win\COM.nsh"
!include "Win\Propkey.nsh"
!include "StrFunc.nsh"
${StrCase}
${StrLoc}

{{#if installer_hooks}}
!include "{{installer_hooks}}"
{{/if}}

!define WEBVIEW2APPGUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

!define MANUFACTURER "{{manufacturer}}"
!define PRODUCTNAME "{{product_name}}"
!define VERSION "{{version}}"
!define VERSIONWITHBUILD "{{version_with_build}}"
!define HOMEPAGE "{{homepage}}"
!define INSTALLMODE "{{install_mode}}"
!define LICENSE "{{license}}"
!define INSTALLERICON "{{installer_icon}}"
!define SIDEBARIMAGE "{{sidebar_image}}"
!define HEADERIMAGE "{{header_image}}"
!define MAINBINARYNAME "{{main_binary_name}}"
!define MAINBINARYSRCPATH "{{main_binary_path}}"
!define BUNDLEID "{{bundle_id}}"
!define COPYRIGHT "{{copyright}}"
!define OUTFILE "{{out_file}}"
!define ARCH "{{arch}}"
!define ADDITIONALPLUGINSPATH "{{additional_plugins_path}}"
!define ALLOWDOWNGRADES "{{allow_downgrades}}"
!define DISPLAYLANGUAGESELECTOR "{{display_language_selector}}"
!define INSTALLWEBVIEW2MODE "{{install_webview2_mode}}"
!define WEBVIEW2INSTALLERARGS "{{webview2_installer_args}}"
!define WEBVIEW2BOOTSTRAPPERPATH "{{webview2_bootstrapper_path}}"
!define WEBVIEW2INSTALLERPATH "{{webview2_installer_path}}"
!define MINIMUMWEBVIEW2VERSION "{{minimum_webview2_version}}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUKEY "Software\${MANUFACTURER}"
!define MANUPRODUCTKEY "${MANUKEY}\${PRODUCTNAME}"
!define UNINSTALLERSIGNCOMMAND "{{uninstaller_sign_cmd}}"
!define ESTIMATEDSIZE "{{estimated_size}}"
!define STARTMENUFOLDER "{{start_menu_folder}}"

Var PassiveMode
Var UpdateMode
Var NoShortcutMode
Var WixMode
Var OldMainBinaryName

; --- PORTABLE MODE ---
Var PortableMode

Name "${PRODUCTNAME}"
BrandingText "${COPYRIGHT}"
OutFile "${OUTFILE}"

; We don't actually use this value as default install path,
; it's just for nsis to append the product name folder in the directory selector
; https://nsis.sourceforge.io/Reference/InstallDir
!define PLACEHOLDER_INSTALL_DIR "placeholder\${PRODUCTNAME}"
InstallDir "${PLACEHOLDER_INSTALL_DIR}"

VIProductVersion "${VERSIONWITHBUILD}"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME}"
VIAddVersionKey "LegalCopyright" "${COPYRIGHT}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

# additional plugins
!addplugindir "${ADDITIONALPLUGINSPATH}"

; Uninstaller signing command
!if "${UNINSTALLERSIGNCOMMAND}" != ""
  !uninstfinalize '${UNINSTALLERSIGNCOMMAND}'
!endif

; Handle install mode, `perUser`, `perMachine` or `both`
!if "${INSTALLMODE}" == "perMachine"
  RequestExecutionLevel admin
!endif

!if "${INSTALLMODE}" == "currentUser"
  RequestExecutionLevel user
!endif

!if "${INSTALLMODE}" == "both"
  !define MULTIUSER_MUI
  !define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCTNAME}"
  !define MULTIUSER_INSTALLMODE_COMMANDLINE
  !if "${ARCH}" == "x64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !else if "${ARCH}" == "arm64"
    !define MULTIUSER_USE_PROGRAMFILES64
  !endif
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_KEY "${UNINSTKEY}"
  !define MULTIUSER_INSTALLMODE_DEFAULT_REGISTRY_VALUENAME "CurrentUser"
  !define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
  !define MULTIUSER_INSTALLMODE_FUNCTION RestorePreviousInstallLocation
  !define MULTIUSER_EXECUTIONLEVEL Highest
  !include MultiUser.nsh
!endif

; Installer icon
!if "${INSTALLERICON}" != ""
  !define MUI_ICON "${INSTALLERICON}"
!endif

; Installer sidebar image
!if "${SIDEBARIMAGE}" != ""
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBARIMAGE}"
!endif

; Installer header image
!if "${HEADERIMAGE}" != ""
  !define MUI_HEADERIMAGE
  !define MUI_HEADERIMAGE_BITMAP  "${HEADERIMAGE}"
!endif

; Define registry key to store installer language
!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

; Installer pages, must be ordered as they appear
; 1. Welcome Page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_WELCOME

; 2. License Page (if defined)
!if "${LICENSE}" != ""
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MUI_PAGE_LICENSE "${LICENSE}"
!endif

; 3. Install mode (if it is set to `both`)
!if "${INSTALLMODE}" == "both"
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
  !insertmacro MULTIUSER_PAGE_INSTALLMODE
!endif

; --- PORTABLE MODE --- 4. Install type selection page (Normal vs Portable)
Var InstallTypeRadioNormal
Var InstallTypeRadioPortable

; PÁGINA RETIRADA DEL FLUJO NORMAL (28/07). El modo portátil sigue existiendo,
; pero ya no se pregunta en la instalación interactiva:
;
;   - Llegaba justo DESPUÉS del aviso de SmartScreen (el binario no está
;     firmado). Tras ese susto, «carpeta independiente, sin tocar el registro»
;     es la opción que suena prudente… y es la peor: deja al usuario sin acceso
;     directo en el menú Inicio, sin desinstalador y sin actualizaciones.
;   - Pedía una decisión técnica («registro», «portátil») antes de haber visto
;     la app, y para casi todo el mundo solo hay una respuesta correcta.
;
; Sigue disponible por línea de comandos: `Abrax_..._x64-setup.exe /PORTABLE`,
; que `.onInit` procesa y que ya fija el directorio de destino por su cuenta.
; Las dos funciones se conservan: volver a mostrar la página es descomentar
; esta línea.
;Page custom PageInstallType PageLeaveInstallType

Function PageInstallType
  ; Skip for passive/silent/update modes — portable flag is handled via /PORTABLE
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
  ${OrIf} $UpdateMode = 1
    Abort
  ${EndIf}

  ; Las cadenas van por $(...) igual que las de Tauri: esta página es NUESTRA y
  ; con el texto a mano se quedaba en inglés dentro de un instalador que por lo
  ; demás ya salía traducido. Las tablas están al final, junto a los idiomas.
  !insertmacro MUI_HEADER_TEXT "$(installTypeTitle)" "$(installTypeSubtitle)"

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 24u "$(installTypeIntro)"
  Pop $0

  ${NSD_CreateRadioButton} 30u 35u -30u 12u "$(installTypeNormal)"
  Pop $InstallTypeRadioNormal

  ${NSD_CreateLabel} 44u 49u -44u 20u "$(installTypeNormalDesc)"
  Pop $0

  ${NSD_CreateRadioButton} 30u 75u -30u 12u "$(installTypePortable)"
  Pop $InstallTypeRadioPortable

  ${NSD_CreateLabel} 44u 89u -44u 20u "$(installTypePortableDesc)"
  Pop $0

  ; Pre-select based on current state
  ${If} $PortableMode = 1
    ${NSD_Check} $InstallTypeRadioPortable
  ${Else}
    ${NSD_Check} $InstallTypeRadioNormal
  ${EndIf}

  nsDialogs::Show
FunctionEnd

Function PageLeaveInstallType
  ${NSD_GetState} $InstallTypeRadioPortable $0
  ${If} $0 = ${BST_CHECKED}
    StrCpy $PortableMode 1
    ; --- PORTABLE MODE --- Switch default directory to Desktop\Handy for portable
    ${If} $INSTDIR == "${PLACEHOLDER_INSTALL_DIR}"
    ${OrIf} $INSTDIR == "$LOCALAPPDATA\${PRODUCTNAME}"
      StrCpy $INSTDIR "$DESKTOP\${PRODUCTNAME}"
    ${EndIf}
  ${Else}
    StrCpy $PortableMode 0
    ; Restore normal default if user switched back from portable
    ${If} $INSTDIR == "$DESKTOP\${PRODUCTNAME}"
      StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
    ${EndIf}
  ${EndIf}
FunctionEnd
; --- END PORTABLE MODE ---

; 5. (was 4) Custom page to ask user if he wants to reinstall/uninstall
;    only if a previous installation was detected
Var ReinstallPageCheck
Page custom PageReinstall PageLeaveReinstall
Function PageReinstall
  ; --- PORTABLE MODE --- Skip reinstall page for portable installs
  ${If} $PortableMode = 1
    Abort
  ${EndIf}

  ; Uninstall previous WiX installation if exists.
  ;
  ; A WiX installer stores the installation info in registry
  ; using a UUID and so we have to loop through all keys under
  ; `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`
  ; and check if `DisplayName` and `Publisher` keys match ${PRODUCTNAME} and ${MANUFACTURER}
  ;
  ; This has a potential issue that there maybe another installation that matches
  ; our ${PRODUCTNAME} and ${MANUFACTURER} but wasn't installed by our WiX installer,
  ; however, this should be fine since the user will have to confirm the uninstallation
  ; and they can chose to abort it if doesn't make sense.
  StrCpy $0 0
  wix_loop:
    EnumRegKey $1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall" $0
    StrCmp $1 "" wix_loop_done ; Exit loop if there is no more keys to loop on
    IntOp $0 $0 + 1
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "DisplayName"
    ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "Publisher"
    StrCmp "$R0$R1" "${PRODUCTNAME}${MANUFACTURER}" 0 wix_loop
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1" "UninstallString"
    ${StrCase} $R1 $R0 "L"
    ${StrLoc} $R0 $R1 "msiexec" ">"
    StrCmp $R0 0 0 wix_loop_done
    StrCpy $WixMode 1
    StrCpy $R6 "SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\$1"
    Goto compare_version
  wix_loop_done:

  ; Check if there is an existing installation, if not, abort the reinstall page
  ReadRegStr $R0 SHCTX "${UNINSTKEY}" ""
  ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
  ${IfThen} "$R0$R1" == "" ${|} Abort ${|}

  ; Compare this installar version with the existing installation
  ; and modify the messages presented to the user accordingly
  compare_version:
  StrCpy $R4 "$(older)"
  ${If} $WixMode = 1
    ReadRegStr $R0 HKLM "$R6" "DisplayVersion"
  ${Else}
    ReadRegStr $R0 SHCTX "${UNINSTKEY}" "DisplayVersion"
  ${EndIf}
  ${IfThen} $R0 == "" ${|} StrCpy $R4 "$(unknown)" ${|}

  nsis_tauri_utils::SemverCompare "${VERSION}" $R0
  Pop $R0
  ; Reinstalling the same version
  ${If} $R0 = 0
    StrCpy $R1 "$(alreadyInstalledLong)"
    StrCpy $R2 "$(addOrReinstall)"
    StrCpy $R3 "$(uninstallApp)"
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(chooseMaintenanceOption)"
  ; Upgrading
  ${ElseIf} $R0 = 1
    StrCpy $R1 "$(olderOrUnknownVersionInstalled)"
    StrCpy $R2 "$(uninstallBeforeInstalling)"
    StrCpy $R3 "$(dontUninstall)"
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(choowHowToInstall)"
  ; Downgrading
  ${ElseIf} $R0 = -1
    StrCpy $R1 "$(newerVersionInstalled)"
    StrCpy $R2 "$(uninstallBeforeInstalling)"
    !if "${ALLOWDOWNGRADES}" == "true"
      StrCpy $R3 "$(dontUninstall)"
    !else
      StrCpy $R3 "$(dontUninstallDowngrade)"
    !endif
    !insertmacro MUI_HEADER_TEXT "$(alreadyInstalled)" "$(choowHowToInstall)"
  ${Else}
    Abort
  ${EndIf}

  ; Skip showing the page if passive
  ;
  ; Note that we don't call this earlier at the begining
  ; of this function because we need to populate some variables
  ; related to current installed version if detected and whether
  ; we are downgrading or not.
  ${If} $PassiveMode = 1
    Call PageLeaveReinstall
  ${Else}
    nsDialogs::Create 1018
    Pop $R4
    ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

    ${NSD_CreateLabel} 0 0 100% 24u $R1
    Pop $R1

    ${NSD_CreateRadioButton} 30u 50u -30u 8u $R2
    Pop $R2
    ${NSD_OnClick} $R2 PageReinstallUpdateSelection

    ${NSD_CreateRadioButton} 30u 70u -30u 8u $R3
    Pop $R3
    ; Disable this radio button if downgrading and downgrades are disabled
    !if "${ALLOWDOWNGRADES}" == "false"
      ${IfThen} $R0 = -1 ${|} EnableWindow $R3 0 ${|}
    !endif
    ${NSD_OnClick} $R3 PageReinstallUpdateSelection

    ; Check the first radio button if this the first time
    ; we enter this page or if the second button wasn't
    ; selected the last time we were on this page
    ${If} $ReinstallPageCheck <> 2
      SendMessage $R2 ${BM_SETCHECK} ${BST_CHECKED} 0
    ${Else}
      SendMessage $R3 ${BM_SETCHECK} ${BST_CHECKED} 0
    ${EndIf}

    ${NSD_SetFocus} $R2
    nsDialogs::Show
  ${EndIf}
FunctionEnd
Function PageReinstallUpdateSelection
  ${NSD_GetState} $R2 $R1
  ${If} $R1 == ${BST_CHECKED}
    StrCpy $ReinstallPageCheck 1
  ${Else}
    StrCpy $ReinstallPageCheck 2
  ${EndIf}
FunctionEnd
Function PageLeaveReinstall
  ${NSD_GetState} $R2 $R1

  ; If migrating from Wix, always uninstall
  ${If} $WixMode = 1
    Goto reinst_uninstall
  ${EndIf}

  ; In update mode, always proceeds without uninstalling
  ${If} $UpdateMode = 1
    Goto reinst_done
  ${EndIf}

  ; $R0 holds whether same(0)/upgrading(1)/downgrading(-1) version
  ; $R1 holds the radio buttons state:
  ;   1 => first choice was selected
  ;   0 => second choice was selected
  ${If} $R0 = 0 ; Same version, proceed
    ${If} $R1 = 1              ; User chose to add/reinstall
      Goto reinst_done
    ${Else}                    ; User chose to uninstall
      Goto reinst_uninstall
    ${EndIf}
  ${ElseIf} $R0 = 1 ; Upgrading
    ${If} $R1 = 1              ; User chose to uninstall
      Goto reinst_uninstall
    ${Else}
      Goto reinst_done         ; User chose NOT to uninstall
    ${EndIf}
  ${ElseIf} $R0 = -1 ; Downgrading
    ${If} $R1 = 1              ; User chose to uninstall
      Goto reinst_uninstall
    ${Else}
      Goto reinst_done         ; User chose NOT to uninstall
    ${EndIf}
  ${EndIf}

  reinst_uninstall:
    HideWindow
    ClearErrors

    ${If} $WixMode = 1
      ReadRegStr $R1 HKLM "$R6" "UninstallString"
      ExecWait '$R1' $0
    ${Else}
      ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
      ReadRegStr $R1 SHCTX "${UNINSTKEY}" "UninstallString"
      ${IfThen} $UpdateMode = 1 ${|} StrCpy $R1 "$R1 /UPDATE" ${|} ; append /UPDATE
      ${IfThen} $PassiveMode = 1 ${|} StrCpy $R1 "$R1 /P" ${|} ; append /P
      StrCpy $R1 "$R1 _?=$4" ; append uninstall directory
      ExecWait '$R1' $0
    ${EndIf}

    BringToFront

    ${IfThen} ${Errors} ${|} StrCpy $0 2 ${|} ; ExecWait failed, set fake exit code

    ${If} $0 <> 0
    ${OrIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ; User cancelled wix uninstaller? return to select un/reinstall page
      ${If} $WixMode = 1
      ${AndIf} $0 = 1602
        Abort
      ${EndIf}

      ; User cancelled NSIS uninstaller? return to select un/reinstall page
      ${If} $0 = 1
        Abort
      ${EndIf}

      ; Other erros? show generic error message and return to select un/reinstall page
      MessageBox MB_ICONEXCLAMATION "$(unableToUninstall)"
      Abort
    ${EndIf}
  reinst_done:
FunctionEnd

; 5. Choose install directory page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_DIRECTORY

; 6. Start menu shortcut page
Var AppStartMenuFolder
!if "${STARTMENUFOLDER}" != ""
  ; --- PORTABLE MODE --- Also skip start menu page for portable installs
  !define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassiveOrPortable
  !define MUI_STARTMENUPAGE_DEFAULTFOLDER "${STARTMENUFOLDER}"
!else
  !define MUI_PAGE_CUSTOMFUNCTION_PRE Skip
!endif
!insertmacro MUI_PAGE_STARTMENU Application $AppStartMenuFolder

; 7. Installation page
!insertmacro MUI_PAGE_INSTFILES

; 8. Finish page
;
; Don't auto jump to finish page after installation page,
; because the installation page has useful info that can be used debug any issues with the installer.
!define MUI_FINISHPAGE_NOAUTOCLOSE
; Use show readme button in the finish page as a button create a desktop shortcut
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "$(createDesktop)"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateOrUpdateDesktopShortcut
; Show run app after installation.
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_FUNCTION RunMainBinary
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH

Function RunMainBinary
  nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" ""
FunctionEnd

; Uninstaller Pages
; 1. Confirm uninstall page
Var DeleteAppDataCheckbox
Var DeleteAppDataCheckboxState
; --- MODELOS --- Caché de Hugging Face y peso real de lo que se borraría.
Var HfHubDir
Var TamanoDatos
!define /ifndef WS_EX_LAYOUTRTL         0x00400000

; Dónde viven de verdad los pesos: transcribe-cpp los descarga a la caché
; estándar de Hugging Face, NO al datadir de la app. Respeta HF_HOME si está.
Function un.RutaCacheHF
  ReadEnvStr $HfHubDir "HF_HOME"
  ${If} $HfHubDir == ""
    StrCpy $HfHubDir "$PROFILE\.cache\huggingface\hub"
  ${Else}
    StrCpy $HfHubDir "$HfHubDir\hub"
  ${EndIf}
FunctionEnd

; Peso REAL de lo que se borraría, medido aquí y ahora. Un número fijo mentiría:
; quien solo bajó el modelo de arranque tiene ~200 MB y quien bajó los cinco,
; varios GB. Si algo falla, `$TamanoDatos` queda vacío y la casilla se muestra
; sin cifra — nunca con una inventada.
Function un.MedirDatos
  StrCpy $TamanoDatos ""
  StrCpy $R0 0
  ; ¿La medida es fiable? `GetSize` usa enteros de 32 bits con signo y devuelve
  ; CADENA VACÍA en cuanto una carpeta pasa de 2 GB (verificado: 1,5 GB mide
  ; bien, 2,5 GB devuelve vacío, y cambiar a /S=0M no lo salva). Sumar ese vacío
  ; daría un total confiadamente equivocado, que es peor que no dar ninguno: a
  ; la primera medida imposible se abandona la cifra y la casilla sale sin ella.
  StrCpy $R6 1
  ${If} ${FileExists} "$APPDATA\${BUNDLEID}\*.*"
    ${un.GetSize} "$APPDATA\${BUNDLEID}" "/S=0K" $R1 $R2 $R3
    ${If} $R1 == ""
      StrCpy $R6 0
    ${Else}
      IntOp $R0 $R0 + $R1
    ${EndIf}
  ${EndIf}
  ${If} ${FileExists} "$LOCALAPPDATA\${BUNDLEID}\*.*"
    ${un.GetSize} "$LOCALAPPDATA\${BUNDLEID}" "/S=0K" $R1 $R2 $R3
    ${If} $R1 == ""
      StrCpy $R6 0
    ${Else}
      IntOp $R0 $R0 + $R1
    ${EndIf}
  ${EndIf}
  Call un.RutaCacheHF
  ${If} ${FileExists} "$HfHubDir\*.*"
    FindFirst $R4 $R5 "$HfHubDir\models--handy-computer--*"
    ${DoWhile} $R5 != ""
      ${un.GetSize} "$HfHubDir\$R5" "/S=0K" $R1 $R2 $R3
      ${If} $R1 == ""
        StrCpy $R6 0
      ${Else}
        IntOp $R0 $R0 + $R1
      ${EndIf}
      FindNext $R4 $R5
    ${Loop}
    FindClose $R4
  ${EndIf}
  ${If} $R6 = 0
    StrCpy $TamanoDatos ""
  ${ElseIf} $R0 > 1048576
    IntOp $R1 $R0 / 1048576
    IntOp $R2 $R0 % 1048576
    IntOp $R2 $R2 * 10
    IntOp $R2 $R2 / 1048576
    StrCpy $TamanoDatos "$R1$(separadorDecimal)$R2 GB"
  ${ElseIf} $R0 > 1024
    IntOp $R1 $R0 / 1024
    StrCpy $TamanoDatos "$R1 MB"
  ${EndIf}
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ConfirmShow
Function un.ConfirmShow ; Add add a `Delete app data` check box
  ; $1 inner dialog HWND
  ; $2 window DPI
  ; $3 style
  ; $4 x
  ; $5 y
  ; $6 width
  ; $7 height
  FindWindow $1 "#32770" "" $HWNDPARENT ; Find inner dialog
  System::Call "user32::GetDpiForWindow(p r1) i .r2"
  ${If} $(^RTL) = 1
    StrCpy $3 "${__NSD_CheckBox_EXSTYLE} | ${WS_EX_LAYOUTRTL}"
    IntOp $4 50 * $2
  ${Else}
    StrCpy $3 "${__NSD_CheckBox_EXSTYLE}"
    IntOp $4 0 * $2
  ${EndIf}
  IntOp $5 100 * $2
  IntOp $6 400 * $2
  IntOp $7 25 * $2
  IntOp $4 $4 / 96
  IntOp $5 $5 / 96
  IntOp $6 $6 / 96
  IntOp $7 $7 / 96
  ; La casilla nombra AMBAS cosas —datos y modelos— y su peso real. Antes decía
  ; solo «datos de la aplicación» y dejaba los pesos, que son lo que ocupa.
  ; Va SIN marcar (no hay BM_SETCHECK): borrar es decisión del usuario.
  Call un.MedirDatos
  StrCpy $9 "$(deleteAppDataAndModels)"
  ${If} $TamanoDatos != ""
    StrCpy $9 "$9 ($(aproximadamente) $TamanoDatos)"
  ${EndIf}
  System::Call 'user32::CreateWindowEx(i r3, w "${__NSD_CheckBox_CLASS}", w "$9", i ${__NSD_CheckBox_STYLE}, i r4, i r5, i r6, i r7, p r1, i0, i0, i0) i .s'
  Pop $DeleteAppDataCheckbox
  SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $1
  SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $1 1
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE un.ConfirmLeave
Function un.ConfirmLeave
  SendMessage $DeleteAppDataCheckbox ${BM_GETCHECK} 0 0 $DeleteAppDataCheckboxState
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_PRE un.SkipIfPassive
!insertmacro MUI_UNPAGE_CONFIRM

; 2. Uninstalling Page
!insertmacro MUI_UNPAGE_INSTFILES

;Languages
{{#each languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}
!insertmacro MUI_RESERVEFILE_LANGDLL
{{#each language_files}}
  !include "{{this}}"
{{/each}}

; ─── Cadenas propias de la página «Tipo de instalación» ──────────────────────
; Esa página la añadimos nosotros (Normal vs Portable) y llevaba el texto
; escrito a mano en inglés: el instalador salía en español —título, Atrás,
; Siguiente, Cancelar— y esa única pantalla, no.
;
; Van DESPUÉS de los `MUI_LANGUAGE` porque `${LANG_*}` solo existe una vez que
; el idioma se ha insertado. NSIS exige la cadena en TODAS las tablas
; compiladas: si algún día se añade un idioma a `bundle.windows.nsis.languages`
; en `tauri.conf.json`, el build FALLA hasta que se traduzca aquí. Es
; deliberado — un idioma a medias es peor que un fallo ruidoso.
!ifdef LANG_SPANISH
  LangString installTypeTitle       ${LANG_SPANISH} "Tipo de instalación"
  LangString installTypeSubtitle    ${LANG_SPANISH} "Elige cómo quieres instalar ${PRODUCTNAME}."
  LangString installTypeIntro       ${LANG_SPANISH} "Elige entre una instalación normal o una instalación portátil."
  LangString installTypeNormal      ${LANG_SPANISH} "Instalación normal (recomendada)"
  LangString installTypeNormalDesc  ${LANG_SPANISH} "Se instala en el equipo, con acceso directo en el menú Inicio, desinstalador y actualizaciones automáticas."
  LangString installTypePortable    ${LANG_SPANISH} "Instalación portátil"
  LangString installTypePortableDesc ${LANG_SPANISH} "Una carpeta independiente, sin tocar el registro ni crear accesos directos ni desinstalador. Tus datos se guardan junto a la app."
  LangString installTypePortableDone ${LANG_SPANISH} "Modo portátil: se crearon el archivo marcador y la carpeta Data."
  LangString deleteAppDataAndModels ${LANG_SPANISH} "Eliminar también mis datos y los modelos descargados"
  LangString aproximadamente        ${LANG_SPANISH} "aprox."
  LangString separadorDecimal       ${LANG_SPANISH} ","
  LangString deleteModelsDone       ${LANG_SPANISH} "Modelos descargados eliminados de la caché."
  ; ─── Se REDEFINEN cadenas de Tauri: gana la última, verificado con makensis
  ; (avisa «set multiple times» y usa esta). El texto original decía solo
  ; «Pulse Aceptar para cerrarlo» y CALLABA qué hace Cancelar, que aborta la
  ; instalación o la desinstalación entera sin decir nada. Y como ABRAX se va a
  ; la BANDEJA al pulsar la X, el usuario está convencido de haberla cerrado:
  ; pasó el 29/07 y costó una desinstalación que «no funcionaba». Se nombra la
  ; bandeja y se explican los DOS botones.
  LangString appRunningOkKill ${LANG_SPANISH} "{{product_name}} sigue abierto.$\n$\nRecuerda que al cerrar la ventana queda en la bandeja, junto al reloj.$\n$\nAceptar: lo cierra y continúa.$\nCancelar: no cambia nada y esto se detiene."
  LangString appRunning       ${LANG_SPANISH} "{{product_name}} sigue abierto y hay que cerrarlo para continuar. Si cerraste la ventana, míralo en la bandeja junto al reloj: clic derecho en su icono y «Salir». Después vuelve a intentarlo."
  LangString failedToKillApp  ${LANG_SPANISH} "No se pudo cerrar {{product_name}}. Ciérralo desde la bandeja —clic derecho en su icono junto al reloj y «Salir»— y vuelve a intentarlo."
!endif

!ifdef LANG_ENGLISH
  LangString installTypeTitle       ${LANG_ENGLISH} "Choose Install Type"
  LangString installTypeSubtitle    ${LANG_ENGLISH} "Select how you want to install ${PRODUCTNAME}."
  LangString installTypeIntro       ${LANG_ENGLISH} "Choose whether to perform a normal installation or a portable installation."
  LangString installTypeNormal      ${LANG_ENGLISH} "Normal Installation (recommended)"
  LangString installTypeNormalDesc  ${LANG_ENGLISH} "Installs to your system with Start Menu shortcuts, uninstaller, and auto-update support."
  LangString installTypePortable    ${LANG_ENGLISH} "Portable Installation"
  LangString installTypePortableDesc ${LANG_ENGLISH} "Self-contained folder with no registry changes, shortcuts, or uninstaller. Data stored next to the app."
  LangString installTypePortableDone ${LANG_ENGLISH} "Portable mode: created marker file and Data directory."
  LangString deleteAppDataAndModels ${LANG_ENGLISH} "Also delete my data and the downloaded models"
  LangString aproximadamente        ${LANG_ENGLISH} "approx."
  LangString separadorDecimal       ${LANG_ENGLISH} "."
  LangString deleteModelsDone       ${LANG_ENGLISH} "Downloaded models removed from the cache."
  ; Ver la nota de la tabla en español: se redefinen a propósito.
  LangString appRunningOkKill ${LANG_ENGLISH} "{{product_name}} is still running.$\n$\nNote that closing its window leaves it in the tray, next to the clock.$\n$\nOK: closes it and continues.$\nCancel: changes nothing and stops here."
  LangString appRunning       ${LANG_ENGLISH} "{{product_name}} is still running and must be closed to continue. If you closed its window, look for it in the tray next to the clock: right-click its icon and choose Quit. Then try again."
  LangString failedToKillApp  ${LANG_ENGLISH} "{{product_name}} could not be closed. Close it from the tray — right-click its icon next to the clock and choose Quit — then try again."
!endif

Function .onInit
  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/NS" $NoShortcutMode
  ${IfNot} ${Errors}
    StrCpy $NoShortcutMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  ; --- PORTABLE MODE --- Parse /PORTABLE flag for silent/passive installs
  ${GetOptions} $CMDLINE "/PORTABLE" $PortableMode
  ${IfNot} ${Errors}
    StrCpy $PortableMode 1
  ${EndIf}

  !if "${DISPLAYLANGUAGESELECTOR}" == "true"
    !insertmacro MUI_LANGDLL_DISPLAY
  !endif

  !insertmacro SetContext

  ${If} $INSTDIR == "${PLACEHOLDER_INSTALL_DIR}"
    ; Set default install location
    !if "${INSTALLMODE}" == "perMachine"
      ${If} ${RunningX64}
        !if "${ARCH}" == "x64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else if "${ARCH}" == "arm64"
          StrCpy $INSTDIR "$PROGRAMFILES64\${PRODUCTNAME}"
        !else
          StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
        !endif
      ${Else}
        StrCpy $INSTDIR "$PROGRAMFILES\${PRODUCTNAME}"
      ${EndIf}
    !else if "${INSTALLMODE}" == "currentUser"
      StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
    !endif

    ; --- PORTABLE MODE --- Override default dir for silent/passive portable installs
    ${If} $PortableMode = 1
      StrCpy $INSTDIR "$DESKTOP\${PRODUCTNAME}"
    ${Else}
      Call RestorePreviousInstallLocation
    ${EndIf}
  ${EndIf}


  ; --- PORTABLE MODE --- Auto-detect portable mode during updates.
  ; Preserve portable installs that use either the current magic-string marker
  ; or the legacy empty marker created by older Handy releases. Require Data/
  ; for the legacy empty-marker case so stale scoop side-effect files do not
  ; accidentally opt an updater run into portable mode.
  ${If} $PortableMode <> 1
  ${AndIf} $UpdateMode = 1
  ${AndIf} ${FileExists} "$INSTDIR\portable"
    FileOpen $1 "$INSTDIR\portable" r
    FileRead $1 $2
    FileClose $1
    ${If} $2 == "Abrax Portable Mode"
      StrCpy $PortableMode 1
    ${OrIf} $2 == ""
    ${AndIf} ${FileExists} "$INSTDIR\Data"
      StrCpy $PortableMode 1
    ${EndIf}
  ${EndIf}

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_INIT
  !endif
FunctionEnd


Section EarlyChecks
  ; Abort silent installer if downgrades is disabled
  !if "${ALLOWDOWNGRADES}" == "false"
  ${If} ${Silent}
    ; If downgrading
    ${If} $R0 = -1
      System::Call 'kernel32::AttachConsole(i -1)i.r0'
      ${If} $0 <> 0
        System::Call 'kernel32::GetStdHandle(i -11)i.r0'
        System::call 'kernel32::SetConsoleTextAttribute(i r0, i 0x0004)' ; set red color
        FileWrite $0 "$(silentDowngrades)"
      ${EndIf}
      Abort
    ${EndIf}
  ${EndIf}
  !endif

SectionEnd

Section WebView2
  ; Check if Webview2 is already installed and skip this section
  ${If} ${RunningX64}
    ReadRegStr $4 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${Else}
    ReadRegStr $4 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}
  ${If} $4 == ""
    ReadRegStr $4 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${WEBVIEW2APPGUID}" "pv"
  ${EndIf}

  ${If} $4 == ""
    ; Webview2 installation
    ;
    ; Skip if updating
    ${If} $UpdateMode <> 1
      !if "${INSTALLWEBVIEW2MODE}" == "downloadBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        DetailPrint "$(webview2Downloading)"
        NSISdl::download "https://go.microsoft.com/fwlink/p/?LinkId=2124703" "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Pop $0
        ${If} $0 == "success"
          DetailPrint "$(webview2DownloadSuccess)"
        ${Else}
          DetailPrint "$(webview2DownloadError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "embedBootstrapper"
        Delete "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebview2Setup.exe" "${WEBVIEW2BOOTSTRAPPERPATH}"
        DetailPrint "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebview2Setup.exe"
        Goto install_webview2
      !endif

      !if "${INSTALLWEBVIEW2MODE}" == "offlineInstaller"
        Delete "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        File "/oname=$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe" "${WEBVIEW2INSTALLERPATH}"
        DetailPrint "$(installingWebview2)"
        StrCpy $6 "$TEMP\MicrosoftEdgeWebView2RuntimeInstaller.exe"
        Goto install_webview2
      !endif

      Goto webview2_done

      install_webview2:
        DetailPrint "$(installingWebview2)"
        ; $6 holds the path to the webview2 installer
        ExecWait "$6 ${WEBVIEW2INSTALLERARGS} /install" $1
        ${If} $1 = 0
          DetailPrint "$(webview2InstallSuccess)"
        ${Else}
          DetailPrint "$(webview2InstallError)"
          Abort "$(webview2AbortError)"
        ${EndIf}
      webview2_done:
    ${EndIf}
  ${Else}
    !if "${MINIMUMWEBVIEW2VERSION}" != ""
      ${VersionCompare} "${MINIMUMWEBVIEW2VERSION}" "$4" $R0
      ${If} $R0 = 1
        update_webview:
          DetailPrint "$(installingWebview2)"
          ${If} ${RunningX64}
            ReadRegStr $R1 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate" "path"
          ${Else}
            ReadRegStr $R1 HKLM "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 == ""
            ReadRegStr $R1 HKCU "SOFTWARE\Microsoft\EdgeUpdate" "path"
          ${EndIf}
          ${If} $R1 != ""
            ; Chromium updater docs: https://source.chromium.org/chromium/chromium/src/+/main:docs/updater/user_manual.md
            ; Modified from "HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Microsoft EdgeWebView\ModifyPath"
            ExecWait `"$R1" /install appguid=${WEBVIEW2APPGUID}&needsadmin=true` $1
            ${If} $1 = 0
              DetailPrint "$(webview2InstallSuccess)"
            ${Else}
              MessageBox MB_ICONEXCLAMATION|MB_ABORTRETRYIGNORE "$(webview2InstallError)" IDIGNORE ignore IDRETRY update_webview
              Quit
              ignore:
            ${EndIf}
          ${EndIf}
      ${EndIf}
    !endif
  ${EndIf}
SectionEnd

Section Install
  SetOutPath $INSTDIR

  !ifmacrodef NSIS_HOOK_PREINSTALL
    !insertmacro NSIS_HOOK_PREINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  ; Copy main executable
  File "${MAINBINARYSRCPATH}"

  ; Copy resources
  {{#each resources_dirs}}
    CreateDirectory "$INSTDIR\\{{this}}"
  {{/each}}
  {{#each resources}}
    File /a "/oname={{this.[1]}}" "{{no-escape @key}}"
  {{/each}}

  ; Copy external binaries
  {{#each binaries}}
    File /a "/oname={{this}}" "{{no-escape @key}}"
  {{/each}}

  ; --- PORTABLE MODE --- Skip file associations and deep links for portable installs
  ${If} $PortableMode <> 1
    ; Create file associations
    {{#each file_associations as |association| ~}}
      {{#each association.ext as |ext| ~}}
         !insertmacro APP_ASSOCIATE "{{ext}}" "{{or association.name ext}}" "{{association-description association.description ext}}" "$INSTDIR\${MAINBINARYNAME}.exe,0" "Open with ${PRODUCTNAME}" "$INSTDIR\${MAINBINARYNAME}.exe $\"%1$\""
      {{/each}}
    {{/each}}

    ; Register deep links
    {{#each deep_link_protocols as |protocol| ~}}
      WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "URL Protocol" ""
      WriteRegStr SHCTX "Software\Classes\\{{protocol}}" "" "URL:${BUNDLEID} protocol"
      WriteRegStr SHCTX "Software\Classes\\{{protocol}}\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
      WriteRegStr SHCTX "Software\Classes\\{{protocol}}\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
    {{/each}}
  ${EndIf}

  ; --- PORTABLE MODE --- Create portable marker and Data directory
  ${If} $PortableMode = 1
    FileOpen $0 "$INSTDIR\portable" w
    FileWrite $0 "Abrax Portable Mode"
    FileClose $0
    CreateDirectory "$INSTDIR\Data"
    DetailPrint "$(installTypePortableDone)"
  ${EndIf}

  ; --- PORTABLE MODE --- Skip uninstaller, registry, and shortcuts for portable installs
  ${If} $PortableMode <> 1
    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Save $INSTDIR in registry for future installations
    WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR

    !if "${INSTALLMODE}" == "both"
      ; Save install mode to be selected by default for the next installation such as updating
      ; or when uninstalling
      WriteRegStr SHCTX "${UNINSTKEY}" $MultiUser.InstallMode 1
    !endif

    ; Remove old main binary if it doesn't match new main binary name
    ReadRegStr $OldMainBinaryName SHCTX "${UNINSTKEY}" "MainBinaryName"
    ${If} $OldMainBinaryName != ""
    ${AndIf} $OldMainBinaryName != "${MAINBINARYNAME}.exe"
      Delete "$INSTDIR\$OldMainBinaryName"
    ${EndIf}

    ; Save current MAINBINARYNAME for future updates
    WriteRegStr SHCTX "${UNINSTKEY}" "MainBinaryName" "${MAINBINARYNAME}.exe"

    ; Registry information for add/remove programs
    WriteRegStr SHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
    WriteRegStr SHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""
    WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr SHCTX "${UNINSTKEY}" "Publisher" "${MANUFACTURER}"
    WriteRegStr SHCTX "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
    WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
    WriteRegDWORD SHCTX "${UNINSTKEY}" "NoModify" "1"
    WriteRegDWORD SHCTX "${UNINSTKEY}" "NoRepair" "1"

    ${GetSize} "$INSTDIR" "/M=uninstall.exe /S=0K /G=0" $0 $1 $2
    IntOp $0 $0 + ${ESTIMATEDSIZE}
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD SHCTX "${UNINSTKEY}" "EstimatedSize" "$0"

    !if "${HOMEPAGE}" != ""
      WriteRegStr SHCTX "${UNINSTKEY}" "URLInfoAbout" "${HOMEPAGE}"
      WriteRegStr SHCTX "${UNINSTKEY}" "URLUpdateInfo" "${HOMEPAGE}"
      WriteRegStr SHCTX "${UNINSTKEY}" "HelpLink" "${HOMEPAGE}"
    !endif

    ; Create start menu shortcut
    !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
      Call CreateOrUpdateStartMenuShortcut
    !insertmacro MUI_STARTMENU_WRITE_END

    ; Create desktop shortcut for silent and passive installers
    ; because finish page will be skipped
    ${If} $PassiveMode = 1
    ${OrIf} ${Silent}
      Call CreateOrUpdateDesktopShortcut
    ${EndIf}
  ${EndIf} ; --- END PORTABLE MODE guard ---

  !ifmacrodef NSIS_HOOK_POSTINSTALL
    !insertmacro NSIS_HOOK_POSTINSTALL
  !endif

  ; Auto close this page for passive mode
  ${If} $PassiveMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function .onInstSuccess
  ; Check for `/R` flag only in silent and passive installers because
  ; GUI installer has a toggle for the user to (re)start the app
  ${If} $PassiveMode = 1
  ${OrIf} ${Silent}
    ${GetOptions} $CMDLINE "/R" $R0
    ${IfNot} ${Errors}
      ${GetOptions} $CMDLINE "/ARGS" $R0
      nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}.exe" "$R0"
    ${EndIf}
  ${EndIf}
FunctionEnd

Function un.onInit
  !insertmacro SetContext

  !if "${INSTALLMODE}" == "both"
    !insertmacro MULTIUSER_UNINIT
  !endif

  !insertmacro MUI_UNGETLANGUAGE

  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}
FunctionEnd

Section Uninstall

  !ifmacrodef NSIS_HOOK_PREUNINSTALL
    !insertmacro NSIS_HOOK_PREUNINSTALL
  !endif

  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"

  ; Delete the app directory and its content from disk
  ; Copy main executable
  Delete "$INSTDIR\${MAINBINARYNAME}.exe"

  ; Delete resources
  {{#each resources}}
    Delete "$INSTDIR\\{{this.[1]}}"
  {{/each}}

  ; Delete external binaries
  {{#each binaries}}
    Delete "$INSTDIR\\{{this}}"
  {{/each}}

  ; Delete app associations
  {{#each file_associations as |association| ~}}
    {{#each association.ext as |ext| ~}}
      !insertmacro APP_UNASSOCIATE "{{ext}}" "{{or association.name ext}}"
    {{/each}}
  {{/each}}

  ; Delete deep links
  {{#each deep_link_protocols as |protocol| ~}}
    ReadRegStr $R7 SHCTX "Software\Classes\\{{protocol}}\shell\open\command" ""
    ${If} $R7 == "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
      DeleteRegKey SHCTX "Software\Classes\\{{protocol}}"
    ${EndIf}
  {{/each}}


  ; Delete uninstaller
  Delete "$INSTDIR\uninstall.exe"

  {{#each resources_ancestors}}
  RMDir /REBOOTOK "$INSTDIR\\{{this}}"
  {{/each}}
  RMDir "$INSTDIR"

  ; Remove shortcuts if not updating
  ${If} $UpdateMode <> 1
    !insertmacro DeleteAppUserModelId

    ; Remove start menu shortcut
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
      RMDir "$SMPROGRAMS\$AppStartMenuFolder"
    ${EndIf}
    !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
    ${EndIf}

    ; Remove desktop shortcuts
    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$DESKTOP\${PRODUCTNAME}.lnk"
      Delete "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}

  ; Remove registry information for add/remove programs
  !if "${INSTALLMODE}" == "both"
    DeleteRegKey SHCTX "${UNINSTKEY}"
  !else if "${INSTALLMODE}" == "perMachine"
    DeleteRegKey HKLM "${UNINSTKEY}"
  !else
    DeleteRegKey HKCU "${UNINSTKEY}"
  !endif

  ; Removes the Autostart entry for ${PRODUCTNAME} from the HKCU Run key if it exists.
  ; This ensures the program does not launch automatically after uninstallation if it exists.
  ; If it doesn't exist, it does nothing.
  ; We do this when not updating (to preserve the registry value on updates)
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  ${EndIf}

  ; Delete app data if the checkbox is selected
  ; and if not updating
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ; Clear the install location $INSTDIR from registry
    DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty SHCTX "${MANUKEY}"

    ; Clear the install language from registry
    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
    DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${MANUKEY}"

    SetShellVarContext current
    RmDir /r "$APPDATA\${BUNDLEID}"
    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"

    ; --- MODELOS --- Los pesos NO están en el datadir: transcribe-cpp los baja
    ; a la caché de Hugging Face. Sin esto, «eliminar mis datos» dejaba varios
    ; GB escondidos en una carpeta que el usuario jamás encontraría.
    ;
    ; Se borran SOLO los repos de NUESTRA organización (`models--handy-computer--*`).
    ; Esa caché es COMPARTIDA: cualquier otra app o script del usuario guarda
    ; ahí lo suyo, y borrarla entera destruiría datos ajenos.
    Call un.RutaCacheHF
    ${If} ${FileExists} "$HfHubDir\*.*"
      FindFirst $R0 $R1 "$HfHubDir\models--handy-computer--*"
      ${DoWhile} $R1 != ""
        RMDir /r "$HfHubDir\$R1"
        FindNext $R0 $R1
      ${Loop}
      FindClose $R0
      ; Y los cerrojos de esos mismos repos, que van en su propia carpeta.
      FindFirst $R0 $R1 "$HfHubDir\.locks\models--handy-computer--*"
      ${DoWhile} $R1 != ""
        RMDir /r "$HfHubDir\.locks\$R1"
        FindNext $R0 $R1
      ${Loop}
      FindClose $R0
      ; Sin /r: solo desaparecen si quedaron VACÍAS. Si el usuario tiene
      ; modelos de otras herramientas, su caché sigue intacta.
      RMDir "$HfHubDir\.locks"
      RMDir "$HfHubDir"
      DetailPrint "$(deleteModelsDone)"
    ${EndIf}
  ${EndIf}

  !ifmacrodef NSIS_HOOK_POSTUNINSTALL
    !insertmacro NSIS_HOOK_POSTUNINSTALL
  !endif

  ; Auto close if passive mode or updating
  ${If} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function RestorePreviousInstallLocation
  ReadRegStr $4 SHCTX "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" +2 0
    StrCpy $INSTDIR $4
FunctionEnd

Function Skip
  Abort
FunctionEnd

Function SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd

; --- PORTABLE MODE ---
Function SkipIfPassiveOrPortable
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
  ${IfThen} $PortableMode = 1  ${|} Abort ${|}
FunctionEnd
Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1  ${|} Abort ${|}
FunctionEnd

Function CreateOrUpdateStartMenuShortcut
  ; We used to use product name as MAINBINARYNAME
  ; migrate old shortcuts to target the new MAINBINARYNAME
  StrCpy $R0 0

  !insertmacro IsShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    StrCpy $R0 1
  ${EndIf}

  ${If} $R0 = 1
    Return
  ${EndIf}

  ; Skip creating shortcut if in update mode or no shortcut mode
  ; but always create if migrating from wix
  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  !if "${STARTMENUFOLDER}" != ""
    CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
  !else
    CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
  !endif
FunctionEnd

Function CreateOrUpdateDesktopShortcut
  ; We used to use product name as MAINBINARYNAME
  ; migrate old shortcuts to target the new MAINBINARYNAME
  !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\$OldMainBinaryName"
  Pop $0
  ${If} $0 = 1
    !insertmacro SetShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Return
  ${EndIf}

  ; Skip creating shortcut if in update mode or no shortcut mode
  ; but always create if migrating from wix
  ${If} $WixMode = 0
    ${If} $UpdateMode = 1
    ${OrIf} $NoShortcutMode = 1
      Return
    ${EndIf}
  ${EndIf}

  CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
  !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
FunctionEnd
