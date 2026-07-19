// gen_tildes.mjs — genera src-tauri/src/correccion/tildes_es.tsv
//
// Produce el mapa <sin_tilde> -> <con_tilde> para el corrector determinista de
// tildes, con una única condición de seguridad: se incluye una entrada SOLO si
// la forma sin tilde NO es una palabra válida del español (así añadir la tilde
// nunca cambia el significado). Las tildes diacríticas dependientes de contexto
// (esta/está, mas/más, numero/número) quedan fuera porque su forma sin tilde
// también es palabra válida.
//
// NO se ejecuta en el build (como scripts/gen_catalog.py). Se corre a mano al
// actualizar el diccionario. Entradas (no versionadas — descargar aparte):
//   · es.aff / es.dic  — diccionario RLA-ES (LibreOffice/dictionaries, es/es_ES)
//   · es_full.txt      — hermitdave/FrequencyWords, content/2018/es/es_full.txt
// Dependencia: `bun add nspell` (MIT).
//
// Uso:  bun scripts/gen_tildes.mjs
//
// La ñ NO se restaura (ano/año ambiguas): se desacentúan solo tildes de vocales.

import { readFileSync, writeFileSync } from "node:fs";
import nspell from "nspell";

const spell = nspell(readFileSync("es.aff"), readFileSync("es.dic"));
const des = (w) =>
  w
    .replace(/á/g, "a")
    .replace(/é/g, "e")
    .replace(/í/g, "i")
    .replace(/ó/g, "o")
    .replace(/ú/g, "u")
    .replace(/ü/g, "u");

// Frecuencia mínima de la forma CON tilde para incluirla (descarta ruido raro).
const OBJMIN = 30;

const freq = new Map();
for (const linea of readFileSync("es_full.txt", "utf8").split("\n")) {
  const sp = linea.indexOf(" ");
  if (sp < 0) continue;
  const w = linea.slice(0, sp);
  const f = parseInt(linea.slice(sp + 1), 10) || 0;
  if (!/^[a-záéíóúüñ]+$/.test(w)) continue;
  freq.set(w, f);
}

const porBase = new Map();
for (const [w, f] of freq) {
  const b = des(w);
  if (b === w || b.length < 3) continue;
  if (!porBase.has(b)) porBase.set(b, []);
  porBase.get(b).push({ w, f });
}

const mapa = [];
for (const [base, cands] of porBase) {
  // 1) la forma SIN tilde debe NO ser palabra válida (oráculo real).
  if (spell.correct(base)) continue;
  // 2) objetivos con frecuencia suficiente y que sean palabra válida.
  const validas = cands.filter((c) => c.f >= OBJMIN && spell.correct(c.w));
  if (validas.length === 0) continue;
  // 3) un único objetivo acentuado (si hay varios, es ambiguo: fuera).
  if (new Set(validas.map((c) => c.w)).size > 1) continue;
  mapa.push([base, validas[0].w]);
}
mapa.sort((a, b) => (a[0] < b[0] ? -1 : 1));

const cabecera = `# tildes_es.tsv — mapa de restauración de tildes del español (es-419).
# Formato: <forma_sin_tilde>\\t<forma_con_tilde>, una entrada por línea.
# Las líneas que empiezan con '#' y las vacías se ignoran al cargar.
#
# Solo contiene palabras cuya forma SIN tilde NO existe en español, de modo
# que añadir la tilde jamás cambia el significado (codigo -> código). Las
# tildes diacríticas que dependen del contexto (esta/está, mas/más,
# numero/número) se excluyen A PROPÓSITO: su forma sin tilde también es una
# palabra válida y decidir requiere contexto que estas reglas no tienen.
#
# Derivado de dos fuentes, en tiempo de generación (no en runtime):
#  · RLA-ES / diccionario de español de LibreOffice, como oráculo de validez
#    léxica. Tri-licencia GPLv3 / LGPLv3 / MPL; este archivo derivado se
#    distribuye bajo MPL. Ver ATTRIBUTION.md.
#  · hermitdave/FrequencyWords (OpenSubtitles 2018, MIT) para elegir los
#    objetivos frecuentes y descartar ruido.
# Generado por scripts/gen_tildes.mjs — no editar a mano.
`;

writeFileSync(
  "src-tauri/src/correccion/tildes_es.tsv",
  cabecera + mapa.map(([a, b]) => `${a}\t${b}`).join("\n") + "\n",
);
console.log(`tildes_es.tsv: ${mapa.length} entradas`);
