import { describe, expect, it } from "vitest";
import { extractApiKeysFromOcrText } from "../src/lib/ocr/apiKeyOcr";

describe("extractApiKeysFromOcrText", () => {
  it("extracts common sk-style API keys from noisy OCR text", () => {
    expect(extractApiKeysFromOcrText("API Key: sk-proj_abc1234567890-XYZ")).toBe("sk-proj_abc1234567890-XYZ");
  });

  it("extracts Google-style API keys", () => {
    expect(extractApiKeysFromOcrText("AIzaSyAbCdEfGhIjKlMnOpQrStUvWxYz123456")).toBe(
      "AIzaSyAbCdEfGhIjKlMnOpQrStUvWxYz123456",
    );
  });

  it("extracts JWT-like tokens", () => {
    const token = "eyJhbGciOiJIUzI1NiJ9.payload123.signature4567890";

    expect(extractApiKeysFromOcrText(`token ${token}`)).toBe(token);
  });

  it("compacts OCR whitespace within a single key line", () => {
    expect(extractApiKeysFromOcrText("sk- proj_abc 1234567890")).toBe("sk-proj_abc1234567890");
  });

  it("falls back to cleaned OCR text when no known key shape is found", () => {
    expect(extractApiKeysFromOcrText("  custom-key-value  \n\n  next-line  ")).toBe("custom-key-value\nnext-line");
  });
});

