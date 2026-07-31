import React from "react";
import { useTranslation } from "react-i18next";
import { MoveRight, X } from "lucide-react";
import { useSettings } from "../../hooks/useSettings";
import { useOsType } from "../../hooks/useOsType";
import { Button } from "../ui/Button";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import type { ParMemoria } from "@/bindings";

interface MemoriaSettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Memoria de correcciones: lista de pares `de → a` que ABRAX aprendió de las
 * ediciones del usuario en el Historial, con su toggle y borrado individual o
 * total. El aprendizaje ocurre al guardar una edición en Historial; aquí solo
 * se administra lo aprendido.
 */
export const MemoriaSettings: React.FC<MemoriaSettingsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const osType = useOsType();
    const activa = getSetting("memoria_activa") ?? true;
    const pares: ParMemoria[] = getSetting("memoria_correcciones") ?? [];

    const olvidar = (de: string) => {
      updateSetting(
        "memoria_correcciones",
        pares.filter((p) => p.de !== de),
      );
    };

    return (
      <>
        <ToggleSwitch
          checked={activa}
          onChange={(value) => updateSetting("memoria_activa", value)}
          isUpdating={isUpdating("memoria_activa")}
          label={t("settings.advanced.memoria.title")}
          description={t("settings.advanced.memoria.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {/* Aprender EN EL SITIO es solo Windows: lee el campo de texto enfocado
            por UI Automation, y fuera de Windows `leer_texto_enfocado` devuelve
            None siempre (`memoria_en_sitio.rs`, ramas `#[cfg(not(windows))]`).
            El interruptor estaba SIEMPRE visible y encendido de fábrica, así que
            en macOS y Linux prometía algo que no podía cumplir — un control que
            se deja pulsar y no hace nada es peor que no tenerlo.
            No se toca el ajuste del backend: sigue existiendo con su valor y ahí
            es un no-op inofensivo. Solo deja de ofrecerse donde no funciona,
            calcado de cómo `TypingTool` se oculta fuera de Linux. */}
        {osType === "windows" && (
          <ToggleSwitch
            checked={activa && (getSetting("memoria_en_sitio") ?? true)}
            onChange={(value) => updateSetting("memoria_en_sitio", value)}
            disabled={!activa}
            isUpdating={isUpdating("memoria_en_sitio")}
            label={t("settings.advanced.memoria.enSitioTitle")}
            description={t("settings.advanced.memoria.enSitioDescription")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          />
        )}
        <div
          className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} space-y-1`}
        >
          {pares.length === 0 ? (
            <p className="text-sm text-text/50">
              {t("settings.advanced.memoria.empty")}
            </p>
          ) : (
            <>
              {pares.map((par) => (
                <div
                  key={par.de}
                  className="flex items-center gap-2 text-sm py-0.5"
                >
                  <span className="text-text/60 line-through decoration-text/30 break-all">
                    {par.de}
                  </span>
                  <MoveRight className="w-3.5 h-3.5 shrink-0 text-text/40 rtl:-scale-x-100" />
                  <span className="font-medium break-all">{par.a}</span>
                  {(par.veces ?? 1) > 1 && (
                    <span className="text-xs text-text/40 whitespace-nowrap">
                      ×{par.veces}
                    </span>
                  )}
                  <button
                    type="button"
                    onClick={() => olvidar(par.de)}
                    className="ms-auto p-1 rounded text-text/40 hover:text-logo-primary cursor-pointer"
                    title={t("settings.advanced.memoria.forget")}
                    aria-label={t("settings.advanced.memoria.forget")}
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
              <div className="pt-1">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => updateSetting("memoria_correcciones", [])}
                  disabled={isUpdating("memoria_correcciones")}
                >
                  {t("settings.advanced.memoria.clearAll")}
                </Button>
              </div>
            </>
          )}
        </div>
      </>
    );
  },
);

MemoriaSettings.displayName = "MemoriaSettings";
