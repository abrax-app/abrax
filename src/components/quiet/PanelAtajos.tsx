import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  AudioLines,
  Eraser,
  Headphones,
  LayoutGrid,
  Mic,
  Monitor,
  Palette,
  Radio,
  Smile,
  SpellCheck,
  Volume2,
  type LucideIcon,
} from "lucide-react";
import { commands, type ModelInfo, type VozEscucha } from "@/bindings";
import { events } from "@/bindings";
import type { SpectrumPayload } from "@/lib/types/events";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { confirmarDescarga } from "@/lib/utils/modelDialogs";
import { isLegacyModel } from "../settings/models/ModelsSettings";
import { ShortcutInput } from "../settings/ShortcutInput";
import { CaptureSystemAudio } from "../settings/CaptureSystemAudio";
import { ShellSelector } from "../settings/ShellSelector";
import { PaletteSelector } from "../settings/PaletteSelector";
import { ShowOverlay } from "../settings/ShowOverlay";
import { MicrophoneSelector } from "../settings/MicrophoneSelector";
import { OutputDeviceSelector } from "../settings/OutputDeviceSelector";
import { Dropdown } from "../ui/Dropdown";
import { Chip } from "../bancada/Chip";

/**
 * La pantalla «Atajos»: un panel de control con CUATRO tarjetas —Escucha,
 * Streaming, VOX y General— cada una a lo ancho de la ventana.
 *
 * La idea es que lo que se toca a diario esté en una sola pantalla y sin
 * bucear: la tecla de cada modo, el modelo que usa (y cómo cambiarlo o
 * descargar otro) y los interruptores que de verdad se mueven. Las pantallas de
 * cada modo siguen existiendo en la sidebar con la lista completa; esto es el
 * atajo a lo de siempre, no un sitio nuevo donde vivan ajustes distintos: todos
 * los controles de aquí son LOS MISMOS componentes que usan esas pantallas, así
 * que no puede haber dos verdades.
 */

const contarPalabras = (s: string): number => {
  const limpio = s.trim();
  return limpio ? limpio.split(/\s+/).length : 0;
};

/* ─────────────────────────── piezas de la tarjeta ─────────────────────────── */

const Tarjeta: React.FC<{
  icon: LucideIcon;
  titulo: string;
  children: React.ReactNode;
}> = ({ icon: Icon, titulo, children }) => (
  <section className="space-y-2">
    <div className="px-4 flex items-center gap-2">
      <Icon size={14} className="text-mid-gray" aria-hidden="true" />
      <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
        {titulo}
      </h2>
    </div>
    <div className="bg-background border border-mid-gray/20 rounded-lg overflow-visible">
      <div className="divide-y divide-mid-gray/20">{children}</div>
    </div>
  </section>
);

/** Fila con icono a la izquierda, para los ajustes de apariencia y sonido. */
const FilaIcono: React.FC<{ icon: LucideIcon; children: React.ReactNode }> = ({
  icon: Icon,
  children,
}) => (
  <div className="flex items-start gap-1">
    <div className="ps-4 pt-4 shrink-0">
      <Icon size={15} className="text-mid-gray" aria-hidden="true" />
    </div>
    <div className="flex-1 min-w-0">{children}</div>
  </div>
);

/* ──────────────────────────── selector de modelo ──────────────────────────── */

/**
 * El modelo activo, con un desplegable para cambiarlo.
 *
 * ABRAX tiene UN modelo activo, no uno por modo, así que las dos tarjetas
 * (Escucha y Streaming) muestran el mismo valor: el que está en uso. Lo que
 * cambia entre ellas es lo que OFRECEN — cada una, los modelos que su modo sabe
 * usar— más el activo, que se añade siempre aunque venga del otro grupo: si no,
 * al encender «audio del sistema» (que cambia el modelo solo) esta fila se
 * quedaría vacía y parecería que no hay ninguno.
 *
 * Los que aún no están en el disco se ofrecen igual, marcados: elegirlos pide
 * confirmación, los descarga y, al terminar, los deja activos — que es lo que
 * alguien espera al elegir un modelo de una lista.
 */
