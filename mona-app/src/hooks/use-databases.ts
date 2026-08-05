"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";

import { errorMessage } from "@/lib/api-error";
import { monaClient, type Database } from "@/lib/mona-client";

export const databasesQueryKey = ["databases"] as const;

export function databaseQueryKey(id: string) {
  return ["databases", id] as const;
}

async function listDatabases(): Promise<Database[]> {
  const { data, error } = await monaClient.GET("/databases");
  if (error || !data) {
    throw new Error(await errorMessage(error, "Failed to list databases"));
  }
  return data;
}

async function getDatabase(id: string): Promise<Database> {
  const { data, error } = await monaClient.GET("/databases/{id}", {
    params: { path: { id } },
  });
  if (error || !data) {
    throw new Error(await errorMessage(error, "Failed to get database"));
  }
  return data;
}

export function useDatabases(): UseQueryResult<Database[], Error> {
  return useQuery({
    queryKey: databasesQueryKey,
    queryFn: listDatabases,
  });
}

export function useDatabase(id: string | null): UseQueryResult<Database, Error> {
  return useQuery({
    queryKey: databaseQueryKey(id ?? ""),
    queryFn: () => getDatabase(id!),
    enabled: Boolean(id),
  });
}

export function useCreateDatabase() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => {
      const { data, error } = await monaClient.POST("/databases", {
        body: { name },
      });
      if (error || !data) {
        throw new Error(await errorMessage(error, "Failed to create database"));
      }
      return data;
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: databasesQueryKey });
    },
  });
}

export function useUpdateDatabase() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, name }: { id: string; name: string }) => {
      const { data, error } = await monaClient.PATCH("/databases/{id}", {
        params: { path: { id } },
        body: { name },
      });
      if (error || !data) {
        throw new Error(await errorMessage(error, "Failed to update database"));
      }
      return data;
    },
    onSuccess: async (updated) => {
      await queryClient.invalidateQueries({ queryKey: databasesQueryKey });
      queryClient.setQueryData(databaseQueryKey(updated.id), updated);
    },
  });
}

export function useDeleteDatabase() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const { error, response } = await monaClient.DELETE("/databases/{id}", {
        params: { path: { id } },
      });
      if (error || !response.ok) {
        throw new Error(await errorMessage(error, "Failed to delete database"));
      }
    },
    onSuccess: async (_void, id) => {
      await queryClient.invalidateQueries({ queryKey: databasesQueryKey });
      queryClient.removeQueries({ queryKey: databaseQueryKey(id) });
    },
  });
}
