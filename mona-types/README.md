# mona-types

TypeSpec source of truth for the MonaDB control-plane API.

```bash
npm install
npm run build
```

Outputs:

| Artifact | Path |
| -------- | ---- |
| OpenAPI 3 | `generated/openapi.json` |
| TypeScript types | `generated/openapi.ts` |
| TypeScript client (`openapi-fetch`) | `generated/client.ts` |
| Rust models (typify) | [`../mona-api/src/models.rs`](../mona-api/src/models.rs) |

## Consuming the TypeScript client

```ts
import { createMonaClient, type Database } from "@mona/types/client";

const client = createMonaClient("http://localhost:8000");
const { data } = await client.GET("/databases");
```

`mona-app` wraps this client in React Query hooks under `src/hooks/use-databases.ts`.

## Rust models

`npm run build` regenerates [`mona-api/src/models.rs`](../mona-api/src/models.rs) via typify. Axum handlers use those types directly (`Database`, `CreateDatabaseRequest`, etc.).

## Database CRUD

| Method | Path | Body |
| ------ | ---- | ---- |
| `GET` | `/databases` | — |
| `POST` | `/databases` | `{ "name": string }` |
| `GET` | `/databases/{id}` | — |
| `PATCH` | `/databases/{id}` | `{ "name": string }` |
| `DELETE` | `/databases/{id}` | — |
