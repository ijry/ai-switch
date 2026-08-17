# H5 Cross-Origin Web API Support Design

## Goal

Allow the separate `ai-switch-app` H5 client to call an AI Switch Web Service from a different browser origin. The service must answer browser preflight requests and return the required CORS headers without weakening its existing Bearer-token authorization.

## Confirmed Policy

The user selected an any-origin policy for this service:

- Return `Access-Control-Allow-Origin: *`.
- Allow only `GET`, `POST`, and `OPTIONS` methods.
- Allow only the request headers required by the mobile client: `Authorization` and `Content-Type`.
- Do not enable `Access-Control-Allow-Credentials`; the client authenticates with an explicit Bearer token rather than browser cookies.

## Architecture

Add `tower-http`'s standard `CorsLayer` at the outermost level of `web::router::build_router`.

This placement makes the layer apply consistently to `/health`, `/api/*`, static-asset responses, and router-generated errors. It also lets the middleware answer an `OPTIONS` preflight before the nested `/api` authorization and sensitive-command gates run, so preflights do not need a Bearer token.

The existing API behavior remains unchanged for actual requests:

- `POST /api/:command` still traverses `authorize_api_request` and requires the configured Bearer token.
- Sensitive-command availability checks continue to run after authorization.
- `GET /health` remains the current unauthenticated health endpoint.
- No endpoint gains a new write capability.

## Error Handling and Compatibility

The standard middleware should attach the same CORS policy to successful API responses and error responses. Existing non-browser clients continue to send the same requests; additional response headers are backwards compatible. Requests from arbitrary web origins can only perform an API action when they also know the configured Bearer token.

WebSocket origin policy and a configurable origin allowlist are intentionally outside this change. They can be added later if the service moves from the confirmed compatibility-focused policy to a stricter deployment model.

## Tests

Add router-level coverage that verifies:

1. An `OPTIONS /api/list_platform_capabilities` request with an arbitrary `Origin`, requested `POST` method, and `authorization, content-type` headers succeeds without an Authorization token and returns the expected allow-origin, allow-methods, and allow-headers values.
2. A normal API request with an `Origin` still receives CORS headers but remains rejected with `401 Unauthorized` when its Bearer token is absent or invalid.
3. A normal authenticated API request with an `Origin` succeeds and returns the same CORS policy.

## Scope

Only the Rust AI Switch Web Service router and its dependency/test coverage change. The mobile H5 application requires no protocol changes because it already sends `Authorization` and `Content-Type` exactly as covered by this policy.
