from datetime import datetime
from enum import Enum

from pydantic import BaseModel, Field


class DatabaseStatus(str, Enum):
    pending = "pending"
    ready = "ready"
    sleeping = "sleeping"
    error = "error"


class CreateDatabaseRequest(BaseModel):
    name: str = Field(min_length=1, max_length=64)


class Database(BaseModel):
    id: str
    name: str
    hostname: str
    connectionString: str
    status: DatabaseStatus
    createdAt: datetime

    model_config = {"from_attributes": True}


class NotFoundError(BaseModel):
    detail: str


class RoutingResponse(BaseModel):
    id: str
    backendHost: str
    backendPort: int
    status: DatabaseStatus
