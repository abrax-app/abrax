import React, { useEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Mic,
  FileText,
  Keyboard,
  Radio,
  Volume2,
  Cog,
  Info,
  ShieldCheck,
  CloudOff,
  ArrowDownToLine,
  Minus,
  Square,
  X,
  type LucideIcon,
} from "lucide-react";
import AbraxLogo from "../icons/AbraxLogo";
import AlertsBanner from "../AlertsBanner";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { cerrarDesdeShell } from "@/lib/utils/ventana";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { commands } from "@/bindings";
import { HistorySettings, AboutSettings } from "../settings";
import { ShellSelector } from "../settings/ShellSelector";
import { PaletteSelector } from "../settings/PaletteSelector";
import { ShowOverlay } from "../settings/ShowOverlay";
import { AudioFeedback } from "../settings/AudioFeedback";
import { OutputDeviceSelector } from "../settings/OutputDeviceSelector";
import { VolumeSlider } from "../settings/VolumeSlider";
import { StartHidden } from "../settings/StartHidden";
import { AutostartToggle } from "../settings/AutostartToggle";
import { ShowTrayIcon } from "../settings/ShowTrayIcon";
import { ModelUnloadTimeoutSetting } from "../settings/ModelUnloadTimeout";
import { HistoryLimit } from "../settings/HistoryLimit";
import { RecordingRetentionPeriodSelector } from "../settings/RecordingRetentionPeriod";
import {
  PantallaEscucha,
  PantallaStreaming,
  PantallaVox,
} from "./PantallasModo";
import { SettingsGroup } from "../ui/SettingsGroup";
import { PanelAtajos } from "./PanelAtajos";
import "./quiet.css";

// Skin "Quiet" — minimalista y discreta (mockup PROPUESTA 1): panel oscuro
// redondeado, sidebar limpia y un hero con la esfera + atajo + botón gradiente.
// Inspiración: Wispr Flow / ChatGPT / Spotify. La app "casi desaparece".

// El menú se organiza POR MODO, y cada modo lleva sus modelos dentro (decisión
// de Winston, 31/07). Se acabaron la pantalla «Modelos» suelta y el cajón «Más»
// con pestañas: buscar un modelo era ir a un sitio distinto del que usaba ese
// modelo, y las tres pestañas de «Más» escondían a dos clics justo lo que nos
// diferencia.
//
//   Atajos          las teclas, y solo las teclas
//   Transcripciones el historial
//   Escucha         dictar: los atajos en pantalla + los modelos de dictado
//   Streaming       audio del sistema + los modelos que escriben en vivo
//   VOX             leer en voz alta + sus voces
//   Avanzado        todo lo demás, incluido el sonido que salió de Atajos
//   Acerca de       idioma, tema, paleta y créditos
type QView =
  | "atajos"
  | "transcripciones"
  | "escucha"
  | "streaming"
  | "vox"
  | "avanzado"
  | "acercade";

