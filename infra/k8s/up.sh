#!/usr/bin/env bash
# Manual kind bring-up without Tilt. Prefer `tilt up` from the repo root.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
K8S="${ROOT}/infra/k8s"
CLUSTER_NAME="mona"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}

need kind
need kubectl
need docker
need openssl

echo "==> generating TLS certs for *.mona.localhost"
bash "${K8S}/scripts/gen-certs.sh"

if ! kind get clusters 2>/dev/null | grep -qx "${CLUSTER_NAME}"; then
  echo "==> creating kind cluster ${CLUSTER_NAME}"
  kind create cluster --config "${K8S}/kind.yaml"
else
  echo "==> kind cluster ${CLUSTER_NAME} already exists"
  kind export kubeconfig --name "${CLUSTER_NAME}"
  kubectl cluster-info --context "kind-${CLUSTER_NAME}" >/dev/null
fi

echo "==> building images"
docker build -t mona-api:local "${ROOT}/mona-api"
docker build -f "${ROOT}/mona-gateway/Dockerfile" -t mona-gateway:local "${ROOT}"
docker build -t mona-edge:local "${ROOT}/mona-edge"

echo "==> loading images into kind"
kind load docker-image mona-api:local --name "${CLUSTER_NAME}"
kind load docker-image mona-gateway:local --name "${CLUSTER_NAME}"
kind load docker-image mona-edge:local --name "${CLUSTER_NAME}"

echo "==> applying base manifests"
kubectl apply -f "${K8S}/base/namespace.yaml"
kubectl apply -f "${K8S}/base/rbac.yaml"
kubectl apply -f "${K8S}/base/postgres.yaml"

echo "==> creating/updating TLS secret"
kubectl -n mona create secret tls mona-edge-tls \
  --cert="${K8S}/certs/tls.crt" \
  --key="${K8S}/certs/tls.key" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl apply -f "${K8S}/base/mona-api.yaml"
kubectl apply -f "${K8S}/base/mona-gateway.yaml"
kubectl apply -f "${K8S}/base/mona-edge.yaml"

echo "==> waiting for control plane + gateway"
kubectl -n mona rollout status deployment/postgres --timeout=180s
kubectl -n mona rollout status deployment/mona-api --timeout=180s
kubectl -n mona rollout status deployment/mona-gateway --timeout=180s
kubectl -n mona rollout status deployment/mona-edge --timeout=180s

cat <<EOF

MonaDB local cluster is ready.

  Control plane:  http://localhost:8000
  Mongo edge:     mongodb://db-<id>.mona.localhost:27017/?tls=true&tlsAllowInvalidCertificates=true

Names under .localhost often resolve to loopback automatically. If not, add:

  echo '127.0.0.1 db-example.mona.localhost' | sudo tee -a /etc/hosts

Start the app:

  cd mona-app && npm install && NEXT_PUBLIC_MONA_API_URL=http://localhost:8000 npm run dev

EOF
