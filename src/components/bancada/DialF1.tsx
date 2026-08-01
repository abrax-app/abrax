import React from "react";

// Dial rotativo estilo volante de F1 (rueda multiposición). SIEMPRE funcional:
// mapea un ajuste REAL con posiciones reales (p.ej. VAD OFF/ON, o la paleta
// ABRAX/IMPERIAL/ESCUDERÍA). Un click avanza a la siguiente posición. El puntero
// y los ticks reflejan SOLO las posiciones reales (nada de anillos 1-16 falsos).

export type DialColor = "blue" | "cyan" | "magenta" | "yellow";

interface Props {
  label: string;
  positions: string[];
  index: number;
  color: DialColor;
  onCycle: () => void;
}

// ángulo de la posición i en el arco de 270° abierto abajo (-135°..+135°)
const angleOf = (i: number, n: number) =>
  n > 1 ? (i / (n - 1)) * 270 - 135 : 0;

export const DialF1: React.FC<Props> = ({
  label,
  positions,
  index,
  color,
  onCycle,
}) => {
  const n = positions.length;
  return (
    <div className="bnc-dial">
      <button
        type="button"
        className={`bnc-dial-knob k-${color}`}
        onClick={onCycle}
        aria-label={`${label}: ${positions[index]}`}
        title={`${label}: ${positions[index]}`}
      >
        <span className="bnc-dial-ticks" aria-hidden="true">
          {positions.map((_, i) => (
            <span
              key={i}
              className={`bnc-dial-tick${i === index ? " on" : ""}`}
              style={{ transform: `rotate(${angleOf(i, n)}deg)` }}
            />
          ))}
        </span>
        <span className="bnc-dial-face" aria-hidden="true">
          <span
            className="bnc-dial-ptr"
            style={{ transform: `rotate(${angleOf(index, n)}deg)` }}
          />
        </span>
      </button>
      <span className="bnc-dial-label">{label}</span>
      <span className="bnc-dial-val">{positions[index]}</span>
    </div>
  );
};
