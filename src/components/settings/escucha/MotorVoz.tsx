// [TTS] Selector del motor de voz adaptativo: detecta el hardware, recomienda el
// mejor motor LOCAL que el equipo soporte, deja elegir/descargar otro y probar
// la voz. 100% local: ninguna acción usa la nube.
import React, { useCallback, useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  Cpu,
  Download,
  RefreshCw,
  Sparkles,
  Volume2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type EngineId,
  type EngineStatus,
  type HardwareInfo,
  type VozEscucha,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Select } from "../../ui/Select";
import { opcionesDeVoces } from "./voces";

const OS_LABEL: Record<string, string> = {
  windows: "Windows",
  mac_os: "macOS",
  linux: "Linux",
  other: "—",
};
/**
 * La DETECCIÓN DE HARDWARE queda oculta (decisión de producto del 30/07). La
 * detección sigue corriendo —de ella sale la recomendación de motor y el aviso de
 * «este motor pide GPU»— pero no se le informa al usuario de qué GPU tiene ni
 * cuánta VRAM, ni se le ofrece re-detectar.
 *
 * Motivo: es información técnica que no le sirve para decidir nada. Lo que sí ve
 * es el motor recomendado y el activo, que es la conclusión, no el dato crudo.
 *
 * Pon esto en `true` para volver a mostrarlo tal cual estaba.
 */
const MOSTRAR_HARDWARE: boolean = false;

const VENDOR_LABEL: Record<string, string> = {
  nvidia: "NVIDIA",
  apple: "Apple",
  amd: "AMD",
  intel: "Intel",
  unknown: "",
  none: "",
};

interface Progreso {
  assetId: string;
  percentage: number;
}

// Velocidad de lectura (multiplicador). Slider continuo (paso 0.1), aplica a
// TODOS los motores.
const VEL_MIN = 0.5;
const VEL_MAX = 3.0;
const VEL_STEP = 0.1;
// Tono (pitch en Hz). 3 niveles, SOLO el motor online (edge-tts) lo aplica.
const NIVELES_TONO: { v: number; key: string }[] = [
  { v: -40, key: "tts.pitchLow" },
  { v: 0, key: "tts.pitchNormal" },
  { v: 40, key: "tts.pitchHigh" },
];

interface MotorVozProps {
  /** Interrumpe la lectura en curso del panel (p. ej. antes de "Probar voz",
   *  para que la muestra no pelee con el bucle de lectura ni lo deje corriendo). */
  onInterrumpir?: () => void;
}

