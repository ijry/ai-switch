# Codex Model Test Endpoint Selection

## Goal

Allow Codex real-generation tests to choose between the Responses and Chat Completions endpoints. Remember the last selection globally across pool tests, single-account tests, screen remounts, and application restarts without changing the credential's saved routing configuration.

## User Interface

The existing real-generation test dialog shows an endpoint segmented control only when the active platform is Codex:

- `/responses`
- `/chat/completions`

The initial default is `/responses`. Selecting either option updates the dialog immediately and persists the choice. The model input and submit behavior remain unchanged. Other platform dialogs do not show this control.

## Persistence

Store one global Codex model-test endpoint preference in `localStorage`. Read and validate the value when the Accounts screen initializes. Unknown, malformed, or unavailable values fall back to `/responses`.

The preference is shared by pool tests and single-account tests. It is not stored per credential and does not modify `config_json`, `interface_format`, or application settings.

## Request Contract

Extend `RoutePoolModelTestRequest` with an optional `interface_format` field.

- `/responses` sends `interface_format: "openai-responses"`.
- `/chat/completions` sends `interface_format: "openai"`.

The backend accepts this override only for Codex and only for the two supported values. When the field is absent, existing interface derivation remains unchanged for backward compatibility. Unsupported values return a recoverable validation error.

## Backend Behavior

Pass the validated request override into model-test request construction. When present, it determines the request shape and path for both direct account tests and tests routed through the local proxy:

- `openai-responses`: `POST /responses` with the Responses request body.
- `openai`: `POST /chat/completions` with the Chat Completions request body.

Credential authentication, base URL joining, model mapping, proxy account selection, usage recording, and response parsing continue through the existing paths.

## Error Handling

- Invalid persisted frontend values silently fall back to `/responses`.
- Invalid or non-Codex backend overrides return a recoverable validation error before sending a request.
- Existing requests without `interface_format` continue to work unchanged.

## Testing

Frontend tests cover:

- Codex shows both endpoint options and defaults to `/responses` without a stored value.
- Selecting Chat Completions changes the submitted request and persists the preference.
- Reopening or remounting restores the stored selection for pool and account tests.
- Non-Codex dialogs do not show the endpoint selector.

Backend tests cover:

- Both override values produce the expected interface format, path, and request body.
- Direct and proxy-routed tests honor the override.
- Missing overrides preserve existing derivation.
- Unsupported and non-Codex overrides are rejected.
