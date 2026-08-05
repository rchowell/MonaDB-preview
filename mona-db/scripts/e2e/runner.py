from __future__ import annotations

import argparse
import asyncio
import os
import sys
import tempfile
from pathlib import Path
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


async def check_insert_one(collection) -> Any:
    result = await collection.insert_one({"name": "alice", "score": 10})
    if not result.acknowledged:
        raise E2EError("insert_one was not acknowledged")
    print(f"  insert_one: ok (inserted_id={result.inserted_id})")
    return result.inserted_id


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


async def check_find_one_by_inserted_id(collection, inserted_id: Any) -> None:
    doc = await collection.find_one({"_id": inserted_id})
    if doc is None:
        raise E2EError(f"find_one by inserted_id returned None for {inserted_id}")
    if doc.get("name") != "alice" or doc.get("score") != 10:
        raise E2EError(f"find_one returned unexpected document: {doc}")
    print("  find_one by inserted_id: ok")


async def check_find_empty_filter(collection) -> None:
    docs = await collection.find({}).to_list(length=100)
    names = {doc.get("name") for doc in docs}
    if names != {"alice", "bob", "carol", "Explicit Alice"}:
        raise E2EError(f"find with empty filter returned unexpected docs: {docs}")
    print(f"  find empty filter: ok (count={len(docs)})")


async def check_find_batched_streaming(collection) -> None:
    batch_collection = collection.database["batch_users_stream"]
    await batch_collection.insert_many([{"i": i} for i in range(12)])

    docs = await batch_collection.find({}).batch_size(3).to_list(length=100)
    values = sorted(doc["i"] for doc in docs)
    if values != list(range(12)):
        raise E2EError(f"batched find returned unexpected docs: {values}")
    print(f"  find batched streaming: ok (count={len(docs)})")


async def check_find_limit_with_batch_size(collection) -> None:
    limit_collection = collection.database["limit_users_stream"]
    await limit_collection.insert_many([{"i": i} for i in range(10)])

    docs = await limit_collection.find({}).limit(5).batch_size(2).to_list(length=100)
    values = sorted(doc["i"] for doc in docs)
    if values != list(range(5)):
        raise E2EError(f"limited batched find returned unexpected docs: {values}")
    print(f"  find limit with batch_size: ok (count={len(docs)})")


async def check_kill_cursors(client: AsyncIOMotorClient) -> None:
    collection = client["monadb_e2e"]["kill_users_stream"]
    await collection.insert_many([{"i": i} for i in range(5)])

    cursor = collection.find({}).batch_size(2)
    first_batch = await cursor.to_list(length=2)
    if len(first_batch) != 2:
        raise E2EError(f"expected first cursor batch of 2, got {len(first_batch)}")

    cursor_id = cursor.cursor_id
    if cursor_id is None:
        raise E2EError("expected open cursor id after first batch")

    result = await client["monadb_e2e"].command(
        {
            "killCursors": "kill_users_stream",
            "cursors": [cursor_id],
        }
    )
    if result.get("ok") != 1:
        raise E2EError(f"killCursors failed: {result}")

    print("  killCursors: ok")


async def check_find_one_explicit_id(collection) -> None:
    await collection.insert_one({"_id": "explicit-alice", "name": "Explicit Alice"})
    doc = await collection.find_one({"_id": "explicit-alice"})
    if doc is None:
        raise E2EError("find_one with explicit _id returned None")
    if doc.get("name") != "Explicit Alice":
        raise E2EError(f"find_one returned unexpected document: {doc}")
    print("  find_one explicit _id: ok")


async def check_find_one_by_name(collection) -> None:
    doc = await collection.find_one({"name": "bob"})
    if doc is None or doc.get("score") != 20:
        raise E2EError(f"find_one by name returned unexpected document: {doc}")
    print("  find_one by name: ok")


async def check_update_one_by_name(collection) -> None:
    result = await collection.update_one({"name": "carol"}, {"$set": {"score": 31}})
    if not result.acknowledged or result.matched_count != 1 or result.modified_count != 1:
        raise E2EError(f"update_one by name failed: {result.raw_result}")
    doc = await collection.find_one({"name": "carol"})
    if doc is None or doc.get("score") != 31:
        raise E2EError(f"update_one by name did not persist score=31: {doc}")
    print("  update_one by name: ok")


async def check_delete_one_by_name(collection) -> None:
    result = await collection.delete_one({"name": "bob"})
    if not result.acknowledged or result.deleted_count != 1:
        raise E2EError(f"delete_one by name failed: {result.raw_result}")
    doc = await collection.find_one({"name": "bob"})
    if doc is not None:
        raise E2EError(f"delete_one by name left document behind: {doc}")
    print("  delete_one by name: ok")


