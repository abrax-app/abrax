import React from "react";

// Botón redondo estilo volante de F1. SIEMPRE funcional (regla del repo: nada
// de botones sin comportamiento): cada uno dispara una acción real de dictado o
// alterna un ajuste real. `active` lo enciende (para toggles); `disabled` lo
// apaga cuando la acción no aplica.

export type F1Color =
  | "red"
  | "white"
  | "grey"
  | "blue"
  | "green"
  | "yellow"
  | "orange"
  | "cyan";

interface Props {
  color: F1Color;
  label: string; // nombre accesible + tooltip
  caption?: string; // rótulo corto bajo el botón
  onClick: () => void;
  active?: boolean;
  disabled?: boolean;
}

export const BotonF1: React.FC<Props> = ({
  color,
  label,
  caption,
  onClick,
  active,
  disabled,
}) => (
  <div className="bnc-f1w">
    <button
      type="button"
      className={`bnc-f1btn c-${color}${active ? " on" : ""}`}
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      aria-pressed={active}
      title={label}
    >
      <span className="bnc-f1btn-cap" aria-hidden="true" />
    </button>
    {caption ? <span className="bnc-f1cap">{caption}</span> : null}
  </div>
);
