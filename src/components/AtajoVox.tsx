import React from "react";
import { Trans, useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { formatKeyCombination } from "@/lib/utils/keyboard";

/**
 * El atajo de VOX, visible en TODOS los shells.
 *
 * VOX lee en voz alta lo que tengas seleccionado en cualquier aplicación, y esa
 * es su única forma de uso: no hay botón que pulsar porque el texto vive fuera
 * de ABRAX. Una función que solo existe si conoces una combinación de teclas es,
 * para quien no la conoce, una función que no existe — y hasta ahora la
 * combinación solo aparecía enterrada en Ajustes → General.
 *
 * Muestra el atajo REAL configurado, no uno escrito a mano: si el usuario lo
 * cambia, esta línea cambia con él. Y si se queda sin atajo, lo dice en vez de
 * enseñar un hueco.
 */
export const AtajoVox: React.FC<{ className?: string }> = ({
  className = "",
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();

  const atajo = formatKeyCombination(
    settings?.bindings?.leer_seleccion?.current_binding ?? "",
    osType,
  );

  return (
    <p className={`vox-hint ${className}`}>
      {atajo ? (
        <Trans
          i18nKey="vox.atajoHint"
          values={{ atajo }}
          components={{ k: <span className="vox-kbd" /> }}
        />
      ) : (
        t("vox.atajoSinBinding")
      )}
    </p>
  );
};

AtajoVox.displayName = "AtajoVox";
