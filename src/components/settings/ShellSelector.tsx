import React from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { applyShell, UI_SHELL_OPTIONS } from "@/lib/utils/theme";
import type { UiShell } from "@/bindings";

interface ShellSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Selector de shell (`ui_shell`), el eje de FORMA de la ventana, ortogonal al
 * tema (claro/oscuro) y a la paleta (`ui_theme`). `classic` es el default y el
 * fallback; `retro` es un shell sin marco. El contenido del shell
 * cambia al instante (preview); el marco transparente real de la ventana se
 * decide al construirla, así que se completa al reiniciar Abrax (lo dice la
 * descripción). No se traducen los nombres (son de marca), igual que la paleta.
 */
export const ShellSelector: React.FC<ShellSelectorProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { settings, updateSetting } = useSettings();

    const currentShell: UiShell = settings?.ui_shell ?? "classic";

    const shellOptions = UI_SHELL_OPTIONS.map((value) => ({
      value,
      label: t(`shell.options.${value}`),
    }));

    const handleShellChange = (value: string) => {
      const shell = value as UiShell;
      // Preview inmediato: aplica el atributo data-shell y ajusta el marco al
      // vuelo (la transparencia se completa al reiniciar). Maximizar se
      // habilita/deshabilita aquí también: la ventana pudo crearse con otro
      // shell (p. ej. arrancó en Clásico cuando Clásico no era maximizable) y
      // el botón □ quedaría muerto hasta reiniciar.
      applyShell(shell);
      try {
        const win = getCurrentWindow();
        void win.setDecorations(shell === "classic");
        void win.setMaximizable(shell !== "retro");
      } catch {
        // Ventana no disponible (p. ej. fuera de Tauri): sin preview de marco.
      }
      updateSetting("ui_shell", shell);
    };

    return (
      <SettingContainer
        title={t("shell.title")}
        description={t("shell.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={shellOptions}
          selectedValue={currentShell}
          onSelect={handleShellChange}
        />
      </SettingContainer>
    );
  },
);

ShellSelector.displayName = "ShellSelector";
