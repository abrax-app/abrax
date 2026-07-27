import { useEffect, useState, useRef, type ReactNode } from "react";
import { toast, Toaster } from "sonner";
import { useTranslation } from "react-i18next";
import { platform } from "@tauri-apps/plugin-os";
import {
  checkAccessibilityPermission,
  checkMicrophonePermission,
} from "tauri-plugin-macos-permissions-api";
import "./App.css";
import AccessibilityPermissions from "./components/AccessibilityPermissions";
import AlertsBanner, { alertTitleKey } from "./components/AlertsBanner";
import Footer from "./components/footer";
import Onboarding, { AccessibilityOnboarding } from "./components/onboarding";
import { Sidebar, SidebarSection, SECTIONS_CONFIG } from "./components/Sidebar";
import { RetroShell } from "./components/retro/RetroShell";
import { QuietShell } from "./components/quiet/QuietShell";
import { BancadaShell } from "./components/bancada/BancadaShell";
import { WhatsNewGate } from "./components/whats-new";
import { useSettings } from "./hooks/useSettings";
import { useSettingsStore } from "./stores/settingsStore";
import { commands, events } from "@/bindings";
import { getLanguageDirection, initializeRTL } from "@/lib/utils/rtl";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { useOsType } from "./hooks/useOsType";

type OnboardingStep = "accessibility" | "model" | "done";

const renderSettingsContent = (section: SidebarSection) => {
  const ActiveComponent =
    SECTIONS_CONFIG[section]?.component || SECTIONS_CONFIG.general.component;
  return <ActiveComponent />;
};

