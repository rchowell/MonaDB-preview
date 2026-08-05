use std::path::PathBuf;
use std::time::{Duration, Instant};

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{PersistentVolumeClaim, Service};
use kube::api::{Patch, PatchParams, PostParams};
use kube::{Api, Client, Error as KubeError};
use tracing::{info, warn};

use crate::config::Config;
use crate::error::AppError;

#[derive(Clone)]
pub struct K8sProvisioner {
    config: Config,
    client: Option<Client>,
    disabled: bool,
}

impl K8sProvisioner {
    pub async fn new(config: Config) -> Self {
        if config.k8s_disabled {
            return Self {
                config,
                client: None,
                disabled: true,
            };
        }

        match Client::try_default().await {
            Ok(client) => Self {
                config,
                client: Some(client),
                disabled: false,
            },
            Err(err) => {
                warn!(error = %err, "kubernetes config not found; provisioning disabled");
                Self {
                    config,
                    client: None,
                    disabled: true,
                }
            }
        }
    }

    pub fn enabled(&self) -> bool {
        !self.disabled
    }

    pub fn service_host(&self, k8s_name: &str) -> String {
        format!(
            "{}.{}.svc.cluster.local",
            k8s_name, self.config.k8s_namespace
        )
    }

    fn render(&self, template_name: &str, replacements: &[(&str, &str)]) -> Result<String, AppError> {
        let path = PathBuf::from(&self.config.templates_dir).join(template_name);
        let mut text = std::fs::read_to_string(&path).map_err(|err| {
            AppError::Internal(format!("failed to read template {}: {err}", path.display()))
        })?;
        for (key, value) in replacements {
            text = text.replace(&format!("${{{key}}}"), value);
        }
        Ok(text)
    }

    async fn create_ignore_exists<T>(&self, api: &Api<T>, yaml: &str, kind: &str) -> Result<(), AppError>
    where
        T: kube::Resource
            + serde::de::DeserializeOwned
            + serde::Serialize
            + Clone
            + std::fmt::Debug,
        <T as kube::Resource>::DynamicType: Default,
    {
        let obj: T = serde_yaml::from_str(yaml)
            .map_err(|err| AppError::Internal(format!("invalid {kind} yaml: {err}")))?;
        match api.create(&PostParams::default(), &obj).await {
            Ok(_) => Ok(()),
            Err(KubeError::Api(err)) if err.code == 409 => {
                info!("{kind} already exists");
                Ok(())
            }
            Err(err) => Err(AppError::Internal(format!("failed to create {kind}: {err}"))),
        }
    }

    pub async fn provision_database(&self, db_id: &str, k8s_name: &str) -> Result<(), AppError> {
        if !self.enabled() {
            info!(db_id, "k8s disabled; skip provision");
            return Ok(());
        }
        let client = self.client.as_ref().expect("client present when enabled");
        let ns = &self.config.k8s_namespace;
        let replacements = [
            ("DB_ID", db_id),
            ("K8S_NAME", k8s_name),
            ("NAMESPACE", ns.as_str()),
            ("IMAGE", self.config.monadb_image.as_str()),
        ];

        let pvc_yaml = self.render("pvc.yaml", &replacements)?;
        let service_yaml = self.render("service.yaml", &replacements)?;
        let deployment_yaml = self.render("deployment.yaml", &replacements)?;

        let pvcs: Api<PersistentVolumeClaim> = Api::namespaced(client.clone(), ns);
        let services: Api<Service> = Api::namespaced(client.clone(), ns);
        let deployments: Api<Deployment> = Api::namespaced(client.clone(), ns);

        self.create_ignore_exists(&pvcs, &pvc_yaml, &format!("pvc/{k8s_name}-data"))
            .await?;
        self.create_ignore_exists(&services, &service_yaml, &format!("service/{k8s_name}"))
            .await?;
        self.create_ignore_exists(
            &deployments,
            &deployment_yaml,
            &format!("deployment/{k8s_name}"),
        )
        .await?;
        Ok(())
    }

    pub async fn scale(&self, k8s_name: &str, replicas: i32) -> Result<(), AppError> {
        if !self.enabled() {
            info!(k8s_name, replicas, "k8s disabled; skip scale");
            return Ok(());
        }
        let client = self.client.as_ref().expect("client present when enabled");
        let deployments: Api<Deployment> =
            Api::namespaced(client.clone(), &self.config.k8s_namespace);
        let patch = serde_json::json!({
            "spec": { "replicas": replicas }
        });
        deployments
            .patch(k8s_name, &PatchParams::default(), &Patch::Merge(patch))
            .await
            .map_err(|err| AppError::Internal(format!("failed to scale {k8s_name}: {err}")))?;
        Ok(())
    }

    pub async fn wait_ready(&self, k8s_name: &str, timeout: Duration) -> Result<(), AppError> {
        if !self.enabled() {
            return Ok(());
        }
        let client = self.client.as_ref().expect("client present when enabled");
        let deployments: Api<Deployment> =
            Api::namespaced(client.clone(), &self.config.k8s_namespace);
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let dep = deployments.get(k8s_name).await.map_err(|err| {
                AppError::Internal(format!("failed to read deployment {k8s_name}: {err}"))
            })?;
            let ready = dep
                .status
                .as_ref()
                .and_then(|s| s.ready_replicas)
                .unwrap_or(0);
            let available = dep
                .status
                .as_ref()
                .and_then(|s| s.available_replicas)
                .unwrap_or(0);
            if ready >= 1 && available >= 1 {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        Err(AppError::Internal(format!(
            "deployment {k8s_name} not ready within {}s",
            timeout.as_secs()
        )))
    }
}
