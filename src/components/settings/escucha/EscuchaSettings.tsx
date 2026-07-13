// [ESCUCHA] Panel de lectura en voz alta: abre un archivo (o el portapapeles),
// lo preprocesa en oraciones hablables y las lee una a una — prosa con voz
// natural, código con voz técnica — resaltando la línea en lectura.
//
// La cola vive aquí: el backend habla UNA oración por escucha_speak y este
// panel espera con escucha_status (poll de 100 ms) a que termine para avanzar
// el resaltado y pedir la siguiente. Pausa = detener la oración actual y
// recordar el índice; Continuar relee desde esa oración (MVP honesto: el crate
// tts no expone pause/resume).
import React, { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ClipboardList, FolderOpen, Pause, Play, Square } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  commands,
  type ModoLectura,
  type OracionHablable,
  type VerbosidadSimbolos,
  type VozEscucha,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Alert } from "../../ui/Alert";
import { Button } from "../../ui/Button";
import { Select } from "../../ui/Select";
import { MotorVoz } from "./MotorVoz";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const modoPorExtension = (ruta: string): ModoLectura => {
  const ext = ruta.split(".").pop()?.toLowerCase() ?? "";
  if (ext === "md" || ext === "markdown") return "markdown";
  if (ext === "txt" || ext === "") return "auto";
  return "codigo";
};