function App() {
  const { t, i18n } = useTranslation();
  const [onboardingStep, setOnboardingStep] = useState<OnboardingStep | null>(
    null,
  );
  // Track if this is a returning user who just needs to grant permissions
  // (vs a new user who needs full onboarding including model selection)
  const [isReturningUser, setIsReturningUser] = useState(false);
  const [currentSection, setCurrentSection] =
    useState<SidebarSection>("general");
  const { settings, updateSetting } = useSettings();
  const osType = useOsType();
  const direction = getLanguageDirection(i18n.language);
  const refreshAudioDevices = useSettingsStore(
    (state) => state.refreshAudioDevices,
  );
  const refreshOutputDevices = useSettingsStore(
    (state) => state.refreshOutputDevices,
  );
  const hasCompletedPostOnboardingInit = useRef(false);

  useEffect(() => {
    checkOnboardingStatus();
  }, []);

  // Initialize RTL direction when language changes
  useEffect(() => {
    initializeRTL(i18n.language);
  }, [i18n.language]);

  // Initialize Enigo, shortcuts, and refresh audio devices when main app loads
  useEffect(() => {
    if (onboardingStep === "done" && !hasCompletedPostOnboardingInit.current) {
      hasCompletedPostOnboardingInit.current = true;
      Promise.all([
        commands.initializeEnigo(),
        commands.initializeShortcuts(),
      ]).catch((e) => {
        console.warn("Failed to initialize:", e);
      });
      refreshAudioDevices();
      refreshOutputDevices();
    }
  }, [onboardingStep, refreshAudioDevices, refreshOutputDevices]);

  // Handle keyboard shortcuts for debug mode toggle
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Check for Ctrl+Shift+D (Windows/Linux) or Cmd+Shift+D (macOS)
      const isDebugShortcut =
        event.shiftKey &&
        event.key.toLowerCase() === "d" &&
        (event.ctrlKey || event.metaKey);

      if (isDebugShortcut) {
        event.preventDefault();
        const currentDebugMode = settings?.debug_mode ?? false;
        updateSetting("debug_mode", !currentDebugMode);
      }
    };

    // Add event listener when component mounts
    document.addEventListener("keydown", handleKeyDown);

    // Cleanup event listener when component unmounts
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [settings?.debug_mode, updateSetting]);

  // Canal único de errores (F1): un solo listener convierte cada UserAlertEvent
  // en un toast localizado. La persistencia (centro de errores) vive en
  // alertsStore/AlertsBanner, y la notificación nativa con ventana oculta la
  // envía el backend — misma fuente, tres superficies.
  useEffect(() => {
    const unlisten = events.userAlertEvent.listen((event) => {
      const { kind, detail } = event.payload;
      const title = t(alertTitleKey(kind));
      if (kind === "recording_permission_denied") {
        const currentPlatform = platform();
        const description = t(`errors.micPermissionDenied.${currentPlatform}`, {
          defaultValue: t("errors.micPermissionDenied.generic"),
        });
        toast.error(title, { description });
      } else if (kind === "recording_no_device") {
        toast.error(title, { description: t("errors.noInputDevice") });
      } else if (kind === "recording_too_short") {
        // El usuario soltó la tecla antes de hablar. Se le dice qué teclas
        // mantener, con el atajo REAL que tenga configurado y ya formateado
        // para su plataforma — nada de culpar al micrófono.
        const atajo = formatKeyCombination(
          settings?.bindings?.transcribe?.current_binding ?? "",
          osType,
        );
        toast.error(title, {
          description: atajo
            ? t("errors.recordingTooShort", { atajo })
            : t("errors.recordingTooShortNoBinding"),
        });
      } else if (kind === "transcription_empty") {
        toast.error(title, { description: t("errors.transcriptionEmpty") });
      } else if (kind === "shortcut_registration") {
        toast.error(title, { description: t("errors.shortcutRegistration") });
      } else if (kind === "paste") {
        toast.error(title, { description: t("errors.pasteFailed") });
      } else {
        toast.error(title, { description: detail ?? undefined });
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t, settings?.bindings?.transcribe?.current_binding, osType]);

  // Memoria en el sitio: cuando ABRAX aprende de una corrección hecha donde
  // se dicta, se celebra con el mismo toast del aprendizaje por Historial y
  // se refresca el store (la escritura fue backend-side, sin evento de
  // settings) para que la sección Memoria muestre el par al instante.
  useEffect(() => {
    const unlisten = events.memoriaAprendida.listen((event) => {
      const pares = event.payload.pares;
      if (pares.length === 0) return;
      toast.success(
        t("settings.history.edit.learned", {
          pares: pares.map((p) => `«${p.de} → ${p.a}»`).join(", "),
        }),
      );
      void useSettingsStore.getState().refreshSettings();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);

  const revealMainWindowForPermissions = async () => {
    try {
      await commands.showMainWindowCommand();
    } catch (e) {
      console.warn("Failed to show main window for permission onboarding:", e);
    }
  };

  const checkOnboardingStatus = async () => {
    try {
      const settingsResult = await commands.getAppSettings();
      const hasCompletedOnboarding =
        settingsResult.status === "ok" &&
        settingsResult.data.onboarding_completed === true;
      const currentPlatform = platform();

      if (hasCompletedOnboarding) {
        // Returning user - check if they need to grant permissions first
        setIsReturningUser(true);

        if (currentPlatform === "macos") {
          try {
            const [hasAccessibility, hasMicrophone] = await Promise.all([
              checkAccessibilityPermission(),
              checkMicrophonePermission(),
            ]);
            if (!hasAccessibility || !hasMicrophone) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check macOS permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        if (currentPlatform === "windows") {
          try {
            const microphoneStatus =
              await commands.getWindowsMicrophonePermissionStatus();
            if (
              microphoneStatus.supported &&
              microphoneStatus.overall_access === "denied"
            ) {
              await revealMainWindowForPermissions();
              setOnboardingStep("accessibility");
              return;
            }
          } catch (e) {
            console.warn("Failed to check Windows microphone permissions:", e);
            // If we can't check, proceed to main app and let them fix it there
          }
        }

        setOnboardingStep("done");
      } else {
        // New user - start full onboarding
        setIsReturningUser(false);
        setOnboardingStep("accessibility");
      }
    } catch (error) {
      console.error("Failed to check onboarding status:", error);
      setOnboardingStep("accessibility");
    }
  };

  const handleAccessibilityComplete = () => {
    // Returning users already have models, skip to main app
    // New users need to select a model
    setOnboardingStep(isReturningUser ? "done" : "model");
  };

  const handleModelSelected = () => {
    // Transition to main app - user has started a download
    setOnboardingStep("done");
  };

  // Rendered once around every step below (including onboarding) so
  // toast.error() calls surface to the user. sonner renders via a portal, so
  // its position in the tree doesn't affect layout. Without this, errors during
  // onboarding (e.g. a model download failing because blob.handy.computer is
  // unreachable) are silently swallowed and the wizard just appears to "blink".
  const toaster = (
    <Toaster
      theme="system"
      toastOptions={{
        unstyled: true,
        classNames: {
          toast:
            "bg-background border border-mid-gray/20 rounded-lg shadow-lg px-4 py-3 flex items-center gap-3 text-sm",
          title: "font-medium",
          description: "text-mid-gray",
        },
      }}
    />
  );

  // Still checking onboarding status
  if (onboardingStep === null) {
    return null;
  }

  // Select the content for the current step. The Toaster is rendered once, in a
  // stable wrapper around this node, so crossing between onboarding steps and
  // the main app never remounts it (which would drop any in-flight toast).
  let content: ReactNode;
  if (onboardingStep === "accessibility") {
    content = (
      <AccessibilityOnboarding onComplete={handleAccessibilityComplete} />
    );
  } else if (onboardingStep === "model") {
    content = <Onboarding onModelSelected={handleModelSelected} />;
  } else if (settings?.ui_shell === "retro") {
    // Shell retro: ventanas apilables que hospedan las mismas secciones.
    content = (
      <div dir={direction}>
        <RetroShell />
      </div>
    );
  } else if (settings?.ui_shell === "quiet") {
    // Shell quiet: panel minimalista oscuro (mockup PROPUESTA 1).
    content = (
      <div dir={direction}>
        <QuietShell />
      </div>
    );
  } else if (settings?.ui_shell === "bancada") {
    // Shell bancada: consola de garaje / banco de pruebas (shell F1).
    content = (
      <div dir={direction}>
        <BancadaShell />
      </div>
    );
  } else {
    content = (
      <div
        dir={direction}
        className="h-screen flex flex-col select-none cursor-default"
      >
        <WhatsNewGate />
        {/* Main content area that takes remaining space */}
        <div className="flex-1 flex overflow-hidden">
          <Sidebar
            activeSection={currentSection}
            onSectionChange={setCurrentSection}
          />
          {/* Scrollable content area */}
          <div className="flex-1 flex flex-col overflow-hidden">
            <div className="flex-1 overflow-y-auto">
              <div className="flex flex-col items-center p-4 gap-4">
                <AlertsBanner />
                <AccessibilityPermissions />
                {renderSettingsContent(currentSection)}
              </div>
            </div>
          </div>
        </div>
        {/* Fixed footer at bottom */}
        <Footer />
      </div>
    );
  }

  return (
    <>
      {toaster}
      {content}
    </>
  );
}

export default App;
