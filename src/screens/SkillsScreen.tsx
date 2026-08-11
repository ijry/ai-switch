import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, FilePenLine, FolderOpen, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  skillsDelete,
  skillsList,
  skillsListAgents,
  skillsRead,
  skillsSave,
} from "../lib/api/client";
import type { SkillAgentType, SkillItem, SkillLayout, SkillScope } from "../lib/api/types";
import { DEFAULT_SKILL_CONTENT } from "../components/skills/catalog";

export function SkillsScreen() {
  const queryClient = useQueryClient();
  const [agentType, setAgentType] = useState<SkillAgentType>("codex");
  const [scope, setScope] = useState<SkillScope>("global");
  const [workspacePath, setWorkspacePath] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [draftId, setDraftId] = useState("");
  const [layout, setLayout] = useState<SkillLayout>("skill_directory");
  const agentsQuery = useQuery({ queryKey: ["skills-agents"], queryFn: skillsListAgents });
  const listQuery = useQuery({
    queryKey: ["skills", agentType, scope, workspacePath],
    queryFn: () => skillsList({ agentType, scope, workspacePath: scope === "project" ? workspacePath : null }),
    enabled: scope === "global" || Boolean(workspacePath.trim()),
  });
  const readQuery = useQuery({
    queryKey: ["skill", agentType, scope, workspacePath, selectedId],
    queryFn: () => skillsRead({ agentType, scope, skillId: selectedId ?? "", workspacePath: scope === "project" ? workspacePath : null }),
    enabled: Boolean(selectedId) && !creating,
  });
  const saveMutation = useMutation({
    mutationFn: () => skillsSave({ agentType, scope, skillId: draftId.trim(), content: draft, layout, workspacePath: scope === "project" ? workspacePath : null }),
    onSuccess: async (saved) => {
      setSelectedId(saved.id);
      setCreating(false);
      setEditing(false);
      await Promise.all([queryClient.invalidateQueries({ queryKey: ["skills"] }), queryClient.invalidateQueries({ queryKey: ["skill"] })]);
    },
  });
  const deleteMutation = useMutation({
    mutationFn: () => skillsDelete({ agentType, scope, skillId: selectedId ?? "", workspacePath: scope === "project" ? workspacePath : null }),
    onSuccess: async () => {
      setSelectedId(null);
      setCreating(false);
      setEditing(false);
      await queryClient.invalidateQueries({ queryKey: ["skills"] });
    },
  });

  useEffect(() => {
    if (!readQuery.data || editing) return;
    setDraft(readQuery.data.content);
    setDraftId(readQuery.data.skill.id);
    setLayout(readQuery.data.skill.layout);
  }, [editing, readQuery.data]);
  useEffect(() => {
    const first = listQuery.data?.skills[0];
    if (creating) return;
    if (!selectedId || !listQuery.data?.skills.some((item) => item.id === selectedId)) setSelectedId(first?.id ?? null);
  }, [creating, listQuery.data, selectedId]);

  const selected = useMemo(
    () => creating
      ? { id: draftId || "new-skill", name: draftId || "New skill", scope, layout, path: "Not saved", description: null, read_only: false }
      : listQuery.data?.skills.find((item) => item.id === selectedId) ?? readQuery.data?.skill,
    [creating, draftId, layout, listQuery.data?.skills, readQuery.data?.skill, scope, selectedId],
  );
  const error = listQuery.error ?? readQuery.error ?? saveMutation.error ?? deleteMutation.error;
  const agentOptions = agentsQuery.data ?? [];

  const newSkill = () => {
    setSelectedId(null);
    setCreating(true);
    setEditing(true);
    setDraftId("");
    setDraft(DEFAULT_SKILL_CONTENT);
    setLayout("skill_directory");
  };

  return (
    <section className="space-y-3">
      <header className="flex flex-wrap items-end justify-between gap-3 border-b border-stone-200 px-1 pb-3">
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Instructions</p>
          <h1 className="mt-0.5 text-lg font-semibold text-stone-950">Skills</h1>
          <p className="mt-1 text-[12px] text-stone-500">Browse, edit and share agent Skills from global or project scope.</p>
        </div>
        <button className="inline-flex items-center gap-1.5 border border-stone-300 bg-white px-2.5 py-1.5 text-[12px] font-semibold text-stone-800 hover:bg-stone-50" onClick={() => queryClient.invalidateQueries({ queryKey: ["skills"] })} title="Refresh Skills" type="button"><RefreshCw className="h-3.5 w-3.5" /> Refresh</button>
      </header>

      <div className="grid gap-2 border-b border-stone-200 pb-3 lg:grid-cols-[minmax(180px,240px)_auto_minmax(220px,1fr)_auto] lg:items-end">
        <label className="grid gap-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400"><span>Agent</span><select className="border border-stone-300 bg-white px-2.5 py-2 text-[12px] font-medium normal-case tracking-normal text-stone-800" onChange={(event) => { setAgentType(event.target.value as SkillAgentType); setSelectedId(null); setCreating(false); }} value={agentType}>{agentOptions.map((agent) => <option key={agent.agent_type} value={agent.agent_type}>{agent.display_name}</option>)}</select></label>
        <div className="inline-flex border border-stone-200 bg-white p-0.5"><button className={`px-3 py-1.5 text-[12px] font-semibold ${scope === "global" ? "bg-stone-900 text-white" : "text-stone-600 hover:bg-stone-50"}`} onClick={() => { setScope("global"); setSelectedId(null); setCreating(false); }} type="button">Global</button><button className={`px-3 py-1.5 text-[12px] font-semibold ${scope === "project" ? "bg-stone-900 text-white" : "text-stone-600 hover:bg-stone-50"}`} onClick={() => { setScope("project"); setSelectedId(null); setCreating(false); }} type="button">Project</button></div>
        {scope === "project" ? <label className="grid gap-1 text-[11px] font-semibold uppercase tracking-wide text-stone-400"><span>Project directory</span><div className="flex items-center gap-2 border border-stone-300 bg-white px-2.5 py-1.5"><FolderOpen className="h-3.5 w-3.5 text-stone-400" /><input className="min-w-0 flex-1 text-[12px] normal-case tracking-normal outline-none" onChange={(event) => { setWorkspacePath(event.target.value); setSelectedId(null); setCreating(false); }} placeholder="C:\\Projects\\my-app" value={workspacePath} /></div></label> : <div className="hidden lg:block" />}
        <button className="inline-flex items-center justify-center gap-1.5 bg-stone-900 px-2.5 py-2 text-[12px] font-semibold text-white hover:bg-stone-800" onClick={newSkill} type="button"><Plus className="h-3.5 w-3.5" /> New Skill</button>
      </div>

      <div className="grid min-h-[420px] gap-3 lg:grid-cols-[280px_minmax(0,1fr)]">
        <aside className="border border-stone-200 bg-white p-2 shadow-sm"><div className="flex items-center justify-between px-2 py-1"><p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Available Skills</p><span className="font-mono text-[11px] text-stone-400">{listQuery.data?.skills.length ?? 0}</span></div>{scope === "project" && !workspacePath.trim() ? <p className="px-2 py-5 text-[12px] text-stone-500">Enter a project directory to scan Skills.</p> : listQuery.isLoading ? <p className="px-2 py-5 text-[12px] text-stone-500">Scanning Skill directories...</p> : <div className="mt-1 space-y-0.5">{(listQuery.data?.skills ?? []).map((item) => <SkillListRow item={item} selected={item.id === selectedId} onClick={() => { setSelectedId(item.id); setCreating(false); setEditing(false); }} key={item.id} />)}</div>}{listQuery.data?.locations.length ? <div className="mt-3 border-t border-stone-100 px-2 pt-2">{listQuery.data.locations.map((location) => <p className="truncate font-mono text-[10px] text-stone-400" key={location.path} title={location.path}>{location.exists ? "●" : "○"} {location.path}</p>)}</div> : null}</aside>
        <main className="border border-stone-200 bg-white p-3 shadow-sm">{selected ? <SkillEditor item={selected} draft={draft} draftId={draftId} editing={editing || creating} layout={layout} saving={saveMutation.isPending} onEdit={() => { setEditing(true); if (readQuery.data) { setDraft(readQuery.data.content); setDraftId(readQuery.data.skill.id); setLayout(readQuery.data.skill.layout); } }} onDraftChange={setDraft} onIdChange={setDraftId} onLayoutChange={setLayout} onCancel={() => { setEditing(false); if (creating) { setCreating(false); setSelectedId(null); } }} onSave={() => saveMutation.mutate()} onDelete={() => { if (window.confirm(`Delete ${selected.id}?`)) deleteMutation.mutate(); }} /> : <div className="grid h-full min-h-[360px] place-items-center text-center text-[13px] text-stone-500"><div><FilePenLine className="mx-auto h-8 w-8 text-stone-300" /><p className="mt-2">Select a Skill to preview it, or create a new one.</p></div></div>}</main>
      </div>
      {error ? <p className="border border-red-200 bg-red-50 px-3 py-2 text-[12px] text-red-800" role="alert">{error instanceof Error ? error.message : "The Skill operation failed."}</p> : null}
    </section>
  );
}