export const QuietShell: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting, audioFeedbackEnabled } = useSettings();
  const osType = useOsType();

  // Para la pantalla «Atajos» y el grupo de sonido que se mudo a «Avanzado»:
  // las mismas condiciones que usa el shell Clasico, para que las dos pantallas
  // oculten y deshabiliten exactamente lo mismo.
  const pushToTalk = settings?.push_to_talk ?? false;
  const esLinux = osType === "linux";

  const [view, setView] = useState<QView>("escucha");
  const [grabando, setGrabando] = useState(false);
  // Tras cambiar a Quiet en caliente la ventana conserva el marco nativo hasta
  // reiniciar; mientras tanto los botones −/□/× propios duplicarían los del
  // marco, así que se ocultan (queda solo el de bandeja, que el marco no tiene).
  const [conMarco, setConMarco] = useState(false);

  // Quiet no escala su contenido: tamaños fijos como el shell Clásico, y al
  // agrandar/maximizar la ventana solo se ve más espacio (contenido centrado
  // por max-width en quiet.css). El zoom proporcional se quitó a pedido.

  // La esfera del hero se retiro el 31/07 por decision de producto: ocupaba el
  // centro de la pantalla de inicio y no hacia nada que el usuario pudiera usar.
  // En su lugar van los controles que de verdad se tocan. El motor
  // (`esferaHome.ts`) queda en el repo: la esfera sigue viva en el overlay.

  // Estado de dictado real (sondeo ligero): esfera + etiqueta del botón.
  useEffect(() => {
    let vivo = true;
    let last = false;
    const tick = async () => {
      try {
        const rec = await commands.isRecording();
        if (vivo && rec !== last) {
          last = rec;
          setGrabando(rec);
        }
      } catch {
        // comando aún no listo
      }
    };
    const id = setInterval(tick, 200);
    void tick();
    return () => {
      vivo = false;
      clearInterval(id);
    };
  }, []);

  // El bloque «Transcripciones recientes» del hero se retiro el 31/07: la
  // pantalla de inicio muestra el atajo y los controles, y el historial completo
  // vive en su propia seccion. Con el se van su cargador y su listener, que
  // seguian pidiendo datos que ya no miraba nadie.

  const atajo = formatKeyCombination(
    settings?.bindings?.transcribe?.current_binding ?? "",
    osType,
  );
  const win = getCurrentWindow();

  useEffect(() => {
    let vivo = true;
    getCurrentWindow()
      .isDecorated()
      .then((v) => {
        if (vivo) setConMarco(v);
      })
      .catch(() => {});
    return () => {
      vivo = false;
    };
  }, [settings?.ui_shell]);

  const nav: {
    id: QView;
    icon: LucideIcon;
    label: string;
  }[] = [
    { id: "atajos", icon: Keyboard, label: t("quiet.nav.shortcuts") },
    {
      id: "transcripciones",
      icon: FileText,
      label: t("quiet.nav.transcriptions"),
    },
    { id: "escucha", icon: Mic, label: t("quiet.nav.listen") },
    { id: "streaming", icon: Radio, label: t("quiet.nav.streaming") },
    { id: "vox", icon: Volume2, label: t("sidebar.escucha") },
    { id: "avanzado", icon: Cog, label: t("sidebar.advanced") },
    { id: "acercade", icon: Info, label: t("sidebar.about") },
  ];

  return (
    <div id="quiet-stage" className="select-none">
      <div className="q-panel">
        {/* barra superior: zona de arrastre + controles de ventana */}
        <div className="q-topbar" data-tauri-drag-region>
          <div className="q-winctl">
            {settings?.show_tray_icon !== false && (
              <button
                type="button"
                aria-label={t("quiet.tray")}
                title={t("quiet.tray")}
                onClick={() => void win.hide()}
              >
                <ArrowDownToLine size={14} />
              </button>
            )}
            {!conMarco && (
              <>
                <button
                  type="button"
                  aria-label={t("quiet.min")}
                  title={t("quiet.min")}
                  onClick={() => void win.minimize()}
                >
                  <Minus size={15} />
                </button>
                <button
                  type="button"
                  aria-label={t("quiet.max")}
                  title={t("quiet.max")}
                  onClick={() => void win.toggleMaximize().catch(() => {})}
                >
                  <Square size={11} />
                </button>
                <button
                  type="button"
                  className="q-close"
                  aria-label={t("quiet.close")}
                  title={t("quiet.close")}
                  onClick={() => cerrarDesdeShell(win)}
                >
                  <X size={15} />
                </button>
              </>
            )}
          </div>
        </div>

        <div className="q-body">
          <aside className="q-side">
            <div className="q-brand">
              <AbraxLogo variant="horizontal" width={112} />
            </div>
            <nav className="q-nav">
              {nav.map((n) => {
                const Icon = n.icon;
                return (
                  <button
                    type="button"
                    key={n.id}
                    className={`q-navit${view === n.id ? " on" : ""}`}
                    onClick={() => setView(n.id)}
                  >
                    <Icon size={18} />
                    <span>{n.label}</span>
                  </button>
                );
              })}
            </nav>
            {/* Centro de errores. Vivía solo en el shell clásico, así que al
                pasar Quiet a ser el default los fallos ocurridos con la ventana
                oculta se habrían quedado sin superficie donde verse: el toast se
                pierde y el registro no se mostraba en ningún sitio.
                Va en la sidebar, entre el menú y los sellos: arriba del
                contenido empujaba la pantalla entera hacia abajo cada vez que
                llegaba un aviso, y las tarjetas cambiaban de sitio bajo el
                cursor. Aquí se ve igual desde cualquier sección y no mueve nada. */}
            <div className="q-side-avisos">
              <AlertsBanner compacto />
            </div>
            <div className="q-foot">
              <div className="q-badge">
                <ShieldCheck size={14} />
                <span>{t("quiet.local")}</span>
              </div>
              <div className="q-badge">
                <CloudOff size={14} />
                <span>{t("quiet.noCloud")}</span>
              </div>
            </div>
          </aside>

          <main className="q-main">
            {view === "escucha" ? (
              // Sin hero propio: la cabecera de la pantalla ya dice el modo,
              // que hace y como se dispara. Hubo aqui una esfera, luego cuatro
              // chips de ajustes y luego tres tarjetas con los modos — y esas
              // ultimas eran un SEGUNDO menu al lado del de la izquierda, que
              // ya lleva a las mismas tres pantallas.
              <div className="q-sec">
                <PantallaEscucha grabando={grabando} />
              </div>
            ) : view === "transcripciones" ? (
              <div className="q-sec">
                <HistorySettings />
              </div>
            ) : view === "atajos" ? (
              // Cuatro tarjetas —Escucha, Streaming, VOX y General— a lo ancho
              // de la ventana: el acceso rapido a lo de cada dia. El detalle
              // completo de cada modo vive en su pantalla de la barra lateral.
              <div className="q-sec q-sec-ancha">
                <PanelAtajos />
              </div>
            ) : view === "streaming" ? (
              <div className="q-sec">
                <PantallaStreaming />
              </div>
            ) : view === "vox" ? (
              <div className="q-sec">
                <PantallaVox />
              </div>
            ) : view === "avanzado" ? (
              // Lo que NO es de ningun modo. Los grupos se componen aqui en vez
              // de usar `AdvancedSettings`: ese lo comparten los otros tres
              // shells, y quitarle los grupos que se mudaron a las pantallas de
              // modo los habria dejado sin ellos.
              <div className="q-sec q-ajustes">
                <SettingsGroup title={t("quiet.modo.grupo.apariencia")}>
                  <ShellSelector descriptionMode="tooltip" grouped={true} />
                  <PaletteSelector descriptionMode="tooltip" grouped={true} />
                  <ShowOverlay descriptionMode="tooltip" grouped={true} />
                </SettingsGroup>
                {/* Los pitidos de inicio y fin, y por donde suenan. El
                    dispositivo de salida aparece TAMBIEN en VOX porque manda en
                    los dos sitios; es el mismo componente, no una copia. */}
                <SettingsGroup title={t("settings.sound.title")}>
                  <AudioFeedback descriptionMode="tooltip" grouped={true} />
                  <OutputDeviceSelector
                    descriptionMode="tooltip"
                    grouped={true}
                    disabled={!audioFeedbackEnabled}
                  />
                  <VolumeSlider disabled={!audioFeedbackEnabled} />
                </SettingsGroup>
                <SettingsGroup title={t("settings.advanced.groups.app")}>
                  <StartHidden descriptionMode="tooltip" grouped={true} />
                  <AutostartToggle descriptionMode="tooltip" grouped={true} />
                  <ShowTrayIcon descriptionMode="tooltip" grouped={true} />
                  <ModelUnloadTimeoutSetting
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                </SettingsGroup>
                <SettingsGroup title={t("settings.advanced.groups.history")}>
                  <HistoryLimit descriptionMode="tooltip" grouped={true} />
                  <RecordingRetentionPeriodSelector
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                </SettingsGroup>
              </div>
            ) : (
              <div className="q-sec q-ajustes">
                {/* La paleta ya no se repite aqui: vive en «Avanzado →
                    Apariencia», y `AboutSettings` —que es comun a los cuatro
                    shells— trae la suya. Estaban las dos en la MISMA pantalla,
                    una encima de la otra. */}
                <AboutSettings />
              </div>
            )}
          </main>
        </div>
      </div>
    </div>
  );
};

QuietShell.displayName = "QuietShell";

export default QuietShell;
