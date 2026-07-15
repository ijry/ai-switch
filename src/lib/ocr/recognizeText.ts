import OCRAD from "ocrad.js";

export async function recognizeImageText(source: HTMLImageElement | HTMLCanvasElement): Promise<string> {
  return OCRAD(source).trim();
}
