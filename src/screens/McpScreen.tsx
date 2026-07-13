import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import { createMcpServer, listMcpServers, setMcpServerEnabled } from "../lib/api/client";
import type { McpTransport, NewMcpServer, SetMcpServerEnabledRequest } from "../lib/api/types";

type McpFormState = {
  name: string;
  transport: McpTransport;
  command: string;
  args_json: string;
  url: string;
  env_json: string;
  enabled: boolean;
  notes: string;
};

const initialFormState: McpFormState = {
  name: "",
  transport: "stdio",
  command: "",
  args_json: "[]",
  url: "",
  env_json: "{}",
  enabled: true,
  notes: "",
};

export function McpScreen() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<McpFormState>(initialFormState);
  const [formError, setFormError] = useState<string | null>(null);
  const serversQuery = useQuery({
    queryKey: ["mcp-servers"],
    queryFn: listMcpServers,
  });
  const createMutation = useMutation({
    mutationFn: (request: NewMcpServer) => createMcpServer(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["mcp-servers"] });
      setForm(initialFormState);
      setFormError(null);
    },
  });
  const toggleMutation = useMutation({
    mutationFn: (request: SetMcpServerEnabledRequest) => setMcpServerEnabled(request),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["mcp-servers"] }),
  });
  const servers = serversQuery.data ?? [];

  if (serversQuery.isLoading) {
    return <p className="text-steel">Loading MCP servers...</p>;
  }

  if (serversQuery.error) {
    return <p className="text-ember">Could not load MCP servers.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">MCP</h1>
        <p className="text-steel">
          Register MCP server metadata for future target config rendering without launching servers
          or storing raw secrets.
        </p>
      </div>

      <form
        className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2"
        onSubmit={(event) => {
          event.preventDefault();
          setFormError(null);

          const argsJson = form.args_json.trim() || "[]";
          const envJson = form.env_json.trim() || "{}";
          try {
            const args = JSON.parse(argsJson);
            if (!Array.isArray(args)) {
              setFormError("MCP args JSON must be an array.");
              return;
            }
          } catch {
            setFormError("MCP args JSON must be valid JSON.");
            return;
          }

          try {
            const env = JSON.parse(envJson);
            if (!env || Array.isArray(env) || typeof env !== "object") {
              setFormError("MCP environment JSON must be an object.");
              return;
            }
          } catch {
            setFormError("MCP environment JSON must be valid JSON.");
            return;
          }

          createMutation.mutate({
            name: form.name.trim(),
            transport: form.transport,
            command: form.command.trim() || null,
            args_json: argsJson,
            url: form.url.trim() || null,
            env_json: envJson,
            enabled: form.enabled,
            notes: form.notes.trim() || null,
          });
        }}
      >
        <div className="lg:col-span-2">
          <p className="font-display text-xl font-semibold text-ink">Create MCP server</p>
          <p className="text-sm text-steel">
            Use `env://NAME` or `secret://id` for sensitive environment values.
          </p>
        </div>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Name</span>
          <input
            value={form.name}
            onChange={(event) => setForm((current) => ({ ...current, name: event.target.value }))}
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="Filesystem"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Transport</span>
          <select
            value={form.transport}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                transport: event.target.value as McpTransport,
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
          >
            <option value="stdio">stdio</option>
            <option value="sse">sse</option>
            <option value="streamable_http">streamable_http</option>
          </select>
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Command</span>
          <input
            value={form.command}
            onChange={(event) =>
              setForm((current) => ({ ...current, command: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="npx"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>URL</span>
          <input
            value={form.url}
            onChange={(event) => setForm((current) => ({ ...current, url: event.target.value }))}
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
            placeholder="https://mcp.example.com/sse"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Args JSON</span>
          <textarea
            value={form.args_json}
            onChange={(event) =>
              setForm((current) => ({ ...current, args_json: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Environment JSON</span>
          <textarea
            value={form.env_json}
            onChange={(event) =>
              setForm((current) => ({ ...current, env_json: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-moss"
          />
        </label>

        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Notes</span>
          <textarea
            value={form.notes}
            onChange={(event) => setForm((current) => ({ ...current, notes: event.target.value }))}
            className="min-h-20 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 text-ink outline-none transition-colors focus:border-moss"
          />
        </label>

        <label className="flex items-center gap-2 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(event) =>
              setForm((current) => ({ ...current, enabled: event.target.checked }))
            }
            className="h-4 w-4 rounded border-ink/20 text-moss focus:ring-moss"
          />
          <span>Enabled</span>
        </label>

        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="submit" disabled={createMutation.isPending}>
            {createMutation.isPending ? "Creating..." : "Create MCP server"}
          </Button>
          {formError && <p className="text-sm text-ember">{formError}</p>}
          {createMutation.error && !formError && (
            <p className="text-sm text-ember">Could not create MCP server.</p>
          )}
        </div>
      </form>

      {servers.length === 0 ? (
        <div className="rounded-3xl border border-dashed border-ink/20 bg-white/70 p-8 text-steel shadow-sm">
          No MCP servers yet. Create one above.
        </div>
      ) : (
        <div className="grid gap-3">
          {servers.map((server) => {
            const enabled = server.enabled === 1;
            return (
              <article
                key={server.id}
                className="rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm"
              >
                <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <div>
                    <p className="font-display text-lg font-semibold text-ink">{server.name}</p>
                    <p className="text-sm text-steel">
                      {server.transport} - {server.command ?? server.url ?? "No endpoint"}
                    </p>
                  </div>
                  <span
                    className={`rounded-full px-3 py-1 text-xs font-semibold uppercase tracking-wide ${
                      enabled ? "bg-moss/10 text-moss" : "bg-ink/10 text-steel"
                    }`}
                  >
                    {enabled ? "enabled" : "disabled"}
                  </span>
                </div>

                {server.notes && <p className="mt-3 text-sm text-steel">{server.notes}</p>}

                <div className="mt-4 grid gap-3 md:grid-cols-2">
                  <pre className="overflow-x-auto rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                    {server.args_json}
                  </pre>
                  <pre className="overflow-x-auto rounded-2xl bg-paper/70 p-3 font-mono text-xs text-ink">
                    {server.env_json}
                  </pre>
                </div>

                <div className="mt-4">
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={enabled ? `Disable ${server.name}` : `Enable ${server.name}`}
                    disabled={toggleMutation.isPending}
                    onClick={() =>
                      toggleMutation.mutate({
                        id: server.id,
                        enabled: !enabled,
                      })
                    }
                  >
                    {enabled ? "Disable" : "Enable"}
                  </Button>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
