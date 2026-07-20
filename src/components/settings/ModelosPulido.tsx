import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../ui/Button";
import { commands } from "@/bindings";
import type { ModeloEstado } from "@/bindings";
import { bytesAGb } from "@/lib/utils/format";

interface ProgresoPayload {
  modelo_id: string;
  bajados: number;
  total: number;
}

/**
 * Catálogo de modelos LLM opcionales para el "Pulido con IA" local. Permite
 * descargar (con barra de progreso y cancelación), borrar y elegir el modelo
 * activo. Todo es opcional: sin un modelo elegido, la corrección usa Ollama (si
 * está) o las reglas deterministas. El sidecar corre en un proceso aislado y se
 * apaga por inactividad; nada arranca hasta que el usuario descarga y elige.
 */
export const ModelosPulido: React.FC = React.memo(() => {
  const { t } = useTranslation();
  const [modelos, setModelos] = useState<ModeloEstado[]>([]);
  const [progreso, setProgreso] = useState<Record<string, number>>({});
  const [error, setError] = useState<string | null>(null);

  const recargar = useCallback(() => {
    void commands.listarModelosCorreccion().then((res) => {
      if (res.status === "ok") setModelos(res.data);
    });
  }, []);

  useEffect(() => {
    recargar();
  }, [recargar]);

  useEffect(() => {
    const un = listen<ProgresoPayload>("correccion-modelo-progreso", (e) => {
      const { modelo_id, bajados, total } = e.payload;
      const pct = total > 0 ? Math.round((bajados / total) * 100) : 0;
      setProgreso((p) => ({ ...p, [modelo_id]: pct }));
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const descargar = async (id: string) => {
    setError(null);
    setProgreso((p) => ({ ...p, [id]: 0 }));
    recargar();
    const res = await commands.descargarModeloCorreccion(id);
    setProgreso((p) => {
      const q = { ...p };
      delete q[id];
      return q;
    });
    if (res.status === "error") setError(res.error);
    recargar();
  };

  const cancelar = (id: string) => void commands.cancelarDescargaCorreccion(id);
  const borrar = async (id: string) => {
    await commands.eliminarModeloCorreccion(id);
    recargar();
  };
  const elegir = async (id: string | null) => {
    await commands.seleccionarModeloCorreccion(id);
    recargar();
  };

  return (
    <div className="px-4 py-2 space-y-2">
      <div className="text-xs opacity-70">{t("pulidoModelos.intro")}</div>

      {modelos.map(({ modelo: m, descargado, descargando, seleccionado }) => {
        const pct = progreso[m.id] ?? 0;
        return (
          <div
            key={m.id}
            className="rounded-md border border-black/10 dark:border-white/10 p-2 text-xs"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium">
                {m.recomendado
                  ? t("pulidoModelos.nombreRecomendado", { nombre: m.nombre })
                  : m.nombre}
              </span>
              <span className="opacity-60 whitespace-nowrap">
                {t("pulidoModelos.tamanoLicencia", {
                  gb: bytesAGb(m.tamano_bytes),
                  licencia: m.licencia,
                })}
              </span>
            </div>

            <div className="opacity-70 mt-1">{m.descripcion}</div>

            <div className="mt-2 flex items-center gap-2">
              {descargando ? (
                <>
                  <div className="flex-1 h-1.5 rounded bg-black/10 dark:bg-white/10 overflow-hidden">
                    <div
                      className="h-full bg-black/40 dark:bg-white/50 transition-all"
                      style={{ width: `${pct}%` }}
                    />
                  </div>
                  <span className="opacity-60 tabular-nums">
                    {t("pulidoModelos.porcentaje", { pct })}
                  </span>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => cancelar(m.id)}
                  >
                    {t("pulidoModelos.cancelar")}
                  </Button>
                </>
              ) : descargado ? (
                <>
                  {seleccionado ? (
                    <span className="text-green-600 dark:text-green-400 font-medium">
                      {t("pulidoModelos.activo")}
                    </span>
                  ) : (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => void elegir(m.id)}
                    >
                      {t("pulidoModelos.usar")}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => void borrar(m.id)}
                    className="opacity-70"
                  >
                    {t("pulidoModelos.borrar")}
                  </Button>
                </>
              ) : (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => void descargar(m.id)}
                >
                  {t("pulidoModelos.descargar", {
                    gb: bytesAGb(m.tamano_bytes),
                  })}
                </Button>
              )}
            </div>
          </div>
        );
      })}

      {error && (
        <div className="text-red-600 dark:text-red-400" aria-live="polite">
          {t("pulidoModelos.errorDescarga", { error })}
        </div>
      )}
    </div>
  );
});

ModelosPulido.displayName = "ModelosPulido";
