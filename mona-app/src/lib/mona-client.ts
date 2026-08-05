import { createMonaClient } from "@mona/types/client";

const API_URL = process.env.NEXT_PUBLIC_MONA_API_URL ?? "http://localhost:8000";

export const monaClient = createMonaClient(API_URL);

export type {
  CreateDatabaseRequest,
  Database,
  DatabaseStatus,
  ErrorBody,
  MonaClient,
  UpdateDatabaseRequest,
} from "@mona/types/client";
