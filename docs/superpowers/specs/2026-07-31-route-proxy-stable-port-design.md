# Route Proxy Stable Port Design

## Goal

The local capacity-pool route proxy must prefer a stable port so external client configs do not churn on every start. The default bind target is `127.0.0.1:19527`.

## Behavior

- On start, bind `127.0.0.1:19527` first.
- If that port cannot be bound, try the next port in sequence: `19528`, `19529`, and so on.
- Once a port binds successfully, expose that actual port in `RouteProxyStatus.port` and `RouteProxyStatus.base_url`.
- If every candidate through `65535` fails, return the existing route-proxy bind error shape.
- Stopping the proxy clears runtime state as before. A later start begins from `19527` again.

## Scope

Only the route proxy listener selection changes. HTTPS transport, config rewriting, pool credential selection, and frontend behavior continue to consume the reported `base_url` without separate changes.

## Testing

Add Rust coverage for the fallback path by occupying the default port before starting the proxy, then asserting that the proxy starts on a higher port with a matching base URL.
