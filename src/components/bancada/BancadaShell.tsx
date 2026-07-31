import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  ArrowDownToLine,
  Minus,
  X,
  ShieldCheck,
  Gauge,
  History,
  Cog,
  AudioLines,
  Languages,
  Eraser,
  Palette,
  Copy,
  Trash2,
  Users,
  RotateCcw,
  MonitorSpeaker,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";
import AbraxLogo from "../icons/AbraxLogo";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { cerrarDesdeShell } from "@/lib/utils/ventana";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { applyUiTheme } from "@/lib/utils/theme";
import { commands, events, type UiTheme } from "@/bindings";
import { HistorySettings } from "../settings";
import { ShellSelector } from "../settings/ShellSelector";
import { PaletteSelector } from "../settings/PaletteSelector";
import { ThemeSelector } from "../settings/ThemeSelector";
import { SECTIONS_CONFIG, type SidebarSection } from "../Sidebar";
import { AtajoVox } from "../AtajoVox";
import { montarTacometro, type TacoHandle } from "./tacometro";
import { ShiftLights, type ShiftLightsHandle } from "./ShiftLights";
import { BotonDictar } from "./BotonDictar";
import { Chip } from "./Chip";
import { ESTADO_KEY, type Estado } from "./LamparasEstado";
import "./bancada.css";

const PALETAS: UiTheme[] = ["abrax", "imperial", "escuderia"];
const PALETA_LABEL: Record<UiTheme, string> = {
  abrax: "ABRAX",
  imperial: "IMPERIAL",
  escuderia: "ESCUDERÍA",
};

// Skin "Karting" — consola de garaje / banco de pruebas de motor (paleta
// Escudería por defecto).
//
// El id interno sigue siendo `bancada` a propósito: es una variante serde
// guardada en settings (`UiShell::Bancada`), y renombrarla dejaría en Clásico a
// quien tuviera este skin puesto. La ETIQUETA visible es «Karting» en los 22
// locales. Se dejó de llamar «shell F1» porque prometía Fórmula 1 y el HUD que
// quedó no es eso; el vocabulario de boxes, pit-radio y largada vale igual para
// el karting, así que el resto del copy sigue coherente.
//
// TABLERO: instrumentos héroe (ignición,
// shift-lights, tacómetro, lámparas, LCD de pit-radio) sobre el ciclo de
// dictado real de RetroShell (poll isRecording → startSpectrum/stopSpectrum,
// R8). BITÁCORA/SETUP: componentes REALES embebidos (HistorySettings + las 8
// secciones de SECTIONS_CONFIG) — el ShellSelector es la SALIDA a otros shells.
// Hot-path por REFS + un rAF único gateado por grabación; drawOnce si
// reduced-motion. Cero WebGL → captura CDP fiel. Copy provisional es-419
// (reusa quiet.* de ventana); i18n propia en Fase 4.

type Spec = { bands: number[]; rms: number; bass: number; dominant: number };
type BView = "tablero" | "bitacora" | "setup";

// [vista, clave i18n del label, icono]
const VIEWS: [BView, string, LucideIcon][] = [
  ["tablero", "bancada.nav.tablero", Gauge],
  ["bitacora", "bancada.nav.bitacora", History],
  ["setup", "bancada.nav.setup", Cog],
];

const countWords = (s: string): number => {
  const tt = s.trim();
  return tt ? tt.split(/\s+/).length : 0;
};

// Glifo decorativo del selector «fusionar hablante»: el nombre accesible lo
// aporta el `title` del <select>, no este símbolo.
const GLIFO_FUSION = "⤳";

// Color por hablante (cicla). El acento de marca es el del Hablante 1.
const SPK_COLORS = [
  "var(--bnc-accent)",
  "#2fd9ff",
  "#35c85a",
  "#ffb020",
  "#b98cff",
  "#ff7ac0",
];

