// gen_emoji_es.mjs — genera src-tauri/src/correccion/emoji_es.tsv
//
// Produce el mapa <nombre en español> -> <emoji> para el dictado de emoji:
// «emoji cara feliz» → «🙂». La condición de seguridad no está aquí sino en el
// disparador (ver `correccion/emoji.rs`): sin la palabra «emoji» delante no se
// toca nada, porque los nombres de CLDR incluyen palabras corrientes —«bandera»,
// «ojos», «uno», «diez»— y convertirlas rompería texto normal.
//
// NO se ejecuta en el build (como gen_tildes.mjs y gen_catalog.py). Se corre a
// mano al actualizar la tabla. La entrada se descarga sola, no se versiona:
//   · common/annotations/es.xml — CLDR de Unicode (nombres cortos, type="tts")
//
// Uso:  bun scripts/gen_emoji_es.mjs
//
// Decisiones deliberadas:
//  - Solo `type="tts"` (el nombre corto canónico). Las `keywords` se descartan:
//    son sinónimos sueltos como «cara» o «feliz» que dispararían con cualquier
//    nombre y harían la tabla ambigua.
//  - Solo emoji de verdad: se exige que el `cp` lleve al menos un carácter en
//    rango de pictograma. CLDR también anota signos ASCII («{» → «llave de
//    apertura») y esos NO son emoji.
//  - Se descartan los modificadores de tono de piel y las banderas de país: no
//    tienen sentido dictados y multiplican las colisiones.
//  - Ante dos emoji con el MISMO nombre gana el de secuencia más corta (el
//    básico frente a la variante compuesta), no «el primero que salga».

import { writeFileSync } from "node:fs";

const URL_CLDR =
  "https://raw.githubusercontent.com/unicode-org/cldr/main/common/annotations/es.xml";

const xml = await (await fetch(URL_CLDR)).text();

// <annotation cp="🙂" type="tts">cara sonriendo</annotation>
const RE = /<annotation cp="([^"]+)" type="tts">([^<]+)<\/annotation>/g;

// ¿El cp contiene al menos un pictograma? Descarta las anotaciones de ASCII.
const esPictograma = (cp) =>
  [...cp].some((c) => {
    const p = c.codePointAt(0);
    return (
      (p >= 0x1f000 && p <= 0x1faff) || // pictogramas, emoticonos, símbolos
      (p >= 0x2600 && p <= 0x27bf) || // símbolos misceláneos y dingbats
      p === 0x2b50 ||
      p === 0x2b55 ||
      (p >= 0x1f1e6 && p <= 0x1f1ff) // indicadores regionales (banderas)
    );
  });

const esTonoDePiel = (cp) =>
  [...cp].some((c) => {
    const p = c.codePointAt(0);
    return p >= 0x1f3fb && p <= 0x1f3ff;
  });

const esBanderaDePais = (cp) =>
  [...cp].every((c) => {
    const p = c.codePointAt(0);
    return p >= 0x1f1e6 && p <= 0x1f1ff;
  });

const normalizar = (s) =>
  s
    .toLowerCase()
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "") // fuera tildes: la clave se compara sin ellas
    .replace(/[^a-z0-9ñ ]+/g, " ") // fuera puntuación («cara con “ojos”»)
    .replace(/\s+/g, " ")
    .trim();

const porNombre = new Map();
let vistos = 0;
let descartados = 0;

for (const [, cp, nombre] of xml.matchAll(RE)) {
  vistos++;
  if (!esPictograma(cp) || esTonoDePiel(cp) || esBanderaDePais(cp)) {
    descartados++;
    continue;
  }
  const clave = normalizar(nombre);
  if (!clave || clave.split(" ").length > 6) {
    descartados++;
    continue;
  }
  const previo = porNombre.get(clave);
  // Colisión: gana la secuencia más corta (el emoji básico, no la variante).
  if (!previo || [...cp].length < [...previo].length) porNombre.set(clave, cp);
}

// ─────────────────────── ALIAS CURADOS (a mano, no CLDR) ────────────────────
// CLDR es preciso pero nadie habla así: 🙂 se llama «cara sonriendo
// ligeramente» y 😊 «cara feliz con ojos sonrientes». Nadie dicta eso. Estos
// alias son las formas que la gente SÍ dice, y sin ellos la función falla en su
// caso de uso más obvio («emoji cara feliz»).
//
// El valor apunta al NOMBRE CANÓNICO de CLDR, no al emoji: así, si CLDR cambia
// el carácter de una entrada, el alias lo sigue. Si el canónico no existe, el
// alias se descarta con aviso — no se inventa un emoji.
const ALIAS = {
  "cara feliz": "cara sonriendo ligeramente",
  "carita feliz": "cara sonriendo ligeramente",
  sonrisa: "cara sonriendo",
  "carita sonriente": "cara sonriendo",
  "cara triste": "cara con el ceno ligeramente fruncido",
  "carita triste": "cara con el ceno ligeramente fruncido",
  llorando: "cara llorando",
  "muerto de risa": "cara llorando de risa",
  "llorando de risa": "cara llorando de risa",
  risa: "cara llorando de risa",
  corazon: "corazon rojo",
  "me gusta": "pulgar hacia arriba",
  "pulgar arriba": "pulgar hacia arriba",
  "pulgar abajo": "pulgar hacia abajo",
  "no me gusta": "pulgar hacia abajo",
  aplausos: "manos aplaudiendo",
  guino: "cara guinando el ojo",
  "guinando el ojo": "cara guinando el ojo",
  pensando: "cara pensativa",
  fiesta: "cara de fiesta",
  beso: "cara lanzando un beso",
  enamorado: "cara sonriendo con ojos de corazon",
};

let aliasPuestos = 0;
const aliasHuerfanos = [];
for (const [alias, canonico] of Object.entries(ALIAS)) {
  const cp = porNombre.get(canonico);
  if (!cp) {
    aliasHuerfanos.push(`${alias} -> ${canonico}`);
    continue;
  }
  // Un alias NUNCA pisa un nombre canónico de CLDR.
  if (porNombre.has(alias)) continue;
  porNombre.set(alias, cp);
  aliasPuestos++;
}
if (aliasHuerfanos.length) {
  console.error(
    `AVISO: ${aliasHuerfanos.length} alias sin canónico en CLDR (se descartan):\n  ` +
      aliasHuerfanos.join("\n  "),
  );
}

const filas = [...porNombre.entries()].sort(([a], [b]) => a.localeCompare(b));
const salida =
  filas.map(([nombre, cp]) => `${nombre}\t${cp}`).join("\n") + "\n";
writeFileSync("src-tauri/src/correccion/emoji_es.tsv", salida, "utf8");

const palabras = filas.reduce((n, [k]) => n + k.split(" ").length, 0);
console.error(
  `anotaciones tts vistas: ${vistos} · descartadas: ${descartados} · ` +
    `alias curados puestos: ${aliasPuestos}/${Object.keys(ALIAS).length} · ` +
    `entradas escritas: ${filas.length} · palabras/nombre: ${(
      palabras / filas.length
    ).toFixed(1)}`,
);
