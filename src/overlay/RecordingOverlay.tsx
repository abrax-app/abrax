import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  OverlayStyle,
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import { applyAppearanceToRoot } from "@/lib/utils/theme";
import { getLanguageDirection } from "@/lib/utils/rtl";
import EsferaStage from "./EsferaStage";
import type { EsferaState } from "./esfera/engine";
import type { SpectrumPayload } from "@/lib/types/events";

type OverlayState = "recording" | "streaming" | "transcribing" | "processing";

/** Payload del evento `spectrum` (overlay.rs::emit_spectrum). */
// ── ONDA REACTIVA ───────────────────────────────────────────────────────────
// Una SEÑAL continua, no barras. El upstream (Handy) usa barras de ecualizador,
// que es el cliché de cualquier grabadora; una onda que ondula con la voz se
// reconoce de un vistazo y es nuestra. La forma la comparten todas las variantes
// de la píldora: un solo lenguaje visual.
//
// El espectro llega como 32 bandas log de 70–8000 Hz. Cada punto de la onda
// promedia bandas consecutivas del rango de voz; más puntos que las 9 barras de
// antes porque una curva necesita muestras para no verse angulosa.
const WAVE_PUNTOS = 14;
// Primera banda que se muestrea (~130 Hz hacia arriba): las más bajas llevan
// retumbe de la sala, no voz.
const WAVE_FIRST_BAND = 2;
const WAVE_BANDS_PER_PUNTO = 2;

/** Lienzo de la onda, en unidades de viewBox (y también su tamaño en píxeles). */
const ONDA_ANCHO = 60;
const ONDA_ALTO = 18;
/** Grosor del trazo; se descuenta de la amplitud para que no se recorte. */
const ONDA_TRAZO = 2;

type Punto = { x: number; y: number };

/**
 * Ruta SVG suave que pasa POR los puntos (spline de Catmull-Rom convertido a
 * bezieres cúbicas). Sin suavizado la onda sería una línea quebrada, que es el
 * aspecto anguloso que queremos evitar al dejar las barras.
 *
 * Tiene que pasar por los puntos, no cerca. La versión obvia —bezieres
 * cuadráticas ancladas en los puntos medios— aquí se comporta pésimo: como la
 * onda alterna el signo, TODOS los puntos medios caen justo en el centro y la
 * curva resultante solo alcanza la mitad de la amplitud calculada. Se vería
 * apagada y usaría medio lienzo.
 */
