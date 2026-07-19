//! Módulo de corrección inteligente local — capa entre la transcripción y la
//! inserción del texto. 100% local por diseño: la única red que toca es el
//! loopback (Ollama en `127.0.0.1`, si el usuario lo tiene y optó por usarlo).
//!
//! Estado: reglas deterministas ([`reglas`]) + camino de modelo local completo
//! ([`protegidos`] → [`fraseador`] → [`validador`]) detrás de un ajuste que
//! **por defecto está desactivado** — con `CorreccionMotor::Desactivado` el
//! paso es passthrough byte a byte y ABRAX se comporta exactamente igual que
//! antes de que este módulo existiera.
//!
//! La promesa que este módulo sostiene siempre: **nunca dejar al usuario sin
//! texto**. La cadena de degradación tiene cuatro niveles:
//!
//! ```text
//! modelo (Ollama) → reintento estricto único → reglas deterministas → literal
//! ```
//!
//! Cualquier fallo — Ollama apagado, timeout, marcador protegido perdido,
//! validación de significado rechazada dos veces — baja un peldaño sin ruido.
//! El sidecar GGUF propio (post-freeze, ver auditoría) reemplazará a Ollama
//! detrás de la misma interfaz.
//!
//! Punto de enganche: `process_transcription_output` (actions.rs), después de
//! la conversión de variante china y antes del post-proceso LLM opcional.

pub mod fraseador;
pub mod protegidos;
pub mod reglas;
pub mod tildes;
pub mod validador;

use crate::settings::{AppSettings, CorreccionModo, CorreccionMotor};

/// Cómo se llegó al texto final — se registrará en el historial cuando exista
/// la columna `correction_method` (PR de persistencia).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetodoCorreccion {
    /// El texto quedó tal cual llegó (motor apagado o reglas sin efecto).
    Literal,
    /// Lo transformaron las reglas deterministas.
    Reglas,
    /// Lo corrigió el modelo local y el validador lo aprobó a la primera.
    Modelo,
    /// Lo corrigió el modelo en el reintento estricto (el primer intento
    /// alteró el significado o rompió un marcador).
    ModeloEstricto,
}

/// Resultado de una corrección. Crecerá con `cambios` (el detalle de qué se
/// tocó) cuando el fraseador los reporte.
#[derive(Debug, Clone)]
pub struct ResultadoCorreccion {
    pub texto: String,
    pub metodo: MetodoCorreccion,
}

