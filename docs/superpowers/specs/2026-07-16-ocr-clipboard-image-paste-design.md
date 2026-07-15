# OCR Clipboard Image Paste Design

## Scope

Add support for pasting an image from the system clipboard into the existing OCR screen.

The feature is front-end only. It does not add Tauri commands, clipboard plugins, network calls, or backend APIs.

## Behavior

When the OCR screen is focused and the user presses `Ctrl+V`, the screen reads `event.clipboardData.items` from the paste event.

If the clipboard contains an `image/*` item:

- Use the first image item.
- Convert it to a `File` or `Blob`.
- Reuse the existing preview URL state.
- Clear the previous OCR result.
- Show a source label such as `Pasted image`.
- Do not start OCR automatically.

If the clipboard does not contain an image item:

- Keep the current selected image, if any.
- Show an inline error: `Clipboard does not contain an image.`

## UI

Add a short hint near the file picker:

`You can also paste an image with Ctrl+V.`

The existing `Recognize` button remains the only way to start OCR.

## Internationalization

Add English and Simplified Chinese strings for:

- paste hint;
- pasted image source label;
- no clipboard image error.

## Testing

Extend `tests/OcrScreen.test.tsx` to cover:

- pasting an image file displays the pasted image label and preview-ready state;
- pasting non-image clipboard content shows the clipboard error and keeps OCR result empty.

Run `pnpm vitest run tests/OcrScreen.test.tsx` and `pnpm typecheck`.
