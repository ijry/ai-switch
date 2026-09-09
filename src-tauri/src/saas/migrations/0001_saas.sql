CREATE TABLE saas_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE saas_users (
    id TEXT PRIMARY KEY,
    github_id TEXT NOT NULL UNIQUE,
    login TEXT NOT NULL,
    avatar_url TEXT,
    github_created_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','banned')),
    balance_micros INTEGER NOT NULL DEFAULT 0 CHECK(typeof(balance_micros)='integer'),
    frozen_micros INTEGER NOT NULL DEFAULT 0 CHECK(typeof(frozen_micros)='integer' AND frozen_micros>=0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE saas_oauth_states (
    state_hash TEXT PRIMARY KEY,
    binding_hash TEXT NOT NULL,
    verifier TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    client_id TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX saas_oauth_states_expiry ON saas_oauth_states(expires_at);

CREATE TABLE saas_sessions (
    token_hash TEXT PRIMARY KEY,
    csrf_hash TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    origin TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX saas_sessions_user ON saas_sessions(user_id,expires_at);

CREATE TABLE saas_group_settings (
    group_id TEXT PRIMARY KEY REFERENCES route_pool_groups(id),
    multiplier_micros INTEGER NOT NULL CHECK(multiplier_micros>0),
    max_output_tokens INTEGER NOT NULL CHECK(max_output_tokens>0),
    timeout_seconds INTEGER NOT NULL CHECK(timeout_seconds>0),
    max_concurrency INTEGER NOT NULL CHECK(max_concurrency>0),
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE saas_group_models (
    group_id TEXT NOT NULL REFERENCES route_pool_groups(id),
    model TEXT NOT NULL,
    upstream_model TEXT NOT NULL,
    input_price_micros INTEGER NOT NULL CHECK(input_price_micros>=0),
    cache_price_micros INTEGER NOT NULL CHECK(cache_price_micros>=0),
    output_price_micros INTEGER NOT NULL CHECK(output_price_micros>=0),
    version INTEGER NOT NULL,
    PRIMARY KEY(group_id,model)
);

CREATE TABLE saas_api_keys (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    group_id TEXT NOT NULL REFERENCES route_pool_groups(id),
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    suffix TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','disabled','revoked')),
    expires_at INTEGER,
    limit_micros INTEGER CHECK(limit_micros>=0),
    spent_micros INTEGER NOT NULL DEFAULT 0 CHECK(typeof(spent_micros)='integer' AND spent_micros>=0),
    frozen_micros INTEGER NOT NULL DEFAULT 0 CHECK(typeof(frozen_micros)='integer' AND frozen_micros>=0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX saas_api_keys_user ON saas_api_keys(user_id,created_at);

CREATE TABLE saas_billing_reservations (
    request_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    key_id TEXT NOT NULL REFERENCES saas_api_keys(id),
    group_id TEXT NOT NULL REFERENCES route_pool_groups(id),
    platform TEXT NOT NULL,
    model TEXT NOT NULL,
    price_json TEXT NOT NULL,
    reserved_micros INTEGER NOT NULL CHECK(reserved_micros>=0),
    status TEXT NOT NULL CHECK(status IN ('reserved','pending_review','settled','refunded')),
    price_usd_micros INTEGER CHECK(price_usd_micros>=0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX saas_reservations_pending ON saas_billing_reservations(status,user_id,key_id);

CREATE TABLE saas_wallet_ledger (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    kind TEXT NOT NULL,
    amount_micros INTEGER NOT NULL CHECK(typeof(amount_micros)='integer'),
    balance_after_micros INTEGER NOT NULL CHECK(typeof(balance_after_micros)='integer'),
    source_id TEXT NOT NULL,
    request_id TEXT UNIQUE REFERENCES saas_billing_reservations(request_id),
    idempotency_key TEXT NOT NULL UNIQUE,
    reason TEXT,
    actor TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX saas_ledger_user ON saas_wallet_ledger(user_id,created_at);

CREATE TABLE saas_recharge_orders (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    request_id TEXT NOT NULL,
    amount_cny_fen INTEGER NOT NULL CHECK(amount_cny_fen>0),
    exchange_rate_micros INTEGER NOT NULL CHECK(exchange_rate_micros>0),
    credit_micros INTEGER NOT NULL CHECK(credit_micros>0),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','cancelled')),
    note TEXT,
    reason TEXT,
    reviewed_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(user_id,request_id)
);
CREATE INDEX saas_recharges_user ON saas_recharge_orders(user_id,created_at);

CREATE TABLE saas_redeem_codes (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    suffix TEXT NOT NULL,
    amount_micros INTEGER NOT NULL CHECK(amount_micros>0),
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','disabled','redeemed')),
    expires_at INTEGER,
    used_by TEXT REFERENCES saas_users(id),
    used_at INTEGER,
    created_at INTEGER NOT NULL
);
CREATE INDEX saas_codes_batch ON saas_redeem_codes(batch_id,created_at);

CREATE TABLE saas_usage_hourly (
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    key_id TEXT NOT NULL REFERENCES saas_api_keys(id),
    group_id TEXT NOT NULL REFERENCES route_pool_groups(id),
    model TEXT NOT NULL,
    hour INTEGER NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0 CHECK(typeof(request_count)='integer'),
    input_tokens INTEGER NOT NULL DEFAULT 0 CHECK(typeof(input_tokens)='integer'),
    cache_read_tokens INTEGER NOT NULL DEFAULT 0 CHECK(typeof(cache_read_tokens)='integer'),
    cache_write_tokens INTEGER NOT NULL DEFAULT 0 CHECK(typeof(cache_write_tokens)='integer'),
    output_tokens INTEGER NOT NULL DEFAULT 0 CHECK(typeof(output_tokens)='integer'),
    cost_micros INTEGER NOT NULL DEFAULT 0 CHECK(typeof(cost_micros)='integer'),
    unconfirmed_usage_count INTEGER NOT NULL DEFAULT 0 CHECK(typeof(unconfirmed_usage_count)='integer'),
    PRIMARY KEY(user_id,key_id,group_id,model,hour)
);

CREATE TABLE saas_admin_audit (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    operation TEXT NOT NULL,
    target_id TEXT,
    details_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
