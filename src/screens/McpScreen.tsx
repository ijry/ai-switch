import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { motion } from "motion/react";
import { Check, ExternalLink, Pencil, Plus, RefreshCw, Search, Server, Trash2, X } from "lucide-react";
import { useState } from "react";
import {
  mcpGetMarketplaceServerDetail,
  mcpInstallFromMarketplace,
  mcpListMarketplaces,
  mcpRemoveServer,
  mcpScanLocal,
  mcpSearchMarketplace,
  mcpUpsertLocalServer,
} from "../lib/api/client";
import { ApiClientError } from "../lib/api/errors";
import { apiErrorMessageKey } from "../lib/api/errorMessages";
import { useI18n } from "../lib/i18n";
import { openExternal } from "../lib/openExternal";
import type {
  LocalMcpServer,
  McpAppType,
  McpMarketplaceInstallParameter,
  McpMarketplaceServerDetail,
  McpSpec,
} from "../lib/api/types";
import { McpAppSelector } from "../components/mcp/McpAppSelector";
import { appLabelKey } from "../components/mcp/catalog";
import { MotionPresence } from "../components/motion/MotionPrimitives";

const EMPTY_SPEC = JSON.stringify({ type: "stdio", command: "npx", args: ["-y", "@modelcontextprotocol/server-filesystem", "."] }, null, 2);

