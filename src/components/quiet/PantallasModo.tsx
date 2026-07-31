import React from "react";
import { Trans, useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { SettingsGroup } from "../ui/SettingsGroup";
import { ShortcutInput } from "../settings/ShortcutInput";
import { PushToTalk } from "../settings/PushToTalk";
import { AtajoVox } from "../AtajoVox";
import { MicrophoneSelector } from "../settings/MicrophoneSelector";
import { PruebaMicrofono } from "../settings/PruebaMicrofono";
import { VoiceActivityDetection } from "../settings/VoiceActivityDetection";
import { MuteWhileRecording } from "../settings/MuteWhileRecording";
import { CaptureSystemAudio } from "../settings/CaptureSystemAudio";
import { OutputDeviceSelector } from "../settings/OutputDeviceSelector";
import { ModelSettingsCard } from "../settings/general/ModelSettingsCard";
import { ModelsSettings, EscuchaSettings } from "../settings";
import { AutocorreccionSettings } from "../settings/AutocorreccionSettings";
import { EmojiDictado } from "../settings/EmojiDictado";
import { CustomFillerWords } from "../settings/CustomFillerWords";
import { CustomWords } from "../settings/CustomWords";
import { CustomReplacements } from "../settings/CustomReplacements";
import { AppendTrailingSpace } from "../settings/AppendTrailingSpace";
import { PasteMethodSetting } from "../settings/PasteMethod";
import { TypingToolSetting } from "../settings/TypingTool";
import { ClipboardHandlingSetting } from "../settings/ClipboardHandling";
import { DictionarySettings } from "../settings/DictionarySettings";
import { CorreccionSettings } from "../settings/CorreccionSettings";
import { MemoriaSettings } from "../settings/MemoriaSettings";

/**
 * Las pantallas de los TRES MODOS: Escucha, Streaming y VOX.
 *
 * Cada una reúne TODO lo de su modo y en el orden en que hace falta cuando no
 * sabes qué hace la app: primero qué es esto, después cómo se dispara, después
 * con qué te escucha (o con qué habla), después qué le pasa al resultado y por
 * último dónde acaba. Antes esos mismos ajustes estaban repartidos entre
 * «Avanzado» y la pantalla del modo, así que para configurar el dictado había
 * que ir a dos sitios y ninguno lo decía.
 *
 * `AdvancedSettings` NO se toca: la usan los otros tres shells y quitarle
 * grupos los dejaría sin ellos. Quiet compone aquí los suyos.
 */

/**
 * Encabezado de pantalla: el nombre del modo, en una línea qué hace, y —si el
 * modo se dispara con una tecla— cómo se dispara.
 *
 * Ese tercer renglón vivía en un hero aparte, centrado, encima de todo. El hero
 * se retiró: decía «ABRAX está listo» y debajo repetía en tarjetas los tres
 * modos que ya están en la barra lateral. Lo único que no sobraba de él era la
 * instrucción, y su sitio es este.
 */
const Intro: React.FC<{
  titulo: string;
  que: string;
  comoSeUsa?: React.ReactNode;
}> = ({ titulo, que, comoSeUsa }) => (
  <header className="px-4 pb-2">
    <h1 className="text-xl font-semibold">{titulo}</h1>
    <p className="text-sm text-text/60 mt-1 max-w-prose">{que}</p>
    {comoSeUsa && <p className="text-sm mt-2 max-w-prose">{comoSeUsa}</p>}
  </header>
);

/* ───────────────────────────────── ESCUCHA ────────────────────────────────── */

export const PantallaEscucha: React.FC<{ grabando?: boolean }> = ({
  grabando = false,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();
  const pushToTalk = settings?.push_to_talk ?? false;
  const esLinux = osType === "linux";
  const atajo = formatKeyCombination(
    settings?.bindings?.transcribe?.current_binding ?? "",
    osType,
  );

  // Al dispararse el atajo, esta línea cambia. Es la única forma de que un
  // fallo del hook de teclado se vea en el momento en que se está probando —y
  // de paso enseña el gesto de mantener sin que nadie lea nada.
  const comoSeUsa = grabando ? (
    <span className="q-hint on">{t("quiet.dictando")}</span>
  ) : atajo ? (
    <Trans
      i18nKey="quiet.holdHint"
      values={{ atajo }}
      components={{ k: <span className="q-kbd" /> }}
    />
  ) : (
    t("quiet.holdHintNoBinding")
  );

  return (
    <div className="max-w-3xl w-full space-y-6">
      {/* La MISMA cabecera que Streaming y VOX. Se probó sin ella —el hero de
          arriba ya da la bienvenida— y el resultado fue que Escucha parecía no
          haber cambiado: el hero es identico al de siempre, y las otras dos
          pantallas si abrian con su nombre y una linea de que hacen. Tres
          pantallas de modo tienen que empezar igual o no se leen como tres
          pantallas del mismo tipo. */}
      <Intro
        titulo={t("quiet.nav.listen")}
        que={t("quiet.modo.escucha.que")}
        comoSeUsa={comoSeUsa}
      />

      {/* 1. Cómo se dispara. Lo primero que necesita saber quien abre la app:
             qué tecla, y si hay que mantenerla o pulsarla. */}
      <SettingsGroup title={t("quiet.modo.grupo.gesto")}>
        <ShortcutInput shortcutId="transcribe" grouped={true} />
        <PushToTalk descriptionMode="tooltip" grouped={true} />
        {/* El de cancelar se oculta con pulsar-para-hablar (soltar ya cancela)
            y en Linux (los atajos dinámicos son inestables). */}
        {!esLinux && !pushToTalk && (
          <ShortcutInput shortcutId="cancel" grouped={true} />
        )}
      </SettingsGroup>

      {/* 2. Con qué te escucha. Va antes que el modelo porque un micrófono mal
             elegido no lo arregla ningún modelo — y la prueba de micrófono está
             aquí para descubrirlo en diez segundos. */}
      <SettingsGroup title={t("quiet.modo.grupo.microfono")}>
        <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
        <PruebaMicrofono descriptionMode="tooltip" grouped={true} />
        <VoiceActivityDetection descriptionMode="tooltip" grouped={true} />
        <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      {/* 3. Con qué lo entiende: el modelo, su idioma y su traducción. */}
      <ModelSettingsCard />
      <ModelsSettings capacidad="dictado" />

      {/* 4. Qué le pasa a lo que dijiste antes de aparecer escrito. */}
      <SettingsGroup title={t("quiet.modo.grupo.texto")}>
        <AutocorreccionSettings descriptionMode="tooltip" grouped />
        <EmojiDictado descriptionMode="tooltip" grouped />
        <CustomFillerWords descriptionMode="tooltip" grouped />
        <CustomWords descriptionMode="tooltip" grouped />
        <CustomReplacements descriptionMode="tooltip" grouped />
        <AppendTrailingSpace descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      {/* 5. Los tres subsistemas propios. Van al final y con su propio título
             porque no son interruptores: cada uno es una función entera. */}
      <SettingsGroup title={t("settings.advanced.groups.dictionary")}>
        <DictionarySettings descriptionMode="tooltip" grouped />
      </SettingsGroup>
      <SettingsGroup title={t("settings.advanced.groups.correccion")}>
        <CorreccionSettings descriptionMode="tooltip" grouped />
      </SettingsGroup>
      <SettingsGroup title={t("settings.advanced.groups.memoria")}>
        <MemoriaSettings descriptionMode="tooltip" grouped />
      </SettingsGroup>

      {/* 6. Dónde acaba el texto. Lo último, porque casi nadie lo cambia. */}
      <SettingsGroup title={t("quiet.modo.grupo.pegado")}>
        <PasteMethodSetting descriptionMode="tooltip" grouped={true} />
        <TypingToolSetting descriptionMode="tooltip" grouped={true} />
        <ClipboardHandlingSetting descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
    </div>
  );
};

/* ──────────────────────────────── STREAMING ───────────────────────────────── */

export const PantallaStreaming: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full space-y-6">
      <Intro
        titulo={t("quiet.nav.streaming")}
        que={t("quiet.modo.streaming.que")}
      />

      <SettingsGroup title={t("quiet.modo.grupo.fuente")}>
        <CaptureSystemAudio descriptionMode="inline" grouped />
      </SettingsGroup>

      {/* Solo los que sirven para audio del sistema; la lista la decide el
          backend (`aptitud_audio_sistema`), igual que en el panel «Atajos». */}
      <ModelsSettings capacidad="sistema" />

      {/* Decirlo evita el viaje en balde: quien busca aquí las muletillas o el
          diccionario no los va a encontrar, porque el texto pasa por el MISMO
          proceso que el dictado y se configura una sola vez. */}
      <p className="px-4 text-sm text-text/60 max-w-prose">
        {t("quiet.modo.streaming.resto")}
      </p>
    </div>
  );
};

/* ─────────────────────────────────── VOX ──────────────────────────────────── */

export const PantallaVox: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full space-y-6">
      <Intro titulo={t("sidebar.escucha")} que={t("quiet.modo.vox.que")} />

      <SettingsGroup title={t("quiet.modo.grupo.gesto")}>
        <ShortcutInput shortcutId="leer_seleccion" grouped={true} />
        <AtajoVox className="px-4 pb-2 text-sm text-text/60" />
      </SettingsGroup>

      {/* Por dónde suena. Este ajuste manda de verdad en VOX
          (`managers/tts/manager.rs:307`), no solo en los pitidos de la app, así
          que su sitio es este y no «Avanzado». */}
      <SettingsGroup title={t("quiet.modo.grupo.salida")}>
        <OutputDeviceSelector descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>

      {/* La voz, los símbolos y el lector: el panel propio de VOX, intacto. */}
      <EscuchaSettings />
    </div>
  );
};
