# Crypto Default Base64 Decode Design

## Scope

Change the Crypto Tools screen default operation from Base64 encode to Base64 decode.

## Behavior

When the user opens the Crypto Tools screen:

- The operation selector defaults to `Base64 decode`.
- Pasting valid Base64 text immediately shows decoded UTF-8 output.
- Other operations remain available and unchanged.

## Testing

Update the Crypto Tools screen test so the default behavior verifies Base64 decoding. Keep validation coverage for invalid Hex input.