// Si el texto trae marcadores [Hablante N] (diarización), los renderiza como
// badges de color; si no, devuelve el texto tal cual.
function renderTranscript(
  text: string,
  t: (k: string, o?: Record<string, unknown>) => string,
): React.ReactNode {
  const parts = text.split(/(\[Hablante \d+\])/g);
  if (parts.length <= 1) return text;
  return parts.map((p, i) => {
    const m = p.match(/^\[Hablante (\d+)\]$/);
    if (m) {
      const n = parseInt(m[1], 10);
      const color = SPK_COLORS[(n - 1) % SPK_COLORS.length];
      return (
        <span key={i} className="bnc-spk" style={{ color, borderColor: color }}>
          {t("bancada.speakerLabel", { n })}
        </span>
      );
    }
    return <React.Fragment key={i}>{p}</React.Fragment>;
  });
}

type DiarSegment = { time: string | null; speaker: number; text: string };

// Parsea texto diarizado "[MM:SS] [Hablante N] texto …" (el [MM:SS] es opcional)
// en segmentos por turno. Devuelve null si no hay marcadores de hablante.
function parseDiarized(text: string): DiarSegment[] | null {
  if (!/\[Hablante \d+\]/.test(text)) return null;
  const re = /(?:\[(\d{1,2}:\d{2})\]\s*)?\[Hablante (\d+)\]\s*/g;
  const segs: DiarSegment[] = [];
  let m: RegExpExecArray | null;
  let last: { idx: number; time: string | null; speaker: number } | null = null;
  while ((m = re.exec(text)) !== null) {
    if (last) {
      segs.push({
        time: last.time,
        speaker: last.speaker,
        text: text.slice(last.idx, m.index).trim(),
      });
    }
    last = {
      idx: re.lastIndex,
      time: m[1] ?? null,
      speaker: parseInt(m[2], 10),
    };
  }
  if (last) {
    segs.push({
      time: last.time,
      speaker: last.speaker,
      text: text.slice(last.idx).trim(),
    });
  }
  const clean = segs.filter((s) => s.text);
  return clean.length ? clean : null;
}

