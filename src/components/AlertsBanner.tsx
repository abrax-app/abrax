import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ChevronDown, ChevronUp, Info, X } from "lucide-react";
import type { AlertKind } from "@/bindings";
import { useAlertsStore } from "../stores/alertsStore";
import { Button } from "./ui/Button";

/**
 * Entrada visible del centro de errores (F1): si algo falló — incluso con la
 * app en la bandeja — este banner lo muestra al abrir la ventana, con hora y
 * detalle, hasta que el usuario lo descarte.
 */

// Título localizado por tipo de alerta (las mismas claves que usan los toasts).
/** Avisos que NO son fallos: la app contando algo que hizo, no algo que fallo. */
const KINDS_INFORMATIVOS = new Set<AlertKind>([
  "sistema_modelo_cambiado_sin_vivo",
]);

export const alertTitleKey = (kind: AlertKind): string => {
  switch (kind) {
    case "recording_permission_denied":
      return "errors.micPermissionDeniedTitle";
    case "recording_no_device":
      return "errors.noInputDeviceTitle";
    case "recording_no_audio":
      return "errors.recordingNoAudioTitle";
    case "recording_too_short":
      return "errors.recordingTooShortTitle";
    case "transcription_empty":
      return "errors.transcriptionEmptyTitle";
    case "sistema_sin_modelo_apto":
      return "errors.sistemaSinModeloAptoTitle";
    case "sistema_modelo_cambiado_sin_vivo":
      return "errors.sistemaModeloCambiadoTitle";
    case "shortcut_registration":
      return "errors.shortcutRegistrationTitle";
    // Mismo título: «No se pudo activar el atajo» describe exacto este caso
    // (otra app se quedó con la combinación). Reusarlo ahorra 22 traducciones
    // para un título de banner; el mensaje fino, que NOMBRA el atajo, vive en el
    // toast (`errors.atajoOcupado`), que sí tiene sitio.
    case "atajo_ocupado":
      return "errors.shortcutRegistrationTitle";
    case "recording":
      return "errors.recordingFailedTitle";
    case "transcription":
      return "errors.transcriptionFailedTitle";
    case "paste":
      return "errors.pasteFailedTitle";
    case "model_load":
      return "errors.modelLoadFailedTitle";
    case "model_download":
      return "errors.modelDownloadFailedTitle";
  }
};

const AlertsBanner: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { alerts, initialize, dismissAll } = useAlertsStore();
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    initialize();
  }, [initialize]);

  if (alerts.length === 0) return null;

  const fmtTime = (tsMs: number) =>
    new Date(tsMs).toLocaleTimeString(i18n.language, {
      hour: "2-digit",
      minute: "2-digit",
    });

  const latest = alerts[0];
  const visibleAlerts = expanded ? alerts : [latest];

  // El panel ya no guarda solo fallos: desde el 30/07 recibe tambien avisos
  // INFORMATIVOS —«se cambio el modelo para el audio del sistema»—, que son la
  // app contando algo que hizo bien. Pintarlos de rojo con un triangulo de
  // peligro es acusarla de un error que no cometio.
  //
  // Asi que el aspecto sigue al CONTENIDO: rojo y triangulo solo si hay al menos
  // un fallo de verdad; si son todos informativos, tono neutro e icono de
  // informacion. Cuando llegue un fallo real, el panel se pone rojo solo.
  const hayFallo = alerts.some((a) => !KINDS_INFORMATIVOS.has(a.kind));

  return (
    <div
      role={hayFallo ? "alert" : "status"}
      className={`w-full max-w-150 rounded-lg px-3 py-2 border ${
        hayFallo
          ? "border-red-500/30 bg-red-500/10"
          : "border-mid-gray/25 bg-mid-gray/10"
      }`}
    >
      <div className="flex items-center gap-2">
        {hayFallo ? (
          <AlertTriangle className="w-4 h-4 shrink-0 text-red-500" />
        ) : (
          <Info className="w-4 h-4 shrink-0 text-text/50" />
        )}
        <span className="text-sm font-medium text-text flex-1">
          {t("errors.center.title", { count: alerts.length })}
        </span>
        {alerts.length > 1 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setExpanded((e) => !e)}
            aria-expanded={expanded}
            aria-label={t("errors.center.toggle")}
          >
            {expanded ? (
              <ChevronUp className="w-4 h-4" />
            ) : (
              <ChevronDown className="w-4 h-4" />
            )}
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          onClick={() => dismissAll()}
          aria-label={t("errors.center.dismiss")}
        >
          <X className="w-4 h-4" />
        </Button>
      </div>
      <ul className="mt-1 space-y-1">
        {visibleAlerts.map((alert, i) => (
          <li
            key={`${alert.ts_ms}-${i}`}
            className="text-xs text-text/80 flex gap-2 items-baseline"
          >
            <span className="tabular-nums text-text/50 shrink-0">
              {fmtTime(alert.ts_ms)}
            </span>
            <span className="font-medium">{t(alertTitleKey(alert.kind))}</span>
            {alert.detail && (
              <span className="text-text/60 truncate" title={alert.detail}>
                {alert.detail}
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
};

export default AlertsBanner;
