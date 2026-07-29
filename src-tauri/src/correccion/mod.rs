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

pub mod emoji;
pub mod fraseador;
pub mod identificadores;
pub mod modelos;
pub mod motor_sidecar;
pub mod numeros;
pub mod protegidos;
pub mod reglas;
pub mod simbolos;
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
        t = numeros::normalizar_numeros(&t);
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

/// Endpoint OpenAI-compat de un motor de fraseo local ya resuelto: Ollama en
/// loopback o el sidecar propio (`motor_sidecar`). El orquestador no distingue
/// cuál es — ambos hablan `/v1/chat/completions`.
#[derive(Debug, Clone)]
pub struct Motor {
    pub base_url: String,
    pub modelo: String,
}

/// El camino del modelo: proteger → frasear → restaurar → validar, con un único
/// reintento estricto si el primer intento rompe un marcador o el validador lo
/// rechaza. `None` = este camino no produjo un resultado confiable; el caller
/// degrada a reglas. El `motor` ya viene resuelto por el llamador.
async fn camino_modelo(
    texto: &str,
    modo: CorreccionModo,
    motor: &Motor,
) -> Option<ResultadoCorreccion> {
    let protegido = protegidos::proteger(texto);
    log::debug!(
        "correccion: camino modelo con {} términos protegidos",
        protegido.terminos.len()
    );

    for estricto in [false, true] {
        let respuesta = match fraseador::frasear(
            &motor.base_url,
            &protegido.texto,
            modo,
            &motor.modelo,
            estricto,
        )
        .await
        {
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
///
/// `sidecar` es el motor local ya resuelto por el llamador (que tiene el
/// `AppHandle`): si el usuario eligió un modelo descargado y está corriendo,
/// llega `Some`. Si es `None`, se intenta Ollama en loopback. Ante cualquier
/// ausencia o fallo, degrada a reglas — nunca deja al usuario sin texto.
pub async fn procesar(texto: &str, settings: &AppSettings, sidecar: Option<Motor>) -> String {
    let resultado = match settings.correccion_motor {
        CorreccionMotor::Desactivado => return texto.to_string(),
        CorreccionMotor::SoloReglas => corregir(texto, settings.correccion_modo),
        CorreccionMotor::Auto | CorreccionMotor::Modelo => {
            // Motor: el sidecar si el llamador lo resolvió; si no, Ollama en
            // loopback (primer modelo disponible). Sin motor → reglas.
            let motor = match sidecar {
                Some(m) => Some(m),
                None => fraseador::detectar_ollama()
                    .await
                    .and_then(|ms| ms.into_iter().next())
                    .map(|modelo| Motor {
                        base_url: fraseador::OLLAMA_BASE.to_string(),
                        modelo,
                    }),
            };
            match motor {
                Some(m) => camino_modelo(texto, settings.correccion_modo, &m)
                    .await
                    .unwrap_or_else(|| corregir(texto, settings.correccion_modo)),
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
        assert_eq!(block_on(procesar(sucio, &s, None)), sucio);
    }

    #[test]
    fn modo_literal_no_aplica_autocorrecciones() {
        let s = settings_con(CorreccionMotor::SoloReglas, CorreccionModo::Literal);
        // Espacios y mayúsculas sí; el marcador hablado queda intacto.
        assert_eq!(
            block_on(procesar("vamos el martes, perdón, el miércoles", &s, None)),
            "Vamos el martes, perdón, el miércoles"
        );
    }

    #[test]
    fn modo_limpio_resuelve_la_autocorreccion() {
        let s = settings_con(CorreccionMotor::SoloReglas, CorreccionModo::Limpio);
        assert_eq!(
            block_on(procesar("vamos el martes, perdón, el miércoles", &s, None)),
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
        let motor = block_on(fraseador::detectar_ollama())
            .and_then(|ms| ms.into_iter().next())
            .map(|modelo| Motor {
                base_url: fraseador::OLLAMA_BASE.to_string(),
                modelo,
            })
            .expect("Ollama no responde con ≥1 modelo — smoke requiere Ollama vivo");
        match block_on(camino_modelo(entrada, CorreccionModo::Limpio, &motor)) {
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
