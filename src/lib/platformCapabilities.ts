import type { CapabilityRule, PlatformCapability, PlatformId } from "./api/types";

const capabilityReasons: Record<string, string> = {
  "capability.api_credentials_only": "仅支持已配置 Base URL 和接口格式的 API 账号。",
  "capability.native_config_unavailable": "该平台的原生配置写入尚未实现。",
  "capability.official_account_unavailable": "该平台不支持官方账号导入或官方账号路由。",
  "capability.deeplink_unavailable": "该平台不支持 Deeplink 导入。",
  "capability.quota_unavailable": "该平台不支持官方账号额度刷新。",
};

export function findPlatformCapability(
  capabilities: PlatformCapability[] | undefined,
  platform: PlatformId | string,
): PlatformCapability | undefined {
  return capabilities?.find((capability) => capability.platform === platform);
}

export function operationEnabled(rule: CapabilityRule | undefined): boolean {
  return rule?.availability !== "unavailable";
}

export function capabilityReason(rule: CapabilityRule | undefined): string {
  if (!rule) {
    return "平台能力信息尚未加载。";
  }
  if (rule.reason_code && capabilityReasons[rule.reason_code]) {
    return capabilityReasons[rule.reason_code];
  }
  if (rule.availability === "partial") {
    const constraints: string[] = [];
    if (rule.credential_kinds.length) {
      constraints.push(`仅限 ${rule.credential_kinds.join("/")} 账号`);
    }
    if (rule.requires_base_url) {
      constraints.push("需要 Base URL");
    }
    if (rule.requires_api_dialect) {
      constraints.push("需要接口格式");
    }
    return constraints.length ? constraints.join("，") + "。" : "该功能仅部分支持。";
  }
  if (rule.availability === "unavailable") {
    return "该平台暂不支持此功能。";
  }
  return "";
}

export function credentialKindAllowed(
  rule: CapabilityRule | undefined,
  credentialKind: string,
): boolean {
  if (!operationEnabled(rule)) {
    return false;
  }
  return !rule?.credential_kinds.length || rule.credential_kinds.includes(credentialKind);
}
