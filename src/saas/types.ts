export type SaasLocale = "zh" | "en";
export type SaasEndpoint = "codex" | "claude";
export interface SaasHostProps { onConfigChanged?: () => void }
export interface PageResult<Item> { items: Item[]; total: number; page?: number; pageSize?: number }
export interface SaasErrorEnvelope { code: string; message: string; details?: unknown }
export type AdminOperation = "activation.status" | "activation.unlock" | "config.get" | "config.save" | "overview" | "catalog" | "users.list" | "users.create" | "users.credit" | "users.status" | "groups.list" | "groups.available" | "groups.save" | "subscriptions.plans.list" | "subscriptions.plans.save" | "subscriptions.grant" | "invites.codes.list" | "invites.codes.create" | "invites.codes.disable" | "invites.rewards.list" | "invites.rewards.review" | "recharges.list" | "recharges.review" | "codes.list" | "codes.create" | "codes.disable" | "ledger.list" | "ledger.reconcile" | "logs.query";
export type UserOperation = "overview" | "usage" | "groups" | "subscriptions.list" | "subscriptions.plans" | "subscriptions.purchase" | "checkin" | "invites.overview" | "invites.rewards" | "external-key.status" | "external-key.rotate" | "external-key.revoke" | "keys.list" | "keys.create" | "keys.update" | "keys.rotate" | "recharges.list" | "recharges.create" | "recharges.cancel" | "redeem" | "logs.query";

export interface SaasPublicConfig {
  enabled: boolean;
  registrationEnabled: boolean;
  siteName: string;
  publicBaseUrl: string;
  githubLoginAvailable?: boolean;
  exchangeRateMicros?: number | null;
  announcement?: string;
  rechargeInstructions?: string;
  checkinEnabled?: boolean;
  checkinRewardMicros?: number;
  inviteEnabled?: boolean;
  inviteRegistrationRequired?: boolean;
}

export interface SaasConfig extends SaasPublicConfig {
  githubClientId: string;
  githubClientSecretConfigured: boolean;
  logs: SaasLogConfig;
  checkinEnabled?: boolean;
  checkinRewardMicros?: number;
  inviteEnabled?: boolean;
  inviteRegistrationRequired?: boolean;
  inviteSignupRewardMicros?: number;
  inviteRechargeRateMicros?: number;
}

export interface SaasLogConfig {
  queue?: "memory" | "redis";
  store?: "file" | "postgres";
  directory?: string | null;
  maxRecords?: number;
  maxBytes?: number;
  enqueueTimeoutMs?: number;
  shutdownTimeoutMs?: number;
  batchSize?: number;
  retentionDays?: number | null;
  postgresMaxConnections?: number;
  redisUrlEnv?: string;
  postgresUrlEnv?: string;
  redisUrlConfigured?: boolean;
  postgresUrlConfigured?: boolean;
}

export interface SaasConfigUpdate extends SaasConfig {
  githubClientSecret?: string;
}

export interface SaasActivationStatus { unlocked: boolean }

export interface SaasUser {
  id: string;
  login: string;
  email?: string | null;
  githubId?: string;
  displayName?: string;
  avatarUrl?: string;
  status: string;
  balanceMicros: number;
  frozenMicros: number;
  availableMicros?: number;
  debtMicros?: number;
  createdAt: string;
  githubCreatedAt?: string;
}

export interface SaasSession { user: SaasUser | null; csrfToken: string | null }
export interface UsageTotals { requestCount: number; inputTokens: number; cacheReadTokens: number; cacheWriteTokens?: number; outputTokens: number; costMicros: number }
export interface UsageBucket extends UsageTotals { date: string }
export interface UsageRow extends UsageTotals { hour: string; keyId: string; groupId: string; model: string }
export interface UsageResult extends PageResult<UsageRow> { from: string; to: string; totals?: UsageTotals; buckets?: UsageBucket[] }
export interface UserOverview {
  balanceMicros: number;
  frozenMicros: number;
  today: UsageTotals;
  month: UsageTotals;
  availableMicros?: number;
  debtMicros?: number;
  exchangeRateMicros?: number | null;
  redemptionHistory?: LedgerEntry[];
}
export interface AdminOverview {
  userCount: number;
  groupCount: number;
  pendingRechargeCount: number;
  pendingReviewCount: number;
  pendingReviewMicros: number;
  balanceMicros: number;
  frozenMicros: number;
  debtMicros: number;
  today: UsageTotals;
  month: UsageTotals;
}

