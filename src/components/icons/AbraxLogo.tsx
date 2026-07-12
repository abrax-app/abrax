import React from "react";
import { Trans } from "react-i18next";

// Lockup ABRAX (isotipo + wordmark) del kit de marca aprobado (paleta v1
// cian→violeta→magenta). Las letras y el núcleo usan currentColor para
// adaptarse a tema claro/oscuro; el glifo y la X conservan el degradado
// de marca.
//
// Variantes: "horizontal" (isotipo a la izquierda del wordmark, 450×120) y
// "stacked" (isotipo sobre el wordmark, 300×232 — pensada para el sidebar).
// `withTagline` añade el lema del kit — « Di la palabra. » — con el acento
// cian del hero de la presentación (token --color-brand-cyan, ajustado por
// tema para conservar contraste).

const GLIFO_D =
  "M 92.75 60.00 L 93.33 61.31 L 93.92 62.67 L 94.47 64.08 L 94.95 65.53 L 95.31 67.02 L 95.52 68.53 L 95.55 70.03 L 95.38 71.50 L 95.01 72.92 L 94.44 74.26 L 93.66 75.52 L 92.71 76.67 L 91.62 77.71 L 90.40 78.63 L 89.10 79.45 L 87.76 80.17 L 86.42 80.82 L 85.09 81.43 L 83.82 82.02 L 82.62 82.62 L 81.50 83.25 L 80.46 83.95 L 79.50 84.73 L 78.60 85.60 L 77.75 86.57 L 76.92 87.62 L 76.10 88.74 L 75.24 89.92 L 74.34 91.11 L 73.37 92.28 L 72.32 93.39 L 71.18 94.41 L 69.96 95.30 L 68.65 96.02 L 67.27 96.55 L 65.84 96.86 L 64.38 96.96 L 62.90 96.85 L 61.44 96.54 L 60.00 96.05 L 58.61 95.41 L 57.27 94.67 L 55.99 93.85 L 54.77 93.01 L 53.60 92.17 L 52.47 91.38 L 51.35 90.67 L 50.24 90.05 L 49.10 89.53 L 47.94 89.13 L 46.72 88.82 L 45.43 88.59 L 44.09 88.41 L 42.68 88.26 L 41.22 88.10 L 39.73 87.90 L 38.23 87.61 L 36.75 87.22 L 35.32 86.70 L 33.97 86.03 L 32.73 85.21 L 31.64 84.22 L 30.70 83.10 L 29.94 81.84 L 29.36 80.47 L 28.96 79.02 L 28.72 77.52 L 28.61 75.99 L 28.63 74.46 L 28.72 72.96 L 28.85 71.49 L 28.98 70.08 L 29.08 68.72 L 29.12 67.41 L 29.07 66.15 L 28.91 64.92 L 28.64 63.71 L 28.26 62.50 L 27.79 61.27 L 27.25 60.00 L 26.67 58.69 L 26.08 57.33 L 25.53 55.92 L 25.05 54.47 L 24.69 52.98 L 24.48 51.47 L 24.45 49.97 L 24.62 48.50 L 24.99 47.08 L 25.56 45.74 L 26.34 44.48 L 27.29 43.33 L 28.38 42.29 L 29.60 41.37 L 30.90 40.55 L 32.24 39.83 L 33.58 39.18 L 34.91 38.57 L 36.18 37.98 L 37.38 37.38 L 38.50 36.75 L 39.54 36.05 L 40.50 35.27 L 41.40 34.40 L 42.25 33.43 L 43.08 32.38 L 43.90 31.26 L 44.76 30.08 L 45.66 28.89 L 46.63 27.72 L 47.68 26.61 L 48.82 25.59 L 50.04 24.70 L 51.35 23.98 L 52.73 23.45 L 54.16 23.14 L 55.62 23.04 L 57.10 23.15 L 58.56 23.46 L 60.00 23.95 L 61.39 24.59 L 62.73 25.33 L 64.01 26.15 L 65.23 26.99 L 66.40 27.83 L 67.53 28.62 L 68.65 29.33 L 69.76 29.95 L 70.90 30.47 L 72.06 30.87 L 73.28 31.18 L 74.57 31.41 L 75.91 31.59 L 77.32 31.74 L 78.78 31.90 L 80.27 32.10 L 81.77 32.39 L 83.25 32.78 L 84.68 33.30 L 86.03 33.97 L 87.27 34.79 L 88.36 35.78 L 89.30 36.90 L 90.06 38.16 L 90.64 39.53 L 91.04 40.98 L 91.28 42.48 L 91.39 44.01 L 91.37 45.54 L 91.28 47.04 L 91.15 48.51 L 91.02 49.92 L 90.92 51.28 L 90.88 52.59 L 90.93 53.85 L 91.09 55.08 L 91.36 56.29 L 91.74 57.50 L 92.21 58.73 L 92.75 60.00 Z";

