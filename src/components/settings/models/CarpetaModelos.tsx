import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { HardDrive } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { commands, events } from "@/bindings";
import type {
  CarpetaModelos as Carpeta,
  DiscoInfo,
  ModelInfo,
  MudanzaProgreso,
} from "@/bindings";
import { bytesAGb } from "@/lib/utils/format";

/**
 * Selector de dónde viven los modelos (transcripción; el «Pulido con IA» que
 * compartía esta carpeta se retiró el 29/07). Comparten la
 * elección). Muestra la carpeta actual y los discos con su espacio libre; al
 * elegir uno, las DESCARGAS NUEVAS van a `<disco>/Abrax/models`. Si ya hay
 * modelos descargados, ofrece mudarlos a la carpeta nueva con progreso; la
 * mudanza es segura (copia → verifica → borra) y lo no movido queda intacto.
 */
export const CarpetaModelos: React.FC = React.memo(() => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const models = useModelStore((s) => s.models);
  const [carpeta, setCarpeta] = useState<Carpeta | null>(null);
  const [discos, setDiscos] = useState<DiscoInfo[]>([]);
  const [abierto, setAbierto] = useState(false);
  // Destino elegido a la espera de la decisión mover / solo nuevas.
  const [pendiente, setPendiente] = useState<{ destino: string | null } | null>(
    null,
  );
  const [moviendo, setMoviendo] = useState<MudanzaProgreso | null>(null);
  const [errorMudanza, setErrorMudanza] = useState<string | null>(null);

  const descargados = models.filter((m: ModelInfo) => m.is_downloaded);
  const descargadosMb = descargados.reduce((acc, m) => acc + m.size_mb, 0);

  const recargar = useCallback(() => {
    void commands.obtenerCarpetaModelos().then(setCarpeta);
    void commands.listarDiscos().then(setDiscos);
  }, []);

  useEffect(() => {
    recargar();
  }, [recargar, settings?.models_dir]);

  useEffect(() => {
    const off = events.mudanzaProgreso.listen((event) => {
      const p = event.payload;
      if (p.estado === "progreso") {
        setMoviendo(p);
      } else if (p.estado === "listo") {
        setMoviendo(null);
        recargar();
      } else {
        setMoviendo(null);
        setErrorMudanza(p.detalle);
        recargar();
      }
    });
    return () => {
      void off.then((f) => f());
    };
  }, [recargar]);

  const aplicar = async (destino: string | null, mover: boolean) => {
    setPendiente(null);
    setAbierto(false);
    setErrorMudanza(null);
    if (mover) {
      setMoviendo({
        estado: "progreso",
        archivo: "",
        hechos_bytes: 0,
        total_bytes: 0,
        detalle: "",
      });
    }
    const res = await commands.cambiarCarpetaModelos(destino, mover);
    if (res.status === "error") {
      setMoviendo(null);
      setErrorMudanza(res.error);
      return;
    }
    // El backend ya escribió el ajuste; esto solo sincroniza el store local
    // (mismo valor, escritura idempotente).
    updateSetting("models_dir", destino);
  };

  const elegir = (destino: string | null) => {
    if (descargados.length === 0) {
      void aplicar(destino, false);
    } else {
      setPendiente({ destino });
    }
  };

  const rutaEnDisco = (d: DiscoInfo) => {
    const sep =
      d.punto_montaje.includes("\\") || d.punto_montaje.includes(":")
        ? "\\"
        : "/";
    const base = d.punto_montaje.endsWith(sep)
      ? d.punto_montaje.slice(0, -1)
      : d.punto_montaje;
    return `${base}${sep}Abrax${sep}models`;
  };

  if (!carpeta) return null;

  const pctMudanza =
    moviendo && moviendo.total_bytes > 0
      ? Math.min(100, (moviendo.hechos_bytes / moviendo.total_bytes) * 100)
      : 0;

  return (
    <div className="rounded-md border border-black/10 dark:border-white/10 p-3 text-xs space-y-2">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <HardDrive className="w-4 h-4 shrink-0 opacity-60" />
          <div className="min-w-0">
            <div className="font-medium">{t("carpetaModelos.titulo")}</div>
            <div className="opacity-60 truncate" title={carpeta.actual}>
              {carpeta.actual}
            </div>
          </div>
        </div>
        <Button
          variant="secondary"
          size="sm"
          disabled={moviendo !== null}
          onClick={() => setAbierto(!abierto)}
        >
          {t("carpetaModelos.cambiar")}
        </Button>
      </div>

      {abierto && !pendiente && moviendo === null && (
        <div className="space-y-1">
          {discos.map((d) => (
            <button
              key={d.punto_montaje}
              type="button"
              onClick={() => elegir(rutaEnDisco(d))}
              className="w-full flex items-center justify-between gap-2 rounded px-2 py-1.5 hover:bg-black/5 dark:hover:bg-white/10 text-left"
            >
              <span className="truncate">
                {d.nombre
                  ? t("carpetaModelos.discoConNombre", {
                      nombre: d.nombre,
                      punto: d.punto_montaje,
                    })
                  : d.punto_montaje}
                {d.removible ? ` ${t("carpetaModelos.removible")}` : ""}
              </span>
              <span className="opacity-60 whitespace-nowrap tabular-nums">
                {t("carpetaModelos.espacio", {
                  libre: bytesAGb(d.libre_bytes, 0),
                  total: bytesAGb(d.total_bytes, 0),
                })}
              </span>
            </button>
          ))}
          {carpeta.personalizada && (
            <Button variant="ghost" size="sm" onClick={() => elegir(null)}>
              {t("carpetaModelos.volverPorDefecto")}
            </Button>
          )}
        </div>
      )}

      {pendiente && (
        <div className="space-y-2" aria-live="polite">
          <div>
            {t("carpetaModelos.moverPregunta", {
              gb: (descargadosMb / 1024).toFixed(1),
            })}
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void aplicar(pendiente.destino, true)}
            >
              {t("carpetaModelos.moverAhora")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void aplicar(pendiente.destino, false)}
            >
              {t("carpetaModelos.soloNuevas")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setPendiente(null)}
            >
              {t("carpetaModelos.cancelar")}
            </Button>
          </div>
        </div>
      )}

      {moviendo !== null && (
        <div className="space-y-1" aria-live="polite">
          <div className="flex items-center justify-between gap-2">
            <span className="truncate">
              {t("carpetaModelos.moviendo", { archivo: moviendo.archivo })}
            </span>
            <span className="opacity-60 tabular-nums whitespace-nowrap">
              {pctMudanza.toFixed(0)}%
            </span>
          </div>
          <div className="h-1 rounded bg-black/10 dark:bg-white/10 overflow-hidden">
            <div
              className="h-full bg-current transition-[width] duration-200"
              style={{ width: `${pctMudanza}%` }}
            />
          </div>
        </div>
      )}

      {errorMudanza && (
        <div className="text-red-600 dark:text-red-400" role="alert">
          {t("carpetaModelos.moverError", { detalle: errorMudanza })}
        </div>
      )}

      {carpeta.personalizada && moviendo === null && (
        <div className="opacity-70" aria-live="polite">
          {t("carpetaModelos.avisoNoMueve")}
        </div>
      )}
    </div>
  );
});

CarpetaModelos.displayName = "CarpetaModelos";
