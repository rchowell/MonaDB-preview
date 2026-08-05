"use client";

import { Button } from "@base-ui/react/button";
import { Input } from "@base-ui/react/input";
import { startTransition, useEffect, useState, type FormEvent } from "react";

import { createDatabase, listDatabases } from "@/lib/api";
import type { Database } from "@/lib/types";

function statusClass(status: Database["status"]): string {
  switch (status) {
    case "ready":
      return "text-emerald-700 bg-emerald-50";
    case "sleeping":
      return "text-slate-600 bg-slate-100";
    case "pending":
      return "text-amber-700 bg-amber-50";
    case "error":
      return "text-rose-700 bg-rose-50";
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
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-10 px-6 py-16">
      <header className="space-y-3">
        <p className="font-[family-name:var(--font-display)] text-sm tracking-[0.2em] text-teal-800 uppercase">
          MonaDB
        </p>
        <h1 className="font-[family-name:var(--font-display)] text-4xl leading-tight text-stone-900 sm:text-5xl">
          Create a database
        </h1>
        <p className="max-w-xl text-base leading-relaxed text-stone-600">
          Provision a logical MonaDB instance and copy a hostname-based connection
          string. Pods wake on use and sleep when idle.
        </p>
      </header>

      <form onSubmit={onCreate} className="flex flex-col gap-3 sm:flex-row sm:items-end">
        <label className="flex flex-1 flex-col gap-2 text-sm text-stone-700">
          Database name
          <Input
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="analytics"
            className="h-11 rounded-md border border-stone-300 bg-white/80 px-3 text-base text-stone-900 outline-none focus:border-teal-700"
          />
        </label>
        <Button
          type="submit"
          disabled={loading || name.trim().length === 0}
          className="h-11 rounded-md bg-teal-800 px-5 text-sm font-medium text-white transition enabled:hover:bg-teal-700 disabled:opacity-50"
        >
          {loading ? "Creating…" : "Create database"}
        </Button>
      </form>

      {error ? (
        <p className="rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-800">
          {error}
        </p>
      ) : null}

      {active ? (
        <section className="space-y-4 border-t border-stone-200 pt-8">
          <div className="flex flex-wrap items-center gap-3">
            <h2 className="font-[family-name:var(--font-display)] text-2xl text-stone-900">
              {active.name}
            </h2>
            <span
              className={`rounded px-2 py-0.5 text-xs font-medium tracking-wide uppercase ${statusClass(active.status)}`}
            >
              {active.status}
            </span>
          </div>
          <p className="text-sm text-stone-500">
            Host <span className="font-mono text-stone-800">{active.hostname}</span>
            {" · "}
            add{" "}
            <code className="rounded bg-stone-100 px-1.5 py-0.5 font-mono text-xs text-stone-800">
              127.0.0.1 {active.hostname}
            </code>{" "}
            to your hosts file for local kind.
          </p>
          <div className="flex flex-col gap-2">
            <label className="text-sm text-stone-700">Connection string</label>
            <div className="flex flex-col gap-2 sm:flex-row">
              <code className="flex-1 overflow-x-auto rounded-md border border-stone-200 bg-white/90 px-3 py-3 font-mono text-xs text-stone-800 sm:text-sm">
                {active.connectionString}
              </code>
              <Button
                type="button"
                onClick={() => copyConnection(active.connectionString)}
                className="h-11 shrink-0 rounded-md border border-stone-300 bg-white px-4 text-sm text-stone-800 hover:bg-stone-50"
              >
                {copied ? "Copied" : "Copy"}
              </Button>
            </div>
          </div>
        </section>
      ) : null}

      <section className="space-y-3 border-t border-stone-200 pt-8">
        <h2 className="font-[family-name:var(--font-display)] text-xl text-stone-900">
          Your databases
        </h2>
        {databases.length === 0 ? (
          <p className="text-sm text-stone-500">No databases yet.</p>
        ) : (
          <ul className="divide-y divide-stone-200 border-y border-stone-200">
            {databases.map((db) => (
              <li key={db.id}>
                <button
                  type="button"
                  onClick={() => setSelected(db)}
                  className="flex w-full items-center justify-between gap-4 py-3 text-left transition hover:bg-white/50"
                >
                  <div>
                    <p className="font-medium text-stone-900">{db.name}</p>
                    <p className="font-mono text-xs text-stone-500">{db.hostname}</p>
                  </div>
                  <span
                    className={`rounded px-2 py-0.5 text-xs font-medium tracking-wide uppercase ${statusClass(db.status)}`}
                  >
                    {db.status}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
