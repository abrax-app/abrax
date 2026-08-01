import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export interface DropdownOption {
  value: string;
  label: string;
  disabled?: boolean;
  /**
   * Marca a la izquierda del rótulo (p. ej. ✓ descargado / ↓ hay que
   * descargar). Es DECORATIVA: lo que diga tiene que ir también en `estado`,
   * porque un icono a secas no lo lee un lector de pantalla ni lo entiende
   * quien no distinga los colores.
   */
  icon?: React.ReactNode;
  /** Lo que significa el icono, en palabras: va al nombre accesible y al hint. */
  estado?: string;
}

interface DropdownProps {
  options: DropdownOption[];
  className?: string;
  selectedValue: string | null;
  onSelect: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  onRefresh?: () => void;
  /**
   * Nombre accesible cuando NO hay rótulo visible al lado. Sin esto, un
   * desplegable suelto se anuncia solo con su valor («Nemotron Streaming 3.5»)
   * y no dice de qué es. Mismo patrón que ya usa `Select`.
   */
  ariaLabel?: string;
}

export const Dropdown: React.FC<DropdownProps> = ({
  options,
  selectedValue,
  onSelect,
  className = "",
  placeholder,
  disabled = false,
  onRefresh,
  ariaLabel,
}) => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const selectedOption = options.find(
    (option) => option.value === selectedValue,
  );

  const handleSelect = (value: string) => {
    onSelect(value);
    setIsOpen(false);
  };

  const handleToggle = () => {
    if (disabled) return;
    if (!isOpen && onRefresh) onRefresh();
    setIsOpen(!isOpen);
  };

  return (
    <div className={`relative ${className}`} ref={dropdownRef}>
      <button
        type="button"
        className={`px-2 py-[5px] text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 rounded-md min-w-[200px] w-full text-start grid grid-cols-[1fr_auto] gap-2 items-center transition-all duration-150 ${
          disabled
            ? "opacity-50 cursor-not-allowed"
            : "hover:bg-logo-primary/10 cursor-pointer hover:border-logo-primary"
        }`}
        onClick={handleToggle}
        disabled={disabled}
        aria-label={
          ariaLabel && selectedOption
            ? `${ariaLabel}: ${selectedOption.label}`
            : ariaLabel
        }
        title={ariaLabel}
      >
        {/* El hueco se ve VACÍO cuando lo está: con el mismo peso y color que
            un valor elegido, un desplegable sin elegir se leía como elegido. */}
        <span className="flex items-center gap-1.5 min-w-0">
          {selectedOption?.icon}
          <span
            className={`truncate ${selectedOption ? "" : "font-normal text-text/45"}`}
          >
            {selectedOption?.label || placeholder || t("common.selectOption")}
          </span>
        </span>
        <svg
          className={`w-4 h-4 transition-transform duration-200 ${isOpen ? "transform rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>
      {isOpen && !disabled && (
        <div className="absolute top-full left-0 right-0 mt-1 bg-background border border-mid-gray/80 rounded-md shadow-lg z-50 max-h-60 overflow-y-auto">
          {options.length === 0 ? (
            <div className="px-2 py-1 text-sm text-mid-gray">
              {t("common.noOptionsFound")}
            </div>
          ) : (
            options.map((option) => (
              <button
                key={option.value}
                type="button"
                className={`w-full px-2 py-1 text-sm text-start hover:bg-logo-primary/10 transition-colors duration-150 ${
                  selectedValue === option.value
                    ? "bg-logo-primary/20 font-semibold"
                    : ""
                } ${option.disabled ? "opacity-50 cursor-not-allowed" : ""}`}
                onClick={() => handleSelect(option.value)}
                disabled={option.disabled}
                // Una línea por opción, como ya hacía el botón de arriba: el
                // ajuste anterior partía «Whisper Large v3 Turbo · descargar»
                // en dos renglones y una lista de cuatro ocupaba ocho. Lo que
                // no quepa sigue entero en el hint, no se pierde.
                title={
                  option.estado
                    ? `${option.label} · ${option.estado}`
                    : option.label
                }
                aria-label={
                  option.estado
                    ? `${option.label} · ${option.estado}`
                    : undefined
                }
              >
                <span className="flex items-center gap-1.5 min-w-0">
                  {option.icon}
                  <span className="truncate">{option.label}</span>
                </span>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
};
