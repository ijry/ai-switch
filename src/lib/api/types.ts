export type ApiError = {
  code: string;
  message: string;
  details?: string | null;
  recoverable: boolean;
  operation_id?: string | null;
};

export type Batch = {
  id: string;
  name: string;
  source: string;
  notes?: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type BatchChild = {
  item_type: "provider" | "official_account";
  id: string;
  title: string;
  subtitle?: string | null;
  status: "ok" | "warning" | "error";
};

export type BatchGroup = {
  batch: Batch;
  health: "ok" | "warning" | "error";
  children: BatchChild[];
};

export type Provider = {
  id: string;
  name: string;
  kind: string;
  base_url?: string | null;
  model_config_json: string;
  target_options_json: string;
  secret_ref?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type OfficialAccount = {
  id: string;
  platform: string;
  display_name: string;
  email?: string | null;
  plan?: string | null;
  account_metadata_json: string;
  secret_ref?: string | null;
  quota_snapshot_id?: string | null;
  status: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type ImportJob = {
  id: string;
  source_type: string;
  source_label: string;
  batch_id?: string | null;
  strategy: string;
  status: string;
  success_count: number;
  failure_count: number;
  conflict_count: number;
  summary_json: string;
  created_at: string;
  completed_at?: string | null;
};

export type TargetApp = {
  id: string;
  key: string;
  display_name: string;
  enabled: number;
  sort_order: number;
  created_at: string;
  updated_at: string;
};

export type AppSettings = {
  language: string;
  theme: string;
  copy_import_sources: boolean;
  logging_enabled: boolean;
  secret_storage: string;
  data_dir: string;
};
