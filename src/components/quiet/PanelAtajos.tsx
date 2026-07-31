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
  MonitorSpeaker,
  Palette,
  Radio,
  Smile,
  SpellCheck,
  Volume2,
  type LucideIcon,
} from "lucide-react";
import {
  commands,
  type AptitudAudioSistema,
  type ModelInfo,
  type VozEscucha,
} from "@/bindings";
import { events } from "@/bindings";
import type { SpectrumPayload } from "@/lib/types/events";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { confirmarDescarga } from "@/lib/utils/modelDialogs";
import { isLegacyModel } from "../settings/models/ModelsSettings";
import { ShortcutInput } from "../settings/ShortcutInput";
import { ShellSelector } from "../settings/ShellSelector";
import { PaletteSelector } from "../settings/PaletteSelector";
import { ShowOverlay } from "../settings/ShowOverlay";
import { MicrophoneSelector } from "../settings/MicrophoneSelector";
import { OutputDeviceSelector } from "../settings/OutputDeviceSelector";
import { Dropdown } from "../ui/Dropdown";

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

/**
 * Botón de opción tipo barra de Retro: solo el icono, y al pasar el ratón un
 * hint que dice qué hace y cómo está.
 *
 * Los rótulos completos («Autocorrección hablada») ocupaban tres píldoras
 * anchas y dos filas para tres interruptores. Con el icono, la barra entera
 * ocupa una línea y sigue diciéndolo todo — pero solo cuando lo preguntas.
 *
 * El estado NO se fía del color: además del relleno, va en `aria-pressed` (lo
 * que lee un lector de pantalla) y escrito en el hint.
 */
const BotonIcono: React.FC<{
  icon: LucideIcon;
  etiqueta: string;
  ayuda: string;
  activo: boolean;
  onClick: () => void;
  /** Motivo por el que no se puede tocar; sustituye al hint y lo deshabilita. */
  bloqueado?: string;
}> = ({ icon: Icon, etiqueta, ayuda, activo, onClick, bloqueado }) => {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      className={activo && !bloqueado ? "on" : ""}
      onClick={onClick}
      disabled={!!bloqueado}
      aria-pressed={activo}
      aria-label={etiqueta}
      title={
        bloqueado
          ? `${etiqueta}\n${bloqueado}`
          : `${etiqueta} · ${activo ? t("bancada.on") : t("bancada.off")}\n${ayuda}`
      }
    >
      <Icon size={16} aria-hidden="true" />
    </button>
  );
};

/* ────────────────────────────── tarjeta General ────────────────────────────── */

/**
 * Apariencia y sonido en la misma barra de iconos que los interruptores.
 *
 * Aquí no son interruptores sino ELECCIONES (shell, paleta, overlay, micrófono,
 * salida), así que el icono no enciende nada: abre debajo el control de
 * siempre. Un botón que ciclara valores —como hace Retro con la paleta— no
 * sirve para una lista de micrófonos, y además obligaría a reescribir la
 * lógica de cada selector; aquí se muestran LOS MISMOS componentes, intactos.
 *
 * Con la barra cerrada, cinco filas rotuladas pasan a una. El hint no dice solo
 * qué es cada icono: dice el VALOR ACTUAL, que es lo único que se pierde al
 * quitar los rótulos.
 */
const TarjetaGeneral: React.FC = () => {
  const { t } = useTranslation();
  const { settings, getSetting } = useSettings();
  const [abierto, setAbierto] = useState<string | null>(null);

  const dispositivo = (v: unknown) =>
    typeof v === "string" && v && v.toLowerCase() !== "default"
      ? v
      : t("common.systemDefault");

  const items: {
    id: string;
    icon: LucideIcon;
    nombre: string;
    valor: string;
    ayuda: string;
    panel: React.ReactNode;
  }[] = [
    {
      id: "shell",
      icon: LayoutGrid,
      nombre: t("shell.title"),
      valor: t(`shell.options.${settings?.ui_shell ?? "classic"}`),
      ayuda: t("shell.description"),
      panel: <ShellSelector grouped />,
    },
    {
      id: "paleta",
      icon: Palette,
      nombre: t("palette.title"),
      valor: t(`palette.options.${settings?.ui_theme ?? "abrax"}`),
      ayuda: t("palette.description"),
      panel: <PaletteSelector grouped />,
    },
    {
      id: "overlay",
      icon: Monitor,
      nombre: t("settings.advanced.overlay.style.title"),
      valor: t(
        `settings.advanced.overlay.style.options.${settings?.overlay_style ?? "live"}`,
      ),
      ayuda: t("settings.advanced.overlay.style.description"),
      panel: <ShowOverlay grouped />,
    },
    {
      id: "microfono",
      icon: Mic,
      nombre: t("settings.sound.microphone.title"),
      valor: dispositivo(getSetting("selected_microphone")),
      ayuda: t("settings.sound.microphone.description"),
      panel: <MicrophoneSelector grouped />,
    },
    {
      id: "salida",
      icon: Headphones,
      nombre: t("settings.sound.outputDevice.title"),
      valor: dispositivo(getSetting("selected_output_device")),
      ayuda: t("settings.sound.outputDevice.description"),
      panel: <OutputDeviceSelector grouped />,
    },
  ];

  const activo = items.find((i) => i.id === abierto);

  return (
    <Tarjeta icon={LayoutGrid} titulo={t("sidebar.general")}>
      <div className="px-4 p-2">
        <div className="q-pa-tools">
          {items.map((i) => {
            const Icon = i.icon;
            const abiertoEste = abierto === i.id;
            return (
              <button
                key={i.id}
                type="button"
                onClick={() => setAbierto(abiertoEste ? null : i.id)}
                aria-expanded={abiertoEste}
                aria-label={`${i.nombre}: ${i.valor}`}
                title={`${i.nombre} · ${i.valor}\n${i.ayuda}`}
              >
                <Icon size={16} aria-hidden="true" />
              </button>
            );
          })}
        </div>
      </div>
      {activo && (
        <div role="group" aria-label={activo.nombre}>
          {activo.panel}
        </div>
      )}
    </Tarjeta>
  );
};

