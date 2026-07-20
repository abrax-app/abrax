# Recetas guardadas de la esfera

Aplicables con `ESFERA.tune({...})` en cualquiera de los dos laboratorios
(`lab-esfera.html` motor serio · `lab-loco.html` fork). El motor serio incluye
limbo y anillos orbitales **apagados por defecto** (`anillo: 0`); el fork los
trae encendidos de fábrica y sin topes de seguridad.

## Firma ABRAX — con anillos (2026-07-19)

Los dos anillos del isotipo materializados como órbitas que laten con la voz.

```json
{
  "nucleo": 0.55,
  "halo": 0.6,
  "interior": 1,
  "anillo": 0.7,
  "cruzados": 0,
  "blanco": 0.5,
  "organico": 0.22,
  "ruido": 0.15,
  "ruidoEscala": 2,
  "ruidoVel": 0.7,
  "venas": 0,
  "venaEscala": 3,
  "giro": 0.8
}
```

## Firma ABRAX — sin anillos · suavizada (LA OFICIAL de la landing)

Suavizada el 19-07 tras verla dentro del hero: menos ruido y orgánico, blancos
y limbo más tenues, giro y palabras más lentos. Extras del motor en esa pasada:
chispas 0.09→0.072 op 0.8, limbo 0.38→0.28, feed 900→1200 ms.

```json
{
  "nucleo": 0.55,
  "halo": 0.55,
  "interior": 1,
  "anillo": 0,
  "cruzados": 0,
  "blanco": 0.4,
  "organico": 0.15,
  "ruido": 0.1,
  "ruidoEscala": 2,
  "ruidoVel": 0.55,
  "venas": 0,
  "venaEscala": 3,
  "giro": 0.7
}
```

Sensibilidad de micrófono sugerida: 1.0 (subir a 1.5 para demo dramática).

ESTADO 2026-07-19: **portado y fijado.** El motor serio (`esfera.js`) tiene
limbo + anillos (apagados por defecto), y la landing (`index.html`) fija la
receta "sin anillos" vía `ESFERA.tune(...)` tras el init diferido. El lab
serio tiene el preset "Firma (landing)" para reproducirla.