export const BancadaShell: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const osType = useOsType();

  const [view, setView] = useState<BView>("tablero");
  const [section, setSection] = useState<SidebarSection>("general");
  const [grabando, setGrabando] = useState(false);
  const [fase, setFase] = useState<null | "transcribiendo" | "puliendo">(null);
  const [errOn, setErrOn] = useState(false);
  const [dictado, setDictado] = useState("");
  const [parcial, setParcial] = useState("");
  const [seg, setSeg] = useState(0);

  const grabandoRef = useRef(false);
  const subscribedRef = useRef(false);
  const specRef = useRef<Spec | null>(null);
  const lastDataRef = useRef(0);
  const revRef = useRef(0);
  const rafRef = useRef<number | null>(null);
  const shiftRef = useRef<ShiftLightsHandle>(null);
  const tachoRef = useRef<TacoHandle | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const errTimer = useRef<number | undefined>(undefined);
  const reduceRef = useRef(
    typeof window !== "undefined" &&
      !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches,
  );
  // Palabras por minuto (lectura del tacómetro): métrica real de la sesión.
  const wpmRef = useRef(0);
  const wpmShownRef = useRef(0); // último valor mostrado (se congela al parar)
  const wordsBaseRef = useRef(0); // palabras acumuladas al iniciar la grabación
  const dictadoRef = useRef("");

  // ── bucle rAF único: suaviza rev y actualiza instrumentos por refs ──
  const ensureRaf = useCallback(() => {
    if (rafRef.current != null) return;
    const loop = () => {
      const now = performance.now();
      const s = specRef.current;
      const fresh = now - lastDataRef.current < 350;
      const target =
        grabandoRef.current && fresh && s
          ? Math.min(1, Math.max(0, s.rms * 0.7 + s.bass * 0.5))
          : 0;
      const cur = revRef.current;
      revRef.current = cur + (target - cur) * (target > cur ? 0.35 : 0.12);
      if (revRef.current < 0.001) revRef.current = 0;
      shiftRef.current?.setRev(revRef.current);
      tachoRef.current?.render(revRef.current, wpmRef.current);
      if (grabandoRef.current || revRef.current > 0.002) {
        rafRef.current = requestAnimationFrame(loop);
      } else {
        rafRef.current = null;
        shiftRef.current?.setRev(0);
        tachoRef.current?.render(0, wpmRef.current);
      }
    };
    rafRef.current = requestAnimationFrame(loop);
  }, []);

  const pulse = useCallback(() => {
    if (reduceRef.current) {
      const v = grabandoRef.current ? 0.55 : 0;
      revRef.current = v;
      shiftRef.current?.setRev(v);
      tachoRef.current?.render(v, wpmRef.current);
      return;
    }
    ensureRaf();
  }, [ensureRaf]);

  // ── estado real de dictado: poll 200ms + ciclo startSpectrum/stopSpectrum (R8) ──
  useEffect(() => {
    let vivo = true;
    const tick = async () => {
      try {
        const rec = await commands.isRecording();
        if (!vivo || rec === grabandoRef.current) return;
        grabandoRef.current = rec;
        setGrabando(rec);
        if (rec) {
          setParcial("");
          setFase(null);
          setSeg(0);
          wordsBaseRef.current = countWords(dictadoRef.current);
          if (!subscribedRef.current) {
            void commands.startSpectrum();
            subscribedRef.current = true;
          }
          shiftRef.current?.flash();
        } else if (subscribedRef.current) {
          void commands.stopSpectrum();
          subscribedRef.current = false;
        }
        pulse();
      } catch {
        // comando aún no listo
      }
    };
    const id = setInterval(tick, 200);
    void tick();
    return () => {
      vivo = false;
      clearInterval(id);
      if (subscribedRef.current) {
        void commands.stopSpectrum();
        subscribedRef.current = false;
      }
    };
  }, [pulse]);

  // ── espectro real → specRef (nunca estado React); dispara el rAF ──
  useEffect(() => {
    let un: (() => void) | null = null;
    let cancelled = false;
    listen<Spec>("spectrum", (e) => {
      specRef.current = e.payload;
      lastDataRef.current = performance.now();
      if (rafRef.current == null && !reduceRef.current) ensureRaf();
    }).then((fn) => {
      if (cancelled) fn();
      else un = fn;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, [ensureRaf]);

  // ── texto en vivo (parcial) + texto final acumulado ──
  useEffect(() => {
    let un1: (() => void) | null = null;
    let un2: (() => void) | null = null;
    let cancelled = false;
    events.streamTextEvent
      .listen((e) => {
        const { committed, tentative } = e.payload;
        setParcial(`${committed} ${tentative}`.trim());
      })
      .then((fn) => (cancelled ? fn() : (un1 = fn)));
    events.historyUpdatePayload
      .listen((e) => {
        if (e.payload.action !== "added") return;
        const txt = e.payload.entry.transcription_text?.trim();
        if (txt) setDictado((p) => (p ? `${p} ${txt}` : txt));
        setParcial("");
        setFase(null);
      })
      .then((fn) => (cancelled ? fn() : (un2 = fn)));
    return () => {
      cancelled = true;
      un1?.();
      un2?.();
    };
  }, []);

  // ── fase de trabajo (transcribiendo/puliendo) para las lámparas ──
  useEffect(() => {
    let un: (() => void) | null = null;
    let cancelled = false;
    events.streamPhaseEvent
      .listen((e) => {
        const { phase, kind } = e.payload;
        if (phase === "working")
          setFase(kind === "polishing" ? "puliendo" : "transcribiendo");
        else setFase(null);
      })
      .then((fn) => (cancelled ? fn() : (un = fn)));
    return () => {
      cancelled = true;
      un?.();
    };
  }, []);

  // ── alertas → testigo de FALLA (el toast global lo maneja App.tsx) ──
  useEffect(() => {
    let un: (() => void) | null = null;
    let cancelled = false;
    events.userAlertEvent
      .listen(() => {
        setErrOn(true);
        window.clearTimeout(errTimer.current);
        errTimer.current = window.setTimeout(() => setErrOn(false), 4000);
      })
      .then((fn) => (cancelled ? fn() : (un = fn)));
    return () => {
      cancelled = true;
      un?.();
      window.clearTimeout(errTimer.current);
    };
  }, []);

  // ── cronómetro de vuelta ──
  useEffect(() => {
    if (!grabando) return;
    const id = setInterval(() => setSeg((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, [grabando]);

  // ── tacómetro (canvas-2D): solo montado en TABLERO; re-teñido por paleta ──
  useEffect(() => {
    if (view !== "tablero" || !canvasRef.current) return;
    const taco = montarTacometro(canvasRef.current);
    tachoRef.current = taco;
    taco.render(revRef.current, wpmRef.current);
    shiftRef.current?.setRev(revRef.current);
    const ro = new ResizeObserver(() => taco.resize());
    ro.observe(canvasRef.current);
    const root = document.documentElement;
    const obs = new MutationObserver(() => {
      taco.setPalette();
      taco.render(revRef.current, wpmRef.current);
    });
    obs.observe(root, {
      attributes: true,
      attributeFilter: ["data-ui-theme", "data-theme"],
    });
    return () => {
      ro.disconnect();
      obs.disconnect();
      taco.destroy();
      tachoRef.current = null;
    };
  }, [view]);

  // sincroniza el texto acumulado a un ref (base de palabras por sesión)
  useEffect(() => {
    dictadoRef.current = dictado;
  }, [dictado]);

  // unidad del tacómetro (i18n); se re-aplica al cambiar idioma o volver a TABLERO
  useEffect(() => {
    tachoRef.current?.setUnit(t("bancada.wpmUnit"));
    tachoRef.current?.render(revRef.current, wpmRef.current);
  }, [t, view]);

  const dictar = () => void commands.triggerTranscription();
  const cancelar = () => void commands.cancelOperation();
  const copiar = async () => {
    const txt = dictado.trim();
    if (!txt) return;
    try {
      await navigator.clipboard.writeText(txt);
      toast.success(t("bancada.copied"));
    } catch {
      // ignorar
    }
  };
  const limpiar = () => {
    setDictado("");
    setParcial("");
  };
  const cfw = settings?.custom_filler_words;
  const fillerOn = cfw == null || cfw.length > 0;
  const paletaActual = (settings?.ui_theme ?? "abrax") as UiTheme;
  const ciclarPaleta = () => {
    const next = PALETAS[(PALETAS.indexOf(paletaActual) + 1) % PALETAS.length];
    applyUiTheme(next);
    updateSetting("ui_theme", next);
  };
  const win = getCurrentWindow();

  const estado: Estado = errOn ? "error" : grabando ? "rec" : (fase ?? "idle");

  const atajo = formatKeyCombination(
    settings?.bindings?.transcribe?.current_binding ?? "",
    osType,
  );
  const reloj = `${String(Math.floor(seg / 60)).padStart(2, "0")}:${String(
    seg % 60,
  ).padStart(2, "0")}`;
  const textoVivo =
    grabando && parcial ? `${dictado ? `${dictado} ` : ""}${parcial}` : dictado;

  // ── Diarización: nombres personalizados + fusión de hablantes (panel lateral) ──
  // Nombres por hablante (Hablante 1 → "Joaco"); vacío = sin renombrar.
  const [speakerNames, setSpeakerNames] = useState<Record<number, string>>({});
  // Fusión: hablante origen → hablante destino ("Hablante 3 es en realidad Joaco").
  const [speakerMerge, setSpeakerMerge] = useState<Record<number, number>>({});
  // Solo al soltar (no mientras grabás): en vivo mandamos el streaming plano; la
  // vista por hablante + panel aparecen con el texto final etiquetado.
  const diarSegs = useMemo(
    () => (grabando ? null : parseDiarized(textoVivo)),
    [textoVivo, grabando],
  );
  // Sigue la cadena de fusiones hasta el hablante efectivo.
  const effSpeaker = useCallback(
    (n: number): number => {
      let s = n;
      for (let i = 0; i < 12 && speakerMerge[s] != null; i++)
        s = speakerMerge[s];
      return s;
    },
    [speakerMerge],
  );
  const nameOf = useCallback(
    (n: number): string => speakerNames[n]?.trim() || `Hablante ${n}`,
    [speakerNames],
  );
  // Hablantes efectivos presentes (ordenados) + nº de turnos de cada uno.
  const { distinctSpeakers, turnCounts } = useMemo(() => {
    const counts: Record<number, number> = {};
    for (const s of diarSegs ?? []) {
      const e = effSpeaker(s.speaker);
      counts[e] = (counts[e] ?? 0) + 1;
    }
    return {
      distinctSpeakers: Object.keys(counts)
        .map(Number)
        .sort((a, b) => a - b),
      turnCounts: counts,
    };
  }, [diarSegs, effSpeaker]);
  const speakersEdited =
    Object.keys(speakerNames).length > 0 ||
    Object.keys(speakerMerge).length > 0;

  // Panel de hablantes (cuadradito): va DEBAJO del medidor de PAL/MIN, en la
  // columna izquierda. Solo aparece con una transcripción diarizada (al soltar).
  const panelHablantes = diarSegs ? (
    <div className="bnc-diar-panel">
      <div className="bnc-diar-panel-head">
        <Users size={13} />
        <span>{t("bancada.speakers")}</span>
      </div>
      {distinctSpeakers.map((s) => {
        const color = SPK_COLORS[(s - 1) % SPK_COLORS.length];
        return (
          <div key={s} className="bnc-diar-row">
            <span
              className="bnc-diar-dot"
              style={{ background: color }}
              aria-hidden="true"
            />
            <input
              className="bnc-diar-name"
              value={speakerNames[s] ?? ""}
              placeholder={`Hablante ${s}`}
              spellCheck={false}
              onChange={(e) =>
                setSpeakerNames((p) => ({ ...p, [s]: e.target.value }))
              }
            />
            <span className="bnc-diar-count">{turnCounts[s]}</span>
            {distinctSpeakers.length > 1 && (
              <select
                className="bnc-diar-merge"
                value=""
                title={t("bancada.mergeInto")}
                onChange={(e) => {
                  const tgt = parseInt(e.target.value, 10);
                  if (tgt) setSpeakerMerge((p) => ({ ...p, [s]: tgt }));
                }}
              >
                <option value="">{GLIFO_FUSION}</option>
                {distinctSpeakers
                  .filter((o) => o !== s)
                  .map((o) => (
                    <option key={o} value={o}>
                      {nameOf(o)}
                    </option>
                  ))}
              </select>
            )}
          </div>
        );
      })}
      {speakersEdited && (
        <button
          type="button"
          className="bnc-diar-reset"
          onClick={() => {
            setSpeakerNames({});
            setSpeakerMerge({});
          }}
        >
          <RotateCcw size={12} />
          {t("bancada.resetSpeakers")}
        </button>
      )}
    </div>
  ) : null;

  // Palabras por minuto de la sesión: palabras dictadas ÷ minutos grabando.
  // Real y honesto (sin números inventados); el idle muestra el último promedio.
  //
  // SE MIDE CUANDO HAY ALGO QUE MEDIR, no solo mientras se graba. La versión
  // anterior solo calculaba con `grabando` en verdadero, y eso ataba el medidor
  // al texto PARCIAL del streaming: con un modelo que no transmite en vivo
  // —Cohere, Whisper, cualquiera menos Nemotron— no hay parcial, así que las
  // palabras de la sesión valían 0 todo el rato, el medidor marcaba 0, y al
  // soltar congelaba ese 0 sin volver a mirarlo jamás. El tacómetro se quedaba
  // clavado en cero para siempre.
  //
  // Encontrado el 30/07: «Audio del sistema» cambia el modelo solo, puso Cohere,
  // y las palabras por minuto dejaron de aparecer sin que nada lo explicara.
  //
  // Ahora la condición es la que importa —hay palabras y hay tiempo— y da igual
  // de dónde venga el texto: del streaming mientras se dicta, o del final al
  // soltar. Con Nemotron el número sube en vivo, como antes; con los demás
  // aparece al terminar. En los dos casos son palabras reales entre minutos
  // reales, que es lo único que este medidor promete.
  const sesWords = Math.max(0, countWords(textoVivo) - wordsBaseRef.current);
  if (seg >= 2 && sesWords > 0) {
    wpmShownRef.current = Math.round(sesWords / (seg / 60));
  }
  const wpm = wpmShownRef.current;
  wpmRef.current = wpm;

  // Secciones de SETUP: reusa SECTIONS_CONFIG (misma lógica enabled que el
  // sidebar clásico). Historial vive en su propia pestaña BITÁCORA.
  const setupSections = Object.entries(SECTIONS_CONFIG)
    .filter(([id, c]) => id !== "history" && c.enabled(settings))
    .map(([id, c]) => ({ id: id as SidebarSection, ...c }));
  const activeSection = setupSections.some((s) => s.id === section)
    ? section
    : "general";
  const ActiveComp = SECTIONS_CONFIG[activeSection].component;

  const tablero = (
    <div className="bnc-hud">
      {/* barra de ritmo (reactiva a la voz) */}
      <ShiftLights ref={shiftRef} className="bnc-ritmo" />

      <div className="bnc-hud-main">
        {/* columna izquierda: medidor PAL/MIN + panel de hablantes debajo */}
        <div className="bnc-hud-left">
          <div className="bnc-gauge">
            <canvas
              ref={canvasRef}
              className="bnc-taco-cv"
              aria-hidden="true"
            />
          </div>
          {panelHablantes}
        </div>

        {/* panel de transcripción en vivo */}
        <div className="bnc-live">
          <div className="bnc-live-head">
            <span className={`bnc-status s-${estado}`}>
              <span className="bnc-status-dot" aria-hidden="true" />
              {t(`bancada.estado.${ESTADO_KEY[estado]}`)}
            </span>
            <div className="bnc-live-actions">
              <span className="bnc-live-clock">{reloj}</span>
              {grabando ? (
                <button
                  type="button"
                  className="bnc-iconbtn"
                  onClick={cancelar}
                  aria-label={t("bancada.btn.cancel")}
                  title={t("bancada.btn.cancel")}
                >
                  <X size={15} />
                </button>
              ) : null}
              <button
                type="button"
                className="bnc-iconbtn"
                onClick={copiar}
                disabled={!dictado.trim()}
                aria-label={t("bancada.btn.copy")}
                title={t("bancada.btn.copy")}
              >
                <Copy size={14} />
              </button>
              <button
                type="button"
                className="bnc-iconbtn"
                onClick={limpiar}
                disabled={!dictado && !parcial}
                aria-label={t("bancada.btn.clear")}
                title={t("bancada.btn.clear")}
              >
                <Trash2 size={14} />
              </button>
            </div>
          </div>
          <div className="bnc-live-body">
            {diarSegs ? (
              <div className="bnc-diar-doc">
                {diarSegs.map((s, i) => {
                  const eff = effSpeaker(s.speaker);
                  const color = SPK_COLORS[(eff - 1) % SPK_COLORS.length];
                  return (
                    <div key={i} className="bnc-diar-seg">
                      {s.time && (
                        <span className="bnc-diar-time">{s.time}</span>
                      )}
                      <span
                        className="bnc-spk"
                        style={{ color, borderColor: color }}
                      >
                        {nameOf(eff)}
                      </span>
                      <span className="bnc-diar-text">{s.text}</span>
                    </div>
                  );
                })}
              </div>
            ) : textoVivo ? (
              <span className="bnc-live-txt">
                {renderTranscript(textoVivo, t)}
                {grabando ? <span className="bnc-caret" /> : null}
              </span>
            ) : (
              <span className="bnc-live-ph">
                {grabando ? t("bancada.lcdListening") : t("bancada.lcdReady")}
              </span>
            )}
          </div>
        </div>
      </div>

      {/* botón de dictado */}
      <div className="bnc-hud-cta">
        <BotonDictar
          grabando={grabando}
          onToggle={dictar}
          label={grabando ? t("bancada.stopping") : t("bancada.dictate")}
          aria={grabando ? t("bancada.stop") : t("bancada.start")}
          hint={atajo || undefined}
        />
      </div>

      <AtajoVox className="bnc-vox" />
      {/* chips de ajustes (funcionales, con rótulo) */}
      <div className="bnc-chips">
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
          onClick={() => updateSetting("vad_enabled", !settings?.vad_enabled)}
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
        {/* OCULTOS (30/07) — «Hablantes» prometía separar voces y NO podía
            hacerlo: la diarización necesita dos modelos ONNX propios
            (`seg.onnx` y `emb.onnx`, independientes del de transcripción) que
            NO se empaquetan, NO se descargan desde la app y no están en el
            equipo de nadie. `diarize_and_label` intentaba cargarlos, fallaba,
            escribía un `warn` y devolvía `None` EN SILENCIO: el usuario pulsaba
            el chip, dictaba, y no pasaba nada. Verificado el 30/07 sobre un
            equipo real: la carpeta `<datadir>/diarization` no existe.
            Y hay un segundo cerrojo: la función vive tras la variable de
            entorno `ABRAX_DIARIZE`, vacía en cualquier instalación normal.

            Un control que miente es peor que no tenerlo — mismo criterio que
            «Carpeta de modelos». El chip del número de hablantes cuelga de este,
            así que se va con él.

            El código Rust, los comandos IPC y las traducciones quedan INTACTOS:
            reactivar es descomentar esto. La función completa (descargar los
            ONNX, poder borrarlos y quitar el cerrojo) está en IDEAS.md como
            trabajo post-entrega — el spike la estimó en ~8 días y esa inferencia
            todavía no ha corrido ni una vez. */}
        <Chip
          icon={Palette}
          label={t("bancada.dial.palette")}
          value={PALETA_LABEL[paletaActual]}
          onClick={ciclarPaleta}
        />
      </div>
    </div>
  );

  return (
    <div id="bancada-stage" className="select-none">
      {/* barra superior: arrastre + estado + controles de ventana */}
      <div className="bnc-topbar" data-tauri-drag-region>
        <AbraxLogo variant="horizontal" width={104} />
        <span className="bnc-top-status" data-tauri-drag-region>
          <ShieldCheck size={12} aria-hidden="true" />
          {t("bancada.local")}
        </span>
        <div className="bnc-winctl">
          {settings?.show_tray_icon !== false && (
            <button
              type="button"
              aria-label={t("bancada.tray")}
              title={t("bancada.tray")}
              onClick={() => void win.hide()}
            >
              <ArrowDownToLine size={14} />
            </button>
          )}
          <button
            type="button"
            aria-label={t("bancada.min")}
            title={t("bancada.min")}
            onClick={() => void win.minimize()}
          >
            <Minus size={15} />
          </button>
          <button
            type="button"
            className="bnc-close"
            aria-label={t("bancada.close")}
            title={t("bancada.close")}
            onClick={() => cerrarDesdeShell(win)}
          >
            <X size={15} />
          </button>
        </div>
      </div>

      {/* navegación TABLERO / BITÁCORA / SETUP */}
      <nav className="bnc-nav">
        {VIEWS.map(([id, label, Icon]) => (
          <button
            key={id}
            type="button"
            className={`bnc-navtab${view === id ? " on" : ""}`}
            onClick={() => setView(id)}
            aria-current={view === id}
          >
            <Icon size={14} aria-hidden="true" />
            <span>{t(label)}</span>
          </button>
        ))}
      </nav>

      <p className="sr-only" role="status" aria-live="polite">
        {t(`bancada.estado.${ESTADO_KEY[estado]}`)}
      </p>

      {view === "tablero" ? (
        tablero
      ) : view === "bitacora" ? (
        <div className="bnc-embed">
          <HistorySettings />
        </div>
      ) : (
        <div className="bnc-embed">
          <div className="bnc-appearance">
            <div className="bnc-appearance-hd">{t("bancada.appearance")}</div>
            <div className="bnc-appearance-row">
              <ShellSelector descriptionMode="tooltip" />
              <PaletteSelector descriptionMode="tooltip" />
              <ThemeSelector descriptionMode="tooltip" />
            </div>
          </div>
          <div className="bnc-setup-nav">
            {setupSections.map((s) => {
              const Icon = s.icon;
              return (
                <button
                  key={s.id}
                  type="button"
                  className={`bnc-subtab${activeSection === s.id ? " on" : ""}`}
                  onClick={() => setSection(s.id)}
                  aria-current={activeSection === s.id}
                >
                  <Icon size={13} aria-hidden="true" />
                  <span>{t(s.labelKey)}</span>
                </button>
              );
            })}
          </div>
          <div className="bnc-setup-body">
            <ActiveComp />
          </div>
        </div>
      )}
    </div>
  );
};

BancadaShell.displayName = "BancadaShell";

export default BancadaShell;