/* ──────────────────────────── selector de modelo ──────────────────────────── */

/**
 * El modelo activo, con un desplegable para cambiarlo.
 *
 * ABRAX tiene UN modelo activo, no uno por modo, así que el valor mostrado es
 * el mismo en las dos tarjetas: el que está en uso. Lo que cambia es lo que
 * cada una OFRECE.
 *
 * «Sistema» ofrece SOLO los que sirven para audio del sistema, y la lista se le
 * pide al backend (`aptitud_audio_sistema`), que es quien decide. Antes se
 * filtraba por `supports_streaming` —de los cinco, solo Nemotron—: escondía
 * tres modelos perfectamente válidos, y encima colaba el activo aunque no
 * sirviera, así que la fila decía «Canary» en la tarjeta de un modo al que
 * Canary no llega.
 *
 * Los que aún no están en el disco se ofrecen igual, marcados: elegirlos pide
 * confirmación, los descarga y, al terminar, los deja activos — que es lo que
 * alguien espera al elegir un modelo de una lista, y es justo lo que necesita
 * quien todavía no puede encender «Audio del sistema».
 */
const SelectorModelo: React.FC<{
  capacidad: "dictado" | "sistema";
  /** Fragmentos de id que sirven para audio del sistema, según el backend. */
  preferidosSistema?: string[];
}> = ({ capacidad, preferidosSistema }) => {
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

  const sirveParaSistema = useCallback(
    (m: ModelInfo) => (preferidosSistema ?? []).some((p) => m.id.includes(p)),
    [preferidosSistema],
  );

  const candidatos = useMemo(
    () =>
      models.filter((m: ModelInfo) => {
        if (isLegacyModel(m) && !m.is_downloaded) return false;
        // El de dictado sigue siendo «el catálogo menos los de streaming en
        // vivo»; el de sistema, la lista que manda el backend.
        return capacidad === "sistema"
          ? sirveParaSistema(m)
          : !m.supports_streaming || m.id === currentModel;
      }),
    [models, currentModel, capacidad, sirveParaSistema],
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

  const activoFuera =
    capacidad === "sistema" && !candidatos.some((m) => m.id === currentModel);

  // El que se USARÁ si se enciende: el primero de la lista del backend que esté
  // en disco. La lista viene ORDENADA por preferencia justo para esto, y ese
  // orden es la regla entera de `modelo_para_sistema_descargado`.
  //
  // Hay que nombrarlo. Con el activo fuera de la lista, el desplegable enseñaba
  // el hueco del placeholder y el interruptor se dejaba pulsar igual: la
  // pantalla decía «no hay modelo» y encendía. No mentía ninguna de las dos
  // —Abrax cambia solo al que sirve— pero juntas se leían como un fallo, y con
  // razón: lo que faltaba era decir a QUÉ va a cambiar.
  const usara = useMemo(() => {
    if (!activoFuera) return null;
    for (const p of preferidosSistema ?? []) {
      const m = models.find(
        (x: ModelInfo) => x.id.includes(p) && x.is_downloaded,
      );
      if (m) return m;
    }
    return null;
  }, [activoFuera, preferidosSistema, models]);

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
          // En «Sistema» la fila enseña EL MODELO DE ESTE MODO, que no siempre
          // es el activo: si el activo no sirve, es el que Abrax pondrá al
          // encender. Dejarla en blanco era lo que hacía que la pantalla
          // dijera «no hay modelo» mientras el interruptor sí se dejaba pulsar.
          selectedValue={usara?.id ?? currentModel ?? null}
          onSelect={(v) => void elegir(v)}
          // «Sin modelo» sería falso en la tarjeta de sistema: hay uno activo,
          // lo que pasa es que no sirve AQUÍ. Se pide elegir, que es la acción.
          placeholder={t(
            capacidad === "sistema"
              ? "quiet.panel.elegirModelo"
              : "quiet.panel.sinModelo",
          )}
          disabled={ocupado || opciones.length === 0}
        />
      </div>
      {activoFuera && !descargando && (
        <p className="text-xs text-amber-500/90">
          {usara
            ? t("quiet.panel.usaraSistema", { modelo: usara.name })
            : t("quiet.panel.sinModeloSistema")}
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
  const { models, currentModel } = useModelStore();

  // Quién sirve para «Audio del sistema» y si HOY se puede encender, según el
  // backend — que es quien de verdad lo decide y quien rechaza el encendido si
  // no hay modelo. Se vuelve a preguntar cuando cambia el catálogo (una
  // descarga) o el modelo activo, que son las dos cosas que mueven la respuesta.
  const [aptitud, setAptitud] = useState<AptitudAudioSistema | null>(null);
  useEffect(() => {
    let vivo = true;
    void commands.aptitudAudioSistema().then((r) => {
      if (vivo && r.status === "ok") setAptitud(r.data);
    });
    return () => {
      vivo = false;
    };
  }, [models, currentModel]);

  // `null` NO es «apagado»: es la lista por defecto del idioma, o sea el filtro
  // ACTIVO. Solo una lista vacía lo apaga (`audio_toolkit/text.rs`). Mismo
  // criterio, palabra por palabra, que el chip de la pantalla de inicio.
  const cfw = settings?.custom_filler_words;
  const muletillasOn = cfw == null || cfw.length > 0;

  return (
    <div className="q-panel-atajos">
      {/* Los atajos van con la descripción en el icono (i), no debajo: en línea
          se comían tres renglones por fila y esta pantalla es justamente la que
          se pidió corta. El texto sigue entero, a un puntero de distancia. */}
      <Tarjeta icon={Mic} titulo={t("quiet.nav.listen")}>
        <ShortcutInput
          shortcutId="transcribe"
          descriptionMode="tooltip"
          grouped
        />
        <SelectorModelo capacidad="dictado" />
        <div className="px-4 p-2">
          <div className="q-pa-tools">
            <BotonIcono
              icon={Eraser}
              etiqueta={t("bancada.btn.fillers")}
              ayuda={t("quiet.panel.hint.fillers")}
              activo={muletillasOn}
              onClick={() =>
                updateSetting("custom_filler_words", muletillasOn ? [] : null)
              }
            />
            <BotonIcono
              icon={SpellCheck}
              etiqueta={t("settings.advanced.autocorreccion.title")}
              ayuda={t("quiet.panel.hint.autocorreccion")}
              activo={settings?.autocorreccion_activa ?? true}
              onClick={() =>
                updateSetting(
                  "autocorreccion_activa",
                  !(settings?.autocorreccion_activa ?? true),
                )
              }
            />
            <BotonIcono
              icon={Smile}
              etiqueta={t("settings.advanced.emojiDictado.title")}
              ayuda={t("quiet.panel.hint.emoji")}
              activo={settings?.emoji_dictado ?? true}
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
        {/* Mismo interruptor que la pantalla «Streaming» de la barra lateral
            (`capture_system_audio`), en formato botón: su descripción son tres
            renglones y aquí vive en el hint. */}
        <div className="px-4 p-2">
          <div className="q-pa-tools">
            <BotonIcono
              icon={MonitorSpeaker}
              etiqueta={t("settings.streaming.systemAudio.label")}
              ayuda={t("quiet.panel.hint.systemAudio")}
              activo={!!settings?.capture_system_audio}
              // Sin modelo apto NO se deja pulsar. El backend ya lo rechazaba
              // —y devolvía error para que el interruptor volviera solo—, pero
              // dejar pulsar algo que va a rebotar es prometer y desdecirse:
              // más vale que no se pueda y se diga por qué.
              bloqueado={
                aptitud && !aptitud.disponible
                  ? t("quiet.panel.sinModeloSistema")
                  : undefined
              }
              onClick={() =>
                updateSetting(
                  "capture_system_audio",
                  !settings?.capture_system_audio,
                )
              }
            />
          </div>
        </div>
        <SelectorModelo
          capacidad="sistema"
          preferidosSistema={aptitud?.preferidos}
        />
        <MedidoresStreaming grabando={grabando} />
      </Tarjeta>

      <Tarjeta icon={Volume2} titulo={t("sidebar.escucha")}>
        <ShortcutInput
          shortcutId="leer_seleccion"
          descriptionMode="tooltip"
          grouped
        />
        <SelectorVoz />
      </Tarjeta>

      {/* La salida NO se deshabilita aquí aunque los sonidos estén apagados:
          este ajuste también decide por dónde habla VOX
          (`managers/tts/manager.rs:307`), así que sigue haciendo algo. */}
      <TarjetaGeneral />
    </div>
  );
};

PanelAtajos.displayName = "PanelAtajos";

export default PanelAtajos;
