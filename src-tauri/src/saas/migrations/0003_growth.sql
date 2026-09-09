ALTER TABLE saas_group_settings ADD COLUMN allow_subscription INTEGER NOT NULL DEFAULT 1 CHECK(allow_subscription IN (0,1));
ALTER TABLE saas_group_settings ADD COLUMN allow_balance INTEGER NOT NULL DEFAULT 1 CHECK(allow_balance IN (0,1));
ALTER TABLE saas_billing_reservations ADD COLUMN funding_type TEXT NOT NULL DEFAULT 'balance' CHECK(funding_type IN ('balance','subscription'));
ALTER TABLE saas_billing_reservations ADD COLUMN subscription_id TEXT;
ALTER TABLE saas_redeem_codes ADD COLUMN subscription_plan_id TEXT;
ALTER TABLE saas_oauth_states ADD COLUMN invite_code_hash TEXT;
ALTER TABLE saas_users ADD COLUMN referral_code TEXT;
ALTER TABLE saas_users ADD COLUMN invited_by TEXT REFERENCES saas_users(id);
ALTER TABLE saas_users ADD COLUMN external_api_key_hash TEXT;
ALTER TABLE saas_users ADD COLUMN external_api_key_prefix TEXT;

CREATE UNIQUE INDEX saas_users_referral_code ON saas_users(referral_code) WHERE referral_code IS NOT NULL;
CREATE UNIQUE INDEX saas_users_external_api_key ON saas_users(external_api_key_hash) WHERE external_api_key_hash IS NOT NULL;

CREATE TABLE saas_subscription_plans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('trial','day','week','month','quarter','year')),
    duration_days INTEGER NOT NULL CHECK(duration_days>0),
    quota_micros INTEGER NOT NULL CHECK(quota_micros>0),
    price_micros INTEGER NOT NULL DEFAULT 0 CHECK(price_micros>=0),
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','disabled')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE saas_subscriptions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    plan_id TEXT NOT NULL REFERENCES saas_subscription_plans(id),
    plan_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    quota_micros INTEGER NOT NULL CHECK(quota_micros>0),
    used_micros INTEGER NOT NULL DEFAULT 0 CHECK(used_micros>=0),
    frozen_micros INTEGER NOT NULL DEFAULT 0 CHECK(frozen_micros>=0),
    starts_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    source TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','cancelled')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX saas_subscriptions_user ON saas_subscriptions(user_id,status,expires_at);

CREATE TABLE saas_subscription_ledger (
    id TEXT PRIMARY KEY,
    subscription_id TEXT NOT NULL REFERENCES saas_subscriptions(id),
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    kind TEXT NOT NULL,
    amount_micros INTEGER NOT NULL,
    used_after_micros INTEGER NOT NULL,
    request_id TEXT UNIQUE,
    reason TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE saas_checkins (
    user_id TEXT NOT NULL REFERENCES saas_users(id),
    day TEXT NOT NULL,
    amount_micros INTEGER NOT NULL CHECK(amount_micros>0),
    created_at INTEGER NOT NULL,
    PRIMARY KEY(user_id,day)
);

CREATE TABLE saas_invite_codes (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    suffix TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','disabled')),
    max_uses INTEGER,
    used_count INTEGER NOT NULL DEFAULT 0 CHECK(used_count>=0),
    expires_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE TABLE saas_invite_rewards (
    id TEXT PRIMARY KEY,
    inviter_id TEXT NOT NULL REFERENCES saas_users(id),
    invitee_id TEXT NOT NULL REFERENCES saas_users(id),
    kind TEXT NOT NULL CHECK(kind IN ('signup','recharge')),
    source_id TEXT NOT NULL,
    base_micros INTEGER NOT NULL CHECK(base_micros>=0),
    amount_micros INTEGER NOT NULL CHECK(amount_micros>0),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected')),
    reason TEXT,
    reviewed_at INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE(kind,source_id)
);
CREATE INDEX saas_invite_rewards_inviter ON saas_invite_rewards(inviter_id,status,created_at);
