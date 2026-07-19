# Atribución

Este producto se basa en trabajo del proyecto open source Handy.

- Proyecto upstream: https://github.com/cjpais/Handy
- Licencia upstream: consulta el archivo `LICENSE`.
- Commit base: completar en `UPSTREAM.md`.

Este fork es un producto independiente y no debe implicar afiliación, respaldo o
continuidad oficial con los mantenedores del proyecto upstream.

## Dependencias y activos adicionales

Completar con nombre, versión, autor, licencia y fuente.

### Mapa de restauración de tildes (`src-tauri/src/correccion/tildes_es.tsv`)

Datos léxicos derivados, generados offline por `scripts/gen_tildes.mjs` a partir de:

- **RLA-ES / diccionario de español de LibreOffice** — usado como oráculo de
  validez léxica para decidir qué formas sin tilde no son palabras. Tri-licencia
  GPLv3 / LGPLv3 / MPL. El archivo derivado `tildes_es.tsv` se distribuye bajo
  **MPL**. Fuente: https://github.com/LibreOffice/dictionaries (carpeta `es/`).
- **hermitdave/FrequencyWords** (listas de frecuencia de OpenSubtitles 2018),
  licencia **MIT** — usado para seleccionar los objetivos frecuentes y descartar
  ruido. Fuente: https://github.com/hermitdave/FrequencyWords

La verificación de validez en el generador usa **nspell** (MIT). El mapa no
incluye datos de esas fuentes verbatim: es una lista derivada de pares
`sin_tilde → con_tilde` restringida a los casos sin ambigüedad.
