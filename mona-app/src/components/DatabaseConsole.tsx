"use client";

import { startTransition, useEffect, useState, type FormEvent } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { createDatabase, listDatabases } from "@/lib/api";
import type { Database } from "@/lib/types";
import { cn } from "@/lib/utils";

function statusVariant(
  status: Database["status"],
): "default" | "secondary" | "destructive" | "outline" {
  switch (status) {
    case "ready":
      return "default";
    case "sleeping":
      return "secondary";
    case "pending":
      return "outline";
    case "error":
      return "destructive";
  }
}

function statusClass(status: Database["status"]): string {
  switch (status) {
    case "ready":
      return "bg-emerald-50 text-emerald-700 border-transparent";
    case "sleeping":
      return "";
    case "pending":
      return "bg-amber-50 text-amber-700 border-amber-200";
    case "error":
      return "";
  }
}

export function DatabaseConsole() {
  const [name, setName] = useState("");
  const [databases, setDatabases] = useState<Database[]>([]);
  const [selected, setSelected] = useState<Database | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  async function refresh() {
    const rows = await listDatabases();
    setDatabases(rows);
  }

  useEffect(() => {
    startTransition(() => {
      refresh().catch((err: unknown) => {
        setError(err instanceof Error ? err.message : "Failed to load databases");
      });
    });
  }, []);

  async function onCreate(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setLoading(true);
    try {
      const created = await createDatabase(name.trim());
      setSelected(created);
      setName("");
      await refresh();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to create database");
    } finally {
      setLoading(false);
    }
  }

  async function copyConnection(value: string) {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  const active = selected ?? databases[0] ?? null;

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-8 p-6">
      <header className="space-y-2">
        <h1 className="font-[family-name:var(--font-display)] text-3xl leading-tight tracking-tight">
          Databases
        </h1>
        <p className="max-w-xl text-sm leading-relaxed text-muted-foreground">
          Provision a logical MonaDB instance and copy a hostname-based connection
          string. Pods wake on use and sleep when idle.
        </p>
      </header>

      <form onSubmit={onCreate} className="flex flex-col gap-3 sm:flex-row sm:items-end">
        <label className="flex flex-1 flex-col gap-2 text-sm text-foreground">
          Database name
          <Input
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="analytics"
          />
        </label>
        <Button type="submit" disabled={loading || name.trim().length === 0} size="lg">
          {loading ? "Creating…" : "Create database"}
        </Button>
      </form>

      {error ? (
        <p className="rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </p>
      ) : null}

      {active ? (
        <section className="space-y-4 border-t pt-6">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="font-[family-name:var(--font-display)] text-2xl tracking-tight">
              {active.name}
            </h2>
            <Badge
              variant={statusVariant(active.status)}
              className={cn("uppercase tracking-wide", statusClass(active.status))}
            >
              {active.status}
            </Badge>
          </div>
          <p className="text-sm text-muted-foreground">
            Host <span className="font-mono text-foreground">{active.hostname}</span>
            {" · "}
            add{" "}
            <code className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-xs text-foreground">
              127.0.0.1 {active.hostname}
            </code>{" "}
            to your hosts file for local kind.
          </p>
          <div className="flex flex-col gap-2">
            <label className="text-sm text-foreground">Connection string</label>
            <div className="flex flex-col gap-2 sm:flex-row">
              <code className="flex-1 overflow-x-auto rounded-lg border bg-muted/40 px-3 py-2.5 font-mono text-xs sm:text-sm">
                {active.connectionString}
              </code>
              <Button
                type="button"
                variant="outline"
                size="lg"
                onClick={() => copyConnection(active.connectionString)}
                className="shrink-0"
              >
                {copied ? "Copied" : "Copy"}
              </Button>
            </div>
          </div>
        </section>
      ) : null}

      <section className="space-y-3 border-t pt-6">
        <h2 className="font-[family-name:var(--font-display)] text-xl tracking-tight">
          Your databases
        </h2>
        {databases.length === 0 ? (
          <p className="text-sm text-muted-foreground">No databases yet.</p>
        ) : (
          <ul className="divide-y border-y">
            {databases.map((db) => (
              <li key={db.id}>
                <button
                  type="button"
                  onClick={() => setSelected(db)}
                  className="flex w-full items-center justify-between gap-4 py-3 text-left transition hover:bg-muted/50"
                >
                  <div>
                    <p className="font-medium">{db.name}</p>
                    <p className="font-mono text-xs text-muted-foreground">{db.hostname}</p>
                  </div>
                  <Badge
                    variant={statusVariant(db.status)}
                    className={cn("uppercase tracking-wide", statusClass(db.status))}
                  >
                    {db.status}
                  </Badge>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