export function McpScreen() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [view, setView] = useState<"local" | "market">("local");
  const [editor, setEditor] = useState<{ id: string; spec: string; apps: McpAppType[] } | null>(null);
  const [editorError, setEditorError] = useState<string | null>(null);
  const [localSearch, setLocalSearch] = useState("");
  const [providerId, setProviderId] = useState("official_registry");
  const [searchText, setSearchText] = useState("");
  const [detailId, setDetailId] = useState<string | null>(null);
  const [optionId, setOptionId] = useState<string | null>(null);
  const [selectedApps, setSelectedApps] = useState<McpAppType[]>(["codex", "claude_code"]);
  const [parameterValues, setParameterValues] = useState<Record<string, unknown>>({});
  const localQuery = useQuery({ queryKey: ["mcp-local"], queryFn: mcpScanLocal });
  const marketplacesQuery = useQuery({ queryKey: ["mcp-marketplaces"], queryFn: mcpListMarketplaces });
  const marketQuery = useQuery({
    queryKey: ["mcp-market", providerId, searchText],
    queryFn: () => mcpSearchMarketplace({ providerId, query: searchText, limit: 30 }),
    enabled: view === "market",
  });
  const detailQuery = useQuery({
    queryKey: ["mcp-market-detail", providerId, detailId],
    queryFn: () => mcpGetMarketplaceServerDetail(providerId, detailId ?? ""),
    enabled: Boolean(detailId),
  });
  const saveMutation = useMutation({
    mutationFn: (input: { serverId: string; spec: McpSpec; apps: McpAppType[] }) => mcpUpsertLocalServer(input),
    onSuccess: async () => {
      setEditor(null);
      await queryClient.invalidateQueries({ queryKey: ["mcp-local"] });
    },
  });
  const deleteMutation = useMutation({
    mutationFn: (serverId: string) => mcpRemoveServer(serverId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["mcp-local"] }),
  });
  const installMutation = useMutation({
    mutationFn: (detail: McpMarketplaceServerDetail) =>
      mcpInstallFromMarketplace({
        providerId: detail.provider_id,
        serverId: detail.server_id,
        apps: selectedApps,
        optionId: optionId ?? detail.default_option_id,
        parameterValues,
      }),
    onSuccess: async () => {
      setDetailId(null);
      await queryClient.invalidateQueries({ queryKey: ["mcp-local"] });
    },
  });

  const openEditor = (server?: LocalMcpServer) => {
    setEditorError(null);
    setEditor({
      id: server?.id ?? "",
      spec: server ? JSON.stringify(server.spec, null, 2) : EMPTY_SPEC,
      apps: server?.apps ?? ["codex", "claude_code"],
    });
  };
  const error = saveMutation.error
    ?? deleteMutation.error
    ?? installMutation.error
    ?? localQuery.error
    ?? marketQuery.error
    ?? detailQuery.error;
  const errorMessage = error instanceof ApiClientError
    ? t(apiErrorMessageKey(error.code))
    : t("mcp.operationFailed");

  return (
    <section className="mcp-screen space-y-3 rounded-2xl bg-white/20 p-1 sm:p-2">
      <header className="flex flex-wrap items-end justify-between gap-3 rounded-2xl border border-stone-200 bg-white/70 px-4 py-3 shadow-sm">
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("mcp.kicker")}</p>
          <h1 className="mt-0.5 text-lg font-semibold text-stone-950">{t("mcp.title")}</h1>
          <p className="mt-1 text-[12px] text-stone-500">{t("mcp.subtitle")}</p>
        </div>
        <button className="inline-flex items-center gap-1.5 border border-stone-300 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-800 hover:bg-stone-50" onClick={() => queryClient.invalidateQueries({ queryKey: ["mcp-local"] })} title={t("mcp.refreshTitle")} type="button">
          <RefreshCw className="h-3.5 w-3.5" /> {t("mcp.refresh")}
        </button>
      </header>

      <div className="flex flex-wrap items-center justify-between gap-2 rounded-2xl border border-stone-200 bg-white/55 p-2 shadow-sm">
        <div className="inline-flex rounded-xl border border-stone-200 bg-white p-0.5" role="tablist">
          <button aria-selected={view === "local"} className={`rounded-lg px-3 py-1.5 text-[12px] font-semibold motion-control ${view === "local" ? "bg-stone-900 text-white" : "text-stone-600 hover:bg-stone-50"}`} onClick={() => setView("local")} role="tab" type="button">{t("mcp.localConfiguration")}</button>
          <button aria-selected={view === "market"} className={`rounded-lg px-3 py-1.5 text-[12px] font-semibold motion-control ${view === "market" ? "bg-stone-900 text-white" : "text-stone-600 hover:bg-stone-50"}`} onClick={() => setView("market")} role="tab" type="button">{t("mcp.marketplace")}</button>
        </div>
        {view === "local" ? <div className="flex flex-wrap items-center gap-2"><label className="flex min-w-[220px] items-center gap-2 border border-stone-300 bg-white px-2.5 py-1.5"><Search className="h-3.5 w-3.5 text-stone-400" /><input aria-label={t("mcp.filterLocal")} className="min-w-0 flex-1 text-[12px] outline-none" onChange={(event) => setLocalSearch(event.target.value)} placeholder={t("mcp.filterLocal")} value={localSearch} /></label><button className="inline-flex items-center gap-1.5 bg-stone-900 px-2.5 py-1.5 text-[12px] font-semibold text-white hover:bg-stone-800" onClick={() => openEditor()} type="button"><Plus className="h-3.5 w-3.5" /> {t("mcp.addServer")}</button></div> : null}
      </div>

      <MotionPresence>
        {view === "local" ? (
          <motion.div
            key="mcp-local-view"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          >
            <LocalServers filter={localSearch} servers={localQuery.data ?? []} loading={localQuery.isLoading} onEdit={openEditor} onDelete={(id) => { if (window.confirm(t("mcp.removeConfirm", { id }))) deleteMutation.mutate(id); }} />
          </motion.div>
        ) : (
          <motion.div
            key="mcp-market-view"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -6 }}
            transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
          >
          <MarketplaceView
          detail={detailQuery.data}
          detailLoading={detailQuery.isLoading}
          detailId={detailId}
          marketplaces={marketplacesQuery.data ?? []}
          providerId={providerId}
          results={marketQuery.data ?? []}
          searchText={searchText}
          selectedApps={selectedApps}
          parameterValues={parameterValues}
          optionId={optionId}
          onProviderChange={(value) => { setProviderId(value); setDetailId(null); setOptionId(null); setParameterValues({}); }}
          onSearchChange={setSearchText}
          onSelect={(id) => { setDetailId(id); setOptionId(null); setParameterValues({}); }}
          onClose={() => setDetailId(null)}
          onAppsChange={setSelectedApps}
          onParameterChange={(key, value) => setParameterValues((current) => ({ ...current, [key]: value }))}
          onOptionChange={(id) => { setOptionId(id); setParameterValues({}); }}
          onInstall={() => detailQuery.data && installMutation.mutate(detailQuery.data)}
          installing={installMutation.isPending}
          />
          </motion.div>
        )}
      </MotionPresence>
      {error ? <p className="border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-800" role="alert">{errorMessage}</p> : null}
      {editor ? <McpEditor editor={editor} error={editorError} saving={saveMutation.isPending} onChange={(value) => { setEditorError(null); setEditor(value); }} onClose={() => setEditor(null)} onSave={() => { try { const spec = JSON.parse(editor.spec) as McpSpec; if (!spec || Array.isArray(spec) || typeof spec !== "object") { setEditorError(t("mcp.invalidJson")); return; } saveMutation.mutate({ serverId: editor.id.trim(), spec, apps: editor.apps }); } catch { setEditorError(t("mcp.invalidJson")); } }} /> : null}
    </section>
  );
}

