---
title: Local Utilities
description: Two fully local tools bundled with AI Switch — text encoding and decoding (Base64 / URL / Hex, both directions) and offline OCR that never uploads your images — plus how OCR is reused when entering API keys.
---

# Local Utilities

Beyond accounts, routing, MCP, and Skills, AI Switch bundles two small everyday tools, both under the "System" group in the sidebar:

- **Crypto Tools** — Base64 / URL / Hex conversion in both directions.
- **OCR Recognition** — pull English letters and digits out of an image, entirely offline.

What they have in common is that **neither touches the network**. They call no remote service and consume none of your pool quota.

## Crypto Tools

You get handed a Base64-encoded token, a URL-encoded callback address, or a hex byte string, and you want to see what is inside. That is what this tool is for.

The layout is minimal: input on the left, conversion type on the right, output and a copy button below. **Conversion happens as you type** — there is no "run" button. The default selection is **Base64 decode**, since that is the most common need.

### Six conversions

| Conversion | Behaviour |
| --- | --- |
| Base64 encode | Text is UTF-8 encoded to bytes, then Base64 encoded |
| Base64 decode | Base64 to bytes, then UTF-8 decoded to text |
| URL encode | Equivalent to `encodeURIComponent` |
| URL decode | Equivalent to `decodeURIComponent` |
| Hex encode | Text is UTF-8 encoded to bytes, output as lowercase hex, two digits per byte |
| Hex decode | Hex to bytes, then UTF-8 decoded to text |

Three behavioural details:

- **Decoding strips all whitespace first.** Base64 and hex copied out of a terminal or log file, wrapped across lines, can be pasted as-is — no need to rejoin them by hand.
- **UTF-8 decoding is strict.** A byte sequence that is not valid UTF-8 produces an error rather than a string of replacement characters.
- **Hex output is lowercase with no separators** (`68656c6c6f`, not `68 65 6C 6C 6F`).

### Four error messages

| Message | Trigger |
| --- | --- |
| Base64 input is invalid. | Characters outside the Base64 alphabet, or a whitespace-stripped length that is 1 mod 4 |
| URL escapes are invalid. | A malformed percent escape, such as a trailing `%A` |
| Hex content must have an even length. | Odd length after stripping whitespace |
| Hex content can only contain 0-9 and A-F. | Characters outside the hex alphabet |

On error the output box is cleared and only the message shows — you never get a partial result that looks like a successful conversion.

::: tip This is encoding, not encryption
Base64, URL encoding, and hex are all **reversible encodings**, not encryption: anyone holding the encoded string can recover the original. Use this tool to inspect and convert formats, not to protect anything.
:::

## OCR Recognition

Lifting text out of a screenshot. The classic cases: a provider console that shows an API key you cannot select, a config string inside a documentation screenshot, an id in an error dialog.

### Three steps

1. **Provide an image.** Two ways:
   - Click "Choose image" and pick a file (PNG, JPEG, GIF, BMP, or WebP).
   - Press **Ctrl+V** on this screen to paste an image from the clipboard — no need to save a screenshot to disk first.
2. A preview appears on the right; confirm it is the image you meant.
3. Click "Recognize". The result lands in the text box below, ready to select and copy.

Choosing a non-image file reports "Please choose an image file."; a clipboard with no image reports "Clipboard does not contain an image."; recognising with no image reports "Choose an image before recognition."; a failure during recognition reports "OCR recognition failed.".

### Genuinely offline

This is the part worth emphasising. The engine is `ocrad.js` — an OCR library compiled to JavaScript that runs inside the browser or WebView. The pipeline is:

1. The image is read into a local blob URL and rendered into an `<img>` for the preview.
2. On "Recognize", the image is drawn onto an in-memory `<canvas>`.
3. The OCR engine reads that canvas's pixel data and computes the text in the same process.

Which means:

- **The image is never uploaded anywhere.** There is no remote OCR API and no cloud recognition service.
- **It consumes no pool quota.** It is entirely unrelated to the AI accounts you configured.
- **It works offline.**
- **The image never hits disk.** The preview's blob URL is revoked when the screen unmounts, and AI Switch keeps no copy.

::: warning In Web Service mode the computation happens in your browser
OCR is pure frontend work. On the desktop it runs in the app process; in [Web Service mode](/en/deploy/web-service) it runs in **your browser** — the image never even reaches the server. That is better for privacy, but it also means large images will be slow on a weak device.
:::

### Where recognition stops working

The UI carries an amber note: **"Best for English, numbers, and simple screenshots."** That is not modesty, it is the engine's real boundary:

- **It does not recognise Chinese.** The character set targets Latin letters and digits.
- High-contrast screenshots — dark text on a light background, reasonably large type — work best.
- Busy backgrounds, gradients, translucent overlays, and very small type degrade accuracy sharply.
- Results are **not guaranteed accurate**. For content where a single wrong character matters, such as an API key, verify it character by character.

If the result is poor, try zooming in before taking the screenshot, cropping down to just the text, or switching to dark-on-light.

## OCR when entering API keys

The same engine is reused once more, on the accounts screen. When creating or editing an API-type credential there is an **OCR** button next to the API key field:

1. Clicking it first tries to read an image from the **system clipboard**.
2. If there is no image there (or the browser does not support clipboard reads), it reports "剪切板中没有图片，请选择图片文件。" and opens a file picker automatically.
3. What comes back is **not** the raw OCR text but the API key candidates extracted from it.

Extraction uses regexes for a few common key shapes:

| Shape | Note |
| --- | --- |
| A long string starting with `sk-` | Used by a great many OpenAI-compatible providers |
| A long string starting with `AIza` | The common prefix for Google API keys |
| Three dot-separated Base64URL segments | JWT-shaped tokens |

When several candidates match, all of them are listed one per line so you can pick. When none match, it **falls back to the raw recognised text** (blank lines removed), so at least you do not have to re-capture the screenshot.

Because OCR line breaks can split a long key in two, extraction runs twice — once over the text as-is, and once over a version with all whitespace removed per line — and the two result sets are merged and deduplicated.

Recognising nothing reports "未识别到 API Key。"; a failure reports "OCR 识别失败，请换一张更清晰的图片。".

::: danger Always verify what you paste in
OCR misreads characters — `0` versus `O`, `1` versus `l`, `5` versus `S` are the usual suspects. One wrong character makes an API key completely unusable, and the resulting error is typically a vague authentication failure that tells you nothing about which digit is wrong. Check the result against the original image, or just run a [model connectivity test](/en/guide/model-test) to confirm the credential actually works.
:::

## Next steps

- [Accounts and the Pool](/en/guide/accounts) — the full field set for API credentials
- [Model Connectivity Tests](/en/guide/model-test) — verify the key you just entered
- [Web Service Mode](/en/deploy/web-service) — both tools work the same in a browser
- [FAQ](/en/faq) — other common questions
