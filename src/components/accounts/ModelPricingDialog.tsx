import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, Loader2, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { useEffect, useState } from "react";
import { getModelPriceConfigs, getRouteProxyKey, getRouteProxyStatus, saveModelPriceConfigs } from "../../lib/api/client";
import { fetchRouteProxyModels } from "../../lib/routeProxyModels";
import { agentPlatforms } from "../layout/AppLayout";
import { mergeModelPriceRows, parsePriceValue, type ModelPriceConfig, type ModelPriceRow } from "../../lib/modelPricing";

type Props = { open: boolean; onClose: () => void };

function numberValue(value: number | null) { return value === null ? "" : String(value); }

export function ModelPricingDialog({ open, onClose }: Props) {
  const queryClient = useQueryClient();
  const [rows, setRows] = useState<ModelPriceRow[]>([]);
  const [manualModel, setManualModel] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const configsQuery = useQuery({ queryKey: ["model-price-configs"], queryFn: getModelPriceConfigs, enabled: open });
  const proxyQuery = useQuery({ queryKey: ["route-proxy-status"], queryFn: getRouteProxyStatus, enabled: open });
  const modelsQuery = useQuery({
    queryKey: ["all-route-proxy-models", proxyQuery.data?.base_url],
    enabled: open && Boolean(proxyQuery.data?.running && proxyQuery.data.base_url),
    queryFn: async () => {
      const status = proxyQuery.data;
      if (!status?.base_url) return [];
      const results = await Promise.allSettled(agentPlatforms.map(async (platform) => {
        const key = await getRouteProxyKey(platform);
        return fetchRouteProxyModels(status.base_url!, key, platform);
      }));
      return results.flatMap((result) => result.status === "fulfilled" ? result.value : []);
    },
  });
  const saveMutation = useMutation({
    mutationFn: (next: Record<string, ModelPriceConfig>) => saveModelPriceConfigs(next),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["model-price-configs"] });
      await queryClient.invalidateQueries({ queryKey: ["usage-overview"] });
      setMessage("价格配置已保存");
    },
    onError: (error) => setMessage(error instanceof Error ? error.message : "保存失败"),
  });

  useEffect(() => {
    if (!open || !configsQuery.data) return;
    setRows(mergeModelPriceRows(modelsQuery.data ?? [], configsQuery.data));
  }, [open, configsQuery.data, modelsQuery.data]);

  const update = (index: number, field: keyof ModelPriceRow, value: string) => {
    setRows((current) => current.map((row, rowIndex) => rowIndex === index ? { ...row, [field]: field === "model" || field === "display_name" ? value : parsePriceValue(value) } : row));
  };
  const addManual = () => {
    const model = manualModel.trim();
    if (!model || rows.some((row) => row.model.toLowerCase() === model.toLowerCase())) return;
    setRows((current) => [...current, { model, display_name: "", input_per_mtok: null, output_per_mtok: null, cache_read_per_mtok: null, cache_write_per_mtok: null }].sort((a, b) => a.model.localeCompare(b.model)));
    setManualModel("");
  };
  const remove = (model: string) => setRows((current) => current.filter((row) => row.model !== model));
  const save = () => {
    const next: Record<string, ModelPriceConfig> = {};
    for (const row of rows) {
      // Empty rows are model discovery results, not overrides. This lets the
      // user configure only the models they bill and leave the rest untouched.
      if (row.input_per_mtok === null || row.output_per_mtok === null) continue;
      next[row.model] = {
        display_name: row.display_name.trim(), input_per_mtok: row.input_per_mtok, output_per_mtok: row.output_per_mtok,
        cache_read_per_mtok: row.cache_read_per_mtok ?? row.input_per_mtok * 0.1,
        cache_write_per_mtok: row.cache_write_per_mtok ?? row.input_per_mtok * 1.25,
      };
    }
    saveMutation.mutate(next);
  };

  if (!open) return null;
  return <div className="fixed inset-0 z-50 grid place-items-center bg-stone-950/35 p-4 backdrop-blur-sm" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <div aria-label="模型价格配置" className="flex max-h-[min(760px,calc(100vh-2rem))] w-full max-w-6xl flex-col overflow-hidden rounded-2xl border border-stone-200 bg-white shadow-2xl" role="dialog">
      <div className="flex items-start justify-between gap-4 border-b border-stone-100 px-5 py-4">
        <div><p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">费用设置</p><h3 className="mt-0.5 text-lg font-semibold text-stone-950">配置各模型 Token 成本</h3><p className="mt-1 text-[12px] text-stone-500">价格单位为 USD / 每百万 Token。模型列表来自本地路由代理，未启动代理时也可以手动添加。</p></div>
        <button aria-label="关闭模型价格配置" className="rounded-xl border border-stone-200 p-1.5 text-stone-500 hover:bg-stone-50" onClick={onClose} type="button"><X className="h-4 w-4" /></button>
      </div>
      <div className="flex flex-wrap items-center gap-2 border-b border-stone-100 bg-stone-50/60 px-5 py-3">
        <div className="flex min-w-[260px] flex-1 items-center gap-2"><input aria-label="手动添加模型" className="min-w-0 flex-1 rounded-lg border border-stone-200 bg-white px-3 py-2 text-[12px] outline-none focus:border-blue-400" onChange={(e) => setManualModel(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") addManual(); }} placeholder="输入模型 ID 手动添加" value={manualModel} /><button className="inline-flex items-center gap-1 rounded-lg bg-blue-600 px-3 py-2 text-[12px] font-semibold text-white hover:bg-blue-700" onClick={addManual} type="button"><Plus className="h-3.5 w-3.5" />添加</button></div>
        <button className="inline-flex items-center gap-1 rounded-lg border border-stone-200 bg-white px-3 py-2 text-[12px] font-semibold text-stone-700 hover:bg-stone-50" onClick={() => { void modelsQuery.refetch(); }} type="button"><RefreshCw className={`h-3.5 w-3.5 ${modelsQuery.isFetching ? "animate-spin" : ""}`} />刷新模型</button>
        {proxyQuery.data?.running ? <span className="text-[11px] text-emerald-700">已读取本地代理模型</span> : <span className="text-[11px] text-amber-700">代理未启动</span>}
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-5 py-3">
        {configsQuery.isLoading ? <div className="flex items-center gap-2 py-10 text-sm text-stone-500"><Loader2 className="h-4 w-4 animate-spin" />正在读取价格配置…</div> : <table className="w-full min-w-[850px] border-collapse text-left text-[12px]"><thead className="sticky top-0 z-10 bg-white text-stone-500"><tr className="border-b border-stone-200"><th className="px-2 py-2 font-semibold">模型</th><th className="px-2 py-2 font-semibold">显示名称</th><th className="px-2 py-2 font-semibold">输入 / 百万</th><th className="px-2 py-2 font-semibold">输出 / 百万</th><th className="px-2 py-2 font-semibold">缓存命中 / 百万</th><th className="px-2 py-2 font-semibold">缓存创建 / 百万</th><th className="w-12 px-2 py-2" /></tr></thead><tbody>{rows.map((row, index) => <tr className="border-b border-stone-100" key={row.model}><td className="max-w-[240px] px-2 py-2 font-mono text-stone-700" title={row.model}>{row.model}</td><td className="px-2 py-2"><input aria-label={`${row.model} 显示名称`} className="w-full rounded-md border border-stone-200 px-2 py-1.5 outline-none focus:border-blue-400" onChange={(e) => update(index, "display_name", e.target.value)} value={row.display_name} /></td>{(["input_per_mtok", "output_per_mtok", "cache_read_per_mtok", "cache_write_per_mtok"] as const).map((field) => <td className="px-2 py-2" key={field}><input aria-label={`${row.model} ${field}`} className="w-28 rounded-md border border-stone-200 px-2 py-1.5 font-mono outline-none focus:border-blue-400" min="0" onChange={(e) => update(index, field, e.target.value)} step="any" type="number" value={numberValue(row[field])} /></td>)}<td className="px-2 py-2"><button aria-label={`删除 ${row.model}`} className="rounded-md p-1.5 text-stone-400 hover:bg-red-50 hover:text-red-600" onClick={() => remove(row.model)} type="button"><Trash2 className="h-4 w-4" /></button></td></tr>)}</tbody></table>}
        {!configsQuery.isLoading && rows.length === 0 ? <p className="py-10 text-center text-sm text-stone-500">暂无模型。请启动本地路由代理或手动添加模型。</p> : null}
      </div>
      <div className="flex items-center justify-between gap-3 border-t border-stone-100 px-5 py-3"><span className="text-[12px] text-stone-500">{message ?? `${rows.length} 个模型 · 缓存价格可留空，默认按输入价格的 0.1x / 1.25x 计算`}</span><div className="flex gap-2"><button className="rounded-lg border border-stone-200 px-3 py-2 text-[12px] font-semibold text-stone-700 hover:bg-stone-50" onClick={onClose} type="button">取消</button><button className="inline-flex items-center gap-1 rounded-lg bg-stone-900 px-3 py-2 text-[12px] font-semibold text-white hover:bg-stone-800 disabled:opacity-50" disabled={saveMutation.isPending} onClick={save} type="button">{saveMutation.isPending ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Check className="h-3.5 w-3.5" />}保存配置</button></div></div>
    </div>
  </div>;
}