async def check_update_one(collection, inserted_id: Any) -> None:
    result = await collection.update_one({"_id": inserted_id}, {"$set": {"score": 99}})
    if not result.acknowledged or result.matched_count != 1 or result.modified_count != 1:
        raise E2EError(f"update_one failed: {result.raw_result}")
    doc = await collection.find_one({"_id": inserted_id})
    if doc is None or doc.get("score") != 99:
        raise E2EError(f"update_one did not persist score=99: {doc}")
    print("  update_one: ok")


async def check_delete_one(collection, inserted_id: Any) -> None:
    result = await collection.delete_one({"_id": inserted_id})
    if not result.acknowledged or result.deleted_count != 1:
        raise E2EError(f"delete_one failed: {result.raw_result}")
    doc = await collection.find_one({"_id": inserted_id})
    if doc is not None:
        raise E2EError(f"delete_one left document behind: {doc}")
    print("  delete_one: ok")


async def check_delete_many(collection) -> None:
    clear_collection = collection.database["clear_users"]
    await clear_collection.insert_many([{"i": i} for i in range(3)])
    result = await clear_collection.delete_many({})
    if not result.acknowledged or result.deleted_count != 3:
        raise E2EError(f"delete_many failed: {result.raw_result}")
    remaining = await clear_collection.find({}).to_list(length=100)
    if remaining:
        raise E2EError(f"delete_many left documents: {remaining}")
    print("  delete_many: ok")


async def check_persistence(data_dir: Path, server: MonaDBServer) -> None:
    persist_id = "persist-doc-1"
    server.stop()

    restart = start(bind_addr="127.0.0.1:0", data_dir=data_dir)
    try:
        client = AsyncIOMotorClient(restart.uri, serverSelectionTimeoutMS=5000)
        try:
            collection = client["monadb_e2e"]["persist_users"]
            doc = await collection.find_one({"_id": persist_id})
            if doc is None or doc.get("v") != 42:
                raise E2EError(f"post-restart find failed: {doc}")
        finally:
            client.close()
    finally:
        restart.stop()

    print("  persistence across restart: ok")


async def run_checks(
    uri: str,
    data_dir: Path | None,
    server: MonaDBServer | None,
) -> None:
    client = AsyncIOMotorClient(uri, serverSelectionTimeoutMS=5000)
    try:
        print("handshake")
        await check_handshake(client)
        await check_ping(client)

        db = client["monadb_e2e"]
        collection = db["users"]

        print("writes")
        inserted_id = await check_insert_one(collection)
        await check_insert_many(collection)

        print("reads")
        await check_find_one_by_inserted_id(collection, inserted_id)
        await check_find_one_explicit_id(collection)
        await check_find_empty_filter(collection)
        await check_find_one_by_name(collection)
        await check_find_batched_streaming(collection)
        await check_find_limit_with_batch_size(collection)
        await check_kill_cursors(client)

        print("equality updates and deletes")
        await check_update_one_by_name(collection)
        await check_delete_one_by_name(collection)

        print("updates and deletes")
        await check_update_one(collection, inserted_id)
        await check_delete_one(collection, inserted_id)
        await check_delete_many(collection)

        if data_dir is not None and server is not None:
            persist_collection = db["persist_users"]
            await persist_collection.insert_one({"_id": "persist-doc-1", "v": 42})
            doc = await persist_collection.find_one({"_id": "persist-doc-1"})
            if doc is None or doc.get("v") != 42:
                raise E2EError(f"pre-restart persist insert/find failed: {doc}")
    finally:
        client.close()

    if data_dir is not None and server is not None:
        print("persistence")
        await check_persistence(data_dir, server)


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
        "--data-dir",
        help="Data directory for spawned MonaDB (default: temp dir per run)",
    )
    parser.add_argument(
        "--no-spawn",
        action="store_true",
        help="Do not spawn MonaDB; require --uri or MONADB_URI",
    )
    return parser.parse_args(argv)


def resolve_uri(args: argparse.Namespace) -> tuple[str, MonaDBServer | None, Path | None]:
    if args.uri:
        data_dir = Path(args.data_dir) if args.data_dir else None
        return args.uri, None, data_dir
    if os.environ.get("MONADB_URI"):
        data_dir = Path(args.data_dir) if args.data_dir else None
        return os.environ["MONADB_URI"], None, data_dir
    if args.no_spawn:
        raise SystemExit("error: --no-spawn requires --uri or MONADB_URI")

    data_dir = Path(args.data_dir) if args.data_dir else Path(tempfile.mkdtemp(prefix="monadb-e2e-"))
    server = start(bind_addr=args.addr, data_dir=data_dir)
    return server.uri, server, data_dir


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)
    server: MonaDBServer | None = None

    try:
        uri, server, data_dir = resolve_uri(args)
        print(f"target: {uri}")
        if data_dir is not None:
            print(f"data-dir: {data_dir}")
        asyncio.run(run_checks(uri, data_dir, server))
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
