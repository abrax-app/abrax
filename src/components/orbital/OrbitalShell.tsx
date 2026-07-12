import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { X } from "lucide-react";
import AbraxLogo from "../icons/AbraxLogo";
import AccessibilityPermissions from "../AccessibilityPermissions";
import AlertsBanner from "../AlertsBanner";
import { WhatsNewGate } from "../whats-new";
import { SECTIONS_CONFIG, type SidebarSection } from "../Sidebar";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { formatKeyCombination } from "@/lib/utils/keyboard";
import { commands } from "@/bindings";
import {
  montarEsfera,
  colorDeg,
  readOrbitalPalette,
  type EsferaHandle,
  type OrbitalPalette,
} from "./esferaHome";
import { WindowControls } from "./WindowControls";
import "./orbital.css";

interface OrbitalShellProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

// Espacio de coordenadas del SVG (mismo que el canvas de esferaHome).
const SZ = 740;
const CTR = SZ / 2;
const R_NODE = 300;
const R_LABEL = 346;

/** Coordenadas polares (0° = arriba). */
const pol = (deg: number, r: number): [number, number] => {
  const a = ((deg - 90) * Math.PI) / 180;
  return [CTR + r * Math.cos(a), CTR + r * Math.sin(a)];
};

export const OrbitalShell: React.FC<OrbitalShellProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const osType = useOsType();

  const cvRef = useRef<HTMLCanvasElement>(null);
  const esf = useRef<EsferaHandle | null>(null);

  // Sección abierta en el iris (null = esfera desnuda). `sel` es el nodo
  // enfocado por teclado/hover cuando el iris está cerrado.
  const [abierta, setAbierta] = useState<SidebarSection | null>(null);
  const [sel, setSel] = useState(-1);
  const [pal, setPal] = useState<OrbitalPalette>(() => readOrbitalPalette());

  // Secciones reales disponibles (mismo registro y filtro que la barra lateral,
  // así el iris hospeda exactamente lo que existe: gated como postproc/debug
  // solo aparecen cuando corresponde).
  const nodos = useMemo(
    () =>
      (
        Object.entries(SECTIONS_CONFIG) as [
          SidebarSection,
          (typeof SECTIONS_CONFIG)[SidebarSection],
        ][]
      )
        .filter(([, c]) => c.enabled(settings))
        .map(([id, c], i, arr) => ({
          id,
          labelKey: c.labelKey,
          Icon: c.icon,
          deg: (i * 360) / arr.length,
        })),
    [settings],
  );

  // Motor de la esfera: montar una vez.
  useEffect(() => {
    if (!cvRef.current) return;
    esf.current = montarEsfera(cvRef.current);
    return () => {
      esf.current?.destroy();
      esf.current = null;
    };
  }, []);

  // Iris abierto/cerrado → apertura de la esfera.
  useEffect(() => {
    esf.current?.setApertura(abierta !== null);
  }, [abierta]);

  // Nodo enfocado → sector dominante del anillo.
  useEffect(() => {
    const deg = sel >= 0 && sel < nodos.length ? nodos[sel].deg : null;
    esf.current?.setSector(deg);
  }, [sel, nodos]);

  // Re-teñir cuando cambia la paleta (`ui_theme`). El atributo data-ui-theme lo
  // fija el selector de paleta; observamos ese cambio y re-leemos los tokens.
  useEffect(() => {
    const root = document.documentElement;
    const reteñir = () => {
      esf.current?.setPalette();
      setPal(readOrbitalPalette());
    };
    reteñir();
    const obs = new MutationObserver(reteñir);
    obs.observe(root, {
      attributes: true,
      attributeFilter: ["data-ui-theme", "data-theme"],
    });
    return () => obs.disconnect();
  }, [settings?.ui_theme]);

  // La esfera reacciona al dictado REAL: sondeo ligero de `isRecording` (sin
  // abrir una segunda captura de micrófono ni suscribirse al espectro 24/7 —
  // la esfera del home está "sin audio en reposo").
  useEffect(() => {
    let vivo = true;
    let last = false;
    const tick = async () => {
      try {
        const rec = await commands.isRecording();
        if (vivo && rec !== last) {
          last = rec;
          esf.current?.setGrabando(rec);
        }
      } catch {
        // ignorar (comando puede no estar listo en el arranque)
      }
    };
    const id = setInterval(tick, 200);
    void tick();
    return () => {
      vivo = false;
      clearInterval(id);
    };
  }, []);

  // Teclado: ←→↑↓ rotan la selección, Enter abre, Esc cierra el iris.
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (abierta !== null) {
          e.preventDefault();
          setAbierta(null);
        }
        return;
      }
      if (abierta !== null || nodos.length === 0) return;
      if (e.key === "ArrowRight" || e.key === "ArrowDown") {
        e.preventDefault();
        setSel((s) => ((s < 0 ? -1 : s) + 1) % nodos.length);
      } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
        e.preventDefault();
        setSel((s) => ((s < 0 ? 1 : s) + nodos.length - 1) % nodos.length);
      } else if (e.key === "Enter" && sel >= 0 && sel < nodos.length) {
        e.preventDefault();
        abrir(nodos[sel].id);
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [abierta, sel, nodos]);

  const abrir = (id: SidebarSection) => {
    onSectionChange(id);
    setAbierta(id);
  };

  const onCanvasClick = () => {
    if (abierta !== null) {
      setAbierta(null);
      return;
    }
    // Clic en la esfera = dictar (mismo toggle que el atajo global).
    void commands.triggerTranscription();
  };

  const atajo = formatKeyCombination(
    settings?.bindings?.transcribe?.current_binding ?? "",
    osType,
  );

  const Activa = abierta ? SECTIONS_CONFIG[abierta].component : null;

  return (
    <div id="orbital-stage" className="select-none">
      <div className="orbital-backdrop" aria-hidden="true" />
      <div className="orbital-dragstrip" data-tauri-drag-region />

      <div className="orbital-marca" data-tauri-drag-region>
        <AbraxLogo variant="horizontal" width={132} />
      </div>
      <WindowControls />

      <WhatsNewGate />
      <div className="orbital-alerts">
        <AlertsBanner />
        <AccessibilityPermissions />
      </div>

      <div id="orbital-orbe">
        <canvas
          ref={cvRef}
          className="orbital-canvas"
          onClick={onCanvasClick}
          aria-hidden="true"
        />

        <svg
          viewBox={`0 0 ${SZ} ${SZ}`}
          className="orbital-menu"
          role="menu"
          aria-label={t("orbital.menu")}
        >
          {nodos.map((n, k) => {
            const [nx, ny] = pol(n.deg, R_NODE);
            const [lx, ly] = pol(n.deg, R_LABEL);
            const c = colorDeg(-90 + n.deg, pal);
            const selected = sel === k;
            const Icon = n.Icon;
            return (
              <g
                key={n.id}
                className={`orbital-nodo${selected ? " sel" : ""}`}
                style={{ color: c }}
                role="menuitem"
                tabIndex={0}
                aria-label={t(n.labelKey)}
                onClick={() => abrir(n.id)}
                onMouseEnter={() => abierta === null && setSel(k)}
                onMouseLeave={() => abierta === null && setSel(-1)}
                onFocus={() => abierta === null && setSel(k)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    abrir(n.id);
                  }
                }}
              >
                <circle className="halo" cx={nx} cy={ny} r={30} />
                <circle className="aro" cx={nx} cy={ny} r={26} />
                <g
                  className="glifo"
                  transform={`translate(${nx - 11} ${ny - 11})`}
                >
                  <Icon width={22} height={22} />
                </g>
                <text className="etq" x={lx} y={ly + 4} textAnchor="middle">
                  {t(n.labelKey)}
                </text>
              </g>
            );
          })}
        </svg>

        <div
          id="orbital-iris"
          className={abierta ? "abierto" : ""}
          aria-hidden={abierta === null}
        >
          <button
            type="button"
            className="orbital-iris-cerrar"
            aria-label={t("orbital.close")}
            title={t("orbital.close")}
            onClick={() => setAbierta(null)}
          >
            <X size={16} aria-hidden="true" />
          </button>
          <div className="orbital-iris-body">{Activa && <Activa />}</div>
        </div>
      </div>

      <div className="orbital-hint" aria-live="polite">
        {atajo
          ? t("orbital.hint", { atajo })
          : t("orbital.hintNoBinding")}
      </div>
    </div>
  );
};

OrbitalShell.displayName = "OrbitalShell";

export default OrbitalShell;