const SelectorModelo: React.FC<{ capacidad: "dictado" | "streaming" }> = ({
  capacidad,
}) => {
  const { t } = useTranslation();
  const {
    models,
    currentModel,
    downloadingModels,
    downloadProgress,
    selectModel,
    downloadModel,
  } = useModelStore();
  const [ocupado, setOcupado] = useState(false);

  const candidatos = useMemo(
    () =>
      models.filter((m: ModelInfo) => {
        if (m.id === currentModel) return true;
        if (isLegacyModel(m) && !m.is_downloaded) return false;
        return capacidad === "streaming"
          ? m.supports_streaming
          : !m.supports_streaming;
      }),
    [models, currentModel, capacidad],
  );

  const opciones = candidatos.map((m: ModelInfo) => ({
    value: m.id,
    label: m.is_downloaded
      ? m.name
      : `${m.name} · ${t("quiet.panel.descargar")}`,
  }));

  const descargando = candidatos.find((m) => m.id in downloadingModels);
  const pct = descargando
    ? Math.round(downloadProgress[descargando.id]?.percentage ?? 0)
    : null;

  // El modelo activo se muestra siempre, venga del grupo que venga — pero en la
  // tarjeta de Streaming un modelo que NO transmite no puede quedarse ahí a
  // secas: se leería como «Streaming usa Canary», que es falso. Se dice.
  const activo = models.find((m: ModelInfo) => m.id === currentModel);
  const noTransmite =
    capacidad === "streaming" && !!activo && !activo.supports_streaming;

  const elegir = useCallback(
    async (id: string) => {
      const modelo = models.find((m: ModelInfo) => m.id === id);
      if (!modelo || ocupado) return;
      setOcupado(true);
      try {
        if (modelo.is_downloaded) {
          await selectModel(id);
          return;
        }
        if (!(await confirmarDescarga(modelo, t))) return;
        // Al terminar la descarga se ACTIVA. Elegir un modelo de un desplegable
        // y que quede sin usar sería justo lo contrario de lo que promete.
        if (await downloadModel(id)) await selectModel(id);
      } finally {
        setOcupado(false);
      }
    },
    [models, ocupado, selectModel, downloadModel, t],
  );

  return (
    <div className="px-4 p-2 space-y-1">
      <div className="flex items-center justify-between gap-3 min-h-12">
        <h3 className="text-sm font-medium">{t("quiet.panel.modelo")}</h3>
        <Dropdown
          options={opciones}
          selectedValue={currentModel || null}
          onSelect={(v) => void elegir(v)}
          placeholder={t("quiet.panel.sinModelo")}
          disabled={ocupado || opciones.length === 0}
        />
      </div>
      {noTransmite && !descargando && (
        <p className="text-xs text-amber-500/90">
          {t("quiet.panel.noTransmite")}
        </p>
      )}
      {descargando && (
        <p className="text-xs text-text/60" aria-live="polite">
          {t("quiet.panel.descargando", {
            modelo: descargando.name,
            pct: pct ?? 0,
          })}
        </p>
      )}
    </div>
  );
};

/* ─────────────────────── palabras por minuto + entrada ─────────────────────── */

/**
 * Las dos lecturas en vivo del modo Streaming: a qué ritmo se está escribiendo
 * y si de verdad está entrando audio.
 *
 * Las dos son medidas REALES, no adorno. Las palabras por minuto salen del texto
 * transcrito de la sesión dividido por los segundos que duró —el mismo cálculo
 * del tablero Karting, con su misma trampa evitada: se recalcula también con el
 * texto FINAL, porque solo Nemotron transmite en vivo y con cualquier otro
 * modelo el parcial no existe y el número se quedaría clavado en cero.
 *
 * La barra de entrada se suscribe al espectro SOLO mientras hay captura (R8):
 * mientras no se dicta no hay nada que medir, y en vez de una barra plana que
 * parece rota se dice con palabras que se moverá al dictar.
 */
