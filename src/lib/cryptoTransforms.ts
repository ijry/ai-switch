export type CryptoOperation =
  | "base64-encode"
  | "base64-decode"
  | "url-encode"
  | "url-decode"
  | "hex-encode"
  | "hex-decode";

export type CryptoTransformResult = {
  output: string;
  error: "invalid-base64" | "invalid-url" | "invalid-hex-length" | "invalid-hex" | null;
};

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder("utf-8", { fatal: true });

function bytesToBase64(bytes: Uint8Array) {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary);
}

function base64ToBytes(input: string) {
  const normalized = input.replace(/\s+/g, "");
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(normalized) || normalized.length % 4 === 1) {
    throw new Error("invalid-base64");
  }
  const binary = atob(normalized);
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

function bytesToHex(bytes: Uint8Array) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function hexToBytes(input: string) {
  const normalized = input.replace(/\s+/g, "");
  if (normalized.length % 2 !== 0) {
    throw new Error("invalid-hex-length");
  }
  if (!/^[0-9a-fA-F]*$/.test(normalized)) {
    throw new Error("invalid-hex");
  }
  const bytes = new Uint8Array(normalized.length / 2);
  for (let index = 0; index < normalized.length; index += 2) {
    bytes[index / 2] = Number.parseInt(normalized.slice(index, index + 2), 16);
  }
  return bytes;
}

export function transformCryptoText(input: string, operation: CryptoOperation): CryptoTransformResult {
  try {
    if (operation === "base64-encode") {
      return { output: bytesToBase64(textEncoder.encode(input)), error: null };
    }
    if (operation === "base64-decode") {
      return { output: textDecoder.decode(base64ToBytes(input)), error: null };
    }
    if (operation === "url-encode") {
      return { output: encodeURIComponent(input), error: null };
    }
    if (operation === "url-decode") {
      return { output: decodeURIComponent(input), error: null };
    }
    if (operation === "hex-encode") {
      return { output: bytesToHex(textEncoder.encode(input)), error: null };
    }
    return { output: textDecoder.decode(hexToBytes(input)), error: null };
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    if (message === "invalid-hex-length") {
      return { output: "", error: "invalid-hex-length" };
    }
    if (message === "invalid-hex") {
      return { output: "", error: "invalid-hex" };
    }
    if (operation === "base64-decode") {
      return { output: "", error: "invalid-base64" };
    }
    if (operation === "url-decode") {
      return { output: "", error: "invalid-url" };
    }
    return { output: "", error: "invalid-hex" };
  }
}
