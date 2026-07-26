import React from "react";
import { useTranslation } from "react-i18next";

// Batería de testigos ("idiot lights") del tablero: RALENTÍ / REC /
// TRANSCRIBIENDO / PULIENDO / FALLA. Cada uno lleva ETIQUETA de texto además
// del color (no depende solo del color). Decorativo para lectores de pantalla:
// el estado real se anuncia por un aria-live en el shell.

export type Estado = "idle" | "rec" | "transcribiendo" | "puliendo" | "error";

// id interno → sufijo de clave i18n (bancada.lamp.*) + clase de color
const LAMPS: { id: Estado; key: string; cls: string }[] = [
  { id: "idle", key: "idle", cls: "ok" },
  { id: "rec", key: "rec", cls: "rec" },
  { id: "transcribiendo", key: "transcribing", cls: "work" },
  { id: "puliendo", key: "polishing", cls: "work2" },
  { id: "error", key: "error", cls: "err" },
];

/** Estado interno → sufijo de clave i18n (bancada.estado.*), para el aria-live. */
export const ESTADO_KEY: Record<Estado, string> = {
  idle: "idle",
  rec: "rec",
  transcribiendo: "transcribing",
  puliendo: "polishing",
  error: "error",
};

export const LamparasEstado: React.FC<{ estado: Estado }> = ({ estado }) => {
  const { t } = useTranslation();
  return (
    <div className="bnc-lamps" aria-hidden="true">
      {LAMPS.map((l) => (
        <div
          key={l.id}
          className={`bnc-lamp2 ${l.cls}${estado === l.id ? " on" : ""}`}
        >
          <span className="bnc-lamp2-dot" />
          <span className="bnc-lamp2-lbl">{t(`bancada.lamp.${l.key}`)}</span>
        </div>
      ))}
    </div>
  );
};