export const EscuchaSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings, refreshSettings } = useSettings();

  const [voces, setVoces] = useState<VozEscucha[]>([]);
  const [motorDisponible, setMotorDisponible] = useState(true);
  const [vozProsa, setVozProsa] = useState<string | null>(null);
  const [vozCodigo, setVozCodigo] = useState<string | null>(null);
  const [rateProsa, setRateProsa] = useState(1.0);
  const [rateCodigo, setRateCodigo] = useState(0.9);
  const [verbosidad, setVerbosidad] = useState<VerbosidadSimbolos>("natural");

  // Hidratar desde settings persistidos una sola vez (los cambios posteriores
  // salen de este panel, así que el estado local manda después).
  const hidratadoRef = useRef(false);
  useEffect(() => {
    if (hidratadoRef.current || !settings) return;
    hidratadoRef.current = true;
    if (settings.escucha_voz_prosa) setVozProsa(settings.escucha_voz_prosa);
    if (settings.escucha_voz_codigo) setVozCodigo(settings.escucha_voz_codigo);
    setRateProsa(settings.escucha_rate_prosa ?? 1.0);
    setRateCodigo(settings.escucha_rate_codigo ?? 0.9);
    setVerbosidad(settings.escucha_verbosidad_simbolos ?? "natural");
  }, [settings]);

  // Persistir la configuración de Escucha (con un pequeño debounce para que
  // arrastrar el slider no escriba el store en cada tick).
  useEffect(() => {
    if (!hidratadoRef.current) return;
    const timer = setTimeout(() => {
      void commands
        .escuchaUpdateSettings(
          vozProsa,
          vozCodigo,
          rateProsa,
          rateCodigo,
          verbosidad,
        )
        .then(() => refreshSettings());
    }, 400);
    return () => clearTimeout(timer);
  }, [vozProsa, vozCodigo, rateProsa, rateCodigo, verbosidad]);

  const [nombreFuente, setNombreFuente] = useState<string | null>(null);
  const [lineas, setLineas] = useState<string[]>([]);
  const [oraciones, setOraciones] = useState<OracionHablable[]>([]);
  const [indice, setIndice] = useState(-1);
  const [leyendo, setLeyendo] = useState(false);
  const [pausado, setPausado] = useState(false);

  // Token de invalidación: cada stop/pausa/lectura nueva lo incrementa y el
  // bucle de lectura en vuelo se da cuenta y termina sin efectos.
  const tokenRef = useRef(0);
  // El bucle lee la config vigente por ref para que cambiar voz/velocidad
  // aplique a partir de la siguiente oración sin reiniciar la lectura.
  const configRef = useRef({ vozProsa, vozCodigo, rateProsa, rateCodigo });
  useEffect(() => {
    configRef.current = { vozProsa, vozCodigo, rateProsa, rateCodigo };
  }, [vozProsa, vozCodigo, rateProsa, rateCodigo]);

  const contenedorRef = useRef<HTMLDivElement>(null);

  // Cargar las voces del motor activo — y RECARGARLAS al cambiar de motor, para
  // que el reparto (es/en, M/F) del nuevo motor aparezca de una. Si la voz
  // elegida ya no existe en el nuevo motor, se cae a una válida (español primero).
  useEffect(() => {
    let cancelado = false;
    commands.escuchaListVoices().then((r) => {
      if (cancelado) return;
      if (r.status === "ok") {
        setVoces(r.data);
        const valido = (id: string | null) => !!id && r.data.some((v) => v.id === id);
        const primeraEs = r.data.find((v) => v.es_espanol) ?? r.data[0];
        if (primeraEs) {
          setVozProsa((prev) => (valido(prev) ? prev : primeraEs.id));
          setVozCodigo((prev) => (valido(prev) ? prev : primeraEs.id));
        }
        setMotorDisponible(r.data.length > 0);
      } else {
        setMotorDisponible(false);
      }
    });
    return () => {
      cancelado = true;
    };
  }, [settings?.tts_selected_engine]);

  const detenerMotor = useCallback(() => {
    tokenRef.current += 1;
    void commands.escuchaStop();
  }, []);

  const detener = useCallback(() => {
    detenerMotor();
    setLeyendo(false);
    setPausado(false);
    setIndice(-1);
  }, [detenerMotor]);

  // Esc detiene la lectura; al desmontar el panel también se detiene.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") detener();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      tokenRef.current += 1;
      void commands.escuchaStop();
    };
  }, [detener]);

  const leerDesde = useCallback(
    async (desde: number, cola: OracionHablable[]) => {
      const esperarFin = async (token: number) => {
        // Margen inicial para que is_speaking alcance a levantarse.
        await sleep(150);
        while (tokenRef.current === token) {
          const r = await commands.escuchaStatus();
          if (r.status !== "ok" || !r.data.hablando) return;
          await sleep(100);
        }
      };
      const token = ++tokenRef.current;
      setLeyendo(true);
      setPausado(false);
      for (let i = desde; i < cola.length; i++) {
        if (tokenRef.current !== token) return;
        setIndice(i);
        const oracion = cola[i];
        const esCodigo = oracion.voz === "codigo";
        const cfg = configRef.current;
        const r = await commands.escuchaSpeak(
          oracion.texto_hablable,
          esCodigo ? cfg.vozCodigo : cfg.vozProsa,
          esCodigo ? cfg.rateCodigo : cfg.rateProsa,
        );
        if (r.status === "error") {
          toast.error(t("escucha.errorSpeak"), { description: r.error });
          break;
        }
        await esperarFin(token);
      }
      if (tokenRef.current === token) {
        setLeyendo(false);
        setPausado(false);
        setIndice(-1);
      }
    },
    [t],
  );

  const cargar = useCallback(
    async (contenido: string, modo: ModoLectura, nombre: string) => {
      detener();
      const r = await commands.escuchaPreprocess(contenido, modo, verbosidad);
      if (r.status === "error") {
        toast.error(t("escucha.errorRead"), { description: r.error });
        return;
      }
      setNombreFuente(nombre);
      setLineas(contenido.split(/\r?\n/));
      setOraciones(r.data);
      setIndice(-1);
    },
    [detener, verbosidad, t],
  );

  const abrirArchivo = useCallback(async () => {
    const ruta = await open({
      multiple: false,
      filters: [
        {
          name: t("escucha.fileFilter"),
          extensions: [
            "md",
            "markdown",
            "txt",
            "rs",
            "ts",
            "tsx",
            "js",
            "jsx",
            "py",
            "json",
            "toml",
            "yaml",
            "yml",
            "css",
            "html",
            "sh",
            "sql",
          ],
        },
      ],
    });
    if (typeof ruta !== "string") return;
    const r = await commands.escuchaReadFile(ruta);
    if (r.status === "error") {
      toast.error(t("escucha.errorRead"), { description: r.error });
      return;
    }
    const nombre = ruta.split(/[\\/]/).pop() ?? ruta;
    await cargar(r.data, modoPorExtension(ruta), nombre);
  }, [cargar, t]);

  const leerPortapapeles = useCallback(async () => {
    const r = await commands.escuchaReadClipboard();
    if (r.status === "error" || r.data.trim().length === 0) {
      toast.error(t("escucha.clipboardEmpty"));
      return;
    }
    await cargar(r.data, "auto", t("escucha.clipboard"));
  }, [cargar, t]);

  const alternarLectura = useCallback(() => {
    if (leyendo) {
      // Pausa: invalida el bucle y corta la oración actual; el índice queda.
      detenerMotor();
      setLeyendo(false);
      setPausado(true);
      return;
    }
    if (oraciones.length === 0) return;
    const desde = pausado && indice >= 0 ? indice : 0;
    void leerDesde(desde, oraciones);
  }, [leyendo, pausado, indice, oraciones, detenerMotor, leerDesde]);

  // Resaltado: auto-scroll a la primera línea en lectura.
  const oracionActual = indice >= 0 ? oraciones[indice] : null;
  useEffect(() => {
    if (!oracionActual || !contenedorRef.current) return;
    const el = contenedorRef.current.querySelector(
      `[data-linea="${oracionActual.linea_inicio}"]`,
    );
    const reducirMovimiento = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    el?.scrollIntoView({
      block: "center",
      behavior: reducirMovimiento ? "auto" : "smooth",
    });
  }, [oracionActual]);

  const opcionesVoz = voces.map((v) => ({
    value: v.id,
    label: `${v.nombre} (${v.idioma})`,
  }));

  const lineaResaltada = (numero: number) =>
    oracionActual != null &&
    numero >= oracionActual.linea_inicio &&
    numero <= oracionActual.linea_fin;

  const reducirMovimiento =
    typeof window !== "undefined" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-4">
      <div className="px-4 flex items-center justify-between">
        <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
          {t("escucha.title")}
        </h2>
        <div className="flex items-center gap-2">
          <Button
            onClick={abrirArchivo}
            variant="secondary"
            size="sm"
            className="flex items-center gap-2"
          >
            <FolderOpen className="w-4 h-4" />
            <span>{t("escucha.openFile")}</span>
          </Button>
          <Button
            onClick={leerPortapapeles}
            variant="secondary"
            size="sm"
            className="flex items-center gap-2"
          >
            <ClipboardList className="w-4 h-4" />
            <span>{t("escucha.readClipboard")}</span>
          </Button>
        </div>
      </div>

      <div className="px-4">
        <MotorVoz />
      </div>

      {!motorDisponible && (
        <div className="px-4">
          <Alert variant="warning">{t("escucha.engineUnavailable")}</Alert>
        </div>
      )}

      <div className="bg-background border border-mid-gray/20 rounded-lg p-4 space-y-4">
        <div className="flex items-center gap-3">
          <Button
            onClick={alternarLectura}
            variant="primary-soft"
            size="md"
            disabled={!motorDisponible || oraciones.length === 0}
            className="flex items-center gap-2"
            title={leyendo ? t("escucha.pauseHint") : t("escucha.play")}
          >
            {leyendo ? (
              <Pause className="w-4 h-4" />
            ) : (
              <Play className="w-4 h-4" />
            )}
            <span>
              {leyendo
                ? t("escucha.pause")
                : pausado
                  ? t("escucha.resume")
                  : t("escucha.play")}
            </span>
          </Button>
          <Button
            onClick={detener}
            variant="ghost"
            size="md"
            disabled={!leyendo && !pausado}
            className="flex items-center gap-2"
            title={t("escucha.stop")}
          >
            <Square className="w-4 h-4" />
            <span>{t("escucha.stop")}</span>
          </Button>
          <p className="text-xs text-text/50 ms-auto">
            {t("escucha.pauseHint")} · {t("escucha.escHint")}
          </p>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div className="space-y-2">
            <p className="text-sm font-medium">{t("escucha.voiceProse")}</p>
            <Select
              value={vozProsa}
              options={opcionesVoz}
              onChange={(v) => setVozProsa(v)}
              isClearable={false}
              placeholder={t("escucha.noVoices")}
            />
            <label className="flex items-center gap-2 text-xs text-text/70">
              <span className="w-24 shrink-0">
                {t("escucha.speed")} {rateProsa.toFixed(1)}×
              </span>
              <input
                type="range"
                min={0.5}
                max={2}
                step={0.1}
                value={rateProsa}
                onChange={(e) => setRateProsa(parseFloat(e.target.value))}
                className="flex-grow h-2 rounded-lg appearance-none cursor-pointer"
              />
            </label>
          </div>
          <div className="space-y-2">
            <p className="text-sm font-medium">{t("escucha.voiceCode")}</p>
            <Select
              value={vozCodigo}
              options={opcionesVoz}
              onChange={(v) => setVozCodigo(v)}
              isClearable={false}
              placeholder={t("escucha.noVoices")}
            />
            <label className="flex items-center gap-2 text-xs text-text/70">
              <span className="w-24 shrink-0">
                {t("escucha.speed")} {rateCodigo.toFixed(1)}×
              </span>
              <input
                type="range"
                min={0.5}
                max={2}
                step={0.1}
                value={rateCodigo}
                onChange={(e) => setRateCodigo(parseFloat(e.target.value))}
                className="flex-grow h-2 rounded-lg appearance-none cursor-pointer"
              />
            </label>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <p className="text-sm font-medium">{t("escucha.verbosity")}</p>
          <Select
            className="min-w-56"
            value={verbosidad}
            options={[
              { value: "natural", label: t("escucha.verbosityNatural") },
              { value: "literal", label: t("escucha.verbosityLiteral") },
            ]}
            onChange={(v) => {
              if (v === "natural" || v === "literal") setVerbosidad(v);
            }}
            isClearable={false}
          />
        </div>
      </div>

      <div className="bg-background border border-mid-gray/20 rounded-lg overflow-hidden">
        {lineas.length === 0 ? (
          <div className="px-4 py-10 text-center text-text/50 text-sm">
            {t("escucha.empty")}
          </div>
        ) : (
          <>
            <div className="px-4 py-2 border-b border-mid-gray/20 flex items-center justify-between">
              <p className="text-xs text-text/60 truncate">{nombreFuente}</p>
              <p className="text-xs text-text/40">
                {t("escucha.sentences", { total: oraciones.length })}
              </p>
            </div>
            <div
              ref={contenedorRef}
              className="max-h-96 overflow-y-auto font-mono text-xs leading-5 py-2"
            >
              {lineas.map((texto, i) => {
                const numero = i + 1;
                const activa = lineaResaltada(numero);
                return (
                  <div
                    key={numero}
                    data-linea={numero}
                    className={`px-4 whitespace-pre-wrap break-words ${
                      reducirMovimiento ? "" : "transition-colors duration-200"
                    } ${activa ? "bg-logo-primary/25" : ""}`}
                  >
                    <span className="select-none text-text/30 me-3 inline-block w-8 text-end">
                      {numero}
                    </span>
                    {texto || " "}
                  </div>
                );
              })}
            </div>
          </>
        )}
      </div>
    </div>
  );
};
