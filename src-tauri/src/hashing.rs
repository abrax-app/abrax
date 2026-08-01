//! Hash de archivos. Único sitio del crate que implementa sha256-de-archivo:
//! los verificadores de descargas (modelos de transcripción, assets TTS,
//! modelos de Pulido y runtime del sidecar) comparten esta función en vez de
//! copiar el bucle.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

/// SHA-256 en hex (minúsculas) de un archivo, leído por trozos de 64 KiB —
/// nunca carga el archivo entero a memoria. Los llamadores comparan con
/// `eq_ignore_ascii_case` para tolerar hashes esperados en mayúsculas.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_conocido_de_contenido_conocido() {
        let dir = std::env::temp_dir();
        let p = dir.join("abrax_hashing_test.bin");
        std::fs::write(&p, b"abc").unwrap();
        let h = sha256_file(&p).unwrap();
        // sha256("abc") — vector de prueba estándar de FIPS 180.
        assert_eq!(
            h,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn archivo_inexistente_da_error_no_panic() {
        assert!(sha256_file(Path::new("no-existe-\u{1F980}.bin")).is_err());
    }
}
