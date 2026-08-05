#!/usr/bin/env python3
"""TLS/SNI edge proxy that routes MongoDB connections to per-DB backends."""

from __future__ import annotations

import argparse
import asyncio
import logging
import os
import ssl
from typing import Optional

import httpx

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
logger = logging.getLogger("mona-edge")


async def pipe(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
    try:
        while True:
            data = await reader.read(65536)
            if not data:
                break
            writer.write(data)
            await writer.drain()
    except (ConnectionResetError, BrokenPipeError, asyncio.CancelledError):
        pass
    finally:
        try:
            writer.close()
            await writer.wait_closed()
        except Exception:
            pass


async def resolve_backend(control_plane: str, hostname: str) -> tuple[str, int, str]:
    url = f"{control_plane.rstrip('/')}/internal/routing/{hostname}"
    async with httpx.AsyncClient(timeout=120.0) as client:
        response = await client.get(url)
        response.raise_for_status()
        payload = response.json()
        return payload["backendHost"], int(payload["backendPort"]), payload["id"]


async def touch_activity(control_plane: str, db_id: str) -> None:
    url = f"{control_plane.rstrip('/')}/internal/activity/{db_id}"
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            await client.post(url)
    except Exception:
        logger.exception("failed to touch activity for %s", db_id)


async def handle_client(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    *,
    certfile: str,
    keyfile: str,
    control_plane: str,
) -> None:
    """Accept TLS, capture SNI via callback, route, then plaintext-forward to backend."""
    peer = writer.get_extra_info("peername")
    captured: dict[str, Optional[str]] = {"sni": None}

    def on_sni(_sslobj: ssl.SSLObject, server_name: str, _initial: ssl.SSLSocket | None) -> None:
        captured["sni"] = server_name

    ssl_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ssl_context.load_cert_chain(certfile, keyfile)
    ssl_context.sni_callback = on_sni

    try:
        await writer.start_tls(ssl_context, server_side=True, ssl_handshake_timeout=30)
    except Exception:
        logger.exception("TLS handshake failed from %s", peer)
        writer.close()
        return

    hostname = captured["sni"]
    if not hostname:
        logger.warning("missing SNI from %s", peer)
        writer.close()
        return

    try:
        backend_host, backend_port, db_id = await resolve_backend(control_plane, hostname)
    except Exception:
        logger.exception("routing failed for %s", hostname)
        writer.close()
        return

    logger.info("route %s -> %s:%s (db=%s)", hostname, backend_host, backend_port, db_id)
    await touch_activity(control_plane, db_id)

    try:
        backend_reader, backend_writer = await asyncio.open_connection(backend_host, backend_port)
    except Exception:
        logger.exception("backend connect failed for %s", hostname)
        writer.close()
        return

    task_a = asyncio.create_task(pipe(reader, backend_writer))
    task_b = asyncio.create_task(pipe(backend_reader, writer))
    _done, pending = await asyncio.wait({task_a, task_b}, return_when=asyncio.FIRST_COMPLETED)
    for task in pending:
        task.cancel()


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--listen", default=os.environ.get("EDGE_LISTEN", "0.0.0.0:27017"))
    parser.add_argument(
        "--control-plane",
        default=os.environ.get("CONTROL_PLANE_URL", "http://mona-api.mona.svc.cluster.local:8000"),
    )
    parser.add_argument("--cert", default=os.environ.get("TLS_CERT", "/certs/tls.crt"))
    parser.add_argument("--key", default=os.environ.get("TLS_KEY", "/certs/tls.key"))
    args = parser.parse_args()

    host, port_s = args.listen.rsplit(":", 1)
    port = int(port_s)

    async def _handle(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        await handle_client(
            reader,
            writer,
            certfile=args.cert,
            keyfile=args.key,
            control_plane=args.control_plane,
        )

    server = await asyncio.start_server(_handle, host=host, port=port)
    sockets = ", ".join(str(s.getsockname()) for s in server.sockets or [])
    logger.info("edge listening on %s (control plane %s)", sockets, args.control_plane)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    asyncio.run(main())
