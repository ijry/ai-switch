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
    config_write: rule("supported"),
    official_import: rule("unavailable", "capability.official_account_unavailable"),
    official_account_routing: rule("unavailable", "capability.official_account_unavailable"),
    deeplink_import: rule("unavailable", "capability.deeplink_unavailable"),
    official_quota: rule("unavailable", "capability.quota_unavailable"),
    model_test: rule("partial", "capability.api_credentials_only", ["api"]),
    terminal_launch: rule("supported"),
    session_resume: rule("supported"),
  },
};

describe("platform capabilities", () => {
  it("keeps Hermes partial while native config writing is enabled", () => {
    const hermes = findPlatformCapability([hermesCapability], "hermes");
    expect(hermes?.support_level).toBe("partial");
    // Hermes has a real config-write adapter; what it still lacks is anything
    // to do with an official vendor account.
    expect(operationEnabled(hermes!.operations.config_write)).toBe(true);
    expect(capabilityReason(hermes!.operations.config_write)).toBe("");
    expect(operationEnabled(hermes!.operations.official_import)).toBe(false);
    expect(capabilityReason(hermes!.operations.official_import)).toContain("官方账号");
  });

  it("spells out what a partial rule requires", () => {
    const hermes = findPlatformCapability([hermesCapability], "hermes");
    expect(capabilityReason(hermes!.operations.generic_api_routing)).toContain("接口格式");
  });

  it("falls back to a generic constraint list for an unknown reason code", () => {
    expect(capabilityReason(rule("partial", "capability.something_new", ["api"]))).toBe(
      "仅限 api 账号，需要 Base URL，需要接口格式。",
    );
  });
});