const rutaSuave = (puntos: Punto[]): string => {
  if (puntos.length < 2) return "";
  const en = (i: number): Punto =>
    puntos[Math.max(0, Math.min(puntos.length - 1, i))];
  const n2 = (v: number) => v.toFixed(2);

  let d = `M ${n2(puntos[0].x)} ${n2(puntos[0].y)}`;
  for (let i = 0; i < puntos.length - 1; i++) {
    // Los extremos se repiten a sí mismos: la tangente inicial y final sale de
    // los puntos que hay, sin inventar muestras.
    const p0 = en(i - 1);
    const p1 = en(i);
    const p2 = en(i + 1);
    const p3 = en(i + 2);
    const c1x = p1.x + (p2.x - p0.x) / 6;
    const c1y = p1.y + (p2.y - p0.y) / 6;
    const c2x = p2.x - (p3.x - p1.x) / 6;
    const c2y = p2.y - (p3.y - p1.y) / 6;
    d += ` C ${n2(c1x)} ${n2(c1y)} ${n2(c2x)} ${n2(c2y)} ${n2(p2.x)} ${n2(p2.y)}`;
  }
  return d;
};

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  const [style, setStyle] = useState<OverlayStyle>("minimal");
  const [levels, setLevels] = useState<number[]>(Array(WAVE_PUNTOS).fill(0));
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = useState(0);
  // Overlay placement (top vs bottom of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one.
  const [position, setPosition] = useState<"top" | "bottom">("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = useState(false);

  const smoothedLevelsRef = useRef<number[]>(Array(WAVE_PUNTOS).fill(0));
  // Se lee una vez al montar: la onda es movimiento continuo y hay que poder
  // apagarla. No es un `useState` porque no necesita re-render propio — el de
  // la onda ya llega con cada trama de espectro.
  const reducirMovimiento = useRef(
    typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true,
  ).current;
  // Live-text scroll-back: the text region "sticks" to the newest line while the
  // user is at the bottom; if they scroll up to read history, auto-follow pauses
  // until they scroll back down.
  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const direction = getLanguageDirection(i18n.language);

  useEffect(() => {
    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        await syncLanguageFromSettings();
        // The Live panel flows downward from a top overlay and upward from a
        // bottom one; read the placement so the layout can flip to match. The
        // style decides which visual renders (pill, panel, or esfera).
        try {
          const settings = await commands.getAppSettings();
          if (settings.status === "ok") {
            setPosition(
              settings.data.overlay_position === "top" ? "top" : "bottom",
            );
            setStyle(settings.data.overlay_style ?? "minimal");
            // La paleta activa llega por los mismos tokens CSS que la app:
            // se aplica a la raíz ANTES de volverse visible, para que la
            // esfera (que lee los tokens al montar) nazca ya teñida.
            applyAppearanceToRoot(
              settings.data.theme ?? "system",
              settings.data.ui_theme ?? "abrax",
            );
          }
        } catch {
          // Keep the previous/default placement if settings can't be read.
        }
        const overlayState = event.payload as OverlayState;
        setState(overlayState);
        if (overlayState === "recording" || overlayState === "streaming") {
          setStreamText({ committed: "", tentative: "" });
          setElapsed(0);
        }
        if (overlayState === "streaming") {
          setPhase("listening");
          setWorkKind("transcribing");
          setSession((s) => s + 1); // remount the card fresh for this session
        }
        setIsVisible(true);
        // Subscribe to spectrum frames only while visible (R8): the backend
        // gates FFT + emission on this subscription.
        void commands.startSpectrum();
      });

      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        // Unsubscribe immediately — emission must stop with the overlay.
        void commands.stopSpectrum();
      });

      const unlistenLevel = await listen<SpectrumPayload>(
        "spectrum",
        (event) => {
          const { bands } = event.payload;
          // Cada punto de la onda promedia un grupo de bandas log contiguas del
          // rango de voz; la media exponencial mantiene el movimiento sereno y
          // es la que hace innecesaria una transición CSS sobre `d`.
          const smoothed = smoothedLevelsRef.current.map((prev, i) => {
            const start = WAVE_FIRST_BAND + i * WAVE_BANDS_PER_PUNTO;
            let sum = 0;
            for (let b = start; b < start + WAVE_BANDS_PER_PUNTO; b++) {
              sum += bands[b] ?? 0;
            }
            const target = sum / WAVE_BANDS_PER_PUNTO;
            return prev * 0.7 + target * 0.3;
          });
          smoothedLevelsRef.current = smoothed;
          setLevels(smoothed);
        },
      );

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        setStreamText(event.payload);
      });

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
      });

      return () => {
        unlistenShow();
        unlistenHide();
        unlistenLevel();
        unlistenStream();
        unlistenPhase();
      };
    };

    const setupPromise = setupEventListeners();
    return () => {
      // setupEventListeners resolves to the unlisten cleanup; await it so the
      // 5 listeners are always torn down on unmount. Previously the returned
      // cleanup was dropped, leaking listeners on remount / double-registering
      // under StrictMode in dev (F2).
      setupPromise.then((cleanup) => cleanup?.());
    };
  }, []);

  // Elapsed timer while the Live overlay is visible (the Esfera style also
  // shows it while listening).
  useEffect(() => {
    const wantsTimer =
      state === "streaming" || (style === "esfera" && state === "recording");
    if (!wantsTimer || !isVisible) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, style, isVisible]);

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    // Fade the top edge only once text actually overflows the cap.
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  // ---- Shared building blocks (one visual language for every overlay form) ----
  // La onda oscila a ambos lados de la línea central alternando el signo por
  // punto: el espectro da MAGNITUD por banda, no una señal con signo, así que la
  // alternancia es lo que la hace leerse como onda y no como un lomo. En
  // silencio todos los puntos valen 0 y queda una línea recta — «no oigo nada»
  // se ve sin leer nada.
  //
  // Con `prefers-reduced-motion` la amplitud se anula y la línea queda quieta:
  // el estado lo sigue comunicando el punto de la izquierda.
  const waveform = (
    <div className="swave">
      <svg
        width={ONDA_ANCHO}
        height={ONDA_ALTO}
        viewBox={`0 0 ${ONDA_ANCHO} ${ONDA_ALTO}`}
        aria-hidden="true"
      >
        <path
          d={rutaSuave(
            levels.map((v, i) => {
              const x =
                levels.length > 1
                  ? (i / (levels.length - 1)) * ONDA_ANCHO
                  : ONDA_ANCHO / 2;
              const maxAmp = ONDA_ALTO / 2 - ONDA_TRAZO / 2;
              const amp = reducirMovimiento ? 0 : Math.pow(v, 0.7) * maxAmp;
              return {
                x,
                y: ONDA_ALTO / 2 + (i % 2 === 0 ? -amp : amp),
              };
            }),
          )}
        />
      </svg>
    </div>
  );

  const cancelBtn = (
    <button
      className="sx"
      aria-label="cancel"
      onClick={() => commands.cancelOperation()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );

  // dot (left) | waveform (center) | timer + cancel (right) — same structure for
  // pill & panel, so the Live morph is a pure width change.
  const listeningRow = (showTimer: boolean, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className="sdot" />
      </div>
      {waveform}
      <div className="sbase-r">
        {showTimer && <span className="stimer">{fmtTime(elapsed)}</span>}
        {showCancel && cancelBtn}
      </div>
    </div>
  );

  // spinner (left) | label (center) | cancel (right) — same 3-zone grid as the
  // listening row, so the label is centered.
  const workingRow = (label: string, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className="sspinner" />
      </div>
      <span className="swork-label">{label}</span>
      <div className="sbase-r">{showCancel && cancelBtn}</div>
    </div>
  );

  // ---- Esfera overlay: the audio-reactive sphere on a cosmic stage, with the
  // shared control row underneath (timer + cancel while listening; spinner +
  // label while working). Mounted only while visible — hide disposes the GPU.
  if (style === "esfera") {
    const working = state === "transcribing" || state === "processing";
    const esferaState: EsferaState = working
      ? (state as EsferaState)
      : "recording";
    return (
      <div
        dir={direction}
        className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
      >
        <div className="scard esfera">
          <div className="esfera-stage">
            <EsferaStage state={esferaState} active={isVisible} />
          </div>
          {working ? (
            workingRow(
              state === "processing"
                ? t("overlay.processing")
                : t("overlay.transcribing"),
              true,
            )
          ) : (
            <div className="sbase">
              <div className="sbase-l">
                <span className="sdot" />
              </div>
              <span className="stimer">{fmtTime(elapsed)}</span>
              <div className="sbase-r">{cancelBtn}</div>
            </div>
          )}
        </div>
      </div>
    );
  }

  // ---- Live overlay: a pill that sculpts open into a panel ----
  if (state === "streaming") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const working = phase === "working";
    // Keep the panel open whenever there's text — even while finalizing — so the
    // transcript stays put under a working spinner instead of collapsing and
    // squishing the text mid-stream. Only fall back to the small working pill
    // when there was no text to preserve.
    const open = hasText;
    const collapsed = working && !hasText;

    return (
      <div dir={direction} className={`ov-stage ${position}`}>
        <div
          key={session}
          className={`scard ${open ? "open" : ""} ${collapsed ? "working" : ""} ${
            isVisible ? "" : "leaving"
          }`}
        >
          <div className="stext">
            <div className="stext-clip">
              <div
                className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                ref={capRef}
                onScroll={handleStreamScroll}
              >
                <p>
                  <span className="committed">
                    {streamText.committed ? streamText.committed + " " : ""}
                  </span>
                  <span className="tentative">{streamText.tentative}</span>
                  {/* Drop the blinking caret once finalizing — it's no longer
                      capturing, and a static spinner conveys the work. */}
                  {!working && <span className="scaret" />}
                </p>
              </div>
            </div>
          </div>
          {working
            ? workingRow(
                workKind === "polishing"
                  ? t("overlay.processing")
                  : t("overlay.transcribing"),
                true,
              )
            : listeningRow(open, true)}
        </div>
      </div>
    );
  }

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing). Never both. The pill animates its
  // width between them; the cancel button is in both rows so it stays put.
  const working = state === "transcribing" || state === "processing";
  const workLabel =
    state === "processing"
      ? t("overlay.processing")
      : t("overlay.transcribing");

  return (
    <div
      dir={direction}
      className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
    >
      <div
        className={`scard compact ${working && isVisible ? "cworking" : ""}`}
      >
        {working ? workingRow(workLabel, true) : listeningRow(false, true)}
      </div>
    </div>
  );
};

export default RecordingOverlay;
