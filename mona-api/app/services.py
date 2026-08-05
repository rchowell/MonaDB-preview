from __future__ import annotations

import asyncio
import secrets
import string
from datetime import datetime, timedelta, timezone

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from app.config import Settings
from app.k8s import K8sProvisioner
from app.models import Database, DatabaseStatus
from app.schemas import Database as DatabaseSchema
from app.schemas import RoutingResponse


def _new_id() -> str:
    alphabet = string.ascii_lowercase + string.digits
    return "".join(secrets.choice(alphabet) for _ in range(8))


def hostname_for(db_id: str, settings: Settings) -> str:
    return f"db-{db_id}.{settings.edge_domain}"


def connection_string_for(db_id: str, settings: Settings) -> str:
    host = hostname_for(db_id, settings)
    return f"mongodb://{host}:27017/?tls=true&tlsAllowInvalidCertificates=true"


def to_schema(row: Database, settings: Settings) -> DatabaseSchema:
    return DatabaseSchema(
        id=row.id,
        name=row.name,
        hostname=hostname_for(row.id, settings),
        connectionString=connection_string_for(row.id, settings),
        status=row.status,  # type: ignore[arg-type]
        createdAt=row.created_at,
    )


async def list_databases(session: AsyncSession, settings: Settings) -> list[DatabaseSchema]:
    result = await session.execute(select(Database).order_by(Database.created_at.desc()))
    return [to_schema(row, settings) for row in result.scalars().all()]


async def get_database(
    session: AsyncSession, settings: Settings, db_id: str
) -> DatabaseSchema | None:
    row = await session.get(Database, db_id)
    if row is None:
        return None
    return to_schema(row, settings)


async def create_database(
    session: AsyncSession,
    settings: Settings,
    provisioner: K8sProvisioner,
    name: str,
) -> DatabaseSchema:
    db_id = _new_id()
    k8s_name = f"mona-db-{db_id}"
    now = datetime.now(timezone.utc)
    row = Database(
        id=db_id,
        name=name,
        status=DatabaseStatus.pending,
        k8s_name=k8s_name,
        last_active_at=now,
        created_at=now,
    )
    session.add(row)
    await session.commit()
    await session.refresh(row)

    try:
        await asyncio.to_thread(provisioner.provision_database, db_id, k8s_name)
        await asyncio.to_thread(provisioner.scale, k8s_name, 1)
        await asyncio.to_thread(provisioner.wait_ready, k8s_name)
        row.status = DatabaseStatus.ready
        row.last_active_at = datetime.now(timezone.utc)
    except Exception:
        row.status = DatabaseStatus.error
        await session.commit()
        raise

    await session.commit()
    await session.refresh(row)
    return to_schema(row, settings)


def parse_db_id_from_hostname(hostname: str, settings: Settings) -> str | None:
    host = hostname.lower().split(":")[0]
    suffix = f".{settings.edge_domain}"
    if not host.startswith("db-") or not host.endswith(suffix):
        return None
    return host[len("db-") : -len(suffix)]


async def resolve_routing(
    session: AsyncSession,
    settings: Settings,
    provisioner: K8sProvisioner,
    hostname: str,
) -> RoutingResponse | None:
    db_id = parse_db_id_from_hostname(hostname, settings)
    if db_id is None:
        return None

    row = await session.get(Database, db_id)
    if row is None:
        return None

    if row.status != DatabaseStatus.ready:
        row.status = DatabaseStatus.pending
        await session.commit()
        await asyncio.to_thread(provisioner.scale, row.k8s_name, 1)
        await asyncio.to_thread(provisioner.wait_ready, row.k8s_name)
        row.status = DatabaseStatus.ready

    row.last_active_at = datetime.now(timezone.utc)
    await session.commit()
    await session.refresh(row)

    return RoutingResponse(
        id=row.id,
        backendHost=provisioner.service_host(row.k8s_name),
        backendPort=27017,
        status=row.status,  # type: ignore[arg-type]
    )


async def touch_activity(session: AsyncSession, db_id: str) -> bool:
    row = await session.get(Database, db_id)
    if row is None:
        return False
    row.last_active_at = datetime.now(timezone.utc)
    if row.status == DatabaseStatus.sleeping:
        row.status = DatabaseStatus.ready
    await session.commit()
    return True


async def sleep_idle_databases(
    session: AsyncSession,
    settings: Settings,
    provisioner: K8sProvisioner,
) -> int:
    cutoff = datetime.now(timezone.utc) - timedelta(seconds=settings.idle_timeout_seconds)
    result = await session.execute(
        select(Database).where(
            Database.status == DatabaseStatus.ready,
            Database.last_active_at < cutoff,
        )
    )
    slept = 0
    for row in result.scalars().all():
        await asyncio.to_thread(provisioner.scale, row.k8s_name, 0)
        row.status = DatabaseStatus.sleeping
        slept += 1
    if slept:
        await session.commit()
    return slept
