import React from "react";
import type { LucideIcon } from "lucide-react";
import "./Chip.css";

// Chip moderno y plano: icono + rótulo + estado. Como TOGGLE (punto encendido)
// o como SELECTOR (muestra el valor actual y cicla al click). Siempre funcional
// y siempre rotulado (claro y fácil de entender).

interface Props {
  icon: LucideIcon;
  label: string;
  value?: string; // si se define → selector (muestra valor); si no → toggle
  active?: boolean;
  onClick: () => void;
}

export const Chip: React.FC<Props> = ({
  icon: Icon,
  label,
  value,
  active,
  onClick,
}) => {
  const isSelector = value !== undefined;
  return (
    <button
      type="button"
      className={`bnc-chip${active ? " on" : ""}`}
      onClick={onClick}
      aria-pressed={isSelector ? undefined : !!active}
      aria-label={isSelector ? `${label}: ${value}` : label}
      title={isSelector ? `${label}: ${value}` : label}
    >
      <Icon size={15} aria-hidden="true" className="bnc-chip-ic" />
      <span className="bnc-chip-lbl">{label}</span>
      {isSelector ? (
        <span className="bnc-chip-val">{value}</span>
      ) : (
        <span
          className={`bnc-chip-dot${active ? " on" : ""}`}
          aria-hidden="true"
        />
      )}
    </button>
  );
};
