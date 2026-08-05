#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CERT_DIR="${ROOT}/certs"
mkdir -p "${CERT_DIR}"

if [[ -f "${CERT_DIR}/tls.crt" && -f "${CERT_DIR}/tls.key" ]]; then
  echo "certs already exist in ${CERT_DIR}"
  exit 0
fi

openssl req -x509 -nodes -newkey rsa:2048 -days 3650 \
  -keyout "${CERT_DIR}/tls.key" \
  -out "${CERT_DIR}/tls.crt" \
  -subj "/CN=*.mona.local" \
  -addext "subjectAltName=DNS:*.mona.local,DNS:mona.local"

echo "wrote ${CERT_DIR}/tls.crt and ${CERT_DIR}/tls.key"