/// Aplica las reglas deterministas según el modo. `Literal` corrige solo
/// ortotipografía (espacios y mayúsculas); `Limpio` y `Pulido` añaden las
/// autocorrecciones habladas y la restauración de tildes seguras. Es el suelo
/// de la cadena: funciones totales que no pueden fallar.
pub fn corregir(texto: &str, modo: CorreccionModo) -> ResultadoCorreccion {
    let mut t = texto.to_string();
    if matches!(modo, CorreccionModo::Limpio | CorreccionModo::Pulido) {
        t = reglas::autocorreccion_hablada(&t);
        // Tildes: corrección léxica, fuera de `Literal` (que promete no cambiar
        // ninguna palabra). Solo añade acentos cuya omisión no es una palabra
        // válida — nunca cambia el sentido. Ver [`tildes`].
        t = tildes::restaurar_tildes(&t);
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

/// El camino del modelo local: proteger → frasear → restaurar → validar, con
/// un único reintento estricto si el primer intento rompe un marcador o el
/// validador lo rechaza. `None` = este camino no produjo un resultado
/// confiable; el caller degrada a reglas.
///
/// Hoy `Auto` y `Modelo` se comportan igual: primer modelo que Ollama tenga
/// descargado. El selector de modelo llegará con el PR de UI; el contrato de
/// degradación no cambia.
async fn camino_modelo(texto: &str, modo: CorreccionModo) -> Option<ResultadoCorreccion> {
    let modelos = fraseador::detectar_ollama().await?;
    // `Some(vec![])` = Ollama vive pero sin modelos: no hay camino.
    let modelo = modelos.first()?.clone();

    let protegido = protegidos::proteger(texto);
    log::debug!(
        "correccion: camino modelo con {} términos protegidos",
        protegido.terminos.len()
    );

    for estricto in [false, true] {
        let respuesta = match fraseador::frasear(&protegido.texto, modo, &modelo, estricto).await {
            Ok(r) => r,
            Err(e) => {
                // Error de transporte/formato: reintentar no cambia nada — el
                // reintento estricto es para resultados sospechosos, no para
                // un motor caído.
                log::debug!("correccion: fraseador falló ({e:?}), se degrada a reglas");
                return None;
            }
        };

        let Some(restaurado) = protegidos::restaurar(&respuesta.texto, &protegido.terminos) else {
            log::debug!(
                "correccion: el modelo rompió un marcador (estricto={estricto}), {}",
                if estricto {
                    "se degrada a reglas"
                } else {
                    "reintento estricto"
                }
            );
            continue;
        };

        let alertas = validador::validar(texto, &restaurado);
        if alertas.is_empty() {
            let metodo = if estricto {
                MetodoCorreccion::ModeloEstricto
            } else {
                MetodoCorreccion::Modelo
            };
            return Some(ResultadoCorreccion {
                texto: restaurado,
                metodo,
            });
        }
        // Los detalles de las alertas no llevan contenido del usuario, pero al
        // log solo van el conteo y los tipos (regla S3, defensa en profundidad).
        log::debug!(
            "correccion: validador rechazó (estricto={estricto}): {} alertas {:?}",
            alertas.len(),
            alertas.iter().map(|a| a.tipo).collect::<Vec<_>>()
        );
    }
    None
}

/// Entrada del pipeline. Con el motor `Desactivado` (el default) devuelve el
/// texto **sin tocar** — ni siquiera lo pasa por las reglas. `SoloReglas` va
/// directo al suelo determinista. `Auto` y `Modelo` intentan el camino del
/// modelo local y degradan a reglas ante cualquier fallo.
pub async fn procesar(texto: &str, settings: &AppSettings) -> String {
    let resultado = match settings.correccion_motor {
        CorreccionMotor::Desactivado => return texto.to_string(),
        CorreccionMotor::SoloReglas => corregir(texto, settings.correccion_modo),
        CorreccionMotor::Auto | CorreccionMotor::Modelo => {
            match camino_modelo(texto, settings.correccion_modo).await {
                Some(r) => r,
                None => corregir(texto, settings.correccion_modo),
            }
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
    use tauri::async_runtime::block_on;

    fn settings_con(motor: CorreccionMotor, modo: CorreccionModo) -> AppSettings {
        let mut s = crate::settings::get_default_settings();
        s.correccion_motor = motor;
        s.correccion_modo = modo;
        s
    }

    /// La garantía central: con el default de fábrica, el pipeline no cambia
    /// ni un byte — incluso ante texto "sucio" que las reglas sí tocarían.
    /// (Y no toca la red: retorna antes de cualquier detección.)
    #[test]
    fn motor_desactivado_es_passthrough_exacto() {
        let s = crate::settings::get_default_settings();
        assert!(matches!(s.correccion_motor, CorreccionMotor::Desactivado));
        let sucio = "  hola , mundo. el martes, perdón, el miércoles  ";
        assert_eq!(block_on(procesar(sucio, &s)), sucio);
    }

    #[test]
    fn modo_literal_no_aplica_autocorrecciones() {
        let s = settings_con(CorreccionMotor::SoloReglas, CorreccionModo::Literal);
        // Espacios y mayúsculas sí; el marcador hablado queda intacto.
        assert_eq!(
            block_on(procesar("vamos el martes, perdón, el miércoles", &s)),
            "Vamos el martes, perdón, el miércoles"
        );
    }

    #[test]
    fn modo_limpio_resuelve_la_autocorreccion() {
        let s = settings_con(CorreccionMotor::SoloReglas, CorreccionModo::Limpio);
        assert_eq!(
            block_on(procesar("vamos el martes, perdón, el miércoles", &s)),
            "Vamos el miércoles"
        );
    }

    #[test]
    fn corregir_reporta_el_metodo() {
        let limpio = corregir("Ya está bien.", CorreccionModo::Literal);
        assert_eq!(limpio.metodo, MetodoCorreccion::Literal);
        let tocado = corregir("hola , mundo", CorreccionModo::Literal);
        assert_eq!(tocado.metodo, MetodoCorreccion::Reglas);
    }

    // Nota: no hay test unitario de `procesar` con Auto/Modelo a propósito —
    // su resultado depende de si la máquina tiene Ollama corriendo (con Ollama
    // vivo llamaría a un LLM real, no determinista). El camino del modelo se
    // cubre con el humo manual de abajo; la degradación a reglas la garantiza
    // la estructura (todo `None` de `camino_modelo` cae en `corregir`).

    /// Humo E2E del camino del modelo: requiere Ollama vivo con ≥1 modelo.
    /// Correr a mano: `cargo test correccion_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn correccion_smoke_camino_modelo() {
        let entrada = "no puedo ir el 22/07, perdón, el 23/07, cuesta 15 dólares";
        match block_on(camino_modelo(entrada, CorreccionModo::Limpio)) {
            Some(r) => {
                println!("[smoke] metodo={:?}", r.metodo);
                println!("[smoke] entrada : {entrada}");
                println!("[smoke] salida  : {}", r.texto);
                // Pase lo que pase con la redacción, el significado protegido
                // sobrevive: la negación, las fechas y la cifra siguen ahí.
                assert!(r.texto.contains("23/07"));
                assert!(r.texto.contains("15"));
                assert!(validador::validar(entrada, &r.texto).is_empty());
            }
            None => println!(
                "[smoke] sin Ollama disponible o resultado rechazado — degradaría a reglas"
            ),
        }
    }
}
