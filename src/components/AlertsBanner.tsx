import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, ChevronDown, ChevronUp, X } from "lucide-react";
import type { AlertKind } from "@/bindings";
import { useAlertsStore } from "../stores/alertsStore";
import { Button } from "./ui/Button";

/**
 * Entrada visible del centro de errores (F1): si algo falló — incluso con la
 * app en la bandeja — este banner lo muestra al abrir la ventana, con hora y
 * detalle, hasta que el usuario lo descarte.
 */

// Título localizado por tipo de alerta (las mismas claves que usan los toasts).
export const alertTitleKey = (kind: AlertKind): string => {
  switch (kind) {
    case "recording_permission_denied":
      return "errors.micPermissionDeniedTitle";
    case "recording_no_device":
      return "errors.noInputDeviceTitle";
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

  return (
    <div
      role="alert"
      className="w-full max-w-150 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2"
    >
      <div className="flex items-center gap-2">
        <AlertTriangle className="w-4 h-4 shrink-0 text-red-500" />
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
