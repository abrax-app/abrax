import React from "react";
import { useTranslation } from "react-i18next";
import { ShowOverlay } from "../ShowOverlay";
import { ModelUnloadTimeoutSetting } from "../ModelUnloadTimeout";
import { CustomWords } from "../CustomWords";
import { CustomFillerWords } from "../CustomFillerWords";
import { AutocorreccionSettings } from "../AutocorreccionSettings";
import { EmojiDictado } from "../EmojiDictado";
import { MemoriaSettings } from "../MemoriaSettings";
import { CustomReplacements } from "../CustomReplacements";
import { CorreccionSettings } from "../CorreccionSettings";
import { DictionarySettings } from "../DictionarySettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { StartHidden } from "../StartHidden";
import { AutostartToggle } from "../AutostartToggle";
import { ShowTrayIcon } from "../ShowTrayIcon";
import { PasteMethodSetting } from "../PasteMethod";
import { TypingToolSetting } from "../TypingTool";
import { ClipboardHandlingSetting } from "../ClipboardHandling";
import { AppendTrailingSpace } from "../AppendTrailingSpace";
import { HistoryLimit } from "../HistoryLimit";
import { RecordingRetentionPeriodSelector } from "../RecordingRetentionPeriod";
import { VoiceActivityDetection } from "../VoiceActivityDetection";

/**
 * El grupo «Corrección local» estuvo oculto tras esta bandera desde el 25/07
 * («se rediseña por separado»), y ese rediseño nunca volvió: cinco días con un
 * paquete entero —símbolos dictados, tildes, correos, tartamudeo, ortotipografía—
 * que nadie podía encender ni sabía que existía.
 *
 * Se destapa el 30/07 por decisión de producto. Lo que queda son dos ejes
 * deterministas (motor apagado/solo-reglas y modo literal/limpio), sin Ollama ni
 * LLM de por medio desde que se retiró el «Pulido con IA».
 *
 * La constante se deja en vez de borrarla: si algún día hay que volver a
 * esconderlo para una entrega, es una línea y no una cirugía.
 */
const MOSTRAR_CORRECCION: boolean = true;

export const AdvancedSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.advanced.groups.app")}>
        <StartHidden descriptionMode="tooltip" grouped={true} />
        <AutostartToggle descriptionMode="tooltip" grouped={true} />
        <ShowTrayIcon descriptionMode="tooltip" grouped={true} />
        <ShowOverlay descriptionMode="tooltip" grouped={true} />
        <ModelUnloadTimeoutSetting descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.advanced.groups.output")}>
        <PasteMethodSetting descriptionMode="tooltip" grouped={true} />
        <TypingToolSetting descriptionMode="tooltip" grouped={true} />
        <ClipboardHandlingSetting descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.advanced.groups.transcription")}>
        <VoiceActivityDetection descriptionMode="tooltip" grouped={true} />
        <CustomWords descriptionMode="tooltip" grouped />
        <CustomFillerWords descriptionMode="tooltip" grouped />
        <AutocorreccionSettings descriptionMode="tooltip" grouped />
        <EmojiDictado descriptionMode="tooltip" grouped />
        <AppendTrailingSpace descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.advanced.groups.memoria")}>
        <MemoriaSettings descriptionMode="tooltip" grouped />
      </SettingsGroup>

      {MOSTRAR_CORRECCION && (
        <SettingsGroup title={t("settings.advanced.groups.correccion")}>
          <CorreccionSettings descriptionMode="tooltip" grouped />
        </SettingsGroup>
      )}

      <SettingsGroup title={t("settings.advanced.groups.dictionary")}>
        <DictionarySettings descriptionMode="tooltip" grouped />
        <CustomReplacements descriptionMode="tooltip" grouped />
      </SettingsGroup>

      <SettingsGroup title={t("settings.advanced.groups.history")}>
        <HistoryLimit descriptionMode="tooltip" grouped={true} />
        <RecordingRetentionPeriodSelector
          descriptionMode="tooltip"
          grouped={true}
        />
      </SettingsGroup>
    </div>
  );
};
