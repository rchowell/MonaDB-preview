# MonaDB

Serverless MongoDB-compatible database. This repo contains:

| Package                     | Role                                                            |
| --------------------------- | --------------------------------------------------------------- |
| [`mona-db`](mona-db/)       | Rust MongoDB wire-protocol engine (SlateDB)                     |
| [`mona-api`](mona-api/)     | FastAPI control plane (Postgres 18 metadata + K8s provisioning) |
| [`mona-app`](mona-app/)     | Next.js console (Tailwind + [Base UI](https://base-ui.com/))    |
| [`mona-types`](mona-types/) | TypeSpec source of truth for the control-plane API              |
| [`infra/k8s`](infra/k8s/)   | Local kind cluster, edge TLS/SNI proxy, manifests               |

## What this slice does

Create a logical database and get a hostname-based connection string:

```text
mongodb://db-<id>.mona.local:27017/?tls=true&tlsAllowInvalidCertificates=true
```

Each database runs as its own `mona-db` Deployment (scale to 0 after 5 minutes idle). An edge proxy terminates TLS, reads SNI, asks the control plane for the backend, and wakes the pod if needed.

Auth, shared gateway fleets, branching, and billing are intentionally out of scope for now.

## Prerequisites

- Docker
- [kind](https://kind.sigs.k8s.io/)
- kubectl
- openssl
- Node.js 20+
- [Tilt](https://docs.tilt.dev/install.html)
- (optional) [mongosh](https://www.mongodb.com/docs/mongodb-shell/)

## Bring up the full stack

```bash
tilt up
```

Tilt creates/uses the `mona` kind cluster, builds `mona-db` / `mona-api` / `mona-edge`, applies Postgres + control plane + edge, and runs the Next.js console. Open the Tilt UI (usually http://localhost:10350) for build/deploy status.

Once ready:

| Service        | URL / address |
| -------------- | ------------- |
| Console        | http://localhost:3000 |
| Control plane  | http://localhost:8000 |
| Mongo edge     | `mongodb://db-<id>.mona.local:27017/?tls=true&tlsAllowInvalidCertificates=true` |

Host ports `8000` and `27017` come from kind NodePort mappings in [`infra/k8s/kind.yaml`](infra/k8s/kind.yaml).

### DNS for `*.mona.local`

Point database hostnames at localhost. After creating a database named in the UI, add:

```bash
sudo sh -c 'echo "127.0.0.1 db-<id>.mona.local" >> /etc/hosts'
```

### Manual cluster bring-up (without Tilt)

```bash
./infra/k8s/up.sh
cd mona-app && npm install && NEXT_PUBLIC_MONA_API_URL=http://localhost:8000 npm run dev
```

## Smoke test with mongosh

```bash
mongosh "mongodb://db-<id>.mona.local:27017/?tls=true&tlsAllowInvalidCertificates=true"
```

Then:

```js
db.smoke.insertOne({ ok: true })
db.smoke.find()
```

## Control plane API

TypeSpec lives in [`mona-types`](mona-types/). Regenerate OpenAPI + TS types:

```bash
cd mona-types && npm install && npm run build
```

Useful endpoints:

- `POST /databases` `{ "name": "analytics" }`
- `GET /databases`
- `GET /databases/{id}`
- `GET /internal/routing/{hostname}` (edge)
- `POST /internal/activity/{id}` (edge)

## Development notes

- Engine image listens on `0.0.0.0:27017` and stores data under `/data` (PVC per database).
- Idle sleeper in `mona-api` scales Deployments to 0 after `IDLE_TIMEOUT_SECONDS` (default 300).
- Next infra step after this vertical slice: shared gateway fleet with SlateDB handle LRU + writer leases (instead of one pod per database).
