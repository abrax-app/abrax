import React, { useCallback, useEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Mic,
  FileText,
  Boxes,
  Settings,
  ShieldCheck,
  CloudOff,
  ArrowDownToLine,
  Minus,
  Square,
  X,
  type LucideIcon,
} from "lucide-react";
import AbraxLogo from "../icons/AbraxLogo";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { cerrarDesdeShell } from "@/lib/utils/ventana";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { commands, type HistoryEntry } from "@/bindings";
import { GeneralSettings, HistorySettings, ModelsSettings } from "../settings";
import { ShellSelector } from "../settings/ShellSelector";
import { PaletteSelector } from "../settings/PaletteSelector";
import { montarEsfera, type EsferaHandle } from "./esferaHome";
import "./quiet.css";

// Skin "Quiet" — minimalista y discreta (mockup PROPUESTA 1): panel oscuro
// redondeado, sidebar limpia y un hero con la esfera + atajo + botón gradiente.
// Inspiración: Wispr Flow / ChatGPT / Spotify. La app "casi desaparece".

type QView = "escuchar" | "transcripciones" | "modelos" | "ajustes";

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

export const QuietShell: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();

  const [view, setView] = useState<QView>("escuchar");
  const [grabando, setGrabando] = useState(false);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  // Tras cambiar a Quiet en caliente la ventana conserva el marco nativo hasta
  // reiniciar; mientras tanto los botones −/□/× propios duplicarían los del
  // marco, así que se ocultan (queda solo el de bandeja, que el marco no tiene).
  const [conMarco, setConMarco] = useState(false);

  // Quiet no escala su contenido: tamaños fijos como el shell Clásico, y al
  // agrandar/maximizar la ventana solo se ve más espacio (contenido centrado
  // por max-width en quiet.css). El zoom proporcional se quitó a pedido.

  // ── esfera del hero (motor canvas 2D que respira; reacciona al dictado) ──
  const cvRef = useRef<HTMLCanvasElement>(null);
  const esf = useRef<EsferaHandle | null>(null);
  useEffect(() => {
    if (view !== "escuchar" || !cvRef.current) return;
    esf.current = montarEsfera(cvRef.current);
    esf.current.setGrabando(grabando);
    return () => {
      esf.current?.destroy();
      esf.current = null;
    };
    // Se re-monta al volver a "escuchar"; grabando se sincroniza por su efecto.
  }, [view]);

  // Re-teñir la esfera al cambiar de paleta/tema.
  useEffect(() => {
    const root = document.documentElement;
    const obs = new MutationObserver(() => esf.current?.setPalette());
    obs.observe(root, {
      attributes: true,
      attributeFilter: ["data-ui-theme", "data-theme"],
    });
    return () => obs.disconnect();
  }, []);

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
          esf.current?.setGrabando(rec);
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
            {view === "escuchar" ? (
              <div className="q-hero">
                <h1 className="q-h1">{t("quiet.ready")}</h1>
                <p className="q-hint">
                  {atajo ? (
                    <Trans
                      i18nKey="quiet.holdHint"
                      values={{ atajo }}
                      components={{ k: <span className="q-kbd" /> }}
                    />
                  ) : (
                    t("quiet.holdHintNoBinding")
                  )}
                </p>
                <div className="q-orb">
                  <canvas ref={cvRef} className="q-orb-cv" aria-hidden="true" />
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
