# AI Switch IDE Account Importers E4 Design

## Context

C4 introduced metadata-only official account JSON import for Codex, Claude, and
Gemini. Phase E calls for more official IDE account importers. E4 expands the
same safe pasted JSON path to additional IDE account platforms.

E4 does not read real IDE credential stores, perform OAuth, refresh tokens, call
remote APIs, or store raw credentials.

## Goals

- Accept metadata-only account bundles for `cursor`, `windsurf`, `zed`, and
  `vscode`.
- Keep existing `codex`, `claude`, and `gemini` import behavior.
- Expose new IDE platforms in the `Imports` screen account platform selector.
- Preserve secret safety validation for imported metadata.

## Non-Goals

- No filesystem scanning for IDE account stores.
- No token extraction.
- No OAuth or browser login flow.
- No quota lookup.
- No network calls.

## Import Shape

The existing official account JSON shape is reused:

```json
{
  "accounts": [
    {
      "display_name": "Team Cursor",
      "email": "team@example.com",
      "plan": "team",
      "metadata": { "workspace": "engineering" },
      "secret_ref": "secret://account/team-cursor"
    }
  ]
}
```

The selected platform is sent separately by the UI or deep link.

## Acceptance Criteria

- Backend accepts `cursor`, `windsurf`, `zed`, and `vscode` platform values.
- Unsupported platforms are still rejected.
- Frontend platform selector exposes the new IDE platforms.
- Tests cover at least one new IDE platform import path.
