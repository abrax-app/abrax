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
import {
  Mic,
  Square,
  SlidersHorizontal,
  ListMusic,
  Wand2,
  Minus,
  X,
  Menu as MenuIcon,
} from "lucide-react";
import AbraxGlyph from "../icons/AbraxGlyph";
import { SECTIONS_CONFIG, type SidebarSection } from "../Sidebar";
import { useSettings } from "@/hooks/useSettings";
import { commands, type HistoryEntry } from "@/bindings";
import { applyShell } from "@/lib/utils/theme";
import { montarEspectro, type EspectroHandle } from "./retroSpectrum";
import "./retro.css";

type WinId = "main" | "ajustes" | "historial" | "seccion";
type WinState = {
  x: number;
  y: number;
  z: number;
  visible: boolean;
  folded: boolean;
};

type SpectrumPayload = {
  bands: number[];
  rms: number;
  bass: number;
  dominant: number;
};

const INITIAL: Record<WinId, WinState> = {
  main: { x: 48, y: 40, z: 40, visible: true, folded: false },
  ajustes: { x: 48, y: 300, z: 30, visible: false, folded: false },
  historial: { x: 540, y: 40, z: 35, visible: true, folded: false },
  seccion: { x: 210, y: 96, z: 20, visible: false, folded: false },
};

