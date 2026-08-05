from __future__ import annotations

import logging
import time
from pathlib import Path

import yaml
from kubernetes import client, config
from kubernetes.client.exceptions import ApiException

from app.config import Settings

logger = logging.getLogger(__name__)


class K8sProvisioner:
    def __init__(self, settings: Settings) -> None:
        self.settings = settings
        self._apps: client.AppsV1Api | None = None
        self._core: client.CoreV1Api | None = None
        self._disabled = settings.k8s_disabled

        if self._disabled:
            return

        try:
            try:
                config.load_incluster_config()
            except config.ConfigException:
                config.load_kube_config()
        except config.ConfigException:
            logger.warning("kubernetes config not found; provisioning disabled")
            self._disabled = True
            return

        self._apps = client.AppsV1Api()
        self._core = client.CoreV1Api()

    @property
    def enabled(self) -> bool:
        return not self._disabled

    def _render(self, template_name: str, replacements: dict[str, str]) -> dict:
        path = Path(self.settings.templates_dir) / template_name
        text = path.read_text()
        for key, value in replacements.items():
            text = text.replace(f"${{{key}}}", value)
        doc = yaml.safe_load(text)
        if not isinstance(doc, dict):
            raise ValueError(f"template {template_name} must be a single YAML mapping")
        return doc

    def _create_or_ignore(self, create_fn, body: dict, *, kind: str) -> None:
        try:
            create_fn(namespace=self.settings.k8s_namespace, body=body)
        except ApiException as exc:
            if exc.status != 409:
                raise
            logger.info("%s already exists", kind)

    def provision_database(self, db_id: str, k8s_name: str) -> None:
        if not self.enabled:
            logger.info("k8s disabled; skip provision for %s", db_id)
            return

        assert self._apps is not None and self._core is not None
        replacements = {
            "DB_ID": db_id,
            "K8S_NAME": k8s_name,
            "NAMESPACE": self.settings.k8s_namespace,
            "IMAGE": self.settings.monadb_image,
        }

        pvc = self._render("pvc.yaml", replacements)
        service = self._render("service.yaml", replacements)
        deployment = self._render("deployment.yaml", replacements)

        self._create_or_ignore(
            self._core.create_namespaced_persistent_volume_claim,
            pvc,
            kind=f"pvc/{k8s_name}-data",
        )
        self._create_or_ignore(
            self._core.create_namespaced_service,
            service,
            kind=f"service/{k8s_name}",
        )
        self._create_or_ignore(
            self._apps.create_namespaced_deployment,
            deployment,
            kind=f"deployment/{k8s_name}",
        )

    def scale(self, k8s_name: str, replicas: int) -> None:
        if not self.enabled:
            logger.info("k8s disabled; skip scale %s -> %s", k8s_name, replicas)
            return

        assert self._apps is not None
        body = {"spec": {"replicas": replicas}}
        self._apps.patch_namespaced_deployment_scale(
            name=k8s_name,
            namespace=self.settings.k8s_namespace,
            body=body,
        )

    def wait_ready(self, k8s_name: str, timeout_seconds: float = 120.0) -> None:
        if not self.enabled:
            return

        assert self._apps is not None
        deadline = time.time() + timeout_seconds
        while time.time() < deadline:
            dep = self._apps.read_namespaced_deployment(
                name=k8s_name,
                namespace=self.settings.k8s_namespace,
            )
            status = dep.status
            if (
                status is not None
                and (status.ready_replicas or 0) >= 1
                and (status.available_replicas or 0) >= 1
            ):
                return
            time.sleep(1.5)
        raise TimeoutError(f"deployment {k8s_name} not ready within {timeout_seconds}s")

    def service_host(self, k8s_name: str) -> str:
        return f"{k8s_name}.{self.settings.k8s_namespace}.svc.cluster.local"
