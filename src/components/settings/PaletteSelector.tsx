import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { applyUiTheme, UI_THEME_OPTIONS } from "@/lib/utils/theme";
import type { UiTheme } from "@/bindings";

interface PaletteSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Selector de paleta (`ui_theme`), ortogonal al tema claro/oscuro. Los
 * nombres de paleta son marca, no copy: no se traducen (mismo valor en los
 * 22 locales). La descripción documenta que Imperial fuerza el modo oscuro.
 */
export const PaletteSelector: React.FC<PaletteSelectorProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { settings, updateSetting } = useSettings();

    const currentUiTheme: UiTheme = settings?.ui_theme ?? "abrax";

    const paletteOptions = UI_THEME_OPTIONS.map((value) => ({
      value,
      label: t(`palette.options.${value}`),
    }));

    const handlePaletteChange = (value: string) => {
      const uiTheme = value as UiTheme;
      applyUiTheme(uiTheme);
      updateSetting("ui_theme", uiTheme);
    };

    return (
      <SettingContainer
        title={t("palette.title")}
        description={t("palette.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={paletteOptions}
          selectedValue={currentUiTheme}
          onSelect={handlePaletteChange}
        />
      </SettingContainer>
    );
  },
);

PaletteSelector.displayName = "PaletteSelector";
