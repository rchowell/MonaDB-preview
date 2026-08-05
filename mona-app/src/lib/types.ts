/** Mirrors mona-types Database schema (see mona-types/generated/openapi.ts). */
export type DatabaseStatus = "pending" | "ready" | "sleeping" | "error";

export type Database = {
  id: string;
  name: string;
  hostname: string;
  connectionString: string;
  status: DatabaseStatus;
  createdAt: string;
};
