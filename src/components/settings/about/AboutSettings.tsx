import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Button } from "../../ui/Button";
import { AppDataDirectory } from "../AppDataDirectory";
import { AppLanguageSelector } from "../AppLanguageSelector";
import { ShowWhatsNewOnUpdate } from "../ShowWhatsNewOnUpdate";
import { PaletteSelector } from "../PaletteSelector";
import { ShellSelector } from "../ShellSelector";
import { ThemeSelector } from "../ThemeSelector";
import { LogDirectory } from "../debug";
import AbraxLogo from "../../icons/AbraxLogo";

/**
 * Atribución de los modelos ASR que Abrax ofrece descargar.
 *
 * `canary-180m-flash` es **CC-BY-4.0** y es el modelo recomendado por defecto:
 * esa licencia EXIGE dar crédito, así que la atribución tiene que estar donde el
 * usuario pueda verla, no solo en un archivo del repositorio. El texto completo
 * vive en `LICENSES-THIRD-PARTY.md`, que ahora sí viaja en el instalador.
 *
 * Nombres, autores y licencias son nombres propios: no pasan por `t()`.
 */
const MODELOS_ASR = [
  {
    nombre: "Canary 180M Flash",
    autor: "NVIDIA Corporation",
    lic: "CC-BY-4.0",
  },
  { nombre: "Whisper Large v3 Turbo", autor: "OpenAI", lic: "Apache-2.0" },
  { nombre: "Whisper Large v3", autor: "OpenAI", lic: "Apache-2.0" },
  {
    nombre: "Nemotron Streaming 3.5",
    autor: "NVIDIA Corporation",
    lic: "OpenMDW-1.1",
  },
  { nombre: "Cohere Transcribe", autor: "Cohere Labs", lic: "Apache-2.0" },
];

export const AboutSettings: React.FC = () => {
  const { t } = useTranslation();
  const [version, setVersion] = useState("");

  useEffect(() => {
    const fetchVersion = async () => {
      try {
        const appVersion = await getVersion();
        setVersion(appVersion);
      } catch (error) {
        console.error("Failed to get app version:", error);
        setVersion("");
      }
    };

    fetchVersion();
  }, []);

  // Las donaciones van al proyecto original (Handy, de CJ Pais): crédito
  // conservado, sin confundir a nadie sobre a quién apoya.
  const handleDonateClick = async () => {
    try {
      await openUrl("https://handy.computer/donate");
    } catch (error) {
      console.error("Failed to open donate link:", error);
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="flex justify-center pt-2 text-text">
        <AbraxLogo width={240} withTagline />
      </div>
      <SettingsGroup title={t("settings.about.title")}>
        <AppLanguageSelector descriptionMode="tooltip" grouped={true} />
        <ThemeSelector descriptionMode="tooltip" grouped={true} />
        <PaletteSelector descriptionMode="tooltip" grouped={true} />
        <ShellSelector descriptionMode="tooltip" grouped={true} />
        <SettingContainer
          title={t("settings.about.version.title")}
          description={t("settings.about.version.description")}
          grouped={true}
        >
          {/* eslint-disable-next-line i18next/no-literal-string */}
          <span className="text-sm font-mono">v{version}</span>
        </SettingContainer>

        <ShowWhatsNewOnUpdate descriptionMode="tooltip" grouped={true} />

        <SettingContainer
          title={t("settings.about.supportDevelopment.title")}
          description={t("settings.about.supportDevelopment.description")}
          grouped={true}
        >
          <Button variant="primary" size="md" onClick={handleDonateClick}>
            {t("settings.about.supportDevelopment.button")}
          </Button>
        </SettingContainer>

        <SettingContainer
          title={t("settings.about.sourceCode.title")}
          description={t("settings.about.sourceCode.description")}
          grouped={true}
        >
          <Button
            variant="secondary"
            size="md"
            onClick={() => openUrl("https://github.com/abrax-app/abrax")}
          >
            {t("settings.about.sourceCode.button")}
          </Button>
        </SettingContainer>

        <AppDataDirectory descriptionMode="tooltip" grouped={true} />
        <LogDirectory grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.about.acknowledgments.title")}>
        <SettingContainer
          title={t("settings.about.acknowledgments.upstream.title")}
          description={t("settings.about.acknowledgments.upstream.description")}
          grouped={true}
          layout="stacked"
        >
          <div className="text-sm text-mid-gray">
            {t("settings.about.acknowledgments.upstream.details")}
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.about.acknowledgments.ggml.title")}
          description={t("settings.about.acknowledgments.ggml.description")}
          grouped={true}
          layout="stacked"
        >
          <div className="text-sm text-mid-gray">
            {t("settings.about.acknowledgments.ggml.details")}
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.about.acknowledgments.modelos.title")}
          description={t("settings.about.acknowledgments.modelos.description")}
          grouped={true}
          layout="stacked"
        >
          <ul className="text-sm text-mid-gray space-y-0.5">
            {MODELOS_ASR.map((m) => (
              <li
                key={m.nombre}
                className="flex flex-wrap items-baseline gap-x-2"
              >
                <span className="text-text">{m.nombre}</span>
                <span>{m.autor}</span>
                <span className="font-mono text-xs opacity-80">{m.lic}</span>
              </li>
            ))}
          </ul>
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