function LocalServers({ servers, loading, filter, onEdit, onDelete }: { servers: LocalMcpServer[]; loading: boolean; filter: string; onEdit: (server?: LocalMcpServer) => void; onDelete: (id: string) => void }) {
  const { t } = useI18n();
  if (loading) return <p className="text-sm text-stone-500">{t("mcp.scanning")}</p>;
  const needle = filter.trim().toLowerCase();
  const visibleServers = needle
    ? servers.filter((server) => `${server.id} ${String(server.spec.type ?? "")} ${String(server.spec.command ?? server.spec.url ?? "")}`.toLowerCase().includes(needle))
    : servers;
  if (!visibleServers.length) return <div className="rounded-2xl border border-dashed border-stone-300 bg-white px-4 py-8 text-center text-[13px] text-stone-500">{servers.length ? t("mcp.noMatches") : t("mcp.empty")}</div>;
  return <div className="space-y-2">{visibleServers.map((server) => <article className="rounded-2xl border border-stone-200 bg-white px-3 py-3 shadow-sm" key={server.id}><div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><Server className="h-4 w-4 text-emerald-600" /><h2 className="truncate text-[14px] font-semibold text-stone-950">{server.id}</h2><span className="rounded-lg border border-stone-200 bg-stone-50 px-1.5 py-0.5 font-mono text-[10px] text-stone-600">{String(server.spec.type ?? "stdio")}</span></div><p className="mt-1 break-all font-mono text-[11px] text-stone-500">{server.spec.type === "stdio" ? String(server.spec.command ?? "") : String(server.spec.url ?? "")}</p><div className="mt-2 flex flex-wrap gap-1">{server.apps.map((app) => <span className="rounded-full border border-emerald-200 bg-emerald-50 px-1.5 py-0.5 text-[10px] font-medium text-emerald-800" key={app}>{t(appLabelKey(app))}</span>)}</div></div><div className="flex shrink-0 gap-1"><button aria-label={`${t("mcp.editServer")} ${server.id}`} className="grid h-8 w-8 place-items-center border border-stone-300 bg-white text-stone-700 hover:bg-stone-50" onClick={() => onEdit(server)} title={t("mcp.editServer")} type="button"><Pencil className="h-3.5 w-3.5" /></button><button aria-label={`${t("mcp.removeServer")} ${server.id}`} className="grid h-8 w-8 place-items-center border border-red-200 bg-white text-red-700 hover:bg-red-50" onClick={() => onDelete(server.id)} title={t("mcp.removeServer")} type="button"><Trash2 className="h-3.5 w-3.5" /></button></div></div></article>)}</div>;
}