const Defs = ({ idPrefix }: { idPrefix: string }) => (
  <defs>
    <linearGradient
      id={`${idPrefix}-glifo`}
      gradientUnits="userSpaceOnUse"
      x1="146"
      y1="10"
      x2="20"
      y2="110"
    >
      <stop offset="0" stopColor="#2FD9FF" />
      <stop offset="0.55" stopColor="#8B5CF6" />
      <stop offset="1" stopColor="#F23DC4" />
    </linearGradient>
    <linearGradient
      id={`${idPrefix}-x`}
      gradientUnits="userSpaceOnUse"
      x1="446"
      y1="26"
      x2="396"
      y2="94"
    >
      <stop offset="0" stopColor="#2FD9FF" />
      <stop offset="0.55" stopColor="#8B5CF6" />
      <stop offset="1" stopColor="#F23DC4" />
    </linearGradient>
  </defs>
);

const Isotipo = ({ idPrefix }: { idPrefix: string }) => (
  <>
    <path
      d={GLIFO_D}
      stroke={`url(#${idPrefix}-glifo)`}
      strokeWidth="2.75"
      strokeLinejoin="round"
    />
    <circle
      cx="60"
      cy="60"
      r="23.65"
      stroke={`url(#${idPrefix}-glifo)`}
      strokeWidth="2.92"
      strokeLinecap="round"
      strokeDasharray="0.01 5.705"
    />
    <circle
      cx="60"
      cy="60"
      r="14.19"
      stroke={`url(#${idPrefix}-glifo)`}
      strokeWidth="2.92"
      strokeLinecap="round"
      strokeDasharray="0.01 5.562"
    />
    <circle cx="60" cy="60" r="7.31" fill="currentColor" opacity="0.22" />
    <circle cx="60" cy="60" r="3.96" fill="currentColor" />
  </>
);

const Wordmark = ({ idPrefix }: { idPrefix: string }) => (
  <>
    <g
      stroke="currentColor"
      strokeWidth="9"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path transform="translate(152 30)" d="M3 60 L23 2 L43 60" />
      <path transform="translate(212 30)" d="M6 2 L6 58" />
      <path
        transform="translate(212 30)"
        d="M6 2 L23 2 C37 2 37 29 23 29 L6 29"
      />
      <path
        transform="translate(212 30)"
        d="M6 29 L26 29 C41 29 41 58 26 58 L6 58"
      />
      <path transform="translate(272 30)" d="M6 2 L6 58" />
      <path
        transform="translate(272 30)"
        d="M6 2 L23 2 C37 2 37 30 23 30 L6 30"
      />
      <path transform="translate(272 30)" d="M22 30 L41 58" />
      <path transform="translate(332 30)" d="M3 60 L23 2 L43 60" />
    </g>
    <g
      stroke={`url(#${idPrefix}-x)`}
      strokeWidth="9"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path transform="translate(392 30)" d="M4 2 L42 58" />
      <path transform="translate(392 30)" d="M42 2 L4 58" />
    </g>
  </>
);

const AbraxLogo = ({
  width,
  height,
  className,
  variant = "horizontal",
  withTagline = false,
}: {
  width?: number;
  height?: number;
  className?: string;
  variant?: "horizontal" | "stacked";
  withTagline?: boolean;
}) => {
  const idPrefix = variant === "stacked" ? "abrax-lockup-s" : "abrax-lockup-h";

  const svg =
    variant === "stacked" ? (
      <svg
        width={width}
        height={height}
        className={withTagline ? undefined : className}
        viewBox="0 0 300 232"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        role="img"
        aria-label="Abrax"
      >
        <Defs idPrefix={idPrefix} />
        <g transform="translate(69 -17) scale(1.35)">
          <Isotipo idPrefix={idPrefix} />
        </g>
        <g transform="translate(-103.3 123.5) scale(0.86)">
          <Wordmark idPrefix={idPrefix} />
        </g>
      </svg>
    ) : (
      <svg
        width={width}
        height={height}
        className={withTagline ? undefined : className}
        viewBox="0 0 450 120"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        role="img"
        aria-label="Abrax"
      >
        <Defs idPrefix={idPrefix} />
        <Isotipo idPrefix={idPrefix} />
        <Wordmark idPrefix={idPrefix} />
      </svg>
    );

  if (!withTagline) {
    return svg;
  }

  return (
    <div className={`flex flex-col items-center ${className ?? ""}`}>
      {svg}
      <p
        className={
          variant === "stacked"
            ? "mt-1.5 font-mono text-[8px] tracking-[0.26em] uppercase whitespace-nowrap"
            : "mt-2.5 font-mono text-[11px] tracking-[0.3em] uppercase whitespace-nowrap"
        }
      >
        <Trans
          i18nKey="logo.tagline"
          components={{
            accent: <span className="text-[var(--color-brand-cyan)]" />,
          }}
        />
      </p>
    </div>
  );
};

export default AbraxLogo;
