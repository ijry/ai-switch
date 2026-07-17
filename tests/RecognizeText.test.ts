import { beforeEach, describe, expect, it, vi } from "vitest";
import OCRAD from "ocrad.js";
import { recognizeImageText } from "../src/lib/ocr/recognizeText";

vi.mock("ocrad.js", () => ({
  default: vi.fn(() => " OCR text \n"),
}));

describe("recognizeImageText", () => {
  beforeEach(() => {
    vi.mocked(OCRAD).mockClear();
  });

  it("draws image elements to a canvas before OCR", async () => {
    const image = document.createElement("img");
    Object.defineProperty(image, "naturalWidth", { configurable: true, value: 320 });
    Object.defineProperty(image, "naturalHeight", { configurable: true, value: 120 });

    const context = {
      drawImage: vi.fn(),
    };
    const canvas = {
      height: 0,
      width: 0,
      getContext: vi.fn(() => context),
    } as unknown as HTMLCanvasElement;
    const originalCreateElement = document.createElement.bind(document);
    const createElementSpy = vi.spyOn(document, "createElement").mockImplementation((tagName) => {
      if (tagName === "canvas") {
        return canvas;
      }
      return originalCreateElement(tagName);
    });

    try {
      await expect(recognizeImageText(image)).resolves.toBe("OCR text");
    } finally {
      createElementSpy.mockRestore();
    }

    expect(canvas.width).toBe(320);
    expect(canvas.height).toBe(120);
    expect(context.drawImage).toHaveBeenCalledWith(image, 0, 0, 320, 120);
    expect(OCRAD).toHaveBeenCalledWith(canvas);
  });
});
