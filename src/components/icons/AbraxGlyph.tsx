import React from "react";

// Glifo ABRAX monocromo (kit/logo/abrax-tray.svg): hereda el color del texto
// vía currentColor, igual que los iconos de lucide.
const AbraxGlyph = ({
  width,
  height,
  size,
  className,
}: {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
}) => (
  <svg
    width={width ?? size ?? 24}
    height={height ?? size ?? 24}
    className={className}
    viewBox="0 0 24 24"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <path
      d="M 21.09 12.00 L 21.33 12.49 L 21.57 13.01 L 21.78 13.55 L 21.93 14.11 L 21.99 14.68 L 21.95 15.23 L 21.80 15.76 L 21.54 16.25 L 21.19 16.68 L 20.75 17.05 L 20.26 17.37 L 19.75 17.63 L 19.23 17.85 L 18.73 18.06 L 18.26 18.26 L 17.85 18.49 L 17.48 18.76 L 17.14 19.08 L 16.84 19.45 L 16.55 19.87 L 16.24 20.33 L 15.92 20.79 L 15.55 21.24 L 15.14 21.65 L 14.68 21.99 L 14.18 22.23 L 13.64 22.37 L 13.09 22.39 L 12.54 22.30 L 12.00 22.11 L 11.48 21.84 L 11.00 21.52 L 10.55 21.18 L 10.12 20.85 L 9.71 20.56 L 9.30 20.31 L 8.88 20.12 L 8.44 20.00 L 7.97 19.92 L 7.45 19.87 L 6.91 19.84 L 6.34 19.79 L 5.77 19.70 L 5.21 19.54 L 4.69 19.31 L 4.22 19.00 L 3.84 18.61 L 3.55 18.14 L 3.35 17.62 L 3.25 17.05 L 3.22 16.47 L 3.25 15.89 L 3.32 15.33 L 3.39 14.80 L 3.44 14.29 L 3.45 13.82 L 3.41 13.36 L 3.30 12.91 L 3.13 12.47 L 2.91 12.00 L 2.67 11.51 L 2.43 10.99 L 2.22 10.45 L 2.07 9.89 L 2.01 9.32 L 2.05 8.77 L 2.20 8.24 L 2.46 7.75 L 2.81 7.32 L 3.25 6.95 L 3.74 6.63 L 4.25 6.37 L 4.77 6.15 L 5.27 5.94 L 5.74 5.74 L 6.15 5.51 L 6.52 5.24 L 6.86 4.92 L 7.16 4.55 L 7.45 4.13 L 7.76 3.67 L 8.08 3.21 L 8.45 2.76 L 8.86 2.35 L 9.32 2.01 L 9.82 1.77 L 10.36 1.63 L 10.91 1.61 L 11.46 1.70 L 12.00 1.89 L 12.52 2.16 L 13.00 2.48 L 13.45 2.82 L 13.88 3.15 L 14.29 3.44 L 14.70 3.69 L 15.12 3.88 L 15.56 4.00 L 16.03 4.08 L 16.55 4.13 L 17.09 4.16 L 17.66 4.21 L 18.23 4.30 L 18.79 4.46 L 19.31 4.69 L 19.78 5.00 L 20.16 5.39 L 20.45 5.86 L 20.65 6.38 L 20.75 6.95 L 20.78 7.53 L 20.75 8.11 L 20.68 8.67 L 20.61 9.20 L 20.56 9.71 L 20.55 10.18 L 20.59 10.64 L 20.70 11.09 L 20.87 11.53 L 21.09 12.00 Z"
      stroke="currentColor"
      strokeWidth="1.9"
      strokeLinejoin="round"
    />
    <circle
      cx="12"
      cy="12"
      r="5.6"
      stroke="currentColor"
      strokeWidth="1.9"
      strokeLinecap="round"
      strokeDasharray="0.01 4.388"
    />
    <circle cx="12" cy="12" r="2.3" fill="currentColor" />
  </svg>
);

export default AbraxGlyph;
