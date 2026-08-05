import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  copySensitiveText,
  downloadRouteCredentialJson,
} from "../../src/lib/routeCredentialTransfer";

describe("route credential transfer helpers", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("downloads JSON through one temporary anchor and always cleans up", () => {
    const blob = { kind: "json-blob" };
    const BlobMock = vi.fn(() => blob);
    const createObjectURL = vi.fn(() => "blob:route-credentials");
    const revokeObjectURL = vi.fn();
    const anchor = document.createElement("a");
    const click = vi.spyOn(anchor, "click").mockImplementation(() => {
      throw new Error("download blocked");
    });
    const remove = vi.spyOn(anchor, "remove");
    vi.spyOn(document, "createElement").mockReturnValue(anchor);
    vi.spyOn(document.body, "appendChild");
    vi.stubGlobal("Blob", BlobMock);
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });

    expect(() =>
      downloadRouteCredentialJson('{"api_key":"secret"}\n', "credentials.json"),
    ).toThrow("download blocked");

    expect(BlobMock).toHaveBeenCalledWith(['{"api_key":"secret"}\n'], {
      type: "application/json",
    });
    expect(createObjectURL).toHaveBeenCalledWith(blob);
    expect(anchor.download).toBe("credentials.json");
    expect(anchor.href).toContain("blob:route-credentials");
    expect(document.body.appendChild).toHaveBeenCalledWith(anchor);
    expect(click).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledTimes(1);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:route-credentials");
  });

  it("copies sensitive text exactly once and preserves rejection", async () => {
    const rejection = new Error("clipboard denied");
    const writeText = vi.fn().mockRejectedValue(rejection);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const source = "ccswitch://account?api_key=secret";

    await expect(copySensitiveText(source)).rejects.toBe(rejection);

    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith(source);
    expect(source).toBe("ccswitch://account?api_key=secret");
  });

  it("revokes the object URL even when anchor removal fails", () => {
    const createObjectURL = vi.fn(() => "blob:route-credentials");
    const revokeObjectURL = vi.fn();
    const anchor = document.createElement("a");
    vi.spyOn(anchor, "click").mockImplementation(() => undefined);
    vi.spyOn(anchor, "remove").mockImplementation(() => {
      throw new Error("remove blocked");
    });
    vi.spyOn(document, "createElement").mockReturnValue(anchor);
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });

    expect(() => downloadRouteCredentialJson("[]\n", "credentials.json")).toThrow(
      "remove blocked",
    );
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:route-credentials");
  });
});
