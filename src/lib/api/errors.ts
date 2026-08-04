import type { ApiError } from "./types";

export class ApiClientError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly details: string | null,
    public readonly recoverable: boolean,
    public readonly operationId: string | null,
  ) {
    super(message);
    this.name = "ApiClientError";
  }
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function parseStringError(value: string): Partial<ApiError> | null {
  try {
    const parsed = JSON.parse(value) as unknown;
    return parsed && typeof parsed === "object" ? (parsed as Partial<ApiError>) : null;
  } catch {
    return null;
  }
}

export function normalizeApiError(
  error: unknown,
  fallbackMessage = "The request failed.",
  fallbackCode = "transport.error",
): ApiClientError {
  if (error instanceof ApiClientError) {
    return error;
  }

  const parsed = typeof error === "string" ? parseStringError(error) : null;
  const record =
    parsed ?? (error && typeof error === "object" ? (error as Partial<ApiError>) : null);
  const rawMessage =
    record?.message ?? (error instanceof Error ? error.message : typeof error === "string" ? error : null);
  const message = optionalString(rawMessage) ?? fallbackMessage;
  const code = optionalString(record?.code) ?? fallbackCode;
  const details = optionalString(record?.details);
  const recoverable =
    typeof record?.recoverable === "boolean" ? record.recoverable : true;
  const operationId = optionalString(record?.operation_id);

  return new ApiClientError(message, code, details, recoverable, operationId);
}