export const MotorVoz: React.FC<MotorVozProps> = ({ onInterrumpir }) => {
  const { t } = useTranslation();
  const { settings, refreshSettings } = useSettings();

  const [hardware, setHardware] = useState<HardwareInfo | null>(null);
  const [recommended, setRecommended] = useState<EngineId | null>(null);
  const [active, setActive] = useState<EngineId | null>(null);
  const [engines, setEngines] = useState<EngineStatus[]>([]);
  const [voces, setVoces] = useState<VozEscucha[]>([]);
  const [vozPrueba, setVozPrueba] = useState<string | null>(null);
  const [expandido, setExpandido] = useState(false);
  const [cargando, setCargando] = useState(true);
  const [ocupado, setOcupado] = useState<EngineId | null>(null);
  const [progreso, setProgreso] = useState<Progreso | null>(null);
  // Motor cuyo servidor/modelo se está cargando por primera vez (indicador).
  const [preparando, setPreparando] = useState<EngineId | null>(null);

  const engineDisplay = useCallback(
    (id: EngineId): string =>
      id === "system"
        ? t("tts.engineSystem")
        : id.charAt(0).toUpperCase() + id.slice(1),
    [t],
  );

  const engineDesc = useCallback(
    (id: EngineId): string =>
      ({
        system: t("tts.descSystem"),
        piper: t("tts.descPiper"),
        kokoro: t("tts.descKokoro"),
        online: t("tts.descOnline"),
      })[id] ?? "",
    [t],
  );

  const recargarVoces = useCallback(async () => {
    const r = await commands.escuchaListVoices();
    if (r.status === "ok") {
      setVoces(r.data);
      // Al cambiar de motor las voces cambian: conserva la selección solo si
      // sigue siendo válida; si no, elige una del nuevo reparto (español primero).
      setVozPrueba((prev) => {
        if (prev && r.data.some((v) => v.id === prev)) return prev;
        return r.data.find((v) => v.es_espanol)?.id ?? r.data[0]?.id ?? null;
      });
    }
  }, []);

  const recargarEstado = useCallback(async () => {
    const [rec, act, list] = await Promise.all([
      commands.getRecommendedEngine(),
      commands.getActiveEngine(),
      commands.listEngines(),
    ]);
    if (rec.status === "ok") setRecommended(rec.data);
    if (act.status === "ok") setActive(act.data);
    if (list.status === "ok") setEngines(list.data);
  }, []);

  const cargar = useCallback(async () => {
    const hw = await commands.detectHardware();
    if (hw.status === "ok") setHardware(hw.data);
    await recargarEstado();
    await recargarVoces();
    setCargando(false);
  }, [recargarEstado, recargarVoces]);

  useEffect(() => {
    void cargar();
  }, []);

  // Progreso de descarga de assets TTS (runtime/voces).
  useEffect(() => {
    const un = listen<Progreso & { asset_id: string; percentage: number }>(
      "tts-download-progress",
      (e) =>
        setProgreso({
          assetId: e.payload.asset_id,
          percentage: e.payload.percentage,
        }),
    );
    return () => {
      void un.then((f) => f());
    };
  }, []);

  // Carga del modelo/servidor de un motor neuronal (la 1.ª vez tarda unos
  // segundos). Se muestra un aviso y se libera con `tts-engine-ready`.
  useEffect(() => {
    const unL = listen<EngineId>("tts-engine-loading", (e) =>
      setPreparando(e.payload),
    );
    const unR = listen<EngineId>("tts-engine-ready", () => setPreparando(null));
    return () => {
      void unL.then((f) => f());
      void unR.then((f) => f());
    };
  }, []);

  const humano = (hw: HardwareInfo): string => {
    const os = OS_LABEL[hw.os] ?? hw.os;
    let gpu = hw.gpu_name || VENDOR_LABEL[hw.gpu_vendor] || "";
    if (!gpu || hw.gpu_vendor === "none") gpu = t("tts.noGpu");
    return `${os} + ${gpu}`;
  };

  const usarMotor = useCallback(
    async (id: EngineId) => {
      const r = await commands.setEngine(id);
      if (r.status === "error") {
        toast.error(t("tts.errorSetEngine"), { description: r.error });
        return;
      }
      await recargarEstado();
      await recargarVoces();
      await refreshSettings();
    },
    [t, recargarEstado, recargarVoces, refreshSettings],
  );

  const redetectar = useCallback(async () => {
    setCargando(true);
    const r = await commands.redetectHardware();
    if (r.status === "ok") setHardware(r.data);
    await recargarEstado();
    setCargando(false);
  }, [recargarEstado]);

  // Descarga/aprovisiona el runtime del motor pedido (Piper: runtime + 1.ª voz;
  // Kokoro/Online: venv con uv).
  const descargarMotor = useCallback(
    async (id: EngineId) => {
      setOcupado(id);
      setProgreso(null);
      try {
        if (id === "piper") {
          const rt = await commands.installPiperRuntime();
          if (rt.status === "error") throw new Error(rt.error);
          const cat = await commands.listPiperVoices();
          const primera = cat.status === "ok" ? cat.data[0] : undefined;
          if (primera) {
            const v = await commands.installPiperVoice(primera.id);
            if (v.status === "error") throw new Error(v.error);
          }
        } else if (id === "kokoro") {
          const r = await commands.installKokoroRuntime();
          if (r.status === "error") throw new Error(r.error);
        } else if (id === "online") {
          const r = await commands.installOnlineRuntime();
          if (r.status === "error") throw new Error(r.error);
        }
        toast.success(t("tts.installed"));
        await recargarEstado();
      } catch (e) {
        toast.error(t("tts.errorDownload"), {
          description: e instanceof Error ? e.message : String(e),
        });
      } finally {
        setOcupado(null);
        setProgreso(null);
      }
    },
    [t, recargarEstado],
  );

  // "Ajustes de voz": velocidad (todos los motores) y tono (solo online).
  // Persisten en settings; se aplican a "Probar voz" y a la lectura del panel.
  // La velocidad tiene estado local para que arrastrar el slider sea fluido; se
  // persiste con debounce (así no spamea el backend ni reinicia la lectura en
  // cada tick — el reinicio en caliente ocurre al asentarse el valor).
  const [velLocal, setVelLocal] = useState<number | null>(null);
  const velocidad = velLocal ?? settings?.tts_velocidad ?? 1.0;
  const tono = settings?.tts_tono ?? 0;

  const guardarAjustes = useCallback(
    async (vel: number, ton: number) => {
      const r = await commands.updateTtsAjustes(vel, ton);
      if (r.status === "error") {
        toast.error(t("tts.errorSetEngine"), { description: r.error });
        return;
      }
      await refreshSettings();
    },
    [t, refreshSettings],
  );

  const tonoRef = useRef(tono);
  useEffect(() => {
    tonoRef.current = tono;
  }, [tono]);
  useEffect(() => {
    if (velLocal == null) return;
    const timer = setTimeout(() => {
      void guardarAjustes(velLocal, tonoRef.current);
    }, 350);
    return () => clearTimeout(timer);
  }, [velLocal, guardarAjustes]);

  const probarVoz = useCallback(async () => {
    // Corta la lectura del panel ANTES de la muestra: si no, el bucle sigue vivo,
    // se come la muestra y el documento se sigue leyendo solo.
    onInterrumpir?.();
    await commands.escuchaStop();
    // El texto de prueba coincide con el IDIOMA de la voz elegida: una voz
    // inglesa leyendo español (o al revés) sonaría mal. Frase de marca fija.
    const vozSel = voces.find((v) => v.id === vozPrueba);
    const texto =
      vozSel && !vozSel.es_espanol
        ? t("tts.sampleTextEn")
        : t("tts.sampleTextEs");
    const r = await commands.escuchaSpeak(texto, vozPrueba, velocidad, tono);
    if (r.status === "error") {
      toast.error(t("escucha.errorSpeak"), { description: r.error });
    }
  }, [t, voces, vozPrueba, velocidad, tono, onInterrumpir]);

  const estadoTexto = (e: EngineStatus): string => {
    if (e.available) return t("tts.available");
    if (
      e.requirements.needs_gpu &&
      hardware &&
      hardware.gpu_vendor !== "nvidia" &&
      hardware.gpu_vendor !== "apple"
    )
      return t("tts.needsGpu");
    if (e.requirements.needs_download) return t("tts.needsDownload");
    return t("tts.unavailable");
  };

  // Agrupadas por idioma/país cuando el motor es online (lista larga navegable);
  // planas para los motores locales.
  const opcionesVoz = opcionesDeVoces(voces, t);

  const recName = recommended ? engineDisplay(recommended) : "";

  // Build de entrega (solo Voces del Sistema): sin selector de motor. La voz del
  // sistema funciona sin configurar y el panel Escucha ya expone sus voces
  // (prosa/código). Con `advanced-tts` (varios motores) se muestra el selector.
  if (!cargando && engines.length <= 1) return null;

  return (
    <div className="bg-background border border-mid-gray/20 rounded-lg p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium flex items-center gap-2">
          <Cpu className="w-4 h-4 text-logo-primary" aria-hidden="true" />
          {t("tts.title")}
        </h3>
        <span className="text-[10px] uppercase tracking-wide text-logo-primary/80 border border-logo-primary/30 rounded px-1.5 py-0.5">
          {t("tts.localBadge")}
        </span>
      </div>

      {preparando && (
        <div
          className="flex items-center gap-2 rounded-md bg-logo-primary/10 border border-logo-primary/30 px-3 py-2 text-sm"
          role="status"
          aria-live="polite"
        >
          <RefreshCw
            className="w-4 h-4 animate-spin text-logo-primary shrink-0"
            aria-hidden="true"
          />
          <span>
            {t("tts.preparingVoice", { engine: engineDisplay(preparando) })}
          </span>
        </div>
      )}

      {cargando && !hardware ? (
        MOSTRAR_HARDWARE ? (
          <p className="text-xs text-text/50">{t("tts.detecting")}</p>
        ) : null
      ) : hardware ? (
        <div className="space-y-1">
          {MOSTRAR_HARDWARE && (
            <p className="text-sm">
              <span className="text-text/60">{t("tts.detected")} </span>
              <span className="font-medium">{humano(hardware)}</span>
              {hardware.vram_mb != null && (
                <span className="text-text/40">
                  {" "}
                  {t("tts.vram", { gb: Math.round(hardware.vram_mb / 1024) })}
                </span>
              )}
            </p>
          )}
          {recommended && (
            <p className="text-sm text-text/70 flex items-center gap-1.5">
              <Sparkles
                className="w-3.5 h-3.5 text-logo-primary"
                aria-hidden="true"
              />
              {t("tts.recommendation", { engine: recName })}
            </p>
          )}
          <p className="text-xs text-text/50">
            {t("tts.activeEngine", {
              engine: active ? engineDisplay(active) : "—",
            })}
          </p>
        </div>
      ) : MOSTRAR_HARDWARE ? (
        <p className="text-xs text-text/50">{t("tts.detectFailed")}</p>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <Button
          onClick={() => recommended && usarMotor(recommended)}
          variant="primary-soft"
          size="sm"
          disabled={!recommended || active === recommended}
          className="flex items-center gap-1.5"
        >
          <Check className="w-4 h-4" aria-hidden="true" />
          {t("tts.useRecommended")}
        </Button>
        <Button
          onClick={() => setExpandido((v) => !v)}
          variant="secondary"
          size="sm"
          className="flex items-center gap-1.5"
          aria-expanded={expandido}
        >
          <ChevronDown
            className={`w-4 h-4 transition-transform ${expandido ? "rotate-180" : ""}`}
            aria-hidden="true"
          />
          {t("tts.chooseOther")}
        </Button>
        {MOSTRAR_HARDWARE && (
          <Button
            onClick={redetectar}
            variant="ghost"
            size="sm"
            disabled={cargando}
            className="flex items-center gap-1.5"
          >
            <RefreshCw
              className={`w-4 h-4 ${cargando ? "animate-spin" : ""}`}
              aria-hidden="true"
            />
            {t("tts.redetect")}
          </Button>
        )}
        <Button
          onClick={probarVoz}
          variant="ghost"
          size="sm"
          disabled={voces.length === 0}
          className="flex items-center gap-1.5 ms-auto"
        >
          <Volume2 className="w-4 h-4" aria-hidden="true" />
          {t("tts.testVoice")}
        </Button>
      </div>

      {voces.length > 0 && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-text/60 shrink-0">
            {t("tts.voice")}
          </span>
          <Select
            className="min-w-56"
            value={vozPrueba}
            options={opcionesVoz}
            onChange={(v) => setVozPrueba(v)}
            isClearable={false}
            placeholder={t("escucha.noVoices")}
            ariaLabel={t("tts.voice")}
          />
        </div>
      )}

      {/* Ajustes de voz: velocidad (todos los motores) y tono (solo online). */}
      <div className="border-t border-mid-gray/20 pt-3 space-y-2">
        <p className="text-xs font-medium text-text/70">
          {t("tts.voiceSettings")}
        </p>
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs text-text/60 w-16 shrink-0">
            {t("tts.speed")}
          </span>
          <input
            type="range"
            min={VEL_MIN}
            max={VEL_MAX}
            step={VEL_STEP}
            value={velocidad}
            onChange={(e) => setVelLocal(parseFloat(e.target.value))}
            className="flex-grow min-w-40 h-2 rounded-lg appearance-none cursor-pointer"
            aria-label={t("tts.speed")}
            aria-valuetext={`${velocidad.toFixed(1)}×`}
          />
          <span className="text-xs tabular-nums text-text/70 w-10 text-end shrink-0">
            {velocidad.toFixed(1)}×
          </span>
        </div>
        {active === "online" && (
          <div className="space-y-1">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-xs text-text/60 w-16 shrink-0">
                {t("tts.pitch")}
              </span>
              <div
                className="flex gap-1"
                role="group"
                aria-label={t("tts.pitch")}
              >
                {NIVELES_TONO.map((n) => (
                  <Button
                    key={n.v}
                    onClick={() => guardarAjustes(velocidad, n.v)}
                    variant={tono === n.v ? "primary-soft" : "secondary"}
                    size="sm"
                    aria-pressed={tono === n.v}
                  >
                    {t(n.key)}
                  </Button>
                ))}
              </div>
            </div>
            <p className="text-xs text-text/40">{t("tts.pitchHint")}</p>
          </div>
        )}
      </div>

      {expandido && (
        <ul className="border-t border-mid-gray/20 pt-3 space-y-2">
          {engines.map((e) => {
            const esActivo = active === e.id;
            const enCurso = ocupado === e.id;
            return (
              <li
                key={e.id}
                className="flex items-start justify-between gap-3 text-sm"
              >
                <div className="min-w-0">
                  <p className="font-medium flex items-center gap-1.5">
                    {engineDisplay(e.id)}
                    {e.recommended && (
                      <span className="text-[10px] text-logo-primary border border-logo-primary/30 rounded px-1">
                        {t("tts.recommended")}
                      </span>
                    )}
                  </p>
                  <p className="text-xs text-text/50">{engineDesc(e.id)}</p>
                  {e.requirements.needs_internet && (
                    <p className="text-xs text-amber-500/90">
                      ⚠ {t("tts.onlineWarning")}
                    </p>
                  )}
                  <p
                    className={`text-xs ${e.available ? "text-green-500/80" : "text-text/40"}`}
                  >
                    {estadoTexto(e)}
                  </p>
                </div>
                <div className="shrink-0">
                  {esActivo ? (
                    <span className="text-xs text-logo-primary flex items-center gap-1">
                      <Check className="w-3.5 h-3.5" aria-hidden="true" />
                      {t("tts.activeShort")}
                    </span>
                  ) : e.available ? (
                    <Button
                      onClick={() => usarMotor(e.id)}
                      variant="secondary"
                      size="sm"
                    >
                      {t("tts.use")}
                    </Button>
                  ) : e.requirements.needs_download ? (
                    <Button
                      onClick={() => descargarMotor(e.id)}
                      variant="secondary"
                      size="sm"
                      disabled={enCurso}
                      className="flex items-center gap-1.5"
                    >
                      <Download className="w-3.5 h-3.5" aria-hidden="true" />
                      {enCurso
                        ? progreso
                          ? `${Math.round(progreso.percentage)}%`
                          : t("tts.downloading")
                        : t("tts.download")}
                    </Button>
                  ) : null}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
};
