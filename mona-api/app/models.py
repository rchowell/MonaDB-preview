import enum
from datetime import datetime

from sqlalchemy import DateTime, Enum, String, func
from sqlalchemy.orm import Mapped, mapped_column

from app.db import Base


class DatabaseStatus(str, enum.Enum):
    pending = "pending"
    ready = "ready"
    sleeping = "sleeping"
    error = "error"


class Database(Base):
    __tablename__ = "databases"

    id: Mapped[str] = mapped_column(String(32), primary_key=True)
    name: Mapped[str] = mapped_column(String(64), nullable=False)
    status: Mapped[DatabaseStatus] = mapped_column(
        Enum(DatabaseStatus, name="database_status", native_enum=False),
        nullable=False,
        default=DatabaseStatus.pending,
    )
    k8s_name: Mapped[str] = mapped_column(String(63), nullable=False, unique=True)
    last_active_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
    )
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
    )