function SkillListRow({ item, selected, onClick }: { item: SkillItem; selected: boolean; onClick: () => void }) {
  return <button aria-current={selected ? "true" : undefined} className={`w-full border px-2.5 py-2 text-left ${selected ? "border-stone-300 bg-stone-100" : "border-transparent hover:border-stone-200 hover:bg-stone-50"}`} onClick={onClick} type="button"><div className="flex items-center justify-between gap-2"><span className="truncate text-[12px] font-semibold text-stone-900">{item.name}</span>{item.read_only ? <span className="shrink-0 text-[10px] text-stone-400">built-in</span> : null}</div><p className="mt-0.5 truncate font-mono text-[10px] text-stone-500">{item.id}</p>{item.description ? <p className="mt-1 line-clamp-1 text-[11px] text-stone-500">{item.description}</p> : null}</button>;
}

function SkillEditor({ item, draft, draftId, editing, layout, saving, onEdit, onDraftChange, onIdChange, onLayoutChange, onCancel, onSave, onDelete }: { item: SkillItem; draft: string; draftId: string; editing: boolean; layout: SkillLayout; saving: boolean; onEdit: () => void; onDraftChange: (value: string) => void; onIdChange: (value: string) => void; onLayoutChange: (value: SkillLayout) => void; onCancel: () => void; onSave: () => void; onDelete: () => void }) {
  return <div className="flex h-full min-h-[360px] flex-col"><div className="flex flex-wrap items-start justify-between gap-3 border-b border-stone-200 pb-3"><div className="min-w-0"><div className="flex items-center gap-2"><h2 className="truncate text-[15px] font-semibold text-stone-950">{item.name}</h2>{item.read_only ? <span className="border border-amber-200 bg-amber-50 px-1.5 py-0.5 text-[10px] font-semibold text-amber-800">Read only</span> : null}</div><p className="mt-1 break-all font-mono text-[11px] text-stone-500">{item.path}</p></div><div className="flex gap-1">{!editing ? <button aria-label="Edit Skill" className="grid h-8 w-8 place-items-center border border-stone-300 text-stone-700 hover:bg-stone-50 disabled:opacity-40" disabled={item.read_only} onClick={onEdit} title={item.read_only ? "Built-in Skill is read-only" : "Edit Skill"} type="button"><FilePenLine className="h-3.5 w-3.5" /></button> : null}{!item.read_only ? <button aria-label="Delete Skill" className="grid h-8 w-8 place-items-center border border-red-200 text-red-700 hover:bg-red-50" onClick={onDelete} title="Delete Skill" type="button"><Trash2 className="h-3.5 w-3.5" /></button> : null}</div></div>{editing ? <div className="mt-3 flex min-h-0 flex-1 flex-col gap-3"><div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_180px]"><label className="grid gap-1 text-[11px] font-semibold text-stone-600">Skill id<input className="border border-stone-300 px-2 py-1.5 font-mono text-[12px] font-normal outline-none focus:border-blue-400" onChange={(event) => onIdChange(event.target.value)} value={draftId} /></label><label className="grid gap-1 text-[11px] font-semibold text-stone-600">Layout<select className="border border-stone-300 bg-white px-2 py-1.5 text-[12px] font-normal" onChange={(event) => onLayoutChange(event.target.value as SkillLayout)} value={layout}><option value="skill_directory">Skill directory</option><option value="markdown_file">Markdown file</option></select></label></div><textarea className="min-h-[260px] flex-1 resize-y border border-stone-300 p-2 font-mono text-[12px] leading-5 outline-none focus:border-blue-400" onChange={(event) => onDraftChange(event.target.value)} spellCheck={false} value={draft} /><div className="flex justify-end gap-2"><button className="border border-stone-300 px-3 py-2 text-[12px] font-semibold text-stone-700 hover:bg-stone-50" onClick={onCancel} type="button">Cancel</button><button className="inline-flex items-center gap-1.5 bg-stone-900 px-3 py-2 text-[12px] font-semibold text-white hover:bg-stone-800 disabled:opacity-50" disabled={saving || !draftId.trim()} onClick={onSave} type="button"><Check className="h-3.5 w-3.5" />{saving ? "Saving..." : "Save"}</button></div></div> : <pre className="mt-3 min-h-0 flex-1 overflow-auto border border-stone-200 bg-stone-50 p-3 font-mono text-[12px] leading-5 text-stone-700">{draft || "Loading Skill content..."}</pre>}</div>;
}
