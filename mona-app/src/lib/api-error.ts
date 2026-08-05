import type { ErrorBody } from "@mona/types/client";

export async function errorMessage(
  error: unknown,
  fallback = "Request failed",
): Promise<string> {
  if (error && typeof error === "object") {
    const body = error as Partial<ErrorBody>;
    if (typeof body.detail === "string" && body.detail.length > 0) {
      return body.detail;
    }
  }
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return fallback;
}
