import { describe, expect, it } from "vitest";
import {
  capabilityReason,
  findPlatformCapability,
  operationEnabled,
} from "../src/lib/platformCapabilities";
import type {
  CapabilityAvailability,
  CapabilityRule,
  PlatformCapability,
} from "../src/lib/api/types";

describe("platform capabilities", () => {
  it("marks Hermes partial and disables native config writing", () => {
    const rule = (
      availability: CapabilityAvailability,
      reason_code: string | null = null,
      credential_kinds: string[] = [],
    ): CapabilityRule => ({
      availability,
      reason_code,
      credential_kinds,
      requires_base_url: availability === "partial",
      requires_api_dialect: availability === "partial",
    });
    const hermesCapability: PlatformCapability = {
      platform: "hermes",
      display_name: "Hermes",
      support_level: "partial",
      operations: {
        route_credentials: rule("supported"),
        generic_api_routing: rule("partial", "capability.api_credentials_only", ["api"]),
        config_write: rule("unavailable", "capability.native_config_unavailable"),
        official_import: rule("unavailable", "capability.official_account_unavailable"),
        official_account_routing: rule("unavailable", "capability.official_account_unavailable"),
        deeplink_import: rule("unavailable", "capability.deeplink_unavailable"),
        official_quota: rule("unavailable", "capability.quota_unavailable"),
        model_test: rule("partial", "capability.api_credentials_only", ["api"]),
        terminal_launch: rule("supported"),
        session_resume: rule("supported"),
      },
    };

    const hermes = findPlatformCapability([hermesCapability], "hermes");
    expect(hermes?.support_level).toBe("partial");
    expect(operationEnabled(hermes!.operations.config_write)).toBe(false);
    expect(capabilityReason(hermes!.operations.config_write)).toContain("原生配置");
  });
});
