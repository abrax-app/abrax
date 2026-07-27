# -*- coding: utf-8 -*-
"""Genera las imágenes de marca del instalador NSIS de Windows.

Por qué existe: sin estas imágenes, el asistente usa las que trae NSIS por
defecto — un banner ROJO con un icono genérico de disco. Es el primer contacto
con el producto, antes de que la app se abra siquiera, y no se parece en nada a
ABRAX. Se detectó midiendo la instalación en un Windows limpio (26/07).

Salidas (formatos que MUI2 exige, no negociables):
  src-tauri/nsis/header.bmp    150 x 57   BMP 24 bits  — banda superior
  src-tauri/nsis/sidebar.bmp   164 x 314  BMP 24 bits  — bienvenida y final

Requisitos: `resvg` (cargo install resvg) y Pillow. Ambos son herramientas
locales de generación, no dependencias del producto: el instalador solo consume
los .bmp ya generados, que sí van versionados.

Uso:  python scripts/gen_installer_images.py
"""
import io
import os
import subprocess
import sys
import tempfile

from PIL import Image

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LOGOS = os.path.join(RAIZ, "..", "..", "kit", "logo")
SALIDA = os.path.join(RAIZ, "src-tauri", "nsis")

# Paleta de marca (kit/logo/presentacion-logo.html).
TINTA = (0x05, 0x06, 0x0F)
PANEL = (0x0B, 0x0D, 0x1C)
CIAN = (0x2F, 0xD9, 0xFF)
VIOLETA = (0x8B, 0x5C, 0xF6)
MAGENTA = (0xF2, 0x3D, 0xC4)


def rasterizar(svg, ancho):
    """SVG -> PIL.Image RGBA usando resvg."""
    ruta = os.path.join(LOGOS, svg)
    if not os.path.exists(ruta):
        sys.exit("No encuentro %s" % ruta)
    with tempfile.TemporaryDirectory() as tmp:
        png = os.path.join(tmp, "o.png")
        subprocess.run(
            ["resvg", "--width", str(ancho), ruta, png],
            check=True,
            capture_output=True,
        )
        with open(png, "rb") as fh:
            return Image.open(io.BytesIO(fh.read())).convert("RGBA")


def mezclar(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def degradado_vertical(ancho, alto, paradas):
    """Degradado vertical a partir de paradas [(pos 0..1, color)]."""
    img = Image.new("RGB", (ancho, alto))
    px = img.load()
    for y in range(alto):
        t = y / max(1, alto - 1)
        for i in range(len(paradas) - 1):
            p0, c0 = paradas[i]
            p1, c1 = paradas[i + 1]
            if p0 <= t <= p1:
                color = mezclar(c0, c1, (t - p0) / max(1e-6, p1 - p0))
                break
        else:
            color = paradas[-1][1]
        for x in range(ancho):
            px[x, y] = color
    return img


def guardar_bmp(img, nombre):
    """MUI2 exige BMP de 24 bits: RGB sin alfa."""
    ruta = os.path.join(SALIDA, nombre)
    img.convert("RGB").save(ruta, "BMP")
    print("  %-14s %s" % (nombre, img.size))
    return ruta


def header():
    """150x57. Fondo tinta + lockup ABRAX a la izquierda."""
    w, h = 150, 57
    img = Image.new("RGB", (w, h), TINTA)
    # El lockup es 450x120 (ratio 3.75): a 118 px de ancho quedan ~31 de alto,
    # con margen suficiente arriba y abajo.
    lockup = rasterizar("abrax-lockup.svg", 118)
    img.paste(lockup, (12, (h - lockup.height) // 2), lockup)
    # Filo de marca abajo, para que la banda no muera en un corte plano.
    px = img.load()
    for x in range(w):
        t = x / (w - 1)
        px[x, h - 1] = mezclar(CIAN, MAGENTA, t) if t < 1 else MAGENTA
    return guardar_bmp(img, "header.bmp")


def sidebar():
    """164x314. Panel con degradado de marca + isotipo."""
    w, h = 164, 314
    img = degradado_vertical(
        w,
        h,
        [
            (0.0, mezclar(TINTA, VIOLETA, 0.16)),
            (0.45, PANEL),
            (1.0, TINTA),
        ],
    )
    iso = rasterizar("abrax-isotipo.svg", 96)
    img.paste(iso, ((w - iso.width) // 2, 74), iso)
    # Filo de marca a la derecha: separa el panel del contenido del asistente.
    px = img.load()
    for y in range(h):
        px[w - 1, y] = mezclar(CIAN, MAGENTA, y / (h - 1))
    return guardar_bmp(img, "sidebar.bmp")


if __name__ == "__main__":
    os.makedirs(SALIDA, exist_ok=True)
    print("Imágenes del instalador ->", SALIDA)
    header()
    sidebar()
    print("Listo. Referenciadas desde bundle.windows.nsis de tauri.conf.json.")
