import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { desktopOnlyCommands } from "../../src/lib/api/commandSupport";

function readSource(path: string) {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

function extractMatches(source: string, pattern: RegExp) {
  return new Set([...source.matchAll(pattern)].map((match) => match[1]));
}

describe("command contract", () => {
  it("keeps client commands registered across supported transports", () => {
    const clientCommands = extractMatches(
      readSource("src/lib/api/client.ts"),
      /\binvoke(?:<[^>]+>)?\(\s*"([a-z0-9_]+)"/g,
    );
    const tauriSource = readSource("src-tauri/src/lib.rs");
    const tauriBlock = tauriSource.match(/tauri::generate_handler!\[([\s\S]*?)\]\)/)?.[1];
    expect(tauriBlock).toBeTruthy();
    const tauriCommands = extractMatches(tauriBlock ?? "", /\b([a-z][a-z0-9_]*)\b/g);
    const webCommands = extractMatches(
      readSource("src-tauri/src/web/handlers/mod.rs"),
      /^\s*"([a-z0-9_]+)"\s*=>/gm,
    );

    expect([...clientCommands].filter((command) => !tauriCommands.has(command))).toEqual([]);
    expect(
      [...clientCommands].filter(
        (command) =>
          !desktopOnlyCommands.includes(command as (typeof desktopOnlyCommands)[number]) &&
          !webCommands.has(command),
      ),
    ).toEqual([]);
    expect([...webCommands].filter((command) => !clientCommands.has(command))).toEqual([
      "health",
      // Mobile-only: the paired mobile client calls it over HTTP, so the desktop
      // client never wraps it.
      "resume_session_terminal",
      "get_route_credential",
    ]);
  });

  it("exposes export in both transports and save only on desktop", () => {
    const clientSource = readSource("src/lib/api/client.ts");
    const tauriSource = readSource("src-tauri/src/lib.rs");
    const webSource = readSource("src-tauri/src/web/handlers/mod.rs");
    const commandModule = readSource(
      "src-tauri/src/commands/route_credential_transfer_commands.rs",
    );

    expect(commandModule).toContain("pub async fn export_route_credentials(");
    expect(commandModule).toContain("input: ExportRouteCredentialsInput");
    expect(commandModule).toContain("pub async fn save_route_credential_export(");
    expect(commandModule).toMatch(
      /#\[tauri::command\(rename_all = "snake_case"\)\]\s*pub async fn save_route_credential_export\(/,
    );
    expect(commandModule).toContain("suggested_file_name: String");
    expect(commandModule).toContain("json_text: String");
    expect(tauriSource).toMatch(/generate_handler!\[[\s\S]*\bexport_route_credentials\b/);
    expect(tauriSource).toMatch(/generate_handler!\[[\s\S]*\bsave_route_credential_export\b/);
    expect(webSource).toMatch(/^\s*"export_route_credentials"\s*=>/m);
    expect(webSource).not.toMatch(/^\s*"save_route_credential_export"\s*=>/m);
    expect(clientSource).toContain('invoke("export_route_credentials", { input })');
    expect(clientSource).toContain('invoke("save_route_credential_export", {');
    expect(clientSource).toContain("suggested_file_name: input.suggested_file_name");
    expect(clientSource).toContain("json_text: input.json_text");
    expect(desktopOnlyCommands).toContain("save_route_credential_export");
  });

  it("exposes import preview and commit in both transports", () => {
    const clientSource = readSource("src/lib/api/client.ts");
    const tauriSource = readSource("src-tauri/src/lib.rs");
    const webSource = readSource("src-tauri/src/web/handlers/mod.rs");
    const commandModule = readSource(
      "src-tauri/src/commands/route_credential_transfer_commands.rs",
    );

    for (const command of ["preview_route_credential_import", "import_route_credentials"]) {
      expect(commandModule).toContain(`pub async fn ${command}(`);
      expect(tauriSource).toMatch(new RegExp(`generate_handler![\\s\\S]*\\b${command}\\b`));
      expect(webSource).toMatch(new RegExp(`^\\s*"${command}"\\s*=>`, "m"));
    }
    expect(clientSource).toContain('invoke("preview_route_credential_import", { input })');
    expect(clientSource).toContain('invoke("import_route_credentials", { input })');
    expect(desktopOnlyCommands).not.toContain("preview_route_credential_import");
    expect(desktopOnlyCommands).not.toContain("import_route_credentials");
  });

  it("exposes Skill package commands in both transports", () => {
    const clientSource = readSource("src/lib/api/client.ts");
    const tauriSource = readSource("src-tauri/src/lib.rs");
    const webSource = readSource("src-tauri/src/web/handlers/mod.rs");

    for (const command of ["skills_list_packages", "skills_read_package", "skills_install_package"]) {
      expect(clientSource).toContain(`invoke("${command}"`);
      expect(tauriSource).toMatch(new RegExp(`generate_handler![\\s\\S]*\\b${command}\\b`));
      expect(webSource).toMatch(new RegExp(`^\\s*"${command}"\\s*=>`, "m"));
    }
    for (const command of ["skills_list_packages", "skills_read_package"]) {
      expect(webSource).not.toMatch(new RegExp(`"${command}"\\s*\\|`));
    }
    expect(webSource).toMatch(/"skills_delete"\s*\|\s*"skills_install_package"/);
  });

  it("keeps system terminal recovery desktop-only", () => {
    const clientSource = readSource("src/lib/api/client.ts");
    const tauriSource = readSource("src-tauri/src/lib.rs");
    const webSource = readSource("src-tauri/src/web/handlers/mod.rs");

    expect(clientSource).toContain('invoke("open_session_terminal", { input })');
    expect(tauriSource).toMatch(/generate_handler![\s\S]*\bopen_session_terminal\b/);
    expect(desktopOnlyCommands).toContain("open_session_terminal");
    expect(webSource).not.toMatch(/^\s*"open_session_terminal"\s*=>/m);
  });
});
