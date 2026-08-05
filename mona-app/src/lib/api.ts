import type { Database } from "./types";

const API_URL = process.env.NEXT_PUBLIC_MONA_API_URL ?? "http://localhost:8000";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_URL}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    cache: "no-store",
  });

  if (!response.ok) {
    let detail = response.statusText;
    try {
      const body = (await response.json()) as { detail?: string };
      if (body.detail) detail = body.detail;
    } catch {
      // ignore
    }
    throw new Error(detail);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export function listDatabases(): Promise<Database[]> {
  return request<Database[]>("/databases");
}

export function createDatabase(name: string): Promise<Database> {
  return request<Database>("/databases", {
    method: "POST",
    body: JSON.stringify({ name }),
  });
}
