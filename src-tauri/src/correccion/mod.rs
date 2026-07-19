//! Módulo de corrección inteligente local — capa entre la transcripción y la
//! inserción del texto. 100% local por diseño: nada de este módulo toca la red.
//!
//! Estado (PR-1): solo las reglas deterministas ([`reglas`]) detrás de un
//! ajuste que **por defecto está desactivado** — con `CorreccionMotor::
//! Desactivado` el paso es passthrough byte a byte y ABRAX se comporta
//! exactamente igual que antes de que este módulo existiera.
//!
//! Piezas futuras (ver `imperio-docs/auditoria-correccion-local.md`): términos
//! protegidos, validador de significado, fraseador con micro-modelo GGUF local
//! vía sidecar en 127.0.0.1, y gestión de ese modelo. La promesa que este
//! módulo debe sostener siempre: **nunca dejar al usuario sin texto** — todo
//! fallo degrada a las reglas, y todo fallo de las reglas degrada al literal.
//!
//! Punto de enganche: `process_transcription_output` (actions.rs), después de
//! la conversión de variante china y antes del post-proceso LLM opcional.

pub mod reglas;

use crate::settings::{AppSettings, CorreccionModo, CorreccionMotor};

/// Cómo se llegó al texto final — se registrará en el historial cuando exista
/// la columna `correction_method` (PR de persistencia).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetodoCorreccion {
    /// El texto quedó tal cual llegó (motor apagado o reglas sin efecto).
    Literal,
    /// Lo transformaron las reglas deterministas.
    Reglas,
}

/// Resultado de una corrección. Crecerá con `cambios` (el detalle de qué se
/// tocó) cuando el fraseador local exista y los reporte.
#[derive(Debug, Clone)]
pub struct ResultadoCorreccion {
    pub texto: String,
    pub metodo: MetodoCorreccion,
}

/// Aplica las reglas deterministas según el modo. `Literal` corrige solo
/// ortotipografía (espacios y mayúsculas); `Limpio` y `Pulido` añaden las
/// autocorrecciones habladas. `Pulido` se diferenciará de `Limpio` cuando
/// exista el fraseador local; hoy son equivalentes a propósito.
pub fn corregir(texto: &str, modo: CorreccionModo) -> ResultadoCorreccion {
    let mut t = texto.to_string();
    if matches!(modo, CorreccionModo::Limpio | CorreccionModo::Pulido) {
        t = reglas::autocorreccion_hablada(&t);
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
/// texto **sin tocar** — ni siquiera lo pasa por las reglas. `Auto` y `Modelo`
/// degradan a reglas mientras el fraseador local no exista: preferimos un
/// resultado parcial a prometer un motor que no está.
pub fn procesar(texto: &str, settings: &AppSettings) -> String {
    if matches!(settings.correccion_motor, CorreccionMotor::Desactivado) {
        return texto.to_string();
    }
    corregir(texto, settings.correccion_modo).texto
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

    /// La garantía del PR-1: con el default de fábrica, el pipeline no cambia
    /// ni un byte — incluso ante texto "sucio" que las reglas sí tocarían.
    #[test]
    fn motor_desactivado_es_passthrough_exacto() {
        let s = crate::settings::get_default_settings();
        assert!(matches!(s.correccion_motor, CorreccionMotor::Desactivado));
        let sucio = "  hola , mundo. el martes, perdón, el miércoles  ";
        assert_eq!(procesar(sucio, &s), sucio);
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

    #[test]
    fn auto_y_modelo_degradan_a_reglas_por_ahora() {
        for motor in [CorreccionMotor::Auto, CorreccionMotor::Modelo] {
            let s = settings_con(motor, CorreccionModo::Limpio);
            assert_eq!(procesar("hola , mundo", &s), "Hola, mundo");
        }
    }

    #[test]
    fn corregir_reporta_el_metodo() {
        let limpio = corregir("Ya está bien.", CorreccionModo::Literal);
        assert_eq!(limpio.metodo, MetodoCorreccion::Literal);
        let tocado = corregir("hola , mundo", CorreccionModo::Literal);
        assert_eq!(tocado.metodo, MetodoCorreccion::Reglas);
    }
}
