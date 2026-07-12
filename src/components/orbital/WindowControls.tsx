import React from "react";
import { useTranslation } from "react-i18next";
import { Minus, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Controles de ventana fantasma para los shells sin marco del SO (orbital /
 * retro): minimizar y "cerrar a la bandeja" (ocultar, NO salir — mismo
 * comportamiento que el botón de cerrar del shell clásico). La ventana
 * transparente pierde los controles nativos, así que los reponemos aquí.
 */
export const WindowControls: React.FC = () => {
  const { t } = useTranslation();
  const w = getCurrentWindow();

  return (
    <div className="orbital-winctrl">
      <button
        type="button"
        aria-label={t("orbital.controls.minimize")}
        title={t("orbital.controls.minimize")}
        onClick={() => void w.minimize()}
      >
        <Minus size={15} aria-hidden="true" />
      </button>
      <button
        type="button"
        aria-label={t("orbital.controls.hide")}
        title={t("orbital.controls.hide")}
        onClick={() => void w.hide()}
      >
        <X size={15} aria-hidden="true" />
      </button>
    </div>
  );
};

WindowControls.displayName = "WindowControls";
