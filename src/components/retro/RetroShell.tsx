import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { cerrarDesdeShell } from "@/lib/utils/ventana";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  Mic,
  ArrowDownToLine,
  Minus,
  Square,
  X,
  FileText,
  Boxes,
  Settings,
  Clock,
  CloudOff,
  VenetianMask,
  Palette,
  Languages,
  Eraser,
  AudioLines,
  Orbit,
  RectangleHorizontal,
  Copy,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import AbraxLogo from "../icons/AbraxLogo";
import { useSettings } from "@/hooks/useSettings";
import { commands, events, type HistoryEntry } from "@/bindings";
import { applyShell, UI_SHELL_OPTIONS } from "@/lib/utils/theme";
import type { UiShell } from "@/bindings";
import { EsferaEngine, readEsferaPalette } from "../../overlay/esfera/engine";
import "./retro.css";

// Skin "ABRAX" — reproductor vertical (mockup del usuario): ESTADO + nivel de
// entrada, botones de acción, transporte, ecualizador, transcripciones recientes
// + modelo activo, y nav con íconos. Controles cableados a features reales; el
// ecualizador y el transporte de reproducción son cosméticos (ABRAX no edita
// audio ni reproduce), pensados como "vibe" del reproductor.

type RView =
  | "escuchar"
  | "transcripciones"
  | "modelos"
  | "ajustes"
  | "historial";

const NAV: [RView, string, LucideIcon][] = [
  ["escuchar", "ESCUCHAR", Mic],
  ["transcripciones", "TRANSCRIPCIONES", FileText],
  ["modelos", "MODELOS", Boxes],
  ["ajustes", "AJUSTES", Settings],
  ["historial", "HISTORIAL", Clock],
];

const EQ_LABELS = ["60", "170", "310", "600", "1K", "3K", "6K", "12K"];

// Los skins NO se enumeran aquí: se derivan de `UI_SHELL_OPTIONS`, la misma
// fuente que usa el selector de Ajustes, y las etiquetas salen de `t()`. Estuvo
// escrito a mano y se desincronizó: la lista se quedó en tres y Karting, añadido
// después, era INALCANZABLE desde Retro — el skin existía pero no había forma de
// llegar a él si estabas aquí. Derivarla hace que un skin nuevo aparezca solo.

const LANG_NAMES: Record<string, string> = {
  auto: "Auto",
  es: "Español",
  en: "English",
  pt: "Português",
  fr: "Français",
  de: "Deutsch",
  it: "Italiano",
};

const fmtHora = (secs: number): string => {
  try {
    return new Date(secs * 1000).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return "";
  }
};

