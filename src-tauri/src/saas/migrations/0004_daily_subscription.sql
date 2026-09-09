CREATE TABLE IF NOT EXISTS saas_subscription_daily_usage (
    subscription_id TEXT NOT NULL REFERENCES saas_subscriptions(id),
    day TEXT NOT NULL,
    used_micros INTEGER NOT NULL DEFAULT 0 CHECK(used_micros>=0),
    frozen_micros INTEGER NOT NULL DEFAULT 0 CHECK(frozen_micros>=0),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(subscription_id,day)
);

DROP INDEX saas_codes_batch;
ALTER TABLE saas_redeem_codes RENAME TO saas_redeem_codes_legacy;
CREATE TABLE saas_redeem_codes (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    suffix TEXT NOT NULL,
    amount_micros INTEGER NOT NULL DEFAULT 0 CHECK(amount_micros>=0),
    subscription_plan_id TEXT REFERENCES saas_subscription_plans(id),
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','disabled','redeemed')),
    expires_at INTEGER,
    used_by TEXT REFERENCES saas_users(id),
    used_at INTEGER,
    created_at INTEGER NOT NULL,
    CHECK(amount_micros>0 OR subscription_plan_id IS NOT NULL)
);
INSERT INTO saas_redeem_codes(id,batch_id,token_hash,prefix,suffix,amount_micros,subscription_plan_id,status,expires_at,used_by,used_at,created_at)
SELECT id,batch_id,token_hash,prefix,suffix,amount_micros,subscription_plan_id,status,expires_at,used_by,used_at,created_at FROM saas_redeem_codes_legacy;
DROP TABLE saas_redeem_codes_legacy;
CREATE INDEX saas_codes_batch ON saas_redeem_codes(batch_id,created_at);
