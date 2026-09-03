import { describe, expect, it } from "vitest";
import { parseUsageMetadata, prettyJsonOrText } from "../../src/lib/usageMetadata";

describe("parseUsageMetadata", () => {
  it("pulls out the fields the detail panel names", () => {
    const parsed = parseUsageMetadata(
      JSON.stringify({
        path: "/v1/messages",
        target_url: "https://api.anthropic.com/v1/messages",
        status: 200,
        success: true,
        duration_ms: 1842,
        trace_id: "trace-1",
        requested_model: "claude-opus-5",
        upstream_model: "opus-upstream",
        response_body: '{"id":"msg_a"}',
      }),
    );

    expect(parsed.valid).toBe(true);
    expect(parsed.targetUrl).toBe("https://api.anthropic.com/v1/messages");
    expect(parsed.traceId).toBe("trace-1");
    // A number field still has to reach the UI as text.
    expect(parsed.durationMs).toBe("1842");
    expect(parsed.requestedModel).toBe("claude-opus-5");
    expect(parsed.upstreamModel).toBe("opus-upstream");
    expect(parsed.responseBody).toBe('{"id":"msg_a"}');
    expect(parsed.errorMessage).toBeNull();
    // Keys with no named field of their own stay visible through `formatted`.
    expect(parsed.formatted).toContain('"path": "/v1/messages"');
  });

  it("treats blank and non-scalar fields as absent", () => {
    const parsed = parseUsageMetadata(
      JSON.stringify({ target_url: "   ", trace_id: null, error_message: { code: 1 } }),
    );

    expect(parsed.targetUrl).toBeNull();
    expect(parsed.traceId).toBeNull();
    expect(parsed.errorMessage).toBeNull();
  });

  it("keeps unparseable metadata readable instead of dropping the row", () => {
    const parsed = parseUsageMetadata("{bad json");

    expect(parsed.valid).toBe(false);
    expect(parsed.formatted).toBe("{bad json");
    expect(parsed.responseBody).toBeNull();
  });

  it("accepts valid JSON that is not an object", () => {
    const parsed = parseUsageMetadata("[1,2]");

    expect(parsed.valid).toBe(true);
    expect(parsed.formatted).toBe("[\n  1,\n  2\n]");
    expect(parsed.targetUrl).toBeNull();
  });
});

describe("prettyJsonOrText", () => {
  it("indents JSON", () => {
    expect(prettyJsonOrText('{"a":1}')).toBe('{\n  "a": 1\n}');
  });

  it("returns an SSE body unchanged", () => {
    const body = 'event: message_start\ndata: {"type":"message_start"}\n\n';
    expect(prettyJsonOrText(body)).toBe(body);
  });
});
