import React from "react";

// Botón de ARRANQUE/IGNICIÓN del shell Bancada: un <button> real (enfocable,
// aria-pressed) bajo una tapa abatible de seguridad estilo volante de F1 /
// cubierta de interruptor. Hover levanta la tapa (rotateX); click hunde el
// botón (translateY). Es el gesto héroe del video. La tapa es decorativa
// (pointer-events:none) para no bloquear el click. Dispara triggerTranscription.

interface Props {
  grabando: boolean;
  onToggle: () => void;
  label: string;
  sublabel?: string;
  /** Texto del núcleo del botón (START/STOP), ya localizado. */
  core: string;
}

export const BotonArranque: React.FC<Props> = ({
  grabando,
  onToggle,
  label,
  sublabel,
  core,
}) => {
  return (
    <div className={`bnc-ign${grabando ? " armed" : ""}`}>
      <div className="bnc-ign-slot">
        <div className="bnc-ign-cover" aria-hidden="true">
          <span className="bnc-ign-cover-face" />
          <span className="bnc-ign-cover-lip" />
        </div>
        <button
          type="button"
          className="bnc-ign-btn"
          onClick={onToggle}
          aria-pressed={grabando}
          aria-label={label}
          title={label}
        >
          <span className="bnc-ign-ring" aria-hidden="true" />
          <span className="bnc-ign-core">{core}</span>
        </button>
      </div>
      {sublabel ? <div className="bnc-ign-cap">{sublabel}</div> : null}
    </div>
  );
};
