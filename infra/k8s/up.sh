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

echo "==> generating TLS certs for *.mona.local"
bash "${K8S}/scripts/gen-certs.sh"

if ! kind get clusters 2>/dev/null | grep -qx "${CLUSTER_NAME}"; then
  echo "==> creating kind cluster ${CLUSTER_NAME}"
  kind create cluster --config "${K8S}/kind.yaml"
else
  echo "==> kind cluster ${CLUSTER_NAME} already exists"
  kubectl cluster-info --context "kind-${CLUSTER_NAME}" >/dev/null
fi

echo "==> syncing deployment templates into mona-api image context"
rm -rf "${ROOT}/mona-api/templates"
cp -R "${K8S}/templates" "${ROOT}/mona-api/templates"

echo "==> building images"
docker build -t mona-db:local "${ROOT}/mona-db"
docker build -t mona-api:local "${ROOT}/mona-api"
docker build -t mona-edge:local "${K8S}/edge"

echo "==> loading images into kind"
kind load docker-image mona-db:local --name "${CLUSTER_NAME}"
kind load docker-image mona-api:local --name "${CLUSTER_NAME}"
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
kubectl apply -f "${K8S}/base/edge.yaml"

echo "==> waiting for control plane"
kubectl -n mona rollout status deployment/postgres --timeout=180s
kubectl -n mona rollout status deployment/mona-api --timeout=180s
kubectl -n mona rollout status deployment/mona-edge --timeout=180s

cat <<EOF

MonaDB local cluster is ready.

  Control plane:  http://localhost:8000
  Mongo edge:     mongodb://db-<id>.mona.local:27017/?tls=true&tlsAllowInvalidCertificates=true

Add a hosts entry so *.mona.local resolves to 127.0.0.1, for example:

  echo '127.0.0.1 db-example.mona.local' | sudo tee -a /etc/hosts

Or use a resolver that maps *.mona.local -> 127.0.0.1.

Start the app:

  cd mona-app && npm install && NEXT_PUBLIC_MONA_API_URL=http://localhost:8000 npm run dev

EOF
