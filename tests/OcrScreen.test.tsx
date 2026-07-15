import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../src/lib/i18n";
import { recognizeImageText } from "../src/lib/ocr/recognizeText";
import { OcrScreen } from "../src/screens/OcrScreen";

vi.mock("../src/lib/ocr/recognizeText", () => ({
  recognizeImageText: vi.fn(),
}));

beforeEach(() => {
  vi.mocked(recognizeImageText).mockReset();
  Object.defineProperty(URL, "createObjectURL", {
    configurable: true,
    value: vi.fn(() => "blob:sample"),
  });
  Object.defineProperty(URL, "revokeObjectURL", {
    configurable: true,
    value: vi.fn(),
  });
});

function renderScreen() {
  return render(
    <I18nProvider initialLanguage="zh-CN">
      <OcrScreen />
    </I18nProvider>,
  );
}

function getOcrRoot() {
  const root = screen.getByText("OCR识别").closest("section");
  if (!root) {
    throw new Error("OCR root section not found");
  }
  return root;
}

describe("OcrScreen", () => {
  it("loads an image file and shows mocked recognition text", async () => {
    vi.mocked(recognizeImageText).mockResolvedValue("ABCD 1234");
    renderScreen();

    const file = new File(["fake"], "sample.png", { type: "image/png" });
    await userEvent.upload(screen.getByLabelText("选择图片"), file);

    expect(screen.getByText("sample.png")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "开始识别" }));

    await waitFor(() => expect(recognizeImageText).toHaveBeenCalled());
    expect(await screen.findByLabelText("识别结果")).toHaveValue("ABCD 1234");
  });

  it("rejects non-image files", async () => {
    renderScreen();

    const file = new File(["hello"], "notes.txt", { type: "text/plain" });
    fireEvent.change(screen.getByLabelText("选择图片"), {
      target: { files: [file] },
    });

    expect(screen.getByText("请选择图片文件。")).toBeInTheDocument();
  });

  it("loads a pasted image from the clipboard and keeps recognition manual", async () => {
    vi.mocked(recognizeImageText).mockResolvedValue("ABCD 1234");
    renderScreen();

    expect(screen.getByText("也可以按 Ctrl+V 粘贴图片。")).toBeInTheDocument();

    const file = new File(["fake"], "clipboard.png", { type: "image/png" });
    fireEvent.paste(getOcrRoot(), {
      clipboardData: {
        items: [
          {
            kind: "file",
            type: "image/png",
            getAsFile: () => file,
          },
        ],
        files: [file],
        types: ["Files"],
        getData: () => "",
      },
    });

    expect(screen.getByText("粘贴的图片")).toBeInTheDocument();
    expect(recognizeImageText).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "开始识别" }));
    await waitFor(() => expect(recognizeImageText).toHaveBeenCalled());
    expect(await screen.findByLabelText("识别结果")).toHaveValue("ABCD 1234");
  });

  it("shows a clipboard error and keeps the current image when pasted content is not an image", async () => {
    renderScreen();

    const file = new File(["fake"], "sample.png", { type: "image/png" });
    await userEvent.upload(screen.getByLabelText("选择图片"), file);

    fireEvent.paste(getOcrRoot(), {
      clipboardData: {
        items: [
          {
            kind: "string",
            type: "text/plain",
            getAsFile: () => null,
          },
        ],
        files: [],
        types: ["text/plain"],
        getData: () => "hello",
      },
    });

    expect(screen.getByText("sample.png")).toBeInTheDocument();
    expect(screen.getByText("剪切板中没有图片。")).toBeInTheDocument();
    expect(screen.getByLabelText("识别结果")).toHaveValue("");
  });
});
