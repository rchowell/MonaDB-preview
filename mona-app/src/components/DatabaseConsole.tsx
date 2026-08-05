"use client";

import { useState, type FormEvent } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  useCreateDatabase,
  useDatabases,
  useDeleteDatabase,
  useUpdateDatabase,
} from "@/hooks/use-databases";
import type { Database } from "@/lib/mona-client";
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
  const [rename, setRename] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const { data: databases = [], error: listError, isLoading } = useDatabases();
  const createDatabase = useCreateDatabase();
  const updateDatabase = useUpdateDatabase();
  const deleteDatabase = useDeleteDatabase();

  const active =
    databases.find((db) => db.id === selectedId) ?? databases[0] ?? null;

  const error =
    listError?.message ??
    createDatabase.error?.message ??
    updateDatabase.error?.message ??
    deleteDatabase.error?.message ??
    null;

  async function onCreate(event: FormEvent) {
    event.preventDefault();
    const created = await createDatabase.mutateAsync(name.trim());
    setSelectedId(created.id);
    setName("");
  }

  async function onRename(event: FormEvent) {
    event.preventDefault();
    if (!active) return;
    const updated = await updateDatabase.mutateAsync({
      id: active.id,
      name: rename.trim(),
    });
    setSelectedId(updated.id);
    setRename("");
  }

  async function onDelete() {
    if (!active) return;
    const id = active.id;
    await deleteDatabase.mutateAsync(id);
    setSelectedId(null);
  }

  async function copyConnection(value: string) {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

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
        <Button
          type="submit"
          disabled={createDatabase.isPending || name.trim().length === 0}
          size="lg"
        >
          {createDatabase.isPending ? "Creating…" : "Create database"}
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

          <form onSubmit={onRename} className="flex flex-col gap-3 sm:flex-row sm:items-end">
            <label className="flex flex-1 flex-col gap-2 text-sm text-foreground">
              Rename
              <Input
                required
                value={rename}
                onChange={(event) => setRename(event.target.value)}
                placeholder={active.name}
              />
            </label>
            <Button
              type="submit"
              variant="outline"
              disabled={updateDatabase.isPending || rename.trim().length === 0}
              size="lg"
            >
              {updateDatabase.isPending ? "Saving…" : "Save name"}
            </Button>
          </form>

          <Button
            type="button"
            variant="destructive"
            disabled={deleteDatabase.isPending}
            onClick={() => void onDelete()}
          >
            {deleteDatabase.isPending ? "Deleting…" : "Delete database"}
          </Button>
        </section>
      ) : null}

      <section className="space-y-3 border-t pt-6">
        <h2 className="font-[family-name:var(--font-display)] text-xl tracking-tight">
          Your databases
        </h2>
        {isLoading ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : databases.length === 0 ? (
          <p className="text-sm text-muted-foreground">No databases yet.</p>
        ) : (
          <ul className="divide-y border-y">
            {databases.map((db) => (
              <li key={db.id}>
                <button
                  type="button"
                  onClick={() => setSelectedId(db.id)}
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
