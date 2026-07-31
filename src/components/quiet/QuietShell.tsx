import React, { useCallback, useEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Mic,
  FileText,
  Boxes,
  Settings,
  LayoutGrid,
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
import { commands, type HistoryEntry } from "@/bindings";
import { GeneralSettings, HistorySettings, ModelsSettings } from "../settings";
import { SECTIONS_CONFIG, type SidebarSection } from "../Sidebar";
import { ShellSelector } from "../settings/ShellSelector";
import { PaletteSelector } from "../settings/PaletteSelector";
import { AtajoVox } from "../AtajoVox";
import { Chip } from "../bancada/Chip";
import { AudioLines, Eraser, Languages, MonitorSpeaker } from "lucide-react";
import "./quiet.css";

// Skin "Quiet" — minimalista y discreta (mockup PROPUESTA 1): panel oscuro
// redondeado, sidebar limpia y un hero con la esfera + atajo + botón gradiente.
// Inspiración: Wispr Flow / ChatGPT / Spotify. La app "casi desaparece".

type QView = "escuchar" | "transcripciones" | "modelos" | "ajustes" | "mas";

const fmtHora = (secs: number): string => {
  try {
    // timestamp del historial está en SEGUNDos → a ms.
    return new Date(secs * 1000).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
};

// Sub-pestañas del ítem «Más». El orden importa: **Avanzado va primero y es la
// activa por defecto** porque ahí viven el Diccionario vivo y la Memoria de
// correcciones, que son los diferenciadores de ABRAX. Enterrarlos tras dos clics
// y un scroll haría que el shell bonito nos costara justo lo que nos hace únicos.
// Se hospedan las secciones REALES del registro compartido (`SECTIONS_CONFIG`),
// igual que hace el shell Bancada: cero UI duplicada.
const MAS_SECCIONES = [
  "advanced",
  "escucha",
  "about",
] as const satisfies readonly SidebarSection[];
type MasSeccion = (typeof MAS_SECCIONES)[number];

export const QuietShell: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const osType = useOsType();

  const [view, setView] = useState<QView>("escuchar");
  const [seccionMas, setSeccionMas] = useState<SidebarSection>("advanced");
  const [grabando, setGrabando] = useState(false);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
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

  // Transcripciones recientes (para el hero).
  const loadHistory = useCallback(async () => {
    try {
      const r = await commands.getHistoryEntries(null, 3);
      if (r.status === "ok") setEntries(r.data.entries);
    } catch {
      // ignorar
    }
  }, []);
  useEffect(() => {
    void loadHistory();
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    listen("history-update-payload", () => void loadHistory()).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadHistory]);

  const atajo = formatKeyCombination(
    settings?.bindings?.transcribe?.current_binding ?? "",
    osType,
  );
  // Secciones del ítem «Más». Se derivan del registro compartido con
  // `Object.entries` —igual que el shell Bancada— porque indexar
  // `SECTIONS_CONFIG` por una clave literal conserva el tipo estrecho de
  // `as const` y `enabled` queda declarado sin argumentos.
  const seccionesMas = Object.entries(SECTIONS_CONFIG)
    .filter(
      ([id, c]) =>
        MAS_SECCIONES.includes(id as MasSeccion) && c.enabled(settings),
    )
    .map(([id, c]) => ({ id: id as SidebarSection, ...c }));
  // Si la activa se deshabilita en caliente (p. ej. se apaga el modo
  // depuración), se cae a la primera disponible en vez de renderizar una
  // sección que ya no existe.
  const SeccionMasActiva = (
    seccionesMas.find((s) => s.id === seccionMas) ?? seccionesMas[0]
  )?.component;

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
    { id: "escuchar", icon: Mic, label: t("quiet.nav.listen") },
    {
      id: "transcripciones",
      icon: FileText,
      label: t("quiet.nav.transcriptions"),
    },
    { id: "modelos", icon: Boxes, label: t("quiet.nav.models") },
    { id: "ajustes", icon: Settings, label: t("quiet.nav.settings") },
    { id: "mas", icon: LayoutGrid, label: t("quiet.nav.more") },
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
            {view === "escuchar" ? (
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
                <AtajoVox />
                <div className="q-chips">
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
                  <Chip
                    icon={AudioLines}
                    label={t("bancada.dial.vad")}
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
                    active={(settings?.custom_filler_words ?? null) !== null}
                    onClick={() =>
                      updateSetting(
                        "custom_filler_words",
                        (settings?.custom_filler_words ?? null) !== null
                          ? []
                          : null,
                      )
                    }
                  />
                </div>
                <div className="q-recent">
                  <div className="q-recent-h">{t("quiet.recent")}</div>
                  <div className="q-recent-list">
                    {entries.length === 0 ? (
                      <div className="q-empty">{t("quiet.empty")}</div>
                    ) : (
                      entries.map((e) => (
                        <button
                          type="button"
                          key={e.id}
                          className="q-row"
                          onClick={() => setView("transcripciones")}
                          title={e.title || e.transcription_text}
                        >
                          <span className="q-row-t">
                            {fmtHora(e.timestamp)}
                          </span>
                          <span className="q-row-x">
                            {e.title || e.transcription_text}
                          </span>
                        </button>
                      ))
                    )}
                  </div>
                </div>
              </div>
            ) : view === "transcripciones" ? (
              <div className="q-sec">
                <HistorySettings />
              </div>
            ) : view === "modelos" ? (
              <div className="q-sec">
                <ModelsSettings />
              </div>
            ) : view === "mas" ? (
              <div className="q-sec">
                <div className="q-subtabs" role="tablist">
                  {seccionesMas.map((s) => (
                    <button
                      type="button"
                      key={s.id}
                      role="tab"
                      aria-selected={seccionMas === s.id}
                      className={`q-subtab${seccionMas === s.id ? " on" : ""}`}
                      onClick={() => setSeccionMas(s.id)}
                    >
                      {t(s.labelKey)}
                    </button>
                  ))}
                </div>
                <SeccionMasActiva />
              </div>
            ) : (
              <div className="q-sec q-ajustes">
                <ShellSelector descriptionMode="inline" />
                <PaletteSelector descriptionMode="inline" />
                <GeneralSettings />
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
