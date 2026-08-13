import type { TranslationKey } from "../i18n";

const ERROR_KEYS: Record<string, TranslationKey> = {
  "mcp.config_invalid": "errors.mcp.configInvalid",
  "mcp.config_io": "errors.mcp.configIo",
  "mcp.invalid_spec": "errors.mcp.invalidSpec",
  "mcp.marketplace_network": "errors.mcp.marketplaceNetwork",
  "mcp.server_not_found": "errors.mcp.serverNotFound",
  "skills.config_invalid": "errors.skills.configInvalid",
  "skills.config_io": "errors.skills.configIo",
  "skills.invalid_id": "errors.skills.invalidId",
  "skills.directory_missing": "errors.skills.directoryMissing",
  "skills.path_invalid": "errors.skills.pathInvalid",
  "skills.read_only": "errors.skills.readOnly",
  "skills.not_found": "errors.skills.notFound",
  "skills.manifest_invalid": "errors.skills.manifestInvalid",
  "skills.package_member_missing": "errors.skills.packageMemberMissing",
  "skills.package_not_found": "errors.skills.packageNotFound",
  "skills.package_scan_failed": "errors.skills.packageScanFailed",
  "skills.package_operation_unsupported": "errors.skills.packageOperationUnsupported",
};

export function apiErrorMessageKey(code: string): TranslationKey {
  return ERROR_KEYS[code] ?? "errors.operationFailed";
}
