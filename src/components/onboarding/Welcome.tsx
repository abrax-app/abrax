import React from "react";
import { useTranslation } from "react-i18next";
import { ShieldCheck, CloudOff, Gift } from "lucide-react";
import AbraxLogo from "../icons/AbraxLogo";
import { Button } from "../ui/Button";

interface WelcomeProps {
  onContinue: () => void;
}

/**
 * Pantalla de bienvenida — el PRIMER contacto con el producto.
 *
 * Antes, lo primero que veía alguien recién instalado era una lista de modelos
 * para descargar: sin logo, sin lema y sin una línea que dijera qué es esto. En
 * la medición sobre un Windows limpio salió textual —«falta pantalla de
 * bienvenida, para saber qué es ABRAX»— junto con «abruma mucho».
 *
 * Por eso esta pantalla dice tres cosas y nada más: qué es, que es privado, y
 * recién entonces por qué hace falta descargar un modelo. El botón es el único
 * elemento interactivo: no hay nada que decidir todavía.
 */
export const Welcome: React.FC<WelcomeProps> = ({ onContinue }) => {
  const { t } = useTranslation();

  const insignias = [
    { icon: ShieldCheck, label: t("onboarding.welcome.badgeLocal") },
    { icon: CloudOff, label: t("onboarding.welcome.badgeNoCloud") },
    { icon: Gift, label: t("onboarding.welcome.badgeFree") },
  ];

  return (
    <div className="h-screen w-screen flex flex-col items-center justify-center gap-7 p-8 text-center">
      <AbraxLogo width={280} withTagline />

      <p className="text-lg text-text/85 max-w-md font-medium">
        {t("onboarding.welcome.pitch")}
      </p>

      <div className="flex flex-wrap items-center justify-center gap-2.5">
        {insignias.map(({ icon: Icon, label }) => (
          <span
            key={label}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-mid-gray/10 border border-mid-gray/25 text-sm text-text/70"
          >
            <Icon className="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
            {label}
          </span>
        ))}
      </div>

      <div className="flex flex-col items-center gap-3 pt-1">
        {/* El modelo se pide DESPUÉS y explicando por qué: pedir una descarga de
            cientos de MB sin decir para qué es lo que hacía la versión anterior. */}
        <p className="text-sm text-text/55 max-w-sm">
          {t("onboarding.welcome.whyModel")}
        </p>
        <Button variant="primary" size="lg" onClick={onContinue}>
          {t("onboarding.welcome.start")}
        </Button>
      </div>
    </div>
  );
};

export default Welcome;
