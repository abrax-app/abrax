# Atribución

Este producto se basa en trabajo del proyecto open source Handy.

- Proyecto upstream: https://github.com/cjpais/Handy
- Licencia upstream: consulta el archivo `LICENSE`.
- Commit base: `38825767cf933f59b90209c3f3c5f1e3bbe98766` (capturado el
  2026-07-11; ver `UPSTREAM.md`).

Este fork es un producto independiente y no debe implicar afiliación, respaldo o
continuidad oficial con los mantenedores del proyecto upstream.

## Dependencias y activos adicionales

Las licencias de terceros —modelos de reconocimiento de voz y motores de voz—
se declaran en `LICENSES-THIRD-PARTY.md`, con las atribuciones que exige cada
una. Lo de abajo son activos derivados que se generan en este repositorio.

### Tabla de emoji en español (`src-tauri/src/correccion/emoji_es.tsv`)

Nombres de emoji en español para el dictado («emoji cara feliz» → 🙂), generados
offline por `scripts/gen_emoji_es.mjs` a partir de:

- **CLDR de Unicode** — anotaciones de emoji en español
  (`common/annotations/es.xml`), de donde salen los nombres cortos canónicos.
  Licencia **Unicode-3.0** (permisiva, con aviso de copyright).
  Fuente: https://github.com/unicode-org/cldr
  © 1991-presente Unicode, Inc. Unicode y el logotipo de Unicode son marcas
  registradas de Unicode, Inc. en Estados Unidos y otros países.

La tabla **no reproduce CLDR verbatim**: es una lista derivada de pares
`nombre → emoji` filtrada (se descartan las anotaciones de signos ASCII, los
modificadores de tono de piel y las banderas de país), más unos 20 **alias
escritos a mano** —«cara feliz», «me gusta», «aplausos»— que no vienen de CLDR
y existen porque sus nombres canónicos son precisos pero nadie los dicta así.

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
