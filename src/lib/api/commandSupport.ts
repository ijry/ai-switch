export const desktopOnlyCommands = [
  "open_route_proxy_https_certificate_dir",
  "save_route_credential_export",
] as const;

export function isDesktopOnlyCommand(command: string) {
  return (desktopOnlyCommands as readonly string[]).includes(command);
}
