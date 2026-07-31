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
import {
  HistorySettings,
  ModelsSettings,
  AdvancedSettings,
  AboutSettings,
  EscuchaSettings,
} from "../settings";
import { ShellSelector } from "../settings/ShellSelector";
import { PaletteSelector } from "../settings/PaletteSelector";
import { ShortcutInput } from "../settings/ShortcutInput";
import { PushToTalk } from "../settings/PushToTalk";
import { MicrophoneSelector } from "../settings/MicrophoneSelector";
import { PruebaMicrofono } from "../settings/PruebaMicrofono";
import { MuteWhileRecording } from "../settings/MuteWhileRecording";
import { AudioFeedback } from "../settings/AudioFeedback";
import { OutputDeviceSelector } from "../settings/OutputDeviceSelector";
import { VolumeSlider } from "../settings/VolumeSlider";
import { CaptureSystemAudio } from "../settings/CaptureSystemAudio";
import { ModelSettingsCard } from "../settings/general/ModelSettingsCard";
import { SettingsGroup } from "../ui/SettingsGroup";
import { AtajoVox } from "../AtajoVox";
import { PanelAtajos } from "./PanelAtajos";
import { Chip } from "../bancada/Chip";
import { AudioLines, Eraser, Languages, MonitorSpeaker } from "lucide-react";
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

  // Mismo criterio que Bancada y Retro, palabra por palabra: `null` NO es
  // «apagado», es «lista por defecto del idioma» —o sea, el filtro ACTIVO—
  // y solo una lista vacía lo apaga (`audio_toolkit/text.rs`, doc de
  // `filter_transcription_output`). El chip de aquí tenía las dos ramas
  // cambiadas: pintaba apagado lo que está encendido y, saliendo de `null`,
  // volvía a escribir `null` — no se podía mover.
  const cfw = settings?.custom_filler_words;
  const fillerOn = cfw == null || cfw.length > 0;

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
            {/* Centro de errores. Vivía solo en el shell clásico, así que al
                pasar Quiet a ser el default los fallos ocurridos con la ventana
                oculta se habrían quedado sin superficie donde verse: el toast se
                pierde y el registro no se mostraba en ningún sitio. */}
            <AlertsBanner />
            {view === "escucha" ? (
              <div className="q-hero">
                <h1 className="q-h1">{t("quiet.ready")}</h1>
                <p className={`q-hint${grabando ? " on" : ""}`}>
                  {/* Al dispararse el atajo, el texto confirma que FUNCIONÓ. Es
                      la única forma de que un fallo del hook de teclado se vea
                      en el momento en que el usuario está mirando, y de paso
                      enseña el gesto de mantener sin que nadie lea nada. */}
                  {grabando ? (
                    t("quiet.dictando")
                  ) : atajo ? (
                    <Trans
                      i18nKey="quiet.holdHint"
                      values={{ atajo }}
                      components={{ k: <span className="q-kbd" /> }}
                    />
                  ) : (
                    t("quiet.holdHintNoBinding")
                  )}
                </p>
                {/* Los mismos controles que el tablero de Karting, aqui.
                    Antes este sitio lo ocupaba una esfera decorativa: bonita,
                    pero en la pantalla de INICIO —lo primero que ve un juez— el
                    espacio central tiene que servir para algo. Son los cuatro
                    ajustes que de verdad se tocan al dictar, y son los MISMOS
                    componentes que usa Karting, no una copia. */}
                <AtajoVox className="q-hint" kbdClassName="q-kbd" />
                <div className="q-chips bnc-chips">
                  <Chip
                    icon={MonitorSpeaker}
                    label={t("bancada.systemAudio")}
                    active={!!settings?.capture_system_audio}
                    onClick={() =>
                      updateSetting(
                        "capture_system_audio",
                        !settings?.capture_system_audio,
                      )
                    }
                  />
                  {/* «VAD» es jerga inglesa y esta es la PRIMERA pantalla. Se
                      reusa el rotulo que ya existe en Ajustes y esta traducido
                      en los 22 locales, en vez de crear una clave nueva.
                      Bancada y Retro conservan «VAD» en sus diales porque ahi
                      el hueco es de tres letras. */}
                  <Chip
                    icon={AudioLines}
                    label={t("settings.advanced.voiceActivityDetection.title")}
                    active={!!settings?.vad_enabled}
                    onClick={() =>
                      updateSetting("vad_enabled", !settings?.vad_enabled)
                    }
                  />
                  <Chip
                    icon={Languages}
                    label={t("bancada.btn.translate")}
                    active={!!settings?.translate_to_english}
                    onClick={() =>
                      updateSetting(
                        "translate_to_english",
                        !settings?.translate_to_english,
                      )
                    }
                  />
                  <Chip
                    icon={Eraser}
                    label={t("bancada.btn.fillers")}
                    active={fillerOn}
                    onClick={() =>
                      updateSetting("custom_filler_words", fillerOn ? [] : null)
                    }
                  />
                </div>

                {/* Los modelos de dictado viven AQUI, dentro del modo que los
                    usa: buscar un modelo ya no es ir a otra pantalla. La
                    cabecera la pone el hero de arriba, asi que la lista entra
                    sin la suya. */}
                <div className="q-hero-modelos">
                  <ModelSettingsCard />
                  <ModelsSettings capacidad="dictado" sinCabecera />
                </div>
              </div>
            ) : view === "transcripciones" ? (
              <div className="q-sec">
                <HistorySettings />
              </div>
            ) : view === "atajos" ? (
              // Cuatro tarjetas —Escucha, Streaming, VOX y General— a lo ancho
              // de la ventana. Los atajos de cancelar y pulsar-para-hablar
              // siguen en «Avanzado»: aqui va lo que se toca a diario.
              <div className="q-sec q-sec-ancha">
                <PanelAtajos grabando={grabando} />
              </div>
            ) : view === "streaming" ? (
              <div className="q-sec q-ajustes">
                <SettingsGroup title={t("quiet.nav.streaming")}>
                  <CaptureSystemAudio descriptionMode="tooltip" grouped />
                </SettingsGroup>
                {/* Los modelos que de verdad escriben mientras hablas. Es una
                    lista corta a proposito: hoy solo Nemotron declara
                    `supports_streaming`. */}
                <ModelsSettings capacidad="streaming" sinCabecera />
              </div>
            ) : view === "vox" ? (
              <div className="q-sec">
                <EscuchaSettings />
              </div>
            ) : view === "avanzado" ? (
              <div className="q-sec q-ajustes">
                <ShellSelector descriptionMode="inline" />
                {/* Los dos ajustes de teclado que NO son «la tecla de un modo»:
                    el gesto de mantener y el atajo de cancelar. Vivian en la
                    pantalla «Atajos» y bajan aqui al convertirse esa en el panel
                    de los cuatro modos — este es su unico sitio en Quiet, asi
                    que sacarlos sin recolocarlos los habria dejado sin ninguno. */}
                <SettingsGroup title={t("quiet.nav.shortcuts")}>
                  <PushToTalk descriptionMode="tooltip" grouped={true} />
                  {/* El de cancelar se oculta con pulsar-para-hablar (soltar ya
                      cancela) y en Linux (los atajos dinamicos son inestables),
                      igual que en el shell Clasico. */}
                  {!esLinux && !pushToTalk && (
                    <ShortcutInput shortcutId="cancel" grouped={true} />
                  )}
                </SettingsGroup>
                <SettingsGroup title={t("settings.sound.title")}>
                  <MicrophoneSelector
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <PruebaMicrofono descriptionMode="tooltip" grouped={true} />
                  <MuteWhileRecording
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <AudioFeedback descriptionMode="tooltip" grouped={true} />
                  <OutputDeviceSelector
                    descriptionMode="tooltip"
                    grouped={true}
                    disabled={!audioFeedbackEnabled}
                  />
                  <VolumeSlider disabled={!audioFeedbackEnabled} />
                </SettingsGroup>
                <AdvancedSettings />
              </div>
            ) : (
              <div className="q-sec q-ajustes">
                <PaletteSelector descriptionMode="inline" />
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
