import OCRAD from "ocrad.js";

export async function recognizeImageText(source: HTMLImageElement | HTMLCanvasElement): Promise<string> {
  const ocrSource = source instanceof HTMLImageElement ? imageToCanvas(source) : source;
  return OCRAD(ocrSource).trim();
}

function imageToCanvas(image: HTMLImageElement): HTMLCanvasElement {
  const width = image.naturalWidth || image.width;
  const height = image.naturalHeight || image.height;
  if (width <= 0 || height <= 0) {
    throw new Error("Image has no drawable dimensions");
  }

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("Could not create OCR canvas context");
  }

  context.drawImage(image, 0, 0, width, height);
  return canvas;
}
