from functools import lru_cache

from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    database_url: str = "postgresql+asyncpg://mona:mona@localhost:5432/mona"
    edge_domain: str = "mona.local"
    monadb_image: str = "mona-db:local"
    k8s_namespace: str = "mona"
    idle_timeout_seconds: int = 300
    sleep_poll_seconds: int = 30
    templates_dir: str = "templates"
    # When true, skip talking to the Kubernetes API (local unit/smoke without cluster).
    k8s_disabled: bool = False


@lru_cache
def get_settings() -> Settings:
    return Settings()
