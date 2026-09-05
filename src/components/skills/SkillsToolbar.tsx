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

const SCOPES: SkillScope[] = ["global", "project"];

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
      {/* Both actions sit in the header, so the row below holds filters only. With
          the button in that row it was a fifth cell in a five-column grid whose
          minimums already overflowed the pane, and "新建技能" came out one glyph
          per line. Every control here is `whitespace-nowrap` for the same reason:
          a squeezed flex item breaks CJK text between characters, not words. */}
      <header className="flex flex-wrap items-end justify-between gap-3 rounded-2xl border border-stone-200 bg-white/65 px-4 py-3 shadow-sm">
        <div className="min-w-0">
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">{t("skills.kicker")}</p>
          <h1 className="mt-0.5 text-lg font-semibold text-stone-950">{t("skills.title")}</h1>
          <p className="mt-1 text-[12px] text-stone-500">{t("skills.subtitle")}</p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-xl bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-800 ring-1 ring-stone-300 motion-control hover:bg-stone-50" onClick={props.onRefresh} title={t("skills.refreshTitle")} type="button">
            <RefreshCw className="h-3.5 w-3.5" /> {t("skills.refresh")}
          </button>
          <button className="inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-xl bg-stone-900 px-2.5 py-1.5 text-[12px] font-semibold text-white motion-control hover:bg-stone-800" onClick={props.onNew} type="button">
            <Plus className="h-3.5 w-3.5" /> {t("skills.new")}
          </button>
        </div>
      </header>
      <div className="flex flex-wrap items-end gap-2 rounded-2xl border border-stone-200 bg-white/55 p-3 shadow-sm">
        <label className="grid min-w-[168px] flex-1 gap-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400">
          <span className="truncate">{t("skills.agent")}</span>
          <select className="w-full rounded-xl border border-stone-300 bg-white px-2.5 py-2 text-[12px] font-medium normal-case tracking-normal text-stone-800 motion-control" onChange={(event) => props.onAgentChange(event.target.value as SkillAgentType)} value={props.agentType}>
            {props.agents.map((agent) => <option key={agent.agent_type} value={agent.agent_type}>{agent.display_name}</option>)}
          </select>
        </label>
        <div className="flex shrink-0 rounded-xl bg-white p-0.5 ring-1 ring-stone-200">
          {SCOPES.map((value) => (
            <button className={`whitespace-nowrap rounded-lg px-3 py-1.5 text-[12px] font-semibold motion-control ${props.scope === value ? "bg-stone-900 text-white" : "text-stone-600 hover:bg-stone-50"}`} key={value} onClick={() => props.onScopeChange(value)} type="button">
              {value === "global" ? t("skills.global") : t("skills.project")}
            </button>
          ))}
        </div>
        {props.scope === "project" ? (
          <label className="grid min-w-[220px] flex-[2] gap-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400">
            <span className="truncate">{t("skills.projectDirectory")}</span>
            <div className="flex items-center gap-2 rounded-xl border border-stone-300 bg-white px-2.5 py-1.5">
              <FolderOpen className="h-3.5 w-3.5 shrink-0 text-stone-400" />
              <input className="min-w-0 flex-1 text-[12px] normal-case tracking-normal outline-none" onChange={(event) => props.onWorkspaceChange(event.target.value)} placeholder="C:\\Projects\\my-app" value={props.workspacePath} />
              <button aria-label={t("skills.chooseProjectDirectory")} className="grid h-6 w-6 shrink-0 place-items-center rounded-lg text-stone-500 motion-control hover:bg-stone-100 disabled:opacity-40" disabled={!props.desktop} onClick={() => void chooseWorkspace()} title={t("skills.chooseProjectDirectory")} type="button"><FolderOpen className="h-3.5 w-3.5" /></button>
            </div>
          </label>
        ) : null}
        <label className="flex min-w-[184px] flex-[2] items-center gap-2 self-end rounded-xl border border-stone-300 bg-white px-2.5 py-1.5">
          <Search className="h-3.5 w-3.5 shrink-0 text-stone-400" />
          <input aria-label={t("skills.filter")} className="min-w-0 flex-1 text-[12px] outline-none" onChange={(event) => props.onFilterChange(event.target.value)} placeholder={t("skills.search")} value={props.filterText} />
        </label>
      </div>
    </>
  );
}
