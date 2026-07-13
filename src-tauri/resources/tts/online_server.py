#!/usr/bin/env python3
"""Servidor de voz ONLINE (edge-tts / voces neuronales de Microsoft) para Abrax.

⚠️ ESTE MOTOR **NO ES LOCAL**: envía el texto a sintetizar a los servidores de
Microsoft (requiere conexión a internet). Es una excepción deliberada al
principio "100% local" de ABRAX — se ofrece etiquetada y desactivada por defecto.
Los demás motores (sistema, Piper, Kokoro) nunca usan la nube.

edge-tts usa el endpoint de lectura en voz alta de Microsoft Edge (no oficial;
para producción correspondería Azure TTS con clave). Devuelve MP3, que aquí se
decodifica a WAV con miniaudio.

API (solo localhost):
    GET  /health        -> {"status":"ok"}
    POST /synthesize     {"text": "...", "voice": "es-MX-DaliaNeural"} -> audio/wav
"""
import argparse
import asyncio
import io
import json
import sys
import wave

import edge_tts
import miniaudio


async def _synth_mp3(text: str, voice: str) -> bytes:
    comm = edge_tts.Communicate(text, voice)
    buf = bytearray()
    async for chunk in comm.stream():
        if chunk["type"] == "audio":
            buf.extend(chunk["data"])
    return bytes(buf)


def synth_wav(text: str, voice: str) -> bytes:
    mp3 = asyncio.run(_synth_mp3(text, voice))
    dec = miniaudio.decode(
        mp3,
        output_format=miniaudio.SampleFormat.SIGNED16,
        nchannels=1,
        sample_rate=24000,
    )
    out = io.BytesIO()
    with wave.open(out, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(dec.sample_rate)
        w.writeframes(dec.samples.tobytes())
    return out.getvalue()


from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def make_handler():
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
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
                self._json(200, {"status": "ok"})
            else:
                self._json(404, {"error": "not found"})

        def do_POST(self):
            if self.path != "/synthesize":
                self._json(404, {"error": "not found"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                raw = self.rfile.read(length) or b"{}"
                try:
                    body = raw.decode("utf-8")
                except UnicodeDecodeError:
                    body = raw.decode("latin-1")
                payload = json.loads(body)
                text = (payload.get("text") or "").strip()
                if not text:
                    self._json(400, {"error": "texto vacío"})
                    return
                voice = payload.get("voice") or "es-MX-DaliaNeural"
                data = synth_wav(text, voice)
                self.send_response(200)
                self.send_header("Content-Type", "audio/wav")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            except Exception as exc:
                self._json(500, {"error": f"síntesis online falló: {type(exc).__name__}: {exc}"})

    return Handler


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, required=True)
    ap.add_argument("--device", default="online")  # ignorado
    ap.add_argument("--host", default="127.0.0.1")
    args = ap.parse_args()

    print("[online] listo (edge-tts)", file=sys.stderr, flush=True)
    server = ThreadingHTTPServer((args.host, args.port), make_handler())
    print(f"READY {args.host}:{args.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
