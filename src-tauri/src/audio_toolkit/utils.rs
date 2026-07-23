/// Returns the appropriate CPAL host for the current platform.
/// On Linux, uses ALSA host. On other platforms, uses the default host.
pub fn get_cpal_host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    {
        cpal::host_from_id(cpal::HostId::Alsa).unwrap_or_else(|_| cpal::default_host())
    }
    #[cfg(not(target_os = "linux"))]
    {
        cpal::default_host()
    }
}

/// AGC de una pasada sobre el clip completo antes del STT.
///
/// Es la paridad con lo que hacen las apps de llamadas: «en Discord sí me
/// escuchan» porque su AGC levanta el nivel del micrófono; el motor de STT
/// recibía la señal cruda y con RMS ≤ -34 dBFS Whisper degenera en bucles de
/// repetición. Umbrales alineados con el veredicto de la Prueba de micrófono
/// (`commands::audio`): solo actúa si el RMS queda BAJO la zona sana
/// (-28 dBFS), levanta hacia -24 dBFS con tope de +18 dB y limitado por pico
/// para no recortar. Silencio digital se deja intacto (amplificarlo solo sube
/// ruido). Devuelve la ganancia aplicada (1.0 = sin cambio).
pub fn normalizar_nivel_para_stt(samples: &mut [f32]) -> f32 {
    const RMS_SANO: f32 = 0.039_81; // -28 dBFS
    const RMS_OBJETIVO: f32 = 0.063_10; // -24 dBFS
    const GANANCIA_MAX: f32 = 7.943; // +18 dB
    const PICO_TECHO: f32 = 0.98;
    const RMS_SILENCIO: f32 = 1e-5; // silencio digital (< -100 dBFS)

    if samples.is_empty() {
        return 1.0;
    }
    let mut suma_sq = 0f64;
    let mut pico = 0f32;
    for &s in samples.iter() {
        suma_sq += (s as f64) * (s as f64);
        pico = pico.max(s.abs());
    }
    let rms = (suma_sq / samples.len() as f64).sqrt() as f32;
    if rms <= RMS_SILENCIO || rms >= RMS_SANO {
        return 1.0;
    }
    let mut ganancia = (RMS_OBJETIVO / rms).min(GANANCIA_MAX);
    if pico * ganancia > PICO_TECHO {
        ganancia = PICO_TECHO / pico;
    }
    if ganancia <= 1.0 {
        return 1.0;
    }
    for s in samples.iter_mut() {
        *s *= ganancia;
    }
    ganancia
}

#[cfg(test)]
mod tests_nivel {
    use super::*;

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / s.len() as f64).sqrt() as f32
    }

    fn seno(amplitud: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amplitud * (i as f32 * 0.3).sin()).collect()
    }

    #[test]
    fn nivel_bajo_se_levanta_hacia_el_objetivo() {
        // RMS ≈ -37 dBFS (amplitud 0.02): el caso real del dictado fallido.
        let mut s = seno(0.02, 16000);
        let g = normalizar_nivel_para_stt(&mut s);
        assert!(g > 1.0, "debió amplificar, ganancia={g}");
        let r = rms(&s);
        assert!(
            r > 0.039,
            "el RMS resultante debió salir de la zona baja: {r}"
        );
    }

    #[test]
    fn nivel_sano_queda_intacto() {
        let mut s = seno(0.2, 16000);
        let copia = s.clone();
        let g = normalizar_nivel_para_stt(&mut s);
        assert_eq!(g, 1.0);
        assert_eq!(s, copia);
    }

    #[test]
    fn silencio_digital_no_se_amplifica() {
        let mut s = vec![0.000_001f32; 16000];
        let g = normalizar_nivel_para_stt(&mut s);
        assert_eq!(g, 1.0);
    }

    #[test]
    fn la_ganancia_respeta_el_techo_de_pico() {
        // RMS bajo pero con un transiente fuerte: el pico limita la ganancia
        // y ninguna muestra puede quedar recortada.
        let mut s = seno(0.02, 16000);
        s[100] = 0.5;
        normalizar_nivel_para_stt(&mut s);
        assert!(s.iter().all(|v| v.abs() <= 0.99));
    }
}