function MarketplaceView(props: { detail?: McpMarketplaceServerDetail; detailLoading: boolean; detailId: string | null; marketplaces: Array<{ id: string; name: string; description: string }>; providerId: string; results: Array<{ server_id: string; name: string; description: string; homepage?: string | null; protocols: string[]; verified: boolean; downloads?: number | null }>; searchText: string; selectedApps: McpAppType[]; parameterValues: Record<string, unknown>; optionId: string | null; onProviderChange: (value: string) => void; onSearchChange: (value: string) => void; onSelect: (id: string) => void; onClose: () => void; onAppsChange: (apps: McpAppType[]) => void; onParameterChange: (key: string, value: unknown) => void; onOptionChange: (id: string) => void; onInstall: () => void; installing: boolean }) {
  const { t } = useI18n();
  const detail = props.detail;
  const activeOption = detail?.install_options.find((option) => option.id === (props.optionId ?? detail.default_option_id)) ?? detail?.install_options[0];
  const missingRequired = (activeOption?.parameters ?? []).some((parameter) => {
    if (!parameter.required) return false;
    const value = props.parameterValues[parameter.key] ?? parameter.default_value;
    return value == null || String(value).trim() === "";
  });
  return <div className="space-y-3"><div className="flex flex-wrap gap-2 rounded-2xl border border-stone-200 bg-white/65 p-2"><select aria-label={t("mcp.marketplaceProvider")} className="rounded-xl border border-stone-300 bg-white px-2.5 py-2 text-[12px] font-medium text-stone-800" onChange={(event) => props.onProviderChange(event.target.value)} value={props.providerId}>{props.marketplaces.map((market) => <option key={market.id} value={market.id}>{market.name}</option>)}</select><label className="flex min-w-[220px] flex-1 items-center gap-2 rounded-xl border border-stone-300 bg-white px-2.5 py-1.5"><Search className="h-3.5 w-3.5 text-stone-400" /><input aria-label={t("mcp.searchMarketplace")} className="min-w-0 flex-1 text-[12px] outline-none" onChange={(event) => props.onSearchChange(event.target.value)} placeholder={t("mcp.searchServers")} value={props.searchText} /></label></div><div className="grid gap-2 md:grid-cols-2">{props.results.map((item) => <button className="rounded-2xl border border-stone-200 bg-white p-3 text-left shadow-sm motion-control hover:border-stone-400" key={item.server_id} onClick={() => props.onSelect(item.server_id)} type="button"><div className="flex items-start justify-between gap-2"><div className="min-w-0"><p className="truncate text-[13px] font-semibold text-stone-950">{item.name}</p><p className="mt-0.5 line-clamp-2 text-[12px] text-stone-500">{item.description}</p></div>{item.verified ? <Check className="h-4 w-4 shrink-0 text-emerald-600" /> : null}</div><div className="mt-2 flex flex-wrap gap-1 text-[10px] text-stone-500"><span>{item.protocols.join(" / ") || "stdio"}</span>{item.downloads ? <span>· {item.downloads.toLocaleString()} {t("mcp.uses")}</span> : null}</div></button>)}</div>{props.detailId ? <div className="motion-overlay fixed inset-0 z-40 grid place-items-center bg-stone-950/35 p-4" role="presentation"><div className="max-h-[90vh] w-full max-w-2xl overflow-y-auto rounded-2xl border border-stone-200 bg-white p-4 shadow-xl" role="dialog"><div className="flex items-start justify-between gap-3"><div><p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("mcp.detail")}</p><h2 className="mt-1 text-base font-semibold text-stone-950">{props.detail?.name ?? t("mcp.loadingDetail")}</h2></div><button aria-label={t("mcp.closeDetail")} className="grid h-8 w-8 place-items-center border border-stone-300 text-stone-600 hover:bg-stone-50" onClick={props.onClose} title={t("mcp.closeDetail")} type="button"><X className="h-4 w-4" /></button></div>{props.detailLoading || !props.detail ? <p className="mt-5 text-sm text-stone-500">{t("mcp.loadingDetail")}</p> : <div className="mt-4 space-y-4"><p className="text-[12px] text-stone-600">{props.detail.description}</p>{props.detail.homepage ? <a className="inline-flex items-center gap-1 text-[12px] font-medium text-blue-700 hover:underline" href={props.detail.homepage} onClick={(event) => { event.preventDefault(); const homepage = props.detail?.homepage; if (homepage) { void openExternal(homepage); } }} rel="noreferrer" target="_blank">{t("mcp.openHomepage")} <ExternalLink className="h-3 w-3" /></a> : null}<McpAppSelector legend={t("mcp.installForClients")} onChange={props.onAppsChange} selectedApps={props.selectedApps} />{props.detail.install_options.length > 1 ? <label className="grid gap-1 text-[12px] font-medium text-stone-700"><span>{t("mcp.transport")}</span><select className="rounded-xl border border-stone-300 bg-white px-2 py-1.5 motion-control" onChange={(event) => props.onOptionChange(event.target.value)} value={activeOption?.id ?? ""}>{props.detail.install_options.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}</select></label> : null}<div className="space-y-2">{(activeOption?.parameters ?? []).map((parameter) => <ParameterField key={parameter.key} parameter={parameter} value={props.parameterValues[parameter.key]} onChange={props.onParameterChange} />)}</div><button className="inline-flex items-center gap-1.5 bg-stone-900 px-3 py-2 text-[12px] font-semibold text-white hover:bg-stone-800 disabled:opacity-50" disabled={props.installing || props.selectedApps.length === 0 || missingRequired} onClick={props.onInstall} type="button"><Check className="h-3.5 w-3.5" />{props.installing ? t("mcp.installing") : t("mcp.install")}</button></div>}</div></div> : null}</div>;
}

