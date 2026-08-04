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
      "get_route_credential",
    ]);
  });
});
