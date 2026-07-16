# API Key OCR Button Design

## Goal

Add an OCR button next to the API Key textarea in the API account creation form so users can recognize an API key from an image without leaving the form.

## Decisions

- Image source: clicking the button first tries to read an image from the system clipboard.
- Fallback: if the clipboard has no readable image, show a clear message and open the existing browser file picker for image selection.
- Fill behavior: successful OCR replaces the current API Key textarea content.
- Cleanup behavior: prefer extracting likely API keys such as `sk-...`, `sk-ant-...`, `AIza...`, and JWT-like tokens. If no known key shape is found, fall back to trimmed OCR text.
- OCR engine: reuse the existing offline `ocrad.js` wrapper. No network calls or hosted OCR services.

## UX

The API Key control keeps the existing Base64 decode action and adds a second action labeled `OCR识别`. While OCR is running, the button is disabled and displays `识别中...`. Errors are shown under the API Key field.

## Error Handling

- Clipboard unsupported or blocked: tell the user the app cannot read a clipboard image and prompt file selection.
- Clipboard has no image: tell the user no image was found and prompt file selection.
- Selected file is not an image: show an invalid image error.
- OCR returns empty text or fails: show a recognition failure error.

## Testing

Add focused coverage for:

- OCR button reads an image from clipboard, recognizes a key, and replaces the API Key value.
- Clipboard miss falls back to file input and recognizes the selected image.
- API key extraction prefers likely key tokens and falls back to basic cleaned text.

