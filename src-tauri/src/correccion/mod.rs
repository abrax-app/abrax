//! Módulo de corrección local — capa entre la transcripción y la inserción del
//! texto. **Sin red y sin ningún modelo**: todo son reglas deterministas.
//!
//! El camino del LLM local (el «Pulido con IA»: `protegidos` → `fraseador` →
//! `validador`, con sidecar GGUF propio o Ollama en loopback) **se retiró el
//! 29/07** por decisión de producto: no se alcanzó a probar en condiciones y una
//! función sin probar es un pasivo. Con él se fueron `fraseador`, `modelos`,
//! `motor_sidecar` y —al quedarse sin ningún usuario— `protegidos` y
//! `validador`. Está en el historial si algún día se retoma.
//!
//! Lo que queda es la capa determinista, que es la que se usa de verdad:
//! autocorrección hablada, colapso de repeticiones, tildes, numerales,
//! identificadores, símbolos, espacios y capitalización. Funciones totales que
//! no pueden fallar, así que ya no hay cadena de degradación que sostener: el
//! suelo ES el techo.
//!
//! Con `CorreccionMotor::Desactivado` (el default) el paso es passthrough byte a
//! byte y ABRAX se comporta exactamente igual que si este módulo no existiera.
//!
//! Punto de enganche: `process_transcription_output` (actions.rs), después de la
//! conversión de variante china.

pub mod emoji;
pub mod identificadores;
pub mod numeros;
pub mod reglas;
pub mod simbolos;
pub mod tildes;

use crate::settings::{AppSettings, CorreccionModo, CorreccionMotor};

/// Cómo se llegó al texto final — se registrará en el historial cuando exista
/// la columna `correction_method` (PR de persistencia).
///
/// Tenía dos variantes más (`Modelo` y `ModeloEstricto`) para el camino del LLM
/// local, que se retiró el 29/07.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetodoCorreccion {
    /// El texto quedó tal cual llegó (motor apagado o reglas sin efecto).
    Literal,
    /// Lo transformaron las reglas deterministas.
    Reglas,
}

/// Resultado de una corrección.
#[derive(Debug, Clone)]
pub struct ResultadoCorreccion {
    pub texto: String,
    pub metodo: MetodoCorreccion,
}

/// Aplica las reglas deterministas según el modo. `Literal` corrige solo
/// ortotipografía (espacios y mayúsculas); `Limpio` añade las autocorrecciones
/// habladas, las tildes seguras y la verbalización. Funciones totales que no
/// pueden fallar.
pub fn corregir(texto: &str, modo: CorreccionModo, numeros: bool) -> ResultadoCorreccion {
    let mut t = texto.to_string();
    if matches!(modo, CorreccionModo::Limpio) {
        t = reglas::autocorreccion_hablada(&t);
        // Repetición inmediata accidental: «después después» → «después».
        // Después de la autocorrección (que puede crear una) y antes de la
        // ortotipografía. Ver [`reglas::colapsar_repeticiones`].
        t = reglas::colapsar_repeticiones(&t);
        // Tildes: corrección léxica, fuera de `Literal` (que promete no cambiar
        // ninguna palabra). Solo añade acentos cuya omisión no es una palabra
        // válida — nunca cambia el sentido. Ver [`tildes`].
        t = tildes::restaurar_tildes(&t);
        // Verbalización → forma escrita, en este orden medido: los numerales
        // primero («ocho mil ochenta» → «8080»), los identificadores después
        // (para que «localhost dos puntos 8080» ya tenga el puerto en cifras)
        // y los símbolos al final («100 por ciento» → «100%»). Cada módulo es
        // conservador y se abstiene sin evidencia. Todo ANTES de capitalizar:
        // un correo que abre el dictado debe unirse primero y quedar
        // «whisper@main.io», no capitalizarse como palabra y dar
        // «Whisper@main.io». Ver [`numeros`], [`identificadores`], [`simbolos`].
        // El conversor de numerales tiene su propia llave: es el unico de todo
        // el paquete que cambia el ESTILO del texto («los dos minutos» → «los 2
        // minutos»), y hay quien quiere las tildes y los simbolos sin eso.
        if numeros {
            t = numeros::normalizar_numeros(&t);
        }
        t = identificadores::normalizar_identificadores(&t);
        t = simbolos::normalizar_simbolos(&t);
    }
    t = reglas::normalizar_espacios(&t);
    t = reglas::capitalizar_oraciones(&t);
    let metodo = if t == texto {
        MetodoCorreccion::Literal
    } else {
        MetodoCorreccion::Reglas
    };
    ResultadoCorreccion { texto: t, metodo }
}

