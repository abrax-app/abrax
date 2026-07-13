#!/usr/bin/env python3
"""Servidor local de Kokoro TTS para Abrax (motor CPU premium, 100% local).

ABRAX arranca este servidor como PROCESO SEPARADO en 127.0.0.1 y le habla por
HTTP; el audio NUNCA sale del equipo. kokoro-onnx es Apache-2.0, pero su
fonemizador para español usa espeak-ng (GPL) vía `espeakng-loader`: por eso el
runtime (venv) se aprovisiona aparte en el primer uso y NO se embebe en el repo
MIT ("mere aggregation"). Los pesos `kokoro-v1.0.onnx` + `voices-v1.0.bin`
(Apache-2.0) se descargan con checksum y se cargan desde el directorio de trabajo.

API (solo localhost):
    GET  /health      -> {"status":"ok","sr":24000}
    POST /synthesize   cuerpo JSON {"text": "...", "voice": "em_alex"}
                      -> audio/wav (PCM 16-bit mono)

Uso:  python kokoro_server.py --port 8766 [--device cpu]
"""
import argparse
import io
import json
import sys
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np


class Engine:
    """Carga Kokoro una sola vez desde el directorio de trabajo."""

    def __init__(self):
        from kokoro_onnx import Kokoro

        self.kokoro = Kokoro("kokoro-v1.0.onnx", "voices-v1.0.bin")
        self.sr = 24000

    # Idioma de Abrax (es/en) → código de kokoro-onnx.
    LANG_MAP = {"es": "es", "en": "en-us"}

    def synthesize(self, text: str, voice: str = "em_alex", lang: str = "es") -> np.ndarray:
        kok_lang = self.LANG_MAP.get(lang, lang)
        samples, sr = self.kokoro.create(
            text, voice=voice or "em_alex", speed=1.0, lang=kok_lang
        )
        self.sr = int(sr)
        return np.asarray(samples, dtype=np.float32)


def to_wav_bytes(samples: np.ndarray, sample_rate: int) -> bytes:
    """PCM 16-bit mono. La normalización a -3 dBFS la hace ABRAX (Rust)."""
    clipped = np.clip(samples, -1.0, 1.0)
    pcm16 = (clipped * 32767.0).astype("<i2")
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sample_rate)
        w.writeframes(pcm16.tobytes())
    return buf.getvalue()


def make_handler(engine: Engine):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):  # silencio: sin logs de contenido
            pass

        def _json(self, code, obj):
            body = json.dumps(obj).encode("utf-8")
            self.send_response(code)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            if self.path == "/health":
                self._json(200, {"status": "ok", "sr": engine.sr})
            else:
                self._json(404, {"error": "not found"})

        def do_POST(self):
            if self.path != "/synthesize":
                self._json(404, {"error": "not found"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                raw = self.rfile.read(length) or b"{}"
                # Robusto a la codificación del cliente: UTF-8 y, si no, latin-1
                # (evita UnicodeDecodeError con acentos/ñ si algo no mandó UTF-8).
                try:
                    body = raw.decode("utf-8")
                except UnicodeDecodeError:
                    body = raw.decode("latin-1")
                payload = json.loads(body)
                text = (payload.get("text") or "").strip()
                if not text:
                    self._json(400, {"error": "texto vacío"})
                    return
                voice = payload.get("voice") or "em_alex"
                lang = payload.get("language_id", "es")
                samples = engine.synthesize(text, voice, lang)
                data = to_wav_bytes(samples, engine.sr)
                self.send_response(200)
                self.send_header("Content-Type", "audio/wav")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            except Exception as exc:
                self._json(500, {"error": f"síntesis falló: {type(exc).__name__}: {exc}"})

    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--device", default="cpu")  # ignorado: Kokoro corre en CPU
    ap.add_argument("--host", default="127.0.0.1")  # SOLO localhost
    args = ap.parse_args()

    print("[kokoro] cargando modelo…", file=sys.stderr, flush=True)
    engine = Engine()
    print(f"[kokoro] listo (sr={engine.sr})", file=sys.stderr, flush=True)

    server = ThreadingHTTPServer((args.host, args.port), make_handler(engine))
    print(f"READY {args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
