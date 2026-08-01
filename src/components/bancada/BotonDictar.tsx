import React from "react";
import { Mic, Square } from "lucide-react";

// Botón de dictado HUD: pill grande, claro y con glow. El gesto principal.
// Al grabar cambia a "detener" (icono cuadrado + pulso). Dispara triggerTranscription.

interface Props {
  grabando: boolean;
  onToggle: () => void;
  label: string; // texto visible (Dictar / Detener)
  aria: string; // nombre accesible completo
  hint?: string; // atajo (p.ej. "Ctrl + Space")
}

export const BotonDictar: React.FC<Props> = ({
  grabando,
  onToggle,
  label,
  aria,
  hint,
}) => (
  <button
    type="button"
    className={`bnc-dictar${grabando ? " rec" : ""}`}
    onClick={onToggle}
    aria-label={aria}
    aria-pressed={grabando}
    title={aria}
  >
    <span className="bnc-dictar-ic" aria-hidden="true">
      {grabando ? <Square size={20} /> : <Mic size={22} />}
    </span>
    <span className="bnc-dictar-lbl">{label}</span>
    {hint ? <span className="bnc-dictar-hint">{hint}</span> : null}
  </button>
);
