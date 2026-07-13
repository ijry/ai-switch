import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import {
  createBulkOperation,
  createItemTag,
  createOfficialAccount,
  createMcpServer,
  createPluginLink,
  createPromptAsset,
  createFailoverPolicy,
  createInstance,
  createProxyProfile,
  createProviderFromPreset,
  createSession,
  createSessionEvent,
  createSyncProfile,
  createSyncSnapshot,
  createUpdateChannel,
  createUpdateCheck,
  createUsageEvent,
  createWakeupRun,
  createWakeupTask,
  createTag,
  exportExampleJson,
  importDeepLink,
  importOfficialAccountJson,
  listBulkOperations,
  listFailoverPolicies,
  listInstances,
  listItemTags,
  listMcpServers,
  listOfficialAccountStatuses,
  listOfficialAccounts,
  listProxyProfiles,
  listPromptAssets,
  listProviderPresets,
  listProviders,
  listPluginLinks,
  listSessionEvents,
  listSessions,
  listSyncProfiles,
  listSyncSnapshots,
  listTargetSwitchStatuses,
  listUpdateChannels,
  listUpdateChecks,
  listUsageEvents,
  listWakeupRuns,
  listWakeupTasks,
  listTags,
  refreshTrayMenu,
  refreshOfficialAccountQuotaSnapshot,
  recordOfficialAccountQuotaSnapshot,
  rollbackConfigSnapshot,
  setInstanceStatus,
  setSessionStatus,
  setMcpServerEnabled,
  setPromptAssetEnabled,
  setPluginLinkEnabled,
  setWakeupTaskEnabled,
  switchTargetProvider,
} from "../src/lib/api/client";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("api client provider switching", () => {
  it("invokes provider and target switching commands", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listProviders();
    expect(invoke).toHaveBeenLastCalledWith("list_providers");

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listProviderPresets();
    expect(invoke).toHaveBeenLastCalledWith("list_provider_presets");

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listOfficialAccounts();
    expect(invoke).toHaveBeenLastCalledWith("list_official_accounts");

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listOfficialAccountStatuses();
    expect(invoke).toHaveBeenLastCalledWith("list_official_account_statuses");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "account-1" });
    await createOfficialAccount({
      account: {
        platform: "codex",
        display_name: "Team Codex",
        email: "team@example.com",
        plan: "team",
        account_metadata_json: "{}",
        secret_ref: "secret://account/team",
      },
      batch_id: "batch-1",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_official_account", {
      request: {
        account: {
          platform: "codex",
          display_name: "Team Codex",
          email: "team@example.com",
          plan: "team",
          account_metadata_json: "{}",
          secret_ref: "secret://account/team",
        },
        batch_id: "batch-1",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ quota_snapshot: { id: "quota-1" } });
    await recordOfficialAccountQuotaSnapshot({
      account_id: "account-1",
      status: "warning",
      remaining_label: "12% remaining",
      reset_at: "2026-07-14T00:00:00Z",
      summary_json: "{}",
      raw_excerpt_json: "{}",
    });
    expect(invoke).toHaveBeenLastCalledWith("record_official_account_quota_snapshot", {
      request: {
        account_id: "account-1",
        status: "warning",
        remaining_label: "12% remaining",
        reset_at: "2026-07-14T00:00:00Z",
        summary_json: "{}",
        raw_excerpt_json: "{}",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ quota_snapshot: { id: "quota-2" } });
    await refreshOfficialAccountQuotaSnapshot({ account_id: "account-1" });
    expect(invoke).toHaveBeenLastCalledWith("refresh_official_account_quota_snapshot", {
      request: { account_id: "account-1" },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ provider: { id: "provider-1" } });
    await createProviderFromPreset({
      preset_id: "openai-compatible",
      batch_name: "Preset Batch",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_provider_from_preset", {
      request: {
        preset_id: "openai-compatible",
        batch_name: "Preset Batch",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ json: "{}", provider_count: 0, account_count: 0 });
    await exportExampleJson();
    expect(invoke).toHaveBeenLastCalledWith("export_example_json");

    vi.mocked(invoke).mockResolvedValueOnce({ success_count: 1 });
    await importOfficialAccountJson({
      batch_name: "Official accounts",
      source_label: "manual account paste",
      platform: "codex",
      json: "{\"accounts\":[]}",
    });
    expect(invoke).toHaveBeenLastCalledWith("import_official_account_json", {
      request: {
        batch_name: "Official accounts",
        source_label: "manual account paste",
        platform: "codex",
        json: "{\"accounts\":[]}",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ success_count: 1 });
    await importDeepLink({
      url: "ai-switch://import/example_json?batch_name=Shared&payload=eyJwcm92aWRlcnMiOltdfQ",
    });
    expect(invoke).toHaveBeenLastCalledWith("import_deep_link", {
      request: {
        url: "ai-switch://import/example_json?batch_name=Shared&payload=eyJwcm92aWRlcnMiOltdfQ",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      provider_count: 0,
      target_count: 0,
      switch_item_count: 0,
    });
    await refreshTrayMenu();
    expect(invoke).toHaveBeenLastCalledWith("refresh_tray_menu");

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listTags();
    expect(invoke).toHaveBeenLastCalledWith("list_tags");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "tag-1" });
    await createTag({
      name: "review",
      color: "#3f6f5f",
      description: "Shared review items",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_tag", {
      request: {
        name: "review",
        color: "#3f6f5f",
        description: "Shared review items",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listItemTags();
    expect(invoke).toHaveBeenLastCalledWith("list_item_tags");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "item-tag-1" });
    await createItemTag({
      tag_id: "tag-1",
      item_type: "provider",
      item_id: "provider-1",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_item_tag", {
      request: {
        tag_id: "tag-1",
        item_type: "provider",
        item_id: "provider-1",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listPluginLinks();
    expect(invoke).toHaveBeenLastCalledWith("list_plugin_links");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "plugin-link-1" });
    await createPluginLink({
      name: "Review bridge",
      plugin_key: "review.bridge",
      item_type: "provider",
      item_id: "provider-1",
      config_json: "{\"mode\":\"metadata\"}",
      enabled: true,
      status: "configured",
      notes: "Metadata only",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_plugin_link", {
      request: {
        name: "Review bridge",
        plugin_key: "review.bridge",
        item_type: "provider",
        item_id: "provider-1",
        config_json: "{\"mode\":\"metadata\"}",
        enabled: true,
        status: "configured",
        notes: "Metadata only",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "plugin-link-1", enabled: 0 });
    await setPluginLinkEnabled({ id: "plugin-link-1", enabled: false });
    expect(invoke).toHaveBeenLastCalledWith("set_plugin_link_enabled", {
      request: { id: "plugin-link-1", enabled: false },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listBulkOperations();
    expect(invoke).toHaveBeenLastCalledWith("list_bulk_operations");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "bulk-operation-1" });
    await createBulkOperation({
      name: "Apply review tag",
      operation_type: "tag_apply",
      target_type: "provider",
      item_ids_json: "[\"provider-1\"]",
      parameters_json: "{\"tag_id\":\"tag-1\"}",
      dry_run: true,
      status: "planned",
      summary_json: "{}",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_bulk_operation", {
      request: {
        name: "Apply review tag",
        operation_type: "tag_apply",
        target_type: "provider",
        item_ids_json: "[\"provider-1\"]",
        parameters_json: "{\"tag_id\":\"tag-1\"}",
        dry_run: true,
        status: "planned",
        summary_json: "{}",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listMcpServers();
    expect(invoke).toHaveBeenLastCalledWith("list_mcp_servers");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "mcp-1" });
    await createMcpServer({
      name: "Filesystem",
      transport: "stdio",
      command: "npx",
      args_json: "[\"-y\",\"@modelcontextprotocol/server-filesystem\"]",
      url: null,
      env_json: "{\"BRAVE_API_KEY\":\"env://BRAVE_API_KEY\"}",
      enabled: true,
      notes: "Local files",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_mcp_server", {
      request: {
        name: "Filesystem",
        transport: "stdio",
        command: "npx",
        args_json: "[\"-y\",\"@modelcontextprotocol/server-filesystem\"]",
        url: null,
        env_json: "{\"BRAVE_API_KEY\":\"env://BRAVE_API_KEY\"}",
        enabled: true,
        notes: "Local files",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "mcp-1", enabled: 0 });
    await setMcpServerEnabled({ id: "mcp-1", enabled: false });
    expect(invoke).toHaveBeenLastCalledWith("set_mcp_server_enabled", {
      request: { id: "mcp-1", enabled: false },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listPromptAssets();
    expect(invoke).toHaveBeenLastCalledWith("list_prompt_assets");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "prompt-1" });
    await createPromptAsset({
      item_type: "prompt",
      name: "Review Prompt",
      description: "Find risky behavior changes.",
      body: "Review this diff.",
      tags_json: "[\"review\"]",
      metadata_json: "{\"owner\":\"engineering\"}",
      enabled: true,
    });
    expect(invoke).toHaveBeenLastCalledWith("create_prompt_asset", {
      request: {
        item_type: "prompt",
        name: "Review Prompt",
        description: "Find risky behavior changes.",
        body: "Review this diff.",
        tags_json: "[\"review\"]",
        metadata_json: "{\"owner\":\"engineering\"}",
        enabled: true,
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "prompt-1", enabled: 0 });
    await setPromptAssetEnabled({ id: "prompt-1", enabled: false });
    expect(invoke).toHaveBeenLastCalledWith("set_prompt_asset_enabled", {
      request: { id: "prompt-1", enabled: false },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listProxyProfiles();
    expect(invoke).toHaveBeenLastCalledWith("list_proxy_profiles");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "proxy-1" });
    await createProxyProfile({
      name: "Local Proxy",
      endpoint_url: "http://127.0.0.1:7890",
      auth_ref: "env://LOCAL_PROXY_AUTH",
      enabled: true,
      notes: "Local only",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_proxy_profile", {
      request: {
        name: "Local Proxy",
        endpoint_url: "http://127.0.0.1:7890",
        auth_ref: "env://LOCAL_PROXY_AUTH",
        enabled: true,
        notes: "Local only",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listFailoverPolicies();
    expect(invoke).toHaveBeenLastCalledWith("list_failover_policies");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "failover-1" });
    await createFailoverPolicy({
      name: "Primary then backup",
      strategy: "ordered",
      provider_ids_json: "[\"provider-1\",\"provider-2\"]",
      enabled: true,
      notes: null,
    });
    expect(invoke).toHaveBeenLastCalledWith("create_failover_policy", {
      request: {
        name: "Primary then backup",
        strategy: "ordered",
        provider_ids_json: "[\"provider-1\",\"provider-2\"]",
        enabled: true,
        notes: null,
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listUsageEvents();
    expect(invoke).toHaveBeenLastCalledWith("list_usage_events");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "usage-1" });
    await createUsageEvent({
      provider_id: "provider-1",
      official_account_id: null,
      source_label: "manual",
      metric_type: "request",
      amount: 12,
      unit: "count",
      metadata_json: "{}",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_usage_event", {
      request: {
        provider_id: "provider-1",
        official_account_id: null,
        source_label: "manual",
        metric_type: "request",
        amount: 12,
        unit: "count",
        metadata_json: "{}",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listSyncProfiles();
    expect(invoke).toHaveBeenLastCalledWith("list_sync_profiles");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "sync-1" });
    await createSyncProfile({
      name: "Team WebDAV",
      provider: "webdav",
      endpoint_url: "https://sync.example.com/ai-switch",
      auth_ref: "env://WEBDAV_TOKEN",
      scope_json: "{\"providers\":true}",
      enabled: true,
      notes: "Shared export",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_sync_profile", {
      request: {
        name: "Team WebDAV",
        provider: "webdav",
        endpoint_url: "https://sync.example.com/ai-switch",
        auth_ref: "env://WEBDAV_TOKEN",
        scope_json: "{\"providers\":true}",
        enabled: true,
        notes: "Shared export",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listSyncSnapshots();
    expect(invoke).toHaveBeenLastCalledWith("list_sync_snapshots");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "sync-snapshot-1" });
    await createSyncSnapshot({
      profile_id: "sync-1",
      direction: "export",
      artifact_ref: null,
    });
    expect(invoke).toHaveBeenLastCalledWith("create_sync_snapshot", {
      request: {
        profile_id: "sync-1",
        direction: "export",
        artifact_ref: null,
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listSessions();
    expect(invoke).toHaveBeenLastCalledWith("list_sessions");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "session-1" });
    await createSession({
      title: "Release review",
      target_app_id: "target-codex",
      provider_id: "provider-1",
      official_account_id: "account-1",
      prompt_asset_id: "prompt-1",
      mcp_server_ids_json: "[\"mcp-1\"]",
      tags_json: "[\"review\"]",
      status: "draft",
      notes: "Prepare release notes",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_session", {
      request: {
        title: "Release review",
        target_app_id: "target-codex",
        provider_id: "provider-1",
        official_account_id: "account-1",
        prompt_asset_id: "prompt-1",
        mcp_server_ids_json: "[\"mcp-1\"]",
        tags_json: "[\"review\"]",
        status: "draft",
        notes: "Prepare release notes",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "session-1", status: "active" });
    await setSessionStatus({ id: "session-1", status: "active" });
    expect(invoke).toHaveBeenLastCalledWith("set_session_status", {
      request: { id: "session-1", status: "active" },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listSessionEvents({ session_id: "session-1" });
    expect(invoke).toHaveBeenLastCalledWith("list_session_events", {
      request: { session_id: "session-1" },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "session-event-1" });
    await createSessionEvent({
      session_id: "session-1",
      event_type: "note",
      message: "Started review",
      metadata_json: "{}",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_session_event", {
      request: {
        session_id: "session-1",
        event_type: "note",
        message: "Started review",
        metadata_json: "{}",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listUpdateChannels();
    expect(invoke).toHaveBeenLastCalledWith("list_update_channels");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "update-channel-1" });
    await createUpdateChannel({
      name: "Stable",
      channel: "stable",
      feed_url: "https://updates.example.com/stable.json",
      enabled: true,
      notes: "Main channel",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_update_channel", {
      request: {
        name: "Stable",
        channel: "stable",
        feed_url: "https://updates.example.com/stable.json",
        enabled: true,
        notes: "Main channel",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listUpdateChecks();
    expect(invoke).toHaveBeenLastCalledWith("list_update_checks");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "update-check-1" });
    await createUpdateCheck({
      channel_id: "update-channel-1",
      current_version: "0.1.0",
      latest_version: "0.1.1",
      status: "available",
      release_notes_url: "https://updates.example.com/releases/0.1.1",
      details_json: "{}",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_update_check", {
      request: {
        channel_id: "update-channel-1",
        current_version: "0.1.0",
        latest_version: "0.1.1",
        status: "available",
        release_notes_url: "https://updates.example.com/releases/0.1.1",
        details_json: "{}",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listInstances();
    expect(invoke).toHaveBeenLastCalledWith("list_instances");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "instance-1" });
    await createInstance({
      name: "Codex Review",
      target_app_id: "target-codex",
      provider_id: "provider-1",
      launch_args_json: "[\"--profile\",\"review\"]",
      env_json: "{\"API_KEY\":\"env://API_KEY\"}",
      profile_json: "{\"workspace\":\"review\"}",
      status: "configured",
      notes: "Local metadata only",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_instance", {
      request: {
        name: "Codex Review",
        target_app_id: "target-codex",
        provider_id: "provider-1",
        launch_args_json: "[\"--profile\",\"review\"]",
        env_json: "{\"API_KEY\":\"env://API_KEY\"}",
        profile_json: "{\"workspace\":\"review\"}",
        status: "configured",
        notes: "Local metadata only",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "instance-1", status: "running" });
    await setInstanceStatus({ id: "instance-1", status: "running" });
    expect(invoke).toHaveBeenLastCalledWith("set_instance_status", {
      request: { id: "instance-1", status: "running" },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listWakeupTasks();
    expect(invoke).toHaveBeenLastCalledWith("list_wakeup_tasks");

    vi.mocked(invoke).mockResolvedValueOnce({ id: "wakeup-task-1" });
    await createWakeupTask({
      name: "Morning review",
      managed_instance_id: "instance-1",
      target_app_id: "target-codex",
      provider_id: "provider-1",
      trigger_type: "manual",
      schedule_json: "{\"window\":\"morning\"}",
      action_json: "{\"kind\":\"status_record\"}",
      enabled: true,
      status: "configured",
      notes: "Metadata only",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_wakeup_task", {
      request: {
        name: "Morning review",
        managed_instance_id: "instance-1",
        target_app_id: "target-codex",
        provider_id: "provider-1",
        trigger_type: "manual",
        schedule_json: "{\"window\":\"morning\"}",
        action_json: "{\"kind\":\"status_record\"}",
        enabled: true,
        status: "configured",
        notes: "Metadata only",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "wakeup-task-1", enabled: 0 });
    await setWakeupTaskEnabled({ id: "wakeup-task-1", enabled: false });
    expect(invoke).toHaveBeenLastCalledWith("set_wakeup_task_enabled", {
      request: { id: "wakeup-task-1", enabled: false },
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listWakeupRuns({ task_id: null });
    expect(invoke).toHaveBeenLastCalledWith("list_wakeup_runs", {
      request: { task_id: null },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ id: "wakeup-run-1" });
    await createWakeupRun({
      task_id: "wakeup-task-1",
      outcome: "recorded",
      message: "Ready for manual start",
      metadata_json: "{}",
    });
    expect(invoke).toHaveBeenLastCalledWith("create_wakeup_run", {
      request: {
        task_id: "wakeup-task-1",
        outcome: "recorded",
        message: "Ready for manual start",
        metadata_json: "{}",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ status: "rolled_back" });
    await rollbackConfigSnapshot("snapshot-1");
    expect(invoke).toHaveBeenLastCalledWith("rollback_config_snapshot", {
      snapshotId: "snapshot-1",
    });

    vi.mocked(invoke).mockResolvedValueOnce([]);
    await listTargetSwitchStatuses();
    expect(invoke).toHaveBeenLastCalledWith("list_target_switch_statuses");

    vi.mocked(invoke).mockResolvedValueOnce({ status: "written" });
    await switchTargetProvider({
      target_app_id: "target-1",
      provider_id: "provider-1",
      mode: "sandbox",
    });
    expect(invoke).toHaveBeenLastCalledWith("switch_target_provider", {
      request: {
        target_app_id: "target-1",
        provider_id: "provider-1",
        mode: "sandbox",
      },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ status: "written" });
    await switchTargetProvider({
      target_app_id: "target-codex",
      provider_id: "provider-1",
      mode: "real",
    });
    expect(invoke).toHaveBeenLastCalledWith("switch_target_provider", {
      request: {
        target_app_id: "target-codex",
        provider_id: "provider-1",
        mode: "real",
      },
    });
  });
});
