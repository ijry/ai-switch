# AI Switch Official Account Import C4 Design

## Context

C1 added metadata-only official account management. C2 added quota snapshot cache records. C3 connected account quota state to batch health. The Phase C roadmap calls for official account import for Codex, Claude, Gemini, and other priority platforms.

C4 adds a conservative metadata-only official account import format for Codex, Claude, and Gemini. It does not parse real browser/session/token files and does not store raw credentials.

## Goals

- Add an `official_account_json` import command.
- Support platform-scoped account imports for `codex`, `claude`, and `gemini`.
- Create official account records and attach them to a named batch.
- Record import jobs.
- Reject raw credential-looking metadata keys.
- Add an Imports screen entry point for account bundle paste imports.

## Non-Goals

- No OAuth.
- No token refresh.
- No parsing real app session stores.
- No raw token, password, API key, or secret storage.
- No quota network calls.

## Import Shape

The UI sends the platform separately. The pasted JSON shape is:

```json
{
  "accounts": [
    {
      "display_name": "Team Codex",
      "email": "team@example.com",
      "plan": "team",
      "metadata": {
        "workspace": "engineering"
      },
      "secret_ref": "secret://account/team"
    }
  ]
}
```

`metadata` is serialized into `account_metadata_json`. Empty metadata defaults to `{}`.

Sensitive metadata keys are rejected recursively when they contain:

- `token`
- `api_key`
- `apikey`
- `password`
- `secret`

`secret_ref` is allowed as a top-level field because it is a reference, not a raw credential.

## Completion Criteria

- Backend imports account bundle JSON into a batch and import job.
- Unsupported platforms are rejected.
- Credential-looking metadata is rejected.
- Imports UI can paste/import account bundles.
- Rust and frontend tests cover the flow.
