/**
 * The fields the route proxy writes into a usage row's `metadata_json`, plus the
 * whole blob formatted for display.
 *
 * Only the values the detail panel names on their own are pulled out; everything
 * else stays visible through `formatted`, so a metadata key added on the Rust
 * side still reaches the user without a change here.
 */
export type ParsedUsageMetadata = {
  /**
   * False when the blob is not a JSON object. `formatted` then carries the
   * original text: an unparseable metadata row is exactly the case where seeing
   * what was actually stored matters most.
   */
  valid: boolean;
  /** Pretty-printed JSON when `valid`, the original text when not. */
  formatted: string;
  targetUrl: string | null;
  traceId: string | null;
  /** Stringified because the proxy writes it as a number. */
  durationMs: string | null;
  errorMessage: string | null;
  requestedModel: string | null;
  upstreamModel: string | null;
  /** Upstream body preview, already truncated by the proxy before storage. */
  responseBody: string | null;
};

const EMPTY_FIELDS = {
  targetUrl: null,
  traceId: null,
  durationMs: null,
  errorMessage: null,
  requestedModel: null,
  upstreamModel: null,
  responseBody: null,
} as const;

/** Read one scalar metadata field, treating blank strings as absent. */
function field(record: Record<string, unknown>, key: string): string | null {
  const value = record[key];
  if (typeof value === "string") {
    return value.trim() ? value : null;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

/** Split the named fields out of a usage row's stored proxy metadata. */
export function parseUsageMetadata(metadataJson: string): ParsedUsageMetadata {
  let value: unknown;
  try {
    value = JSON.parse(metadataJson);
  } catch {
    return { ...EMPTY_FIELDS, valid: false, formatted: metadataJson };
  }

  const formatted = JSON.stringify(value, null, 2) ?? metadataJson;
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    // Valid JSON, but not the object shape the named fields live in.
    return { ...EMPTY_FIELDS, valid: true, formatted };
  }

  const record = value as Record<string, unknown>;
  return {
    valid: true,
    formatted,
    targetUrl: field(record, "target_url"),
    traceId: field(record, "trace_id"),
    durationMs: field(record, "duration_ms"),
    errorMessage: field(record, "error_message"),
    requestedModel: field(record, "requested_model"),
    upstreamModel: field(record, "upstream_model"),
    responseBody: field(record, "response_body"),
  };
}

/**
 * Indent a JSON payload, or hand back the text unchanged.
 *
 * Upstream bodies arrive as plain JSON, as an SSE stream, or truncated mid-token,
 * and only the first of those can be re-indented.
 */
export function prettyJsonOrText(value: string): string {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
