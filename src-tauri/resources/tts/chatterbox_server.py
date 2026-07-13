#!/usr/bin/env python3
"""Servidor local de Chatterbox TTS para Abrax (motor PREMIUM, 100% local).

ABRAX arranca este servidor como PROCESO SEPARADO en 127.0.0.1 y le habla por
HTTP; el audio NUNCA sale del equipo. El runtime pesado (Python + torch + CUDA +
chatterbox) se aprovisiona en el primer uso (no se embebe en el instalador MIT):
este script es lo único que ABRAX distribuye.

API (solo localhost):
    GET  /health           -> {"status":"ok","device":"cuda","sr":24000}
    POST /synthesize        cuerpo JSON {"text": "...", "language_id":"es"}
                            -> audio/wav (PCM 16-bit mono) del texto sintetizado

Uso:  python chatterbox_server.py --port 8765 [--device cuda|mps|cpu]

Chatterbox es MIT (Resemble AI). Sin dependencias de nube.
"""
import argparse
import io
import json
import sys
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import numpy as np  # torch trae numpy; disponible en el runtime aprovisionado


def pick_device(requested: str) -> str:
    """Elige el mejor dispositivo local disponible (nunca nube)."""
    try:
        import torch
    except Exception:
        return "cpu"
    if requested and requested != "auto":
        return requested
    if torch.cuda.is_available():
        return "cuda"
    if getattr(torch.backends, "mps", None) and torch.backends.mps.is_available():
        return "mps"
    return "cpu"


class Engine:
    """Carga Chatterbox una sola vez. Prefiere el multilingüe (soporta español)."""

    def __init__(self, device: str):
        self.device = device
        self.multilingual = False
        self.model = None
        self.sr = 24000
        self._load()

    def _load(self):
        # Parche defensivo: en algunos entornos el watermarker opcional de
        # chatterbox-tts (`perth.PerthImplicitWatermarker`) queda en None por un
        # import fallido y rompe la carga. La marca de agua es inaudible y
        # opcional: la sustituimos por un no-op si falta.
        try:
            import perth

            if getattr(perth, "PerthImplicitWatermarker", None) is None:
                class _NoopWatermarker:
                    def apply_watermark(self, wav, sample_rate=None, **kw):
                        return wav

                perth.PerthImplicitWatermarker = lambda *a, **k: _NoopWatermarker()
        except Exception:
            pass

        # Multilingüe primero (es-419); si no está, la base (inglés).
        try:
            from chatterbox.mtl_tts import ChatterboxMultilingualTTS

            self.model = ChatterboxMultilingualTTS.from_pretrained(device=self.device)
            self.multilingual = True
        except Exception:
            from chatterbox.tts import ChatterboxTTS

            self.model = ChatterboxTTS.from_pretrained(device=self.device)
            self.multilingual = False
        self.sr = int(getattr(self.model, "sr", 24000))

    def synthesize(self, text: str, language_id: str = "es") -> np.ndarray:
        if self.multilingual:
            wav = self.model.generate(text, language_id=language_id)
        else:
            wav = self.model.generate(text)
        # wav: torch.Tensor [1, N] o [N]; a numpy float32 mono en [-1, 1].
        arr = wav.detach().to("cpu").numpy()
        arr = np.squeeze(arr).astype(np.float32)
        return arr


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
                self._json(200, {"status": "ok", "device": engine.device, "sr": engine.sr})
            else:
                self._json(404, {"error": "not found"})

        def do_POST(self):
            if self.path != "/synthesize":
                self._json(404, {"error": "not found"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                raw = self.rfile.read(length) or b"{}"
                # Robusto a la codificación del cliente: UTF-8 y, si no, latin-1.
                try:
                    body = raw.decode("utf-8")
                except UnicodeDecodeError:
                    body = raw.decode("latin-1")
                payload = json.loads(body)
                text = (payload.get("text") or "").strip()
                if not text:
                    self._json(400, {"error": "texto vacío"})
                    return
                lang = payload.get("language_id", "es")
                samples = engine.synthesize(text, lang)
                data = to_wav_bytes(samples, engine.sr)
                self.send_response(200)
                self.send_header("Content-Type", "audio/wav")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            except Exception as exc:  # nunca filtra el texto, solo el tipo de fallo
                self._json(500, {"error": f"síntesis falló: {type(exc).__name__}: {exc}"})

    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--device", default="auto")
    ap.add_argument("--host", default="127.0.0.1")  # SOLO localhost
    args = ap.parse_args()

    device = pick_device(args.device)
    print(f"[chatterbox] cargando modelo en {device}...", file=sys.stderr, flush=True)
    engine = Engine(device)
    print(f"[chatterbox] listo (sr={engine.sr}, multilingüe={engine.multilingual})",
          file=sys.stderr, flush=True)

    server = ThreadingHTTPServer((args.host, args.port), make_handler(engine))
    # Marca de "listo" para que ABRAX sepa que puede empezar a pedir.
    print(f"READY {args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