export const RetroShell: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();

  const [wins, setWins] = useState<Record<WinId, WinState>>(INITIAL);
  const zTop = useRef(50);
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ x: number; y: number }>({
    x: 60,
    y: 70,
  });
  const [seccion, setSeccion] = useState<SidebarSection>("general");
  const [dictEnabled, setDictEnabled] = useState<boolean>(false);

  // ── reloj de dictado ──
  const [grabando, setGrabando] = useState(false);
  const [seg, setSeg] = useState(0);
  const grabandoRef = useRef(false);

  // ── historial-playlist ──
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [selEntry, setSelEntry] = useState<number | null>(null);

  // ── espectro (R8) ──
  const cvRef = useRef<HTMLCanvasElement>(null);
  const esp = useRef<EspectroHandle | null>(null);
  const subscribed = useRef(false);

  // Motor del espectro: montar una vez.
  useEffect(() => {
    if (!cvRef.current) return;
    esp.current = montarEspectro(cvRef.current);
    return () => {
      esp.current?.destroy();
      esp.current = null;
      if (subscribed.current) {
        void commands.stopSpectrum();
        subscribed.current = false;
      }
    };
  }, []);

  // Escucha del stream R8 (siempre atada; solo llegan frames si estamos suscritos).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    listen<SpectrumPayload>("spectrum", (e) => {
      esp.current?.push(e.payload.bands);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Sondeo del estado de dictado real → reloj + suscripción R8 (solo al grabar,
  // para no escuchar en reposo; privacidad + regla R8).
  useEffect(() => {
    let vivo = true;
    const tick = async () => {
      try {
        const rec = await commands.isRecording();
        if (!vivo || rec === grabandoRef.current) return;
        grabandoRef.current = rec;
        setGrabando(rec);
        esp.current?.setGrabando(rec);
        if (rec) {
          setSeg(0);
          if (!subscribed.current) {
            void commands.startSpectrum();
            subscribed.current = true;
          }
        } else if (subscribed.current) {
          void commands.stopSpectrum();
          subscribed.current = false;
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

  // Contador del reloj mientras se graba.
  useEffect(() => {
    if (!grabando) return;
    const id = setInterval(() => setSeg((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, [grabando]);

  // Historial: carga inicial + actualizaciones en vivo.
  const loadHistory = useCallback(async () => {
    try {
      const r = await commands.getHistoryEntries(null, 30);
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

  // Estado del diccionario (para la lámpara DICC).
  useEffect(() => {
    commands
      .getDictionaryStats()
      .then((s: any) => setDictEnabled(!!s?.enabled))
      .catch(() => {});
  }, []);

  // Re-tinte del espectro al cambiar la paleta.
  useEffect(() => {
    const root = document.documentElement;
    const obs = new MutationObserver(() => esp.current?.setPalette());
    obs.observe(root, { attributes: true, attributeFilter: ["data-ui-theme"] });
    return () => obs.disconnect();
  }, []);

  // ── gestión de ventanas ──
  const bringFront = (id: WinId) =>
    setWins((w) => ({ ...w, [id]: { ...w[id], z: ++zTop.current } }));
  const toggleWin = (id: WinId) =>
    setWins((w) => ({
      ...w,
      [id]: { ...w[id], visible: !w[id].visible, z: ++zTop.current },
    }));
  const openWin = (id: WinId) =>
    setWins((w) => ({
      ...w,
      [id]: { ...w[id], visible: true, folded: false, z: ++zTop.current },
    }));
  const closeWin = (id: WinId) =>
    setWins((w) => ({ ...w, [id]: { ...w[id], visible: false } }));
  const foldWin = (id: WinId) =>
    setWins((w) => ({ ...w, [id]: { ...w[id], folded: !w[id].folded } }));

  // Arrastre.
  const drag = useRef<{ id: WinId; dx: number; dy: number } | null>(null);
  const onBarPointerDown = (id: WinId) => (e: React.PointerEvent) => {
    if ((e.target as HTMLElement).closest(".rbtn")) return;
    bringFront(id);
    drag.current = {
      id,
      dx: e.clientX - wins[id].x,
      dy: e.clientY - wins[id].y,
    };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onBarPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const x = Math.max(0, e.clientX - d.dx);
    const y = Math.max(0, e.clientY - d.dy);
    setWins((w) => ({ ...w, [d.id]: { ...w[d.id], x, y } }));
  };
  const onBarPointerUp = () => {
    drag.current = null;
  };

  // ── acciones ──
  const dictar = () => {
    if (!grabandoRef.current) void commands.triggerTranscription();
  };
  const detener = () => {
    if (grabandoRef.current) void commands.triggerTranscription();
  };
  const copiar = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(t("retro.copied"));
    } catch {
      // ignorar
    }
  };
  const toggleDict = async () => {
    const next = !dictEnabled;
    setDictEnabled(next);
    try {
      await commands.setDictionaryEnabled(next);
    } catch {
      setDictEnabled(!next);
    }
  };
  // Muletillas: null = filtro por defecto del idioma (ON); [] = desactivado (OFF).
  const fillerOn = useMemo(() => {
    const f = settings?.custom_filler_words;
    return f === null || f === undefined || f.length > 0;
  }, [settings?.custom_filler_words]);
  const toggleFiller = () =>
    updateSetting("custom_filler_words", fillerOn ? [] : null);

  const abrirSeccion = (id: SidebarSection) => {
    setSeccion(id);
    openWin("seccion");
    setMenuOpen(false);
  };
  const cambiarShell = (shell: "orbital" | "classic") => {
    applyShell(shell);
    updateSetting("ui_shell", shell);
    setMenuOpen(false);
  };
  const toggleTema = () => {
    const next = settings?.ui_theme === "imperial" ? "abrax" : "imperial";
    // Efecto inmediato + persistencia (mismo patrón del selector de paleta).
    if (next === "imperial") {
      document.documentElement.dataset.uiTheme = "imperial";
      document.documentElement.dataset.theme = "dark";
    } else {
      delete document.documentElement.dataset.uiTheme;
    }
    updateSetting("ui_theme", next);
  };

  const reloj = `${String(Math.floor(seg / 60)).padStart(2, "0")}:${String(
    seg % 60,
  ).padStart(2, "0")}`;

  const availableSections = useMemo(
    () =>
      (Object.entries(SECTIONS_CONFIG) as [SidebarSection, any][])
        .filter(([, c]) => c.enabled(settings))
        .map(([id, c]) => ({ id, labelKey: c.labelKey })),
    [settings],
  );

  const SeccionComp = SECTIONS_CONFIG[seccion].component;
  // Nombre corto del modelo activo (el id suele ser una ruta larga).
  const modelName = (settings?.selected_model ?? "—")
    .split("/")
    .pop()!
    .replace(/\.(gguf|bin|onnx|safetensors)$/i, "");

  const winStyle = (id: WinId): React.CSSProperties => ({
    left: wins[id].x,
    top: wins[id].y,
    zIndex: wins[id].z,
    display: wins[id].visible ? undefined : "none",
  });

  return (
    <div
      id="retro-stage"
      className="select-none"
      onClick={() => setMenuOpen(false)}
    >
      <div className="retro-backdrop" aria-hidden="true" />

      {/* ═══ VENTANA PRINCIPAL ═══ */}
      <section
        className={`rvent rvent-main${wins.main.folded ? " plegada" : ""}`}
        style={winStyle("main")}
        onPointerDown={() => bringFront("main")}
      >
        <header
          className="rbarra"
          onPointerDown={onBarPointerDown("main")}
          onPointerMove={onBarPointerMove}
          onPointerUp={onBarPointerUp}
          onDoubleClick={() => foldWin("main")}
        >
          <button
            type="button"
            className="rbtn riso"
            aria-label={t("retro.menu")}
            onClick={(e) => {
              e.stopPropagation();
              const r = (
                e.currentTarget as HTMLElement
              ).getBoundingClientRect();
              setMenuPos({ x: r.left, y: r.bottom + 4 });
              setMenuOpen((o) => !o);
            }}
          >
            <AbraxGlyph width={14} height={14} />
          </button>
          <b>{t("retro.titleMain")}</b>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.fold")}
            onClick={() => foldWin("main")}
          >
            <Minus size={11} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.close")}
            onClick={() => closeWin("main")}
          >
            <X size={11} aria-hidden="true" />
          </button>
        </header>

        <div className="rcuerpo">
          <div className={`rhueco rreloj${grabando ? " on" : ""}`}>
            <span className="rrec" aria-hidden="true" />
            <span>{reloj}</span>
          </div>
          <canvas
            ref={cvRef}
            className="rhueco rspectro"
            width={252}
            height={44}
            aria-hidden="true"
          />
          <div className="rhueco rmarquee">
            <span>{t("retro.marquee")}</span>
          </div>

          <div className="rlamparas">
            <span className="rlamp on" title={t("retro.lamp.model")}>
              {modelName}
            </span>
            <span className="rlamp on">{t("retro.lamp.local")}</span>
            <button
              type="button"
              className={`rlamp mg${fillerOn ? " on" : ""}`}
              onClick={toggleFiller}
              aria-pressed={fillerOn}
            >
              {t("retro.lamp.filler")}
            </button>
            <button
              type="button"
              className={`rlamp${dictEnabled ? " on" : ""}`}
              onClick={toggleDict}
              aria-pressed={dictEnabled}
            >
              {t("retro.lamp.dict")}
            </button>
          </div>

          <div className="rtransporte">
            <button
              type="button"
              className="rtbtn rrec-btn"
              onClick={dictar}
              aria-label={t("retro.dictate")}
              title={t("retro.dictate")}
            >
              <Mic size={13} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="rtbtn"
              onClick={detener}
              aria-label={t("retro.stop")}
              title={t("retro.stop")}
            >
              <Square size={12} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="rtbtn"
              onClick={() => toggleWin("ajustes")}
              aria-label={t("retro.settings")}
              title={t("retro.settings")}
            >
              <SlidersHorizontal size={13} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="rtbtn"
              onClick={() => toggleWin("historial")}
              aria-label={t("retro.history")}
              title={t("retro.history")}
            >
              <ListMusic size={13} aria-hidden="true" />
            </button>
            <button
              type="button"
              className="rtbtn"
              onClick={() => abrirSeccion("history")}
              aria-label={t("retro.compiler")}
              title={t("retro.compiler")}
            >
              <Wand2 size={13} aria-hidden="true" />
            </button>
          </div>
        </div>
      </section>

      {/* ═══ VENTANA AJUSTES ═══ */}
      <section
        className={`rvent rvent-ajustes${wins.ajustes.folded ? " plegada" : ""}`}
        style={winStyle("ajustes")}
        onPointerDown={() => bringFront("ajustes")}
      >
        <header
          className="rbarra"
          onPointerDown={onBarPointerDown("ajustes")}
          onPointerMove={onBarPointerMove}
          onPointerUp={onBarPointerUp}
          onDoubleClick={() => foldWin("ajustes")}
        >
          <SlidersHorizontal
            size={12}
            aria-hidden="true"
            className="riso-static"
          />
          <b>{t("retro.titleSettings")}</b>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.fold")}
            onClick={() => foldWin("ajustes")}
          >
            <Minus size={11} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.close")}
            onClick={() => closeWin("ajustes")}
          >
            <X size={11} aria-hidden="true" />
          </button>
        </header>
        <div className="rcuerpo">
          <div className="rfila-onoff">
            <button
              type="button"
              className={`rlamp${settings?.push_to_talk ? " on" : ""}`}
              onClick={() =>
                updateSetting("push_to_talk", !settings?.push_to_talk)
              }
              aria-pressed={!!settings?.push_to_talk}
            >
              {t("retro.lamp.ptt")}
            </button>
            <button
              type="button"
              className={`rlamp${settings?.vad_enabled ? " on" : ""}`}
              onClick={() =>
                updateSetting("vad_enabled", !settings?.vad_enabled)
              }
              aria-pressed={!!settings?.vad_enabled}
            >
              {t("retro.lamp.vad")}
            </button>
            <button
              type="button"
              className={`rlamp${settings?.translate_to_english ? " on" : ""}`}
              onClick={() =>
                updateSetting(
                  "translate_to_english",
                  !settings?.translate_to_english,
                )
              }
              aria-pressed={!!settings?.translate_to_english}
            >
              {t("retro.lamp.translate")}
            </button>
            <button
              type="button"
              className={`rlamp${settings?.overlay_style === "esfera" ? " on" : ""}`}
              onClick={() =>
                updateSetting(
                  "overlay_style",
                  settings?.overlay_style === "esfera" ? "minimal" : "esfera",
                )
              }
              aria-pressed={settings?.overlay_style === "esfera"}
            >
              {t("retro.lamp.sphere")}
            </button>
            <button
              type="button"
              className="rmini"
              onClick={(e) => {
                e.stopPropagation();
                const r = (
                  e.currentTarget as HTMLElement
                ).getBoundingClientRect();
                setMenuPos({ x: r.left, y: r.bottom + 4 });
                setMenuOpen(true);
              }}
            >
              {t("retro.sections")}
            </button>
          </div>
          <div className="req rhueco">
            {[
              {
                key: "extra_recording_buffer_ms",
                label: t("retro.slider.buffer"),
                min: 0,
                max: 500,
                val: settings?.extra_recording_buffer_ms ?? 0,
              },
              {
                key: "audio_feedback_volume",
                label: t("retro.slider.volume"),
                min: 0,
                max: 100,
                val: Math.round((settings?.audio_feedback_volume ?? 0.5) * 100),
              },
              {
                key: "word_correction_threshold",
                label: t("retro.slider.threshold"),
                min: 0,
                max: 100,
                val: Math.round(
                  (settings?.word_correction_threshold ?? 0.18) * 100,
                ),
              },
            ].map((s) => (
              <div className="rbanda" key={s.key}>
                <input
                  type="range"
                  min={s.min}
                  max={s.max}
                  defaultValue={s.val}
                  aria-label={s.label}
                  onChange={(e) => {
                    const v = Number(e.target.value);
                    if (s.key === "extra_recording_buffer_ms")
                      updateSetting("extra_recording_buffer_ms", v);
                    else if (s.key === "audio_feedback_volume")
                      updateSetting("audio_feedback_volume", v / 100);
                    else if (s.key === "word_correction_threshold")
                      updateSetting("word_correction_threshold", v / 100);
                  }}
                />
                <span>{s.label}</span>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* ═══ VENTANA HISTORIAL (playlist) ═══ */}
      <section
        className={`rvent rvent-hist${wins.historial.folded ? " plegada" : ""}`}
        style={winStyle("historial")}
        onPointerDown={() => bringFront("historial")}
      >
        <header
          className="rbarra"
          onPointerDown={onBarPointerDown("historial")}
          onPointerMove={onBarPointerMove}
          onPointerUp={onBarPointerUp}
          onDoubleClick={() => foldWin("historial")}
        >
          <ListMusic size={12} aria-hidden="true" className="riso-static" />
          <b>{t("retro.titleHistory")}</b>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.fold")}
            onClick={() => foldWin("historial")}
          >
            <Minus size={11} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.close")}
            onClick={() => closeWin("historial")}
          >
            <X size={11} aria-hidden="true" />
          </button>
        </header>
        <div className="rlista rhueco">
          {entries.length === 0 ? (
            <div className="rvacio">{t("retro.emptyHistory")}</div>
          ) : (
            entries.map((en, i) => (
              <div
                key={en.id}
                className={`rpista${selEntry === en.id ? " sel" : ""}`}
                onClick={() => setSelEntry(en.id)}
                onDoubleClick={() => copiar(en.transcription_text)}
                title={t("retro.copyHint")}
              >
                <span className="rnum">{i + 1}.</span>
                <span className="rtxt">
                  {en.title || en.transcription_text}
                </span>
              </div>
            ))
          )}
        </div>
        <div className="rbotonera">
          <button
            type="button"
            className="rmini"
            disabled={selEntry === null}
            onClick={() => {
              const e = entries.find((x) => x.id === selEntry);
              if (e) void copiar(e.transcription_text);
            }}
          >
            {t("retro.copy")}
          </button>
          <button
            type="button"
            className="rmini"
            disabled={selEntry === null}
            onClick={() =>
              selEntry !== null &&
              void commands.toggleHistoryEntrySaved(selEntry)
            }
          >
            {t("retro.save")}
          </button>
          <button
            type="button"
            className="rmini"
            disabled={selEntry === null}
            onClick={async () => {
              if (selEntry !== null) {
                await commands.deleteHistoryEntry(selEntry);
                setSelEntry(null);
              }
            }}
          >
            {t("retro.delete")}
          </button>
          <button
            type="button"
            className="rmini"
            onClick={() => abrirSeccion("history")}
          >
            {t("retro.compile")}
          </button>
          <span className="rcontador">
            {t("retro.count", { n: entries.length })}
          </span>
        </div>
      </section>

      {/* ═══ VENTANA SECCIÓN (hospeda una sección real desde el menú) ═══ */}
      <section
        className={`rvent rvent-seccion${wins.seccion.folded ? " plegada" : ""}`}
        style={winStyle("seccion")}
        onPointerDown={() => bringFront("seccion")}
      >
        <header
          className="rbarra"
          onPointerDown={onBarPointerDown("seccion")}
          onPointerMove={onBarPointerMove}
          onPointerUp={onBarPointerUp}
          onDoubleClick={() => foldWin("seccion")}
        >
          <MenuIcon size={12} aria-hidden="true" className="riso-static" />
          <b>{t(SECTIONS_CONFIG[seccion].labelKey)}</b>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.fold")}
            onClick={() => foldWin("seccion")}
          >
            <Minus size={11} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="rbtn"
            aria-label={t("retro.close")}
            onClick={() => closeWin("seccion")}
          >
            <X size={11} aria-hidden="true" />
          </button>
        </header>
        <div className="rseccion-body">
          <SeccionComp />
        </div>
      </section>

      {/* ═══ menú clásico ═══ */}
      {menuOpen && (
        <div
          className="rmenu"
          style={{ left: menuPos.x, top: menuPos.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <div className="rmenu-tit">{t("retro.menuSections")}</div>
          {availableSections.map((s) => (
            <button
              type="button"
              key={s.id}
              className="rmenu-it"
              onClick={() => abrirSeccion(s.id)}
            >
              {t(s.labelKey)}
            </button>
          ))}
          <hr />
          <div className="rmenu-tit">{t("retro.menuWindows")}</div>
          <button
            type="button"
            className="rmenu-it"
            onClick={() => toggleWin("main")}
          >
            {t("retro.titleMain")}
          </button>
          <button
            type="button"
            className="rmenu-it"
            onClick={() => toggleWin("ajustes")}
          >
            {t("retro.titleSettings")}
          </button>
          <button
            type="button"
            className="rmenu-it"
            onClick={() => toggleWin("historial")}
          >
            {t("retro.titleHistory")}
          </button>
          <hr />
          <button
            type="button"
            className="rmenu-it"
            onClick={() => cambiarShell("orbital")}
          >
            {t("retro.toOrbital")}
          </button>
          <button
            type="button"
            className="rmenu-it"
            onClick={() => cambiarShell("classic")}
          >
            {t("retro.toClassic")}
          </button>
          <hr />
          <button type="button" className="rmenu-it" onClick={toggleTema}>
            {t("retro.theme")}:{" "}
            {settings?.ui_theme === "imperial" ? "Imperial" : "ABRAX"}
          </button>
        </div>
      )}

      <div className="retro-hint" aria-hidden="true">
        {t("retro.footer")}
      </div>
    </div>
  );
};

RetroShell.displayName = "RetroShell";

export default RetroShell;
