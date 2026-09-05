import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, FilePenLine, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  skillsDelete,
  skillsInstallPackage,
  skillsList,
  skillsListAgents,
  skillsListPackages,
  skillsRead,
  skillsReadPackage,
  skillsSave,
  skillsUninstallPackage,
} from "../lib/api/client";
import { ApiClientError } from "../lib/api/errors";
import { apiErrorMessageKey } from "../lib/api/errorMessages";
import { useI18n } from "../lib/i18n";
import type { SkillAgentType, SkillItem, SkillLayout, SkillScope } from "../lib/api/types";
import { isDesktop } from "../lib/transport";
import {
  DEFAULT_SKILL_CONTENT,
  skillDisplayCopy,
  skillPackageNameKey,
  skillSearchHaystack,
} from "../components/skills/catalog";
import { SkillsList } from "../components/skills/SkillsList";
import { SkillPackageDetail } from "../components/skills/SkillPackageDetail";
import { SkillPackagesList } from "../components/skills/SkillPackagesList";
import { SkillsTabs, type SkillsView } from "../components/skills/SkillsTabs";
import { SkillsToolbar } from "../components/skills/SkillsToolbar";

export function SkillsScreen() {
  const { t } = useI18n();
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
  const [filterText, setFilterText] = useState("");
  const [view, setView] = useState<SkillsView>("skills");
  const [selectedPackageId, setSelectedPackageId] = useState<string | null>(null);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const agentsQuery = useQuery({ queryKey: ["skills-agents"], queryFn: skillsListAgents });
  const listQuery = useQuery({
    queryKey: ["skills", agentType, scope, workspacePath],
    queryFn: () => skillsList({ agentType, scope, workspacePath: scope === "project" ? workspacePath : null }),
    enabled: scope === "global" || Boolean(workspacePath.trim()),
  });
  const packagesQuery = useQuery({
    queryKey: ["skills-packages", agentType, scope, workspacePath],
    queryFn: () =>
      skillsListPackages({
        agentType,
        scope,
        workspacePath: scope === "project" ? workspacePath : null,
      }),
    enabled: view === "packages" && (scope === "global" || Boolean(workspacePath.trim())),
  });
  const packageDetailQuery = useQuery({
    queryKey: ["skills-package", agentType, scope, workspacePath, selectedPackageId],
    queryFn: () =>
      skillsReadPackage({
        packageId: selectedPackageId ?? "",
        agentType,
        scope,
        workspacePath: scope === "project" ? workspacePath : null,
      }),
    enabled:
      view === "packages" &&
      Boolean(selectedPackageId) &&
      (scope === "global" || Boolean(workspacePath.trim())),
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
  /// Installing or uninstalling a pack member changes the Skills list, the pack's
  /// install counters and the open pack detail at once, so all three refetch.
  const invalidateSkillQueries = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["skills"] }),
      queryClient.invalidateQueries({ queryKey: ["skill"] }),
      queryClient.invalidateQueries({ queryKey: ["skills-packages"] }),
      queryClient.invalidateQueries({ queryKey: ["skills-package"] }),
    ]);
  };
  const installPackageMutation = useMutation({
    mutationFn: (skillIds: string[] | null) =>
      skillsInstallPackage({
        packageId: selectedPackageId ?? "",
        agentType,
        scope,
        workspacePath: scope === "project" ? workspacePath : null,
        skillIds,
      }),
    onSuccess: invalidateSkillQueries,
  });
  const uninstallPackageMutation = useMutation({
    mutationFn: (skillIds: string[] | null) =>
      skillsUninstallPackage({
        packageId: selectedPackageId ?? "",
        agentType,
        scope,
        workspacePath: scope === "project" ? workspacePath : null,
        skillIds,
      }),
    onSuccess: invalidateSkillQueries,
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
  useEffect(() => {
    const first = packagesQuery.data?.packages[0];
    if (!selectedPackageId || !packagesQuery.data?.packages.some((item) => item.id === selectedPackageId)) {
      setSelectedPackageId(first?.id ?? null);
    }
  }, [packagesQuery.data, selectedPackageId]);

  const selected = useMemo(
    () => creating
      ? { id: draftId || "new-skill", name: draftId || t("skills.new"), scope, layout, path: t("skills.notSaved"), description: null, read_only: false }
      : listQuery.data?.skills.find((item) => item.id === selectedId) ?? readQuery.data?.skill,
    [creating, draftId, layout, listQuery.data?.skills, readQuery.data?.skill, scope, selectedId, t],
  );
  const visibleSkills = useMemo(() => {
    const needle = filterText.trim().toLowerCase();
    if (!needle) return listQuery.data?.skills ?? [];
    return (listQuery.data?.skills ?? []).filter((item) => skillSearchHaystack(item).includes(needle));
  }, [filterText, listQuery.data?.skills]);
  const error =
    listQuery.error ??
    readQuery.error ??
    packagesQuery.error ??
    packageDetailQuery.error ??
    saveMutation.error ??
    deleteMutation.error ??
    installPackageMutation.error ??
    uninstallPackageMutation.error;
  const agentOptions = agentsQuery.data ?? [];
  const errorMessage = error instanceof ApiClientError
    ? t(apiErrorMessageKey(error.code))
    : t("skills.operationFailed");
  const packageNameKey = selectedPackageId ? skillPackageNameKey(selectedPackageId) : undefined;
  const packageLabel = packageNameKey
    ? t(packageNameKey)
    : packageDetailQuery.data?.package.name ?? selectedPackageId ?? "";
  // `variables` is the id list the pending call was given: `null` is the whole
  // pack, a single id is one member's row spinner.
  const memberBusyId =
    (installPackageMutation.isPending ? installPackageMutation.variables?.[0] : undefined) ??
    (uninstallPackageMutation.isPending ? uninstallPackageMutation.variables?.[0] : undefined) ??
    null;
  const uninstallMembers = (skillIds: string[] | null) => {
    const confirmed = skillIds?.length === 1
      ? window.confirm(t("skills.packageUninstallMemberConfirm", { id: skillIds[0] }))
      : window.confirm(t("skills.packageUninstallConfirm", {
          count: packageDetailQuery.data?.package.installed_count ?? 0,
          name: packageLabel,
        }));
    if (confirmed) uninstallPackageMutation.mutate(skillIds);
  };

  const newSkill = () => {
    setView("skills");
    setSelectedId(null);
    setCreating(true);
    setEditing(true);
    setDraftId("");
    setDraft(DEFAULT_SKILL_CONTENT);
    setLayout("skill_directory");
  };

  return (
    <section className="skills-screen space-y-3 rounded-2xl bg-white/20 p-1 sm:p-2">
      <SkillsToolbar
        agentType={agentType}
        agents={agentOptions}
        desktop={isDesktop()}
        filterText={filterText}
        onAgentChange={(nextAgent) => { setAgentType(nextAgent); setSelectedId(null); setSelectedPackageId(null); setCreating(false); }}
        onFilterChange={setFilterText}
        onNew={newSkill}
        onPickerError={setPickerError}
        onRefresh={() => {
          void queryClient.invalidateQueries({ queryKey: ["skills"] });
          void queryClient.invalidateQueries({ queryKey: ["skills-packages"] });
        }}
        onScopeChange={(nextScope) => { setScope(nextScope); setSelectedId(null); setSelectedPackageId(null); setCreating(false); }}
        onWorkspaceChange={(path) => { setPickerError(null); setWorkspacePath(path); setSelectedId(null); setSelectedPackageId(null); setCreating(false); }}
        scope={scope}
        workspacePath={workspacePath}
      />

      <div className="rounded-2xl border border-stone-200 bg-white/70 px-2 shadow-sm"><SkillsTabs value={view} onChange={setView} /></div>
      {view === "packages" ? (
        <div className="grid min-h-[420px] min-w-0 gap-3 rounded-2xl lg:grid-cols-[minmax(200px,260px)_minmax(0,1fr)]">
          <SkillPackagesList
            loading={packagesQuery.isLoading}
            onSelect={setSelectedPackageId}
            packages={packagesQuery.data?.packages ?? []}
            selectedId={selectedPackageId}
            warnings={packagesQuery.data?.warnings ?? []}
          />
          <SkillPackageDetail
            busySkillId={memberBusyId}
            detail={packageDetailQuery.data ?? null}
            installing={installPackageMutation.isPending && !installPackageMutation.variables}
            loading={packageDetailQuery.isLoading}
            onInstallMember={(skillId) => installPackageMutation.mutate([skillId])}
            onInstallMissing={() => installPackageMutation.mutate(null)}
            onSelectSkill={(item) => {
              setView("skills");
              setSelectedId(item.id);
              setCreating(false);
              setEditing(false);
            }}
            onUninstallAll={() => uninstallMembers(null)}
            onUninstallMember={(skillId) => uninstallMembers([skillId])}
            uninstalling={uninstallPackageMutation.isPending && !uninstallPackageMutation.variables}
          />
        </div>
      ) : (
        <div className="grid min-h-[420px] min-w-0 gap-3 lg:grid-cols-[minmax(240px,280px)_minmax(0,1fr)]">
          <SkillsList
            filterText={filterText}
            items={visibleSkills}
            loading={listQuery.isLoading}
            locations={listQuery.data?.locations ?? []}
            onSelect={(item) => { setSelectedId(item.id); setCreating(false); setEditing(false); }}
            projectMissing={scope === "project" && !workspacePath.trim()}
            selectedId={selectedId}
            total={listQuery.data?.skills.length ?? 0}
          />
          <main className="min-h-0 min-w-0 overflow-hidden rounded-2xl border border-stone-200 bg-white p-3 shadow-sm">
            {selected ? (
              <SkillEditor
                creating={creating}
                draft={draft}
                draftId={draftId}
                editing={editing || creating}
                item={selected}
                layout={layout}
                onCancel={() => { setEditing(false); if (creating) { setCreating(false); setSelectedId(null); } }}
                onDelete={() => { if (window.confirm(t("skills.deleteConfirm", { id: selected.id }))) deleteMutation.mutate(); }}
                onDraftChange={setDraft}
                onEdit={() => { setEditing(true); if (readQuery.data) { setDraft(readQuery.data.content); setDraftId(readQuery.data.skill.id); setLayout(readQuery.data.skill.layout); } }}
                onIdChange={setDraftId}
                onLayoutChange={setLayout}
                onSave={() => saveMutation.mutate()}
                saving={saveMutation.isPending}
              />
            ) : (
              <div className="grid h-full min-h-[360px] place-items-center text-center text-[13px] text-stone-500">
                <div>
                  <FilePenLine className="mx-auto h-8 w-8 text-stone-300" />
                  <p className="mt-2">{t("skills.emptySelection")}</p>
                </div>
              </div>
            )}
          </main>
        </div>
      )}
      {error || pickerError ? <p className="rounded-xl bg-red-50 px-3 py-2 text-[12px] text-red-800 ring-1 ring-red-200" role="alert">{pickerError ?? errorMessage}</p> : null}
    </section>
  );
}

function SkillEditor({
  creating,
  item,
  draft,
  draftId,
  editing,
  layout,
  saving,
  onEdit,
  onDraftChange,
  onIdChange,
  onLayoutChange,
  onCancel,
  onSave,
  onDelete,
}: {
  creating: boolean;
  item: SkillItem;
  draft: string;
  draftId: string;
  editing: boolean;
  layout: SkillLayout;
  saving: boolean;
  onEdit: () => void;
  onDraftChange: (value: string) => void;
  onIdChange: (value: string) => void;
  onLayoutChange: (value: SkillLayout) => void;
  onCancel: () => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  const { language, t } = useI18n();
  // An unsaved draft keeps whatever the id field says: looking up bundled copy
  // would retitle the editor the moment the typed id happened to match a Skill
  // AI Switch ships.
  const heading = creating ? item.name : skillDisplayCopy(item, language).name;

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col">
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-stone-200 pb-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="truncate text-[15px] font-semibold text-stone-950">{heading}</h2>
            {item.read_only ? <span className="shrink-0 whitespace-nowrap rounded-lg bg-amber-50 px-1.5 py-0.5 text-[10px] font-semibold text-amber-800 ring-1 ring-amber-200">{t("skills.readOnly")}</span> : null}
          </div>
          <p className="mt-1 break-all font-mono text-[11px] text-stone-500">{item.path}</p>
        </div>
        <div className="flex shrink-0 gap-1.5">
          {!editing ? <button aria-label={t("skills.edit")} className="grid h-8 w-8 place-items-center rounded-lg bg-white text-stone-700 ring-1 ring-stone-300 motion-control hover:bg-stone-50 disabled:opacity-40" disabled={item.read_only} onClick={onEdit} title={item.read_only ? t("errors.skills.readOnly") : t("skills.edit")} type="button"><FilePenLine className="h-3.5 w-3.5" /></button> : null}
          {!item.read_only && !creating ? <button aria-label={t("skills.delete")} className="grid h-8 w-8 place-items-center rounded-lg bg-white text-red-700 ring-1 ring-red-200 motion-control hover:bg-red-50" onClick={onDelete} title={t("skills.delete")} type="button"><Trash2 className="h-3.5 w-3.5" /></button> : null}
        </div>
      </div>
      {editing ? (
        <div className="mt-3 flex min-h-0 flex-1 flex-col gap-3">
          <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_180px]">
            <label className="grid gap-1 text-[11px] font-semibold text-stone-600">{t("skills.skillId")}<input className="rounded-lg border border-stone-300 px-2 py-1.5 font-mono text-[12px] font-normal outline-none focus:border-blue-400" onChange={(event) => onIdChange(event.target.value)} value={draftId} /></label>
            <label className="grid gap-1 text-[11px] font-semibold text-stone-600">{t("skills.layout")}<select className="rounded-lg border border-stone-300 bg-white px-2 py-1.5 text-[12px] font-normal" onChange={(event) => onLayoutChange(event.target.value as SkillLayout)} value={layout}><option value="skill_directory">{t("skills.directoryLayout")}</option><option value="markdown_file">{t("skills.markdownLayout")}</option></select></label>
          </div>
          <textarea aria-label={t("skills.title")} className="min-h-[260px] min-w-0 flex-1 resize-y overflow-auto rounded-lg border border-stone-300 p-2 font-mono text-[12px] leading-5 outline-none focus:border-blue-400" onChange={(event) => onDraftChange(event.target.value)} spellCheck={false} value={draft} />
          <div className="flex flex-wrap justify-end gap-2">
            <button className="whitespace-nowrap rounded-xl bg-white px-3 py-2 text-[12px] font-semibold text-stone-700 ring-1 ring-stone-300 motion-control hover:bg-stone-50" onClick={onCancel} type="button">{t("skills.cancel")}</button>
            <button className="inline-flex items-center gap-1.5 whitespace-nowrap rounded-xl bg-stone-900 px-3 py-2 text-[12px] font-semibold text-white motion-control hover:bg-stone-800 disabled:opacity-50" disabled={saving || !draftId.trim()} onClick={onSave} type="button"><Check className="h-3.5 w-3.5" />{saving ? t("skills.saving") : t("skills.save")}</button>
          </div>
        </div>
      ) : (
        <pre className="mt-3 min-h-0 min-w-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-xl bg-stone-50 p-3 font-mono text-[12px] leading-5 text-stone-700">{draft || t("skills.loadingContent")}</pre>
      )}
    </div>
  );
}
