import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus, RefreshCw, Search } from "lucide-react";
import { useI18n } from "../../lib/i18n";
import type { SkillAgentInfo, SkillAgentType, SkillScope } from "../../lib/api/types";

type SkillsToolbarProps = {
  agents: SkillAgentInfo[];
  agentType: SkillAgentType;
  scope: SkillScope;
  workspacePath: string;
  filterText: string;
  desktop: boolean;
  onAgentChange: (agent: SkillAgentType) => void;
  onScopeChange: (scope: SkillScope) => void;
  onWorkspaceChange: (path: string) => void;
  onFilterChange: (text: string) => void;
  onNew: () => void;
  onRefresh: () => void;
  onPickerError: (message: string | null) => void;
};

export function SkillsToolbar(props: SkillsToolbarProps) {
  const { t } = useI18n();
  const chooseWorkspace = async () => {
    if (!props.desktop) return;
    props.onPickerError(null);
    try {
      const selected = await open({ directory: true, multiple: false, title: t("skills.chooseProjectDirectory") });
      if (typeof selected === "string") props.onWorkspaceChange(selected);
    } catch (error) {
      props.onPickerError(error instanceof Error ? error.message : t("errors.operationFailed"));
    }
  };

  return (
    <>
      <header className="flex flex-wrap items-end justify-between gap-3 border-b border-stone-200 px-1 pb-3">
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("skills.kicker")}</p>
          <h1 className="mt-0.5 text-lg font-semibold text-stone-950">{t("skills.title")}</h1>
          <p className="mt-1 text-[12px] text-stone-500">{t("skills.subtitle")}</p>
        </div>
        <button className="inline-flex items-center gap-1.5 border border-stone-300 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-800 hover:bg-stone-50" onClick={props.onRefresh} title={t("skills.refreshTitle")} type="button">
          <RefreshCw className="h-3.5 w-3.5" /> {t("skills.refresh")}
        </button>
      </header>
      <div className="grid gap-2 border-b border-stone-200 pb-3 lg:grid-cols-[minmax(180px,240px)_auto_minmax(180px,240px)_minmax(220px,1fr)_auto] lg:items-end">
        <label className="grid gap-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400">
          <span>{t("skills.agent")}</span>
          <select className="border border-stone-300 bg-white px-2.5 py-2 text-[12px] font-medium normal-case tracking-normal text-stone-800" onChange={(event) => props.onAgentChange(event.target.value as SkillAgentType)} value={props.agentType}>
            {props.agents.map((agent) => <option key={agent.agent_type} value={agent.agent_type}>{agent.display_name}</option>)}
          </select>
        </label>
        <div className="inline-flex border border-stone-200 bg-white p-0.5">
          <button className={`px-3 py-1.5 text-[12px] font-semibold ${props.scope === "global" ? "bg-stone-900 text-white" : "text-stone-600 hover:bg-stone-50"}`} onClick={() => props.onScopeChange("global")} type="button">{t("skills.global")}</button>
          <button className={`px-3 py-1.5 text-[12px] font-semibold ${props.scope === "project" ? "bg-stone-900 text-white" : "text-stone-600 hover:bg-stone-50"}`} onClick={() => props.onScopeChange("project")} type="button">{t("skills.project")}</button>
        </div>
        {props.scope === "project" ? (
          <label className="grid gap-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400">
            <span>{t("skills.projectDirectory")}</span>
            <div className="flex items-center gap-2 border border-stone-300 bg-white px-2.5 py-1.5">
              <FolderOpen className="h-3.5 w-3.5 text-stone-400" />
              <input className="min-w-0 flex-1 text-[12px] normal-case tracking-normal outline-none" onChange={(event) => props.onWorkspaceChange(event.target.value)} placeholder="C:\\Projects\\my-app" value={props.workspacePath} />
              <button aria-label={t("skills.chooseProjectDirectory")} className="grid h-6 w-6 shrink-0 place-items-center text-stone-500 hover:bg-stone-100 disabled:opacity-40" disabled={!props.desktop} onClick={() => void chooseWorkspace()} title={t("skills.chooseProjectDirectory")} type="button"><FolderOpen className="h-3.5 w-3.5" /></button>
            </div>
          </label>
        ) : <div className="hidden lg:block" />}
        <label className="flex min-w-[180px] items-center gap-2 border border-stone-300 bg-white px-2.5 py-1.5">
          <Search className="h-3.5 w-3.5 text-stone-400" />
          <input aria-label={t("skills.filter")} className="min-w-0 flex-1 text-[12px] outline-none" onChange={(event) => props.onFilterChange(event.target.value)} placeholder={t("skills.search")} value={props.filterText} />
        </label>
        <button className="inline-flex items-center justify-center gap-1.5 bg-stone-900 px-2.5 py-2 text-[12px] font-semibold text-white hover:bg-stone-800" onClick={props.onNew} type="button"><Plus className="h-3.5 w-3.5" /> {t("skills.new")}</button>
      </div>
    </>
  );
}