const MedidoresStreaming: React.FC<{ grabando: boolean }> = ({ grabando }) => {
  const { t } = useTranslation();
  const [nivel, setNivel] = useState(0);
  const [seg, setSeg] = useState(0);
  const [parcial, setParcial] = useState("");
  const [finalTexto, setFinalTexto] = useState("");
  const wpmRef = useRef(0);
  const suscritoRef = useRef(false);
  const suaveRef = useRef(0);

  // Texto de la sesión: el parcial mientras se dicta, el definitivo al soltar.
  useEffect(() => {
    let vivo = true;
    let unText: (() => void) | null = null;
    let unHist: (() => void) | null = null;
    void events.streamTextEvent
      .listen((e) => {
        const { committed, tentative } = e.payload;
        setParcial(`${committed} ${tentative}`.trim());
      })
      .then((fn) => (vivo ? (unText = fn) : fn()));
    void events.historyUpdatePayload
      .listen((e) => {
        if (e.payload.action !== "added") return;
        const txt = e.payload.entry.transcription_text?.trim();
        if (txt) setFinalTexto(txt);
        setParcial("");
      })
      .then((fn) => (vivo ? (unHist = fn) : fn()));
    return () => {
      vivo = false;
      unText?.();
      unHist?.();
    };
  }, []);

  // Cronómetro de la sesión + reinicio de la cuenta al empezar a dictar.
  useEffect(() => {
    if (!grabando) return;
    setSeg(0);
    setParcial("");
    setFinalTexto("");
    const id = setInterval(() => setSeg((s) => s + 1), 1000);
    return () => clearInterval(id);
  }, [grabando]);

  // Espectro: suscripción explícita y solo con captura viva (R8). El `emit_to`
  // del backend va por etiqueta de ventana, así que esta ventana recibe sus
  // propios cuadros sin que el overlay tenga que estar abierto.
  useEffect(() => {
    let vivo = true;
    let un: (() => void) | null = null;
    if (grabando) {
      void commands.startSpectrum();
      suscritoRef.current = true;
      void listen<SpectrumPayload>("spectrum", (e) => {
        const { rms, bass } = e.payload;
        const objetivo = Math.min(1, Math.max(0, rms * 0.7 + bass * 0.5));
        suaveRef.current = suaveRef.current * 0.6 + objetivo * 0.4;
        setNivel(suaveRef.current);
      }).then((fn) => (vivo ? (un = fn) : fn()));
    } else {
      suaveRef.current = 0;
      setNivel(0);
    }
    return () => {
      vivo = false;
      un?.();
      if (suscritoRef.current) {
        void commands.stopSpectrum();
        suscritoRef.current = false;
      }
    };
  }, [grabando]);

  const palabras = contarPalabras(parcial || finalTexto);
  if (seg >= 2 && palabras > 0) {
    wpmRef.current = Math.round(palabras / (seg / 60));
  }
  const wpm = wpmRef.current;

  const estado = !grabando
    ? t("quiet.panel.entradaEspera")
    : nivel > 0.02
      ? t("quiet.panel.entradaSi")
      : t("quiet.panel.entradaSilencio");

  return (
    <>
      <div className="px-4 p-2 flex items-center justify-between gap-3 min-h-12">
        <h3 className="text-sm font-medium">{t("quiet.panel.wpm")}</h3>
        <p className="text-sm tabular-nums" aria-live="off">
          {wpm > 0 ? (
            <>
              <span className="font-semibold">{wpm}</span>{" "}
              <span className="text-text/50 text-xs uppercase">
                {t("bancada.wpmUnit")}
              </span>
            </>
          ) : (
            <span className="text-text/50 text-xs">
              {t("quiet.panel.wpmSinDatos")}
            </span>
          )}
        </p>
      </div>
      <div className="px-4 p-2 space-y-2">
        <div className="flex items-center justify-between gap-3">
          <h3 className="text-sm font-medium flex items-center gap-2">
            <AudioLines
              size={15}
              className="text-mid-gray"
              aria-hidden="true"
            />
            {t("quiet.panel.entrada")}
          </h3>
          {/* El estado va en PALABRAS además de en la barra: una barra de color
              no es una respuesta para quien no la ve moverse. */}
          <p className="text-xs text-text/60" aria-live="polite">
            {estado}
          </p>
        </div>
        <div
          className="q-pa-bar"
          role="img"
          aria-label={`${t("quiet.panel.entrada")}: ${estado}`}
        >
          <i style={{ width: `${Math.round(nivel * 100)}%` }} />
        </div>
      </div>
    </>
  );
};

/* ───────────────────────────────── voz de VOX ─────────────────────────────── */

/** La voz con la que VOX lee. Es «el modelo» de este modo: lo que cambia cómo
 *  suena. Escribe en el MISMO ajuste que el panel VOX (`escucha_voz_prosa`) y
 *  conserva los otros dos valores del comando para no pisarlos. */
