import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { HardDrive } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { useSettings } from "@/hooks/useSettings";
import { commands } from "@/bindings";
import type { CarpetaModelos as Carpeta, DiscoInfo } from "@/bindings";
import { bytesAGb } from "@/lib/utils/format";

/**
 * Selector de dónde viven los modelos (transcripción y Pulido comparten la
 * elección). Muestra la carpeta actual y los discos con su espacio libre; al
 * elegir uno, las DESCARGAS NUEVAS van a `<disco>/Abrax/models`. Los modelos ya
 * descargados se quedan donde están (esta fase no mueve archivos): se avisa
 * para que no parezca que "desaparecieron".
 */
export const CarpetaModelos: React.FC = React.memo(() => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const [carpeta, setCarpeta] = useState<Carpeta | null>(null);
  const [discos, setDiscos] = useState<DiscoInfo[]>([]);
  const [abierto, setAbierto] = useState(false);

  const recargar = useCallback(() => {
    void commands.obtenerCarpetaModelos().then(setCarpeta);
    void commands.listarDiscos().then(setDiscos);
  }, []);

  useEffect(() => {
    recargar();
  }, [recargar, settings?.models_dir]);

  const elegirDisco = (d: DiscoInfo) => {
    const sep =
      d.punto_montaje.includes("\\") || d.punto_montaje.includes(":")
        ? "\\"
        : "/";
    const base = d.punto_montaje.endsWith(sep)
      ? d.punto_montaje.slice(0, -1)
      : d.punto_montaje;
    updateSetting("models_dir", `${base}${sep}Abrax${sep}models`);
    setAbierto(false);
  };

  const volverPorDefecto = () => {
    updateSetting("models_dir", null);
    setAbierto(false);
  };

  if (!carpeta) return null;

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
          onClick={() => setAbierto(!abierto)}
        >
          {t("carpetaModelos.cambiar")}
        </Button>
      </div>

      {abierto && (
        <div className="space-y-1">
          {discos.map((d) => (
            <button
              key={d.punto_montaje}
              type="button"
              onClick={() => elegirDisco(d)}
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
            <Button variant="ghost" size="sm" onClick={volverPorDefecto}>
              {t("carpetaModelos.volverPorDefecto")}
            </Button>
          )}
        </div>
      )}

      {carpeta.personalizada && (
        <div className="opacity-70" aria-live="polite">
          {t("carpetaModelos.avisoNoMueve")}
        </div>
      )}
    </div>
  );
});

CarpetaModelos.displayName = "CarpetaModelos";