function ParameterField({ parameter, value, onChange }: { parameter: McpMarketplaceInstallParameter; value: unknown; onChange: (key: string, value: unknown) => void }) {
  const { t } = useI18n();
  const stringValue = value == null ? "" : String(value);
  const fieldClass = "border border-stone-300 px-2 py-1.5 outline-none focus:border-blue-400";
  const control = parameter.enum_values.length > 0
    ? <select className={fieldClass} onChange={(event) => onChange(parameter.key, event.target.value)} value={stringValue}><option value="">{t("mcp.selectValue")}</option>{parameter.enum_values.map((item) => <option key={item} value={item}>{item}</option>)}</select>
    : parameter.kind === "boolean"
      ? <input checked={Boolean(value)} className="h-4 w-4" onChange={(event) => onChange(parameter.key, event.target.checked)} type="checkbox" />
      : parameter.kind === "json"
        ? <textarea className={`${fieldClass} min-h-20 font-mono text-[11px]`} onChange={(event) => onChange(parameter.key, event.target.value)} placeholder={parameter.placeholder ?? "{}"} value={stringValue} />
      : <input className={fieldClass} onChange={(event) => onChange(parameter.key, parameter.kind === "number" || parameter.kind === "integer" ? (event.target.value === "" ? "" : Number(event.target.value)) : event.target.value)} placeholder={parameter.placeholder ?? ""} type={parameter.secret ? "password" : parameter.kind === "number" || parameter.kind === "integer" ? "number" : "text"} value={stringValue} />;
  return <label className="grid gap-1 text-[12px] font-medium text-stone-700"><span>{parameter.label}{parameter.required ? " *" : ""}</span>{parameter.description ? <span className="text-[11px] font-normal text-stone-500">{parameter.description}</span> : null}{control}</label>;
}

function McpEditor({ editor, error, saving, onChange, onClose, onSave }: { editor: { id: string; spec: string; apps: McpAppType[] }; error: string | null; saving: boolean; onChange: (value: { id: string; spec: string; apps: McpAppType[] }) => void; onClose: () => void; onSave: () => void }) {
  const { t } = useI18n();
  return <div className="motion-overlay fixed inset-0 z-40 grid place-items-center bg-stone-950/35 p-4"><div className="w-full max-w-2xl rounded-2xl border border-stone-200 bg-white p-4 shadow-xl"><div className="flex items-start justify-between gap-3"><div><p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("mcp.localConfiguration")}</p><h2 className="mt-1 text-base font-semibold text-stone-950">{editor.id ? t("mcp.editConfiguration") : t("mcp.addConfiguration")}</h2></div><button aria-label={t("mcp.closeEditor")} className="grid h-8 w-8 place-items-center border border-stone-300 text-stone-600 hover:bg-stone-50" onClick={onClose} title={t("mcp.closeEditor")} type="button"><X className="h-4 w-4" /></button></div><div className="mt-4 space-y-3"><label className="grid gap-1 text-[12px] font-medium text-stone-700"><span>{t("mcp.serverId")}</span><input className="border border-stone-300 px-2.5 py-2 outline-none focus:border-blue-400" onChange={(event) => onChange({ ...editor, id: event.target.value })} placeholder="filesystem" value={editor.id} /></label><label className="grid gap-1 text-[12px] font-medium text-stone-700"><span>{t("mcp.canonicalSpec")}</span><textarea className="min-h-48 border border-stone-300 px-2.5 py-2 font-mono text-[12px] outline-none focus:border-blue-400" onChange={(event) => onChange({ ...editor, spec: event.target.value })} spellCheck={false} value={editor.spec} /></label>{error ? <p className="border border-red-200 bg-red-50 px-2.5 py-2 text-[11px] text-red-800" role="alert">{error}</p> : null}<McpAppSelector legend={t("mcp.targetClients")} onChange={(apps) => onChange({ ...editor, apps })} selectedApps={editor.apps} /><div className="flex justify-end gap-2"><button className="border border-stone-300 px-3 py-2 text-[12px] font-semibold text-stone-700 hover:bg-stone-50" onClick={onClose} type="button">{t("mcp.cancel")}</button><button className="inline-flex items-center gap-1.5 bg-stone-900 px-3 py-2 text-[12px] font-semibold text-white hover:bg-stone-800 disabled:opacity-50" disabled={saving || !editor.id.trim() || editor.apps.length === 0} onClick={onSave} type="button"><Check className="h-3.5 w-3.5" />{saving ? t("mcp.saving") : t("mcp.save")}</button></div></div></div></div>;
}
