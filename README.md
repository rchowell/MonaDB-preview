# MonaDB

Serverless MongoDB-compatible database. This repo contains:

| Package                         | Role                                                                  |
| ------------------------------- | --------------------------------------------------------------------- |
| [`mona-db`](mona-db/)           | Rust MongoDB wire-protocol engine library (SlateDB)                   |
| [`mona-gateway`](mona-gateway/) | Shared multi-tenant gateway (Axum health + Mongo TCP + SlateDB LRU)   |
| [`mona-api`](mona-api/)         | Axum + SQLx control plane (Postgres metadata)                         |
| [`mona-edge`](mona-edge/)       | Axum health + Tokio TLS/SNI edge proxy for MongoDB connections        |
| [`mona-app`](mona-app/)         | Next.js console (Tailwind + [Base UI](https://base-ui.com/))          |
| [`mona-types`](mona-types/)     | TypeSpec source of truth; generates OpenAPI + TS client + Rust models |
| [`infra/k8s`](infra/k8s/)       | Local kind cluster and Kubernetes manifests                           |

## What this slice does

Create a logical database and get a hostname-based connection string:

```text
mongodb://db-<id>.mona.localhost:27017/?tls=true&tlsAllowInvalidCertificates=true
```

Logical databases are multiplexed onto a single shared `mona-gateway` pod with an in-process SlateDB handle LRU. The edge proxy terminates TLS, reads SNI, asks the control plane for the gateway backend, and sends a short `MONA <db_id>` preamble before forwarding MongoDB bytes.

Auth, multi-replica writer leases, branching, and billing are intentionally out of scope for now.

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

Tilt creates/uses the `mona` kind cluster, builds `mona-api` / `mona-gateway` / `mona-edge`, applies Postgres + control plane + gateway + edge, and runs the Next.js console. Open the Tilt UI (usually http://localhost:10350) for build/deploy status.

Once ready:

| Service       | URL / address                                                                       |
| ------------- | ----------------------------------------------------------------------------------- |
| Console       | http://localhost:3000                                                               |
| Control plane | http://localhost:8000                                                               |
| Mongo edge    | `mongodb://db-<id>.mona.localhost:27017/?tls=true&tlsAllowInvalidCertificates=true` |

Host ports `8000` and `27017` come from kind NodePort mappings in [`infra/k8s/kind.yaml`](infra/k8s/kind.yaml).

### Add a hosts entry after creating a database

After you create a database in the console (or via `POST /databases`), copy its hostname (for example `db-abc12345.mona.localhost`) and point it at loopback:

```bash
sudo sh -c 'echo "127.0.0.1 db-<id>.mona.localhost" >> /etc/hosts'
```

Replace `db-<id>.mona.localhost` with the exact hostname from the UI or API response. On some OSes, names under `.localhost` already resolve to `127.0.0.1` without this step; if `mongosh` or the driver fails with a DNS / “nodename nor servname” error, add the hosts line above.

### Manual cluster bring-up (without Tilt)

```bash
./infra/k8s/up.sh
cd mona-app && npm install && NEXT_PUBLIC_MONA_API_URL=http://localhost:8000 npm run dev
```

## Smoke test with mongosh

```bash
mongosh "mongodb://db-<id>.mona.localhost:27017/?tls=true&tlsAllowInvalidCertificates=true"
```

Then:

```js
const { insertedId } = db.smoke.insertOne({ ok: true, n: 1 })
db.smoke.find({ ok: true })
db.smoke.updateOne({ ok: true }, { $set: { n: 2 } })
db.smoke.find({ _id: insertedId })
db.smoke.deleteOne({ ok: true })
db.smoke.find()
```

## Control plane API

TypeSpec lives in [`mona-types`](mona-types/). Regenerate OpenAPI, the TypeScript client, and Rust models (`mona-api/src/models.rs`):

```bash
cd mona-types && npm install && npm run build
```

Database CRUD:

- `POST /databases` `{ "name": "analytics" }`
- `GET /databases`
- `GET /databases/{id}`
- `PATCH /databases/{id}` `{ "name": "renamed" }`
- `DELETE /databases/{id}`

Internal (edge):

- `GET /internal/routing/{hostname}`
- `POST /internal/activity/{id}`

The Next.js console uses React Query hooks backed by the generated `@mona/types/client`.

## Development notes

- Shared gateway listens on `0.0.0.0:27017`, stores tenant data under `/data/{db_id}/`, and exposes Axum `/healthz` on `:8080`.
- Edge → gateway tenant handoff: `MONA <db_id>\n` then MongoDB wire protocol.
- Control plane is Rust (Axum + SQLx); migrations run on process start. Create is metadata-only; storage opens lazily on first connect.
- Idle sleeper marks databases `sleeping` in Postgres after `IDLE_TIMEOUT_SECONDS` (default 300); the gateway pod stays up and evicts LRU handles under memory pressure.
- Next infra step: multi-replica gateway fleet with Postgres-backed writer leases.

### Migrating from per-DB pods

Older clusters that created `mona-db-*` Deployments/Services/PVCs can delete those resources manually; new databases use the shared gateway only.
