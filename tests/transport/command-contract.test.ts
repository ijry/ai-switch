import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join } from "node:path";
import { describe, expect, it } from "vitest";
import { desktopOnlyCommands } from "../../src/lib/api/commandSupport";

function readSource(path: string) {
  return readFileSync(resolve(process.cwd(), path), "utf8");
}

function extractMatches(source: string, pattern: RegExp) {
  return new Set([...source.matchAll(pattern)].map((match) => match[1]));
}

/// Every `invoke("name", { ... })` call with its argument object, found by brace
/// matching so a nested object cannot end the call early.
function invokeCalls(source: string) {
  const calls: { command: string; body: string }[] = [];
  const pattern = /\binvoke(?:<[^>]+>)?\(\s*"([a-z0-9_]+)"\s*,\s*\{/g;
  for (let match = pattern.exec(source); match; match = pattern.exec(source)) {
    const open = pattern.lastIndex - 1;
    let depth = 0;
    for (let index = open; index < source.length; index += 1) {
      if (source[index] === "{") depth += 1;
      else if (source[index] === "}") {
        depth -= 1;
        if (depth === 0) {
          calls.push({ command: match[1], body: source.slice(open + 1, index) });
          break;
        }
      }
    }
  }
  return calls;
}

/// Keys of the argument object itself, skipping anything nested inside a value.
function topLevelKeys(body: string) {
  const keys: string[] = [];
  let depth = 0;
  for (const line of body.split("\n")) {
    if (depth === 0) {
      const key = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:/.exec(line);
      if (key) keys.push(key[1]);
    }
    for (const char of line) {
      if (char === "{" || char === "[") depth += 1;
      else if (char === "}" || char === "]") depth -= 1;
    }
  }
  return keys;
}

function rustSources(dir: string): string[] {
  const absolute = resolve(process.cwd(), dir);
  return readdirSync(absolute).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(resolve(process.cwd(), path)).isDirectory()) {
      return entry === "target" || entry.startsWith("target-") ? [] : rustSources(path);
    }
    return entry.endsWith(".rs") ? [readSource(path)] : [];
  });
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

  it("passes snake_case arguments only to commands that opt out of camelCase", () => {
    // `#[tauri::command]` rewrites argument names to camelCase, so a client that
    // sends `page_size` to a command without `rename_all` is not rejected — the
    // argument simply arrives as `None` and the command silently uses its
    // default, while the web dispatcher (which reads the raw key) works fine.
    // Nothing else in the suite can see that divergence.
    const modules = rustSources("src-tauri/src").join("\n");
    const offenders: string[] = [];

    for (const call of invokeCalls(readSource("src/lib/api/client.ts"))) {
      const snakeKeys = topLevelKeys(call.body).filter((key) => key.includes("_"));
      if (snakeKeys.length === 0) {
        continue;
      }
      const declaration = new RegExp(
        `(#\\[tauri::command[^\\]]*\\])\\s*pub (?:async )?fn ${call.command}\\(`,
      ).exec(modules);
      if (!declaration) {
        offenders.push(`${call.command}: command function not found`);
        continue;
      }
      if (!declaration[1].includes('rename_all = "snake_case"')) {
        offenders.push(`${call.command}: sends ${snakeKeys.join(", ")} without rename_all`);
      }
    }

    expect(offenders).toEqual([]);
  });

  it("reads every argument the client sends under the same name in the web dispatcher", () => {
    // The dispatcher reads arguments by literal key, so renaming one on the
    // client fails neither compilation nor the request — the command just
    // silently receives None. Same shape as the bug e8a5c7b fixed, other half.
    const dispatcher = readSource("src-tauri/src/web/handlers/mod.rs");
    const arms = dispatcher.split(/^\s*"([a-z0-9_]+)"\s*=>/m);
    const bodyByCommand = new Map<string, string>();
    for (let index = 1; index < arms.length; index += 2) {
      bodyByCommand.set(arms[index], arms[index + 1] ?? "");
    }

    const offenders: string[] = [];
    for (const call of invokeCalls(readSource("src/lib/api/client.ts"))) {
      const body = bodyByCommand.get(call.command);
      if (body === undefined) {
        continue; // Desktop-only; covered by the registration test above.
      }
      for (const key of topLevelKeys(call.body)) {
        if (!new RegExp(`"${key}"`).test(body)) {
          offenders.push(`${call.command}: web dispatcher never reads "${key}"`);
        }
      }
    }

    expect(offenders).toEqual([]);
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