export const RetroShell: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();

  const [view, setView] = useState<RView | null>("escuchar");
  const [grabando, setGrabando] = useState(false);
  const [seg, setSeg] = useState(0);
  const grabandoRef = useRef(false);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [selEntry, setSelEntry] = useState<number | null>(null);
  const [eq, setEq] = useState<number[]>([4, 7, 5, 2, 0, -2, 1, 3]);
  const [skinMenu, setSkinMenu] = useState(false);
  // Cuadro de dictado en vivo (vista ESCUCHAR): texto acumulado editable +
  // fragmento parcial en curso (solo modelos streaming).
  const [dictado, setDictado] = useState("");
  const [parcial, setParcial] = useState("");

  // ── esfera oficial (motor WebGL del overlay: "flor cósmica" ~24k puntos) ──
  const cvRef = useRef<HTMLCanvasElement>(null);
  const esf = useRef<EsferaEngine | null>(null);
  const subscribed = useRef(false);
  const stackRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!cvRef.current) return;
    const eng = new EsferaEngine(cvRef.current);
    eng.setPalette(readEsferaPalette());
    eng.setState("recording");
    eng.start();
    esf.current = eng;
    return () => {
      eng.stop();
      eng.dispose();
      esf.current = null;
    };
  }, []);

  // Espectro real del micrófono → esfera (solo mientras se dicta; en reposo
  // no llega audio y la esfera queda calma).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    listen<{ bands: number[]; rms: number; bass: number; dominant: number }>(
      "spectrum",
      (e) => {
        const { bands, rms, bass, dominant } = e.payload;
        esf.current?.setAudioData(bands, rms, bass, dominant);
      },
    ).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let vivo = true;
    const tick = async () => {
      try {
        const rec = await commands.isRecording();
        if (!vivo || rec === grabandoRef.current) return;
        grabandoRef.current = rec;
        setGrabando(rec);
        if (rec) {
          setSeg(0);
          setParcial("");
          if (!subscribed.current) {
            void commands.startSpectrum();
            subscribed.current = true;
          }
        } else if (subscribed.current) {
          void commands.stopSpectrum();
          subscribed.current = false;
        }
      } catch {
        // aún no listo
      }
    };
    const id = setInterval(tick, 200);
    void tick();
    return () => {
      vivo = false;
      clearInterval(id);
      if (subscribed.current) {
        void commands.stopSpectrum();
        subscribed.current = false;
      }
    };
  }, []);

  useEffect(() => {
    if (!grabando) return;
    const id = setInterval(() => setSeg((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, [grabando]);

  const loadHistory = useCallback(async () => {
    try {
      const r = await commands.getHistoryEntries(null, 50);
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

  // Dictado en vivo → cuadro de ESCUCHAR: fragmento parcial (streaming) y, al
  // completar cada dictado, se anexa el texto final (funciona con cualquier
  // modelo vía history-update-payload).
  useEffect(() => {
    let un1: (() => void) | null = null;
    let un2: (() => void) | null = null;
    let cancelled = false;
    events.streamTextEvent
      .listen((e) => {
        const { committed, tentative } = e.payload;
        setParcial(`${committed} ${tentative}`.trim());
      })
      .then((fn) => {
        if (cancelled) fn();
        else un1 = fn;
      });
    events.historyUpdatePayload
      .listen((e) => {
        if (e.payload.action !== "added") return;
        const t = e.payload.entry.transcription_text?.trim();
        if (t) setDictado((prev) => (prev ? `${prev} ${t}` : t));
        setParcial("");
      })
      .then((fn) => {
        if (cancelled) fn();
        else un2 = fn;
      });
    return () => {
      cancelled = true;
      un1?.();
      un2?.();
    };
  }, []);

  useEffect(() => {
    const root = document.documentElement;
    const obs = new MutationObserver(() =>
      esf.current?.setPalette(readEsferaPalette()),
    );
    obs.observe(root, {
      attributes: true,
      attributeFilter: ["data-ui-theme", "data-theme"],
    });
    return () => obs.disconnect();
  }, []);

  // ── ventana = tamaño del reproductor ──
  const ajustar = useCallback(() => {
    const el = stackRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    void getCurrentWindow()
      .setSize(new LogicalSize(Math.ceil(r.right), Math.ceil(r.bottom)))
      .catch(() => {});
  }, []);
  useEffect(() => {
    const el = stackRef.current;
    if (!el) return;
    let raf = 0;
    const s = () => {
      if (!raf)
        raf = requestAnimationFrame(() => {
          raf = 0;
          ajustar();
        });
    };
    s();
    const ro = new ResizeObserver(s);
    ro.observe(el);
    return () => {
      if (raf) cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [ajustar]);
  useEffect(() => {
    const id = requestAnimationFrame(ajustar);
    return () => cancelAnimationFrame(id);
  }, [view, entries.length, ajustar]);

  const dictar = () => void commands.triggerTranscription();
  const cerrar = () => setView(null);
  const copiar = async (t: string) => {
    try {
      await navigator.clipboard.writeText(t);
      toast.success("Copiado al portapapeles");
    } catch {
      // ignorar
    }
  };
  const cambiarShell = async (shell: UiShell) => {
    setSkinMenu(false);
    applyShell(shell);
    try {
      await commands.changeUiShellSetting(shell);
    } catch {
      // se persiste igual
    }
    await relaunch();
  };
  const togglePaleta = () => {
    const next = settings?.ui_theme === "imperial" ? "abrax" : "imperial";
    if (next === "imperial") {
      document.documentElement.dataset.uiTheme = "imperial";
      document.documentElement.dataset.theme = "dark";
    } else {
      delete document.documentElement.dataset.uiTheme;
    }
    updateSetting("ui_theme", next);
  };
  // Estilo de overlay al grabar: "esfera" (los puntitos WebGL de ABRAX) o
  // "minimal" (la píldora ORIGINAL de Handy con waveform). El botón/valor
  // alterna esfera ⇄ Handy(minimal). ("live" = píldora que crece con texto en
  // vivo, solo modelos streaming; se puede fijar desde Ajustes → Overlay.)
  const overlayStyle = settings?.overlay_style ?? "minimal";
  const visualLabel = {
    none: "Ninguna",
    minimal: "Handy",
    live: "Live",
    esfera: "Esfera",
  }[overlayStyle];
  const toggleVisual = () =>
    updateSetting(
      "overlay_style",
      overlayStyle === "esfera" ? "minimal" : "esfera",
    );

  const win = getCurrentWindow();
  const reloj = `${String(Math.floor(seg / 3600)).padStart(2, "0")}:${String(
    Math.floor((seg % 3600) / 60),
  ).padStart(2, "0")}:${String(seg % 60).padStart(2, "0")}`;
  const modelName = (settings?.selected_model ?? "—")
    .split("/")
    .pop()!
    .replace(/\.(gguf|bin|onnx|safetensors)$/i, "")
    .split(/[-_]/)
    .slice(0, 2)
    .join(" ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
  const langLabel =
    LANG_NAMES[settings?.selected_language ?? "auto"] ??
    settings?.selected_language ??
    "Auto";

  const fillerOn = useMemo(() => {
    const f = settings?.custom_filler_words;
    return f === null || f === undefined || f.length > 0;
  }, [settings?.custom_filler_words]);

  const barraTitulo = (
    <header className="rk-top" data-tauri-drag-region>
      <AbraxLogo variant="horizontal" width={100} />
      <span className="rk-top-status" data-tauri-drag-region>
        <i className={`rk-dot${grabando ? " live" : " on"}`} />
        {t("retro.localBadge")}
        <CloudOff size={11} aria-hidden="true" />
      </span>
      <div className="rk-win">
        {settings?.show_tray_icon !== false && (
          <button
            type="button"
            title="Enviar a la bandeja"
            aria-label="Enviar a la bandeja"
            onClick={() => void win.hide()}
          >
            <ArrowDownToLine size={12} aria-hidden="true" />
          </button>
        )}
        <button
          type="button"
          title="Minimizar"
          aria-label="Minimizar"
          onClick={() => void win.minimize()}
        >
          <Minus size={12} aria-hidden="true" />
        </button>
        <button
          type="button"
          title="Cerrar"
          aria-label="Cerrar"
          onClick={() => cerrarDesdeShell(win)}
        >
          <X size={12} aria-hidden="true" />
        </button>
      </div>
    </header>
  );

  const panelEstado = (
    <section className="rk-panel rk-estado-panel">
      <div className="rk-estado-top">
        <div className="rk-estado-l">
          <div className="rk-cap">ESTADO</div>
          <div className={`rk-estado-txt${grabando ? " live" : ""}`}>
            {grabando ? "ESCUCHANDO..." : "LISTO"}
          </div>
          <div className="rk-estado-t">{reloj}</div>
        </div>
        <div className="rk-nivel">
          <div className="rk-esfera">
            <canvas ref={cvRef} className="rk-esfera-cv" aria-hidden="true" />
          </div>
        </div>
      </div>
      <div className="rk-infobar">
        <span>
          <b>{t("retro.infoPalette")}</b>
          {settings?.ui_theme === "imperial" ? "Imperial" : "ABRAX"}
        </span>
        <span>
          <b>{t("retro.infoLanguage")}</b>
          {langLabel}
        </span>
        <span>
          <b>{t("retro.infoFillers")}</b>
          {fillerOn ? "On" : "Off"}
        </span>
        <span>
          <b>{t("retro.infoTranslate")}</b>
          {settings?.translate_to_english ? "On" : "Off"}
        </span>
        <span>
          <b>{t("retro.infoAutoVoice")}</b>
          {settings?.vad_enabled ? "On" : "Off"}
        </span>
        <span>
          <b>{t("retro.infoVisual")}</b>
          {visualLabel}
        </span>
      </div>
    </section>
  );

  // ESCUCHAR: botón de dictado + CUADRO en vivo donde va apareciendo el texto
  // que dictas (editable, para retocarlo o luego procesarlo con el LLM local).
  const textoVivo =
    grabando && parcial ? `${dictado ? `${dictado} ` : ""}${parcial}` : dictado;
  const numPalabras = dictado.trim() ? dictado.trim().split(/\s+/).length : 0;
  const vistaEscuchar = (
    <div className="rk-escuchar">
      <div className="rk-dictar-row">
        <button
          type="button"
          className={`rk-dictar rk-dictar-sm${grabando ? " rec" : ""}`}
          onClick={dictar}
          title={grabando ? "Detener" : "Dictar"}
        >
          {grabando ? (
            <Square size={15} aria-hidden="true" />
          ) : (
            <Mic size={16} aria-hidden="true" />
          )}
        </button>
        <div className="rk-hint rk-dictar-hint">
          {grabando
            ? "Escuchando… habla y aparecerá aquí abajo"
            : "Clic o mantén tu atajo para dictar"}
        </div>
      </div>
      <textarea
        className="rk-dictado-ta"
        value={textoVivo}
        readOnly={grabando}
        onChange={(e) => setDictado(e.target.value)}
        placeholder="Aquí irá apareciendo lo que dictes… (editable)"
        spellCheck={false}
      />
      <div className="rk-dictado-foot">
        <span className="rk-dictado-count">
          {t("retro.words", { n: numPalabras })}
        </span>
        <div className="rk-dictado-acts">
          <button
            type="button"
            onClick={() => void copiar(dictado)}
            disabled={!dictado.trim()}
          >
            <Copy size={12} aria-hidden="true" /> {t("retro.actCopy")}
          </button>
          <button
            type="button"
            onClick={() => {
              setDictado("");
              setParcial("");
            }}
            disabled={!dictado && !parcial}
          >
            <Trash2 size={12} aria-hidden="true" /> {t("retro.actClear")}
          </button>
        </div>
      </div>
    </div>
  );

  const vistaLista = (
    <section className="rk-panel rk-lista">
      <div className="rk-cap">
        {view === "historial" ? "HISTORIAL" : "TRANSCRIPCIONES"}
      </div>
      <div className="rk-recents-list rk-lista-full">
        {entries.length === 0 ? (
          <div className="rk-vacio">{t("retro.emptyTranscriptions")}</div>
        ) : (
          entries.map((e) => (
            <button
              type="button"
              key={e.id}
              className={`rk-rec-row${selEntry === e.id ? " sel" : ""}`}
              onClick={() => setSelEntry(e.id)}
              onDoubleClick={() => copiar(e.transcription_text)}
            >
              <span className="rk-rec-t">
                {e.title || e.transcription_text}
              </span>
              <span className="rk-rec-h">{fmtHora(e.timestamp)}</span>
            </button>
          ))
        )}
      </div>
    </section>
  );

  const vistaModelos = (
    <section className="rk-panel rk-modelos-v">
      <div className="rk-cap">{t("retro.activeModel")}</div>
      <div className="rk-model-name rk-model-big">{modelName}</div>
      <div className="rk-model-sub">{t("retro.localFull")}</div>
    </section>
  );

  const toggle = (
    key: "push_to_talk" | "vad_enabled" | "translate_to_english",
    label: string,
  ) => (
    <button
      type="button"
      className={`rk-sw${settings?.[key] ? " on" : ""}`}
      onClick={() => updateSetting(key, !settings?.[key])}
      aria-pressed={!!settings?.[key]}
    >
      <i className="rk-sw-led" /> {label}
    </button>
  );

  const vistaAjustes = (
    <>
      <section className="rk-panel rk-eq">
        <div className="rk-cap">ECUALIZADOR</div>
        <div className="rk-eq-body">
          <div className="rk-eq-scale">
            <span>+12</span>
            <span>0</span>
            <span>-12</span>
          </div>
          {EQ_LABELS.map((label, i) => (
            <div className="rk-eq-band" key={label}>
              <input
                type="range"
                min={-12}
                max={12}
                value={eq[i]}
                aria-label={`EQ ${label}`}
                onChange={(e) =>
                  setEq((p) => {
                    const n = [...p];
                    n[i] = Number(e.target.value);
                    return n;
                  })
                }
              />
              <span>{label}</span>
            </div>
          ))}
          <div className="rk-eq-scale">
            <span>+12</span>
            <span>0</span>
            <span>-12</span>
          </div>
        </div>
      </section>
      <section className="rk-panel rk-ajustes-v">
        <div className="rk-cap">PROCESO</div>
        <div className="rk-sw-grid">
          {toggle("push_to_talk", "PUSH-TO-TALK")}
          {toggle("vad_enabled", "AUTO-VOZ (VAD)")}
          {toggle("translate_to_english", "TRADUCIR → EN")}
          <button
            type="button"
            className={`rk-sw${fillerOn ? " on" : ""}`}
            onClick={() =>
              updateSetting("custom_filler_words", fillerOn ? [] : null)
            }
            aria-pressed={fillerOn}
          >
            <i className="rk-sw-led" /> MULETILLAS
          </button>
        </div>
        <button
          type="button"
          className="rk-link"
          onClick={() => void cambiarShell("classic")}
        >
          {`↺ ${t("retro.backToClassic")}`}
        </button>
      </section>
    </>
  );

  return (
    <div
      id="retro-stage"
      className="rk select-none"
      onClick={() => setSkinMenu(false)}
    >
      <div className="rk-frame" ref={stackRef}>
        {barraTitulo}
        {panelEstado}

        {/* barra de opciones tipo Winamp (acciones REALES) */}
        <div className="rk-optbar">
          <div className="rk-opt-skin">
            <button
              type="button"
              className={skinMenu ? "on" : ""}
              onClick={(e) => {
                e.stopPropagation();
                setSkinMenu((v) => !v);
              }}
              title="Cambiar de skin / apariencia"
              aria-label="Cambiar de skin / apariencia"
            >
              <VenetianMask size={15} aria-hidden="true" />
            </button>
            {skinMenu && (
              <div className="rk-skinmenu" onClick={(e) => e.stopPropagation()}>
                {UI_SHELL_OPTIONS.filter(
                  (s) => s !== (settings?.ui_shell ?? "retro"),
                ).map((s) => (
                  <button
                    key={s}
                    type="button"
                    onClick={() => void cambiarShell(s)}
                  >
                    {t(`shell.options.${s}`)}
                  </button>
                ))}
              </div>
            )}
          </div>
          <button
            type="button"
            className={settings?.ui_theme === "imperial" ? "on" : ""}
            onClick={togglePaleta}
            title={
              settings?.ui_theme === "imperial"
                ? "Paleta: Imperial (clic → ABRAX)"
                : "Paleta: ABRAX (clic → Imperial)"
            }
            aria-label="Paleta"
          >
            <Palette size={15} aria-hidden="true" />
          </button>
          <button
            type="button"
            className={settings?.vad_enabled ? "on" : ""}
            onClick={() => updateSetting("vad_enabled", !settings?.vad_enabled)}
            title={
              settings?.vad_enabled
                ? "Auto-voz (VAD): ACTIVADO — recorta silencios"
                : "Auto-voz (VAD): desactivado"
            }
            aria-pressed={!!settings?.vad_enabled}
          >
            <AudioLines size={15} aria-hidden="true" />
          </button>
          <button
            type="button"
            className={settings?.translate_to_english ? "on" : ""}
            onClick={() =>
              updateSetting(
                "translate_to_english",
                !settings?.translate_to_english,
              )
            }
            title={
              settings?.translate_to_english
                ? "Traducir a inglés: ACTIVADO"
                : "Traducir a inglés: desactivado"
            }
            aria-pressed={!!settings?.translate_to_english}
          >
            <Languages size={15} aria-hidden="true" />
          </button>
          <button
            type="button"
            className={fillerOn ? "on" : ""}
            onClick={() =>
              updateSetting("custom_filler_words", fillerOn ? [] : null)
            }
            title={
              fillerOn
                ? "Filtro de muletillas: ACTIVADO"
                : "Filtro de muletillas: desactivado"
            }
            aria-pressed={fillerOn}
          >
            <Eraser size={15} aria-hidden="true" />
          </button>
          <button
            type="button"
            className={overlayStyle === "esfera" ? "on" : ""}
            onClick={toggleVisual}
            title={
              overlayStyle === "esfera"
                ? "Visual al grabar: Esfera (clic → Handy)"
                : `Visual al grabar: ${visualLabel} (clic → Esfera)`
            }
            aria-label="Visual al grabar"
          >
            {overlayStyle === "esfera" ? (
              <Orbit size={15} aria-hidden="true" />
            ) : (
              <RectangleHorizontal size={15} aria-hidden="true" />
            )}
          </button>
        </div>

        <nav className="rk-nav">
          {NAV.map(([id, label, Icon]) => (
            <button
              key={id}
              type="button"
              className={`rk-tab${view === id ? " on" : ""}`}
              onClick={() => setView(view === id ? null : id)}
            >
              <Icon size={14} />
              <span>{label}</span>
            </button>
          ))}
        </nav>

        <div className="rk-body">
          {view !== null && (
            <div className="rk-sub">
              <button
                type="button"
                className="rk-sub-close"
                onClick={cerrar}
                title="Cerrar"
                aria-label="Cerrar"
              >
                <X size={14} aria-hidden="true" />
              </button>
              {view === "escuchar" && vistaEscuchar}
              {(view === "transcripciones" || view === "historial") &&
                vistaLista}
              {view === "modelos" && vistaModelos}
              {view === "ajustes" && vistaAjustes}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

RetroShell.displayName = "RetroShell";

export default RetroShell;
