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

// ── ONDA REACTIVA ───────────────────────────────────────────────────────────
// Una SEÑAL EN EL TIEMPO, no barras y tampoco un espectro. El upstream (Handy)
// usa barras de ecualizador, que es el cliché de cualquier grabadora.
//
// LA PRIMERA VERSIÓN ESTABA MAL Y SE VEÍA: mapeaba el eje horizontal a las
// FRECUENCIAS (graves a la izquierda, agudos a la derecha, hasta 8 kHz). La voz
// casi no tiene energía por encima de 4 kHz, así que la mitad derecha quedaba
// plana SIEMPRE, hablaras o no, y la línea no se movía: cada punto era una
// frecuencia fija, no un instante.
//
// Ahora el eje horizontal es el TIEMPO. Cada trama de espectro aporta UN nivel,
// que entra por la derecha y empuja la historia hacia la izquierda, como un
// osciloscopio. La forma viaja y responde en toda su longitud.
//
// La forma la comparten todas las variantes de la píldora: un solo lenguaje
// visual.

/** Muestras de la envolvente a lo ancho: historia reciente del nivel de voz. */
const WAVE_PUNTOS = 24;
/**
 * Bandas que se promedian para sacar UN nivel por trama. Solo el rango de voz:
 * las más bajas llevan retumbe de la sala y las más altas están casi siempre
 * vacías —promediarlas hundía el nivel a la mitad, que es parte de por qué la
 * versión anterior se veía apagada.
 */
const NIVEL_BANDA_INI = 2;
const NIVEL_BANDA_FIN = 22;

/** Lienzo de la onda, en unidades de viewBox (y también su tamaño en píxeles). */
const ONDA_ANCHO = 60;
const ONDA_ALTO = 18;
/** Grosor del trazo; se descuenta de la amplitud para que no se recorte. */
const ONDA_TRAZO = 2;
/** Oscilaciones visibles a lo ancho. Pocas: muchas se leen como ruido. */
const ONDA_CICLOS = 2.6;
/**
 * Velocidad de viaje, en ciclos por segundo. Se calcula con reloj de pared y no
 * por evento recibido: así la onda viaja igual de rápido con cualquier ritmo de
 * emisión del backend, que depende del tamaño de trama del micrófono.
 */
const ONDA_VEL = 0.9;

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

/**
 * Ruta de la onda: portador sinusoidal viajero modulado por la envolvente.
 *
 * @param envolvente historia reciente del nivel de voz, 0..1, la más nueva al
 *   final (entra por la derecha).
 * @param fase desplazamiento del portador en VUELTAS (0..1). Congelarla en un
 *   valor fijo detiene el viaje sin tocar la amplitud.
 */
const rutaOnda = (envolvente: number[], fase: number): string => {
  const n = envolvente.length;
  if (n < 2) return "";
  const centro = ONDA_ALTO / 2;
  const ampMax = ONDA_ALTO / 2 - ONDA_TRAZO / 2;

  return rutaSuave(
    envolvente.map((v, i) => {
      const t = i / (n - 1);
      // `pow(v, 0.7)` comprime el rango: los niveles bajos del habla normal se
      // ven, en vez de quedar pegados a la línea central.
      const amp = Math.pow(Math.max(0, Math.min(1, v)), 0.7) * ampMax;
      const portador = Math.sin(2 * Math.PI * (t * ONDA_CICLOS + fase));
      return { x: t * ONDA_ANCHO, y: centro - amp * portador };
    }),
  );
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

  // Fase del portador, en vueltas (0..1). Avanza con reloj de pared para que la
  // onda viaje a velocidad constante sea cual sea el ritmo de emisión.
  const [fase, setFase] = useState(0);
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

          // UN nivel por trama, promediando solo el rango de voz.
          let suma = 0;
          let n = 0;
          for (let b = NIVEL_BANDA_INI; b < NIVEL_BANDA_FIN; b++) {
            suma += bands[b] ?? 0;
            n++;
          }
          const crudo = n > 0 ? suma / n : 0;

          // Ataque rápido, caída lenta: así un golpe de voz se ve en el acto y
          // la onda no parpadea entre sílabas. Simétrico se veía nervioso.
          const previo = smoothedLevelsRef.current;
          const ultimo = previo[previo.length - 1] ?? 0;
          const nivel =
            crudo > ultimo
              ? ultimo * 0.4 + crudo * 0.6
              : ultimo * 0.82 + crudo * 0.18;

          // La historia se desplaza: lo nuevo entra por la DERECHA y empuja el
          // resto a la izquierda, como un osciloscopio. Esto es lo que hace que
          // se mueva toda la línea y no solo un extremo.
          const historia = [...previo.slice(1), nivel];
          smoothedLevelsRef.current = historia;
          setLevels(historia);

          // Fase del portador por reloj de pared, no por evento: la velocidad de
          // viaje no debe depender del ritmo de emisión del backend.
          setFase(((performance.now() / 1000) * ONDA_VEL) % 1);
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
  // PORTADOR × ENVOLVENTE, que es lo que hace que se mueva TODA la línea:
  //
  //   · el portador es una sinusoide de `ONDA_CICLOS` oscilaciones a lo ancho,
  //     cuya fase avanza con el reloj → la forma VIAJA;
  //   · la envolvente es `levels`, la historia reciente del nivel de voz, que se
  //     desplaza sola al entrar cada trama nueva por la derecha.
  //
  // Así cada punto de la línea responde al audio, no solo un extremo, y la onda
  // se lee como señal y no como un lomo. En silencio la envolvente vale 0 en
  // todas partes y queda una línea recta pase lo que pase con la fase: «no oigo
  // nada» se ve sin leer nada, y no hay animación gratuita.
  //
  // Con `prefers-reduced-motion` se congela el viaje (la fase queda fija) pero la
  // amplitud sigue respondiendo: el único movimiento que queda es el que provoca
  // tu propia voz. Anular también la amplitud dejaba una línea muerta que no
  // comunicaba nada.
  const waveform = (
    <div className="swave">
      <svg
        width={ONDA_ANCHO}
        height={ONDA_ALTO}
        viewBox={`0 0 ${ONDA_ANCHO} ${ONDA_ALTO}`}
        aria-hidden="true"
      >
        <path d={rutaOnda(levels, reducirMovimiento ? 0 : fase)} />
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
