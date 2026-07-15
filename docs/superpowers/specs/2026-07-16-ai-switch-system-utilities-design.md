# AI Switch System Utilities Design

## Scope

Add two system utilities to the left navigation under `System`:

- `Crypto Tools`: reversible text transforms for common encodings.
- `OCR`: offline, lightweight English/number image recognition.

The utilities are local-only. They do not call the app backend, external APIs, or network services.

## Navigation

`AppLayout` will add two system nav entries beside `Settings`:

- `Crypto Tools` opens a new `CryptoToolsScreen`.
- `OCR` opens a new `OcrScreen`.

`App.tsx` will register both screens in the existing in-memory screen switcher. This follows the current app pattern and avoids route or backend changes.

## Crypto Tools

The screen provides a source text area, operation selector, output area, and copy controls.

Supported reversible transforms:

- Base64 encode and decode, using UTF-8-safe conversion.
- URL encode and decode.
- Hex encode and decode, using UTF-8-safe conversion.

Invalid input, such as malformed Base64, URL escape sequences, or odd-length/non-hex Hex input, shows an inline error instead of throwing.

## OCR

The OCR screen accepts a local image file through a file input, shows a preview, and runs recognition on demand.

The OCR engine will be a lightweight offline browser-side package intended for English and numbers. It must run from bundled app assets after install and must not fetch language data at recognition time. Chinese OCR is explicitly out of scope for this iteration because local Chinese language models materially increase package size and integration complexity.

The UI will make the limitation visible: `Best for English, numbers, and simple screenshots.`

## Internationalization

All new labels, hints, buttons, and error messages are added to the existing `i18n.tsx` English and Simplified Chinese dictionaries.

## Error Handling

Crypto errors are synchronous validation errors displayed next to the output.

OCR errors cover:

- no file selected;
- unsupported file type;
- recognition failure from the OCR library.

The OCR screen keeps the selected image preview visible when recognition fails so the user can retry.

## Testing

Add front-end tests for:

- crypto transform success and validation errors;
- system navigation rendering for the new entries;
- OCR screen file selection behavior and recognition result with the OCR dependency mocked.

Run `pnpm test:run` and `pnpm typecheck` after implementation.