/// Entrada del pipeline. Con el motor `Desactivado` (el default) devuelve el
/// texto **sin tocar** — ni siquiera lo pasa por las reglas. `SoloReglas` aplica
/// la capa determinista.
///
/// Ya no es `async` ni recibe un motor: no hay nada que esperar ni ningún
/// proceso al que hablar. Antes intentaba un LLM local y degradaba a reglas ante
/// cualquier fallo; retirado el «Pulido con IA», las reglas no son el suelo de
/// una cadena sino todo lo que hay, y no pueden fallar.
pub fn procesar(texto: &str, settings: &AppSettings) -> String {
    let resultado = match settings.correccion_motor {
        CorreccionMotor::Desactivado => return texto.to_string(),
        CorreccionMotor::SoloReglas => {
            corregir(texto, settings.correccion_modo, settings.correccion_numeros)
        }
    };
    // Solo el método y longitudes — el contenido del dictado jamás va al log
    // (misma regla que el resto del pipeline, fix S3).
    log::debug!(
        "correccion: metodo={:?} modo={:?} {}→{} chars",
        resultado.metodo,
        settings.correccion_modo,
        texto.chars().count(),
        resultado.texto.chars().count()
    );
    resultado.texto
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_con(motor: CorreccionMotor, modo: CorreccionModo) -> AppSettings {
        let mut s = crate::settings::get_default_settings();
        s.correccion_motor = motor;
        s.correccion_modo = modo;
        s
    }

    /// La garantía central, que NO cambia con el default: apagar el motor es
    /// passthrough byte a byte, incluso ante texto «sucio» que las reglas sí
    /// tocarían. Antes se probaba a través del default de fábrica; desde que el
    /// default es `SoloReglas` (30/07) se prueba pidiéndolo explícitamente, que
    /// es lo que de verdad se promete: si lo apagas, no se toca nada.
    #[test]
    fn motor_desactivado_es_passthrough_exacto() {
        let s = settings_con(CorreccionMotor::Desactivado, CorreccionModo::Limpio);
        let sucio = "  hola , mundo. el martes, perdón, el miércoles  ";
        assert_eq!(procesar(sucio, &s), sucio);
    }

    /// Y el reverso: con los valores DE FÁBRICA el paquete sí actúa. Este test
    /// es el que se rompería si alguien volviera a apagarlo por descuido —que es
    /// justo como estuvo cinco días sin que nadie se enterara.
    #[test]
    fn de_fabrica_la_correccion_actua() {
        let s = crate::settings::get_default_settings();
        assert!(matches!(s.correccion_motor, CorreccionMotor::SoloReglas));
        assert_eq!(s.correccion_modo, CorreccionModo::Limpio);
        assert_eq!(procesar("dame el cinco por ciento", &s), "Dame el 5%");
    }

    #[test]
    fn modo_literal_no_aplica_autocorrecciones() {
        let s = settings_con(CorreccionMotor::SoloReglas, CorreccionModo::Literal);
        // Espacios y mayúsculas sí; el marcador hablado queda intacto.
        assert_eq!(
            procesar("vamos el martes, perdón, el miércoles", &s),
            "Vamos el martes, perdón, el miércoles"
        );
    }

    #[test]
    fn modo_limpio_resuelve_la_autocorreccion() {
        let s = settings_con(CorreccionMotor::SoloReglas, CorreccionModo::Limpio);
        assert_eq!(
            procesar("vamos el martes, perdón, el miércoles", &s),
            "Vamos el miércoles"
        );
    }

    /// Guardián de la retirada: `pulido` almacenado en una instalación vieja
    /// tiene que seguir cargando, y caer en `Limpio`. Si alguien quita el alias
    /// de serde, `settings_store.json` de esos usuarios deja de parsear y pierden
    /// TODOS sus ajustes, no solo este.
    #[test]
    fn el_modo_pulido_guardado_sigue_cargando_como_limpio() {
        let modo: CorreccionModo =
            serde_json::from_str("\"pulido\"").expect("«pulido» debe cargar");
        assert_eq!(modo, CorreccionModo::Limpio);
    }

    /// Mismo guardián para el motor: `auto` y `modelo` eran los que pedían LLM.
    #[test]
    fn los_motores_de_llm_guardados_caen_en_solo_reglas() {
        for viejo in ["\"auto\"", "\"modelo\""] {
            let motor: CorreccionMotor =
                serde_json::from_str(viejo).unwrap_or_else(|e| panic!("{viejo} debe cargar: {e}"));
            assert_eq!(motor, CorreccionMotor::SoloReglas, "fallo con {viejo}");
        }
    }

    #[test]
    fn corregir_reporta_el_metodo() {
        let limpio = corregir("Ya está bien.", CorreccionModo::Literal, true);
        assert_eq!(limpio.metodo, MetodoCorreccion::Literal);
        let tocado = corregir("hola , mundo", CorreccionModo::Literal, true);
        assert_eq!(tocado.metodo, MetodoCorreccion::Reglas);
    }

    /// Ya no hay red que tocar. Antes esto era imposible de afirmar en un test
    /// (el camino del modelo hablaba con Ollama), y ahora es una propiedad del
    /// módulo: `procesar` es una función pura y síncrona.
    #[test]
    fn corregir_es_deterministico_y_repetible() {
        let s = settings_con(CorreccionMotor::SoloReglas, CorreccionModo::Limpio);
        let entrada = "no puedo ir el martes, perdón, el miércoles";
        let primera = procesar(entrada, &s);
        for _ in 0..5 {
            assert_eq!(procesar(entrada, &s), primera);
        }
    }

    #[test]
    fn la_llave_de_numeros_apaga_solo_los_numeros() {
        // Con la llave puesta: convierte el numeral.
        let con = corregir("son las diecisiete treinta", CorreccionModo::Limpio, true);
        // Sin ella: el numeral se queda como se dijo…
        let sin = corregir("son las diecisiete treinta", CorreccionModo::Limpio, false);
        assert_ne!(con.texto, sin.texto, "la llave no cambio nada");
        assert!(
            sin.texto.contains("diecisiete"),
            "apagada, el numeral debe quedarse en palabras: {}",
            sin.texto
        );
        // …pero el RESTO del modo Limpio sigue funcionando sin el. Control
        // positivo de que la llave apaga una capa y no el paquete entero.
        let otra = corregir("dame el cinco por ciento", CorreccionModo::Limpio, false);
        assert!(
            otra.texto.contains('%'),
            "apagar numeros no debe apagar los simbolos: {}",
            otra.texto
        );
    }
}