const SelectorVoz: React.FC = () => {
  const { t } = useTranslation();
  const { settings, refreshSettings } = useSettings();
  const [voces, setVoces] = useState<VozEscucha[]>([]);
  const [guardando, setGuardando] = useState(false);

  useEffect(() => {
    let vivo = true;
    void commands.escuchaListVoices().then((r) => {
      if (vivo && r.status === "ok") setVoces(r.data);
    });
    return () => {
      vivo = false;
    };
  }, [settings?.tts_selected_engine]);

  const elegir = useCallback(
    async (id: string) => {
      setGuardando(true);
      try {
        await commands.escuchaUpdateSettings(
          id,
          settings?.escucha_voz_codigo ?? null,
          settings?.escucha_verbosidad_simbolos ?? "natural",
        );
        await refreshSettings();
      } finally {
        setGuardando(false);
      }
    },
    [settings, refreshSettings],
  );

  return (
    <div className="px-4 p-2 flex items-center justify-between gap-3 min-h-12">
      <h3 className="text-sm font-medium">{t("quiet.panel.voz")}</h3>
      <Dropdown
        options={voces.map((v) => ({ value: v.id, label: v.nombre }))}
        selectedValue={settings?.escucha_voz_prosa ?? null}
        onSelect={(v) => void elegir(v)}
        placeholder={t("escucha.noVoices")}
        disabled={guardando || voces.length === 0}
      />
    </div>
  );
};

/* ──────────────────────────────── el panel ────────────────────────────────── */

export const PanelAtajos: React.FC<{ grabando: boolean }> = ({ grabando }) => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();

  // `null` NO es «apagado»: es la lista por defecto del idioma, o sea el filtro
  // ACTIVO. Solo una lista vacía lo apaga (`audio_toolkit/text.rs`). Mismo
  // criterio, palabra por palabra, que el chip de la pantalla de inicio.
  const cfw = settings?.custom_filler_words;
  const muletillasOn = cfw == null || cfw.length > 0;

  return (
    <div className="q-panel-atajos">
      <Tarjeta icon={Mic} titulo={t("quiet.nav.listen")}>
        <ShortcutInput
          shortcutId="transcribe"
          descriptionMode="inline"
          grouped
        />
        <SelectorModelo capacidad="dictado" />
        <div className="px-4 p-2">
          <div className="q-pa-chips bnc-chips">
            <Chip
              icon={Eraser}
              label={t("bancada.btn.fillers")}
              active={muletillasOn}
              onClick={() =>
                updateSetting("custom_filler_words", muletillasOn ? [] : null)
              }
            />
            <Chip
              icon={SpellCheck}
              label={t("settings.advanced.autocorreccion.title")}
              active={settings?.autocorreccion_activa ?? true}
              onClick={() =>
                updateSetting(
                  "autocorreccion_activa",
                  !(settings?.autocorreccion_activa ?? true),
                )
              }
            />
            <Chip
              icon={Smile}
              label={t("settings.advanced.emojiDictado.title")}
              active={settings?.emoji_dictado ?? true}
              onClick={() =>
                updateSetting(
                  "emoji_dictado",
                  !(settings?.emoji_dictado ?? true),
                )
              }
            />
          </div>
        </div>
      </Tarjeta>

      <Tarjeta icon={Radio} titulo={t("quiet.nav.streaming")}>
        <CaptureSystemAudio descriptionMode="inline" grouped />
        <SelectorModelo capacidad="streaming" />
        <MedidoresStreaming grabando={grabando} />
      </Tarjeta>

      <Tarjeta icon={Volume2} titulo={t("sidebar.escucha")}>
        <ShortcutInput
          shortcutId="leer_seleccion"
          descriptionMode="inline"
          grouped
        />
        <SelectorVoz />
      </Tarjeta>

      <Tarjeta icon={LayoutGrid} titulo={t("sidebar.general")}>
        <FilaIcono icon={LayoutGrid}>
          <ShellSelector grouped />
        </FilaIcono>
        <FilaIcono icon={Palette}>
          <PaletteSelector grouped />
        </FilaIcono>
        <FilaIcono icon={Monitor}>
          <ShowOverlay grouped />
        </FilaIcono>
        <FilaIcono icon={Mic}>
          <MicrophoneSelector grouped />
        </FilaIcono>
        {/* La salida NO se deshabilita aquí aunque los sonidos estén apagados:
            este ajuste también decide por dónde habla VOX
            (`managers/tts/manager.rs:307`), así que sigue haciendo algo. */}
        <FilaIcono icon={Headphones}>
          <OutputDeviceSelector grouped />
        </FilaIcono>
      </Tarjeta>
    </div>
  );
};

PanelAtajos.displayName = "PanelAtajos";

export default PanelAtajos;