export interface ModelPrice { model: string; upstreamModel: string; inputPriceMicros: number; cachePriceMicros: number; outputPriceMicros: number }
export interface SaasGroup {
  id: string;
  name: string;
  platform: SaasEndpoint;
  isInternal: boolean;
  isActive: boolean;
  configured: boolean;
  accountCount: number;
  multiplierMicros: number;
  models: ModelPrice[];
  availableAccountCount?: number;
  maxConcurrency: number;
  timeoutSeconds: number;
  maxOutputTokens: number;
  allowSubscription?: boolean;
  allowBalance?: boolean;
}
export interface SaasCatalog {
  platforms: SaasEndpoint[];
  platform: SaasEndpoint;
  groupId: string;
  name: string;
  models: string[];
  availableAccountCount: number;
  accounts: { id: string; platform: SaasEndpoint }[];
}
export interface SaasKey {
  id: string;
  name: string;
  prefix: string;
  groupId: string;
  groupName?: string;
  status: "active" | "disabled" | "revoked";
  limitMicros: number | null;
  spentMicros: number;
  frozenMicros?: number;
  suffix?: string;
  createdAt: string;
  expiresAt: string | null;
  lastUsedAt?: string | null;
}
export interface KeySecret extends SaasKey { plaintextKey: string }
export interface RechargeOrder {
  id: string;
  userId: string;
  githubLogin?: string;
  amountCnyFen: number;
  exchangeRateMicros: number;
  creditMicros: number;
  status: string;
  note: string | null;
  createdAt: string;
  reviewedAt?: string | null;
  reviewNote?: string | null;
  reason?: string | null;
  updatedAt?: string;
}
export interface RedemptionResult { amountMicros: number; balanceMicros: number; id: string; status: string; subscription?: UserSubscription | null }
export interface RedemptionCode {
  id: string;
  prefix: string;
  amountMicros: number;
  subscriptionPlanId?: string | null;
  status: string;
  createdAt: string;
  expiresAt?: string | null;
  redeemedAt?: string | null;
  redeemedBy?: string | null;
  batchName?: string;
  batchId: string;
  suffix?: string;
  usedAt?: string | null;
  usedBy?: string | null;
}
export interface CreatedCodes extends PageResult<RedemptionCode & { plaintextCode: string }> { batchId: string }
export interface SubscriptionPlan { id: string; name: string; kind: "trial" | "day" | "week" | "month" | "quarter" | "year"; durationDays: number; dailyQuotaMicros: number; quotaMicros: number; priceMicros: number; status: string }
export interface UserSubscription { id: string; userId: string; planId: string; planName: string; kind: string; dailyQuotaMicros: number; quotaMicros: number; todayUsedMicros: number; todayFrozenMicros: number; todayAvailableMicros: number; usedMicros: number; frozenMicros: number; availableMicros: number; startsAt: string; expiresAt: string; source: string; status: string }
export interface InviteOverview { referralCode: string; pendingMicros: number; settledMicros: number }
export interface InviteCode { id: string; prefix: string; suffix: string; status: string; maxUses?: number | null; usedCount: number; expiresAt?: string | null; createdAt: string }
export type CreatedInviteCodes = PageResult<InviteCode & { code: string }>
export interface InviteReward { id: string; inviterId: string; inviteeId: string; kind: string; baseMicros: number; amountMicros: number; status: string; reason?: string | null; createdAt: string }
export interface ExternalKeyStatus { configured: boolean; prefix?: string | null; plaintextKey?: string }
export interface LedgerEntry {
  id: string;
  userId: string;
  kind: string;
  amountMicros: number | null;
  balanceAfterMicros?: number | null;
  requestId?: string;
  status?: string;
  note?: string;
  reservedMicros?: number;
  reason?: string | null;
  actor?: string;
  sourceId?: string;
  createdAt: string;
}
export interface RequestLog {
  id?: string;
  requestId: string;
  userId?: string;
  keyId: string;
  model: string;
  endpoint: string;
  status: number;
  settlementStatus: string;
  inputTokens: number;
  cacheReadTokens: number;
  outputTokens: number;
  amountUsdMicros: number;
  durationMs?: number;
  createdAt: string;
  errorCode?: string;
}
export interface LogResult extends PageResult<RequestLog> { driver?: string; warning?: string; dropped?: number; queueDepth?: number; malformedLines?: number }
