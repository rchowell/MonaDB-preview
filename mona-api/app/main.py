from __future__ import annotations

import asyncio
import logging
from contextlib import asynccontextmanager

from fastapi import Depends, FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy.ext.asyncio import AsyncSession

from app.config import Settings, get_settings
from app.db import SessionLocal, get_session
from app.k8s import K8sProvisioner
from app.schemas import CreateDatabaseRequest, Database, NotFoundError, RoutingResponse
from app import services

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("mona-api")


class AppState:
    def __init__(self) -> None:
        self.settings = get_settings()
        self.provisioner = K8sProvisioner(self.settings)
        self._sleep_task: asyncio.Task[None] | None = None


state = AppState()


async def _sleep_loop() -> None:
    while True:
        try:
            async with SessionLocal() as session:
                slept = await services.sleep_idle_databases(
                    session, state.settings, state.provisioner
                )
                if slept:
                    logger.info("scaled %s idle database(s) to zero", slept)
        except Exception:
            logger.exception("idle sleeper failed")
        await asyncio.sleep(state.settings.sleep_poll_seconds)


@asynccontextmanager
async def lifespan(_: FastAPI):
    state._sleep_task = asyncio.create_task(_sleep_loop())
    yield
    if state._sleep_task is not None:
        state._sleep_task.cancel()
        try:
            await state._sleep_task
        except asyncio.CancelledError:
            pass


app = FastAPI(title="MonaDB Control Plane", version="0.1.0", lifespan=lifespan)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


def settings_dep() -> Settings:
    return state.settings


def provisioner_dep() -> K8sProvisioner:
    return state.provisioner


@app.get("/healthz")
async def healthz() -> dict[str, str]:
    return {"status": "ok"}


@app.get("/databases", response_model=list[Database])
async def list_databases(
    session: AsyncSession = Depends(get_session),
    settings: Settings = Depends(settings_dep),
) -> list[Database]:
    return await services.list_databases(session, settings)


@app.post("/databases", response_model=Database, status_code=201)
async def create_database(
    body: CreateDatabaseRequest,
    session: AsyncSession = Depends(get_session),
    settings: Settings = Depends(settings_dep),
    provisioner: K8sProvisioner = Depends(provisioner_dep),
) -> Database:
    try:
        return await services.create_database(session, settings, provisioner, body.name)
    except Exception as exc:
        logger.exception("failed to create database")
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@app.get(
    "/databases/{db_id}",
    response_model=Database,
    responses={404: {"model": NotFoundError}},
)
async def get_database(
    db_id: str,
    session: AsyncSession = Depends(get_session),
    settings: Settings = Depends(settings_dep),
) -> Database:
    row = await services.get_database(session, settings, db_id)
    if row is None:
        raise HTTPException(status_code=404, detail=f"database {db_id} not found")
    return row


@app.get(
    "/internal/routing/{hostname}",
    response_model=RoutingResponse,
    responses={404: {"model": NotFoundError}},
)
async def routing(
    hostname: str,
    session: AsyncSession = Depends(get_session),
    settings: Settings = Depends(settings_dep),
    provisioner: K8sProvisioner = Depends(provisioner_dep),
) -> RoutingResponse:
    result = await services.resolve_routing(session, settings, provisioner, hostname)
    if result is None:
        raise HTTPException(status_code=404, detail=f"no route for {hostname}")
    return result


@app.post("/internal/activity/{db_id}", status_code=204)
async def activity(
    db_id: str,
    session: AsyncSession = Depends(get_session),
) -> None:
    ok = await services.touch_activity(session, db_id)
    if not ok:
        raise HTTPException(status_code=404, detail=f"database {db_id} not found")
