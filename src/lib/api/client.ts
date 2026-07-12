import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Batch,
  BatchGroup,
  ImportJob,
  Provider,
  ProviderSwitchOutcome,
  ProviderSwitchRequest,
  TargetApp,
  TargetSwitchStatus,
} from "./types";

export function listBatchGroups(search?: string): Promise<BatchGroup[]> {
  return invoke("list_batch_groups", { search: search || null });
}

export function createBatch(input: {
  name: string;
  source: string;
  notes?: string | null;
}): Promise<Batch> {
  return invoke("create_batch", { input });
}

export function importExampleJson(request: {
  batch_name: string;
  source_label: string;
  strategy: string;
  json: string;
}): Promise<ImportJob> {
  return invoke("import_example_json", { request });
}

export function listTargetApps(): Promise<TargetApp[]> {
  return invoke("list_target_apps");
}

export function listProviders(): Promise<Provider[]> {
  return invoke("list_providers");
}

export function listTargetSwitchStatuses(): Promise<TargetSwitchStatus[]> {
  return invoke("list_target_switch_statuses");
}

export function switchTargetProvider(
  request: ProviderSwitchRequest,
): Promise<ProviderSwitchOutcome> {
  return invoke("switch_target_provider", { request });
}

export function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export function saveSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke("save_settings", { settings });
}
