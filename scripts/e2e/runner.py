from __future__ import annotations

import argparse
import asyncio
import os
import sys
from typing import Any

from motor.motor_asyncio import AsyncIOMotorClient

from e2e.server import MonaDBServer, start


class E2EError(Exception):
    """Raised when an end-to-end check fails."""


async def check_handshake(client: AsyncIOMotorClient) -> None:
    hello = await client.admin.command("hello")
    if hello.get("ok") != 1:
        raise E2EError(f"hello failed: {hello}")
    if hello.get("maxWireVersion") is None:
        raise E2EError(f"hello missing maxWireVersion: {hello}")
    print("  hello: ok")


async def check_ping(client: AsyncIOMotorClient) -> None:
    result = await client.admin.command("ping")
    if result.get("ok") != 1:
        raise E2EError(f"ping failed: {result}")
    print("  ping: ok")


async def check_insert_one(collection) -> None:
    result = await collection.insert_one({"name": "alice", "score": 10})
    if not result.acknowledged:
        raise E2EError("insert_one was not acknowledged")
    print(f"  insert_one: ok (inserted_id={result.inserted_id})")


async def check_insert_many(collection) -> None:
    result = await collection.insert_many(
        [
            {"name": "bob", "score": 20},
            {"name": "carol", "score": 30},
        ]
    )
    if not result.acknowledged or len(result.inserted_ids) != 2:
        raise E2EError(f"insert_many failed: {result}")
    print(f"  insert_many: ok (count={len(result.inserted_ids)})")


async def check_update_one(collection) -> None:
    result = await collection.update_one(
        {"name": "alice"},
        {"$set": {"score": 11}},
    )
    if not result.acknowledged:
        raise E2EError("update_one was not acknowledged")
    print(f"  update_one: ok (matched={result.matched_count}, modified={result.modified_count})")


async def check_update_many(collection) -> None:
    result = await collection.update_many(
        {"score": {"$gte": 20}},
        {"$inc": {"score": 1}},
    )
    if not result.acknowledged:
        raise E2EError("update_many was not acknowledged")
    print(f"  update_many: ok (matched={result.matched_count}, modified={result.modified_count})")


async def check_delete_one(collection) -> None:
    result = await collection.delete_one({"name": "alice"})
    if not result.acknowledged:
        raise E2EError("delete_one was not acknowledged")
    print(f"  delete_one: ok (deleted={result.deleted_count})")


async def check_delete_many(collection) -> None:
    result = await collection.delete_many({"score": {"$gte": 20}})
    if not result.acknowledged:
        raise E2EError("delete_many was not acknowledged")
    print(f"  delete_many: ok (deleted={result.deleted_count})")


async def run_checks(uri: str) -> None:
    client = AsyncIOMotorClient(uri, serverSelectionTimeoutMS=5000)
    try:
        print("handshake")
        await check_handshake(client)
        await check_ping(client)

        db = client["monadb_e2e"]
        collection = db["users"]

        print("writes")
        await check_insert_one(collection)
        await check_insert_many(collection)
        await check_update_one(collection)
        await check_update_many(collection)
        await check_delete_one(collection)
        await check_delete_many(collection)
    finally:
        client.close()


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run MonaDB Motor end-to-end checks")
    parser.add_argument(
        "--uri",
        help="MongoDB URI for an already-running MonaDB instance",
    )
    parser.add_argument(
        "--addr",
        default="127.0.0.1:0",
        help="Bind address when spawning MonaDB (default: ephemeral port)",
    )
    parser.add_argument(
        "--no-spawn",
        action="store_true",
        help="Do not spawn MonaDB; require --uri or MONADB_URI",
    )
    return parser.parse_args(argv)


def resolve_uri(args: argparse.Namespace) -> tuple[str, MonaDBServer | None]:
    if args.uri:
        return args.uri, None
    if os.environ.get("MONADB_URI"):
        return os.environ["MONADB_URI"], None
    if args.no_spawn:
        raise SystemExit("error: --no-spawn requires --uri or MONADB_URI")
    server = start(bind_addr=args.addr)
    return server.uri, server


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)
    server: MonaDBServer | None = None

    try:
        uri, server = resolve_uri(args)
        print(f"target: {uri}")
        asyncio.run(run_checks(uri))
        print("all checks passed")
    except E2EError as exc:
        print(f"e2e failed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    except Exception as exc:
        print(f"e2e error: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    finally:
        if server is not None:
            server.stop()


if __name__ == "__main__":
    main()
