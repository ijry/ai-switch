import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createBulkOperation,
  createItemTag,
  createPluginLink,
  createTag,
  listBulkOperations,
  listItemTags,
  listPluginLinks,
  listTags,
  setPluginLinkEnabled,
} from "../lib/api/client";
import type {
  AutomationItemType,
  BulkOperationStatus,
  BulkOperationType,
  NewBulkOperation,
  NewItemTag,
  NewPluginLink,
  NewTagRecord,
  PluginLinkStatus,
} from "../lib/api/types";

type TagFormState = {
  name: string;
  color: string;
  description: string;
};

type ItemTagFormState = {
  tag_id: string;
  item_type: AutomationItemType;
  item_id: string;
};

type PluginFormState = {
  name: string;
  plugin_key: string;
  item_type: AutomationItemType;
  item_id: string;
  config_json: string;
  enabled: boolean;
  status: PluginLinkStatus;
  notes: string;
};

type BulkFormState = {
  name: string;
  operation_type: BulkOperationType;
  target_type: AutomationItemType;
  item_ids_json: string;
  parameters_json: string;
  dry_run: boolean;
  status: BulkOperationStatus;
  summary_json: string;
};

const itemTypes: AutomationItemType[] = [
  "provider",
  "official_account",
  "mcp_server",
  "prompt_asset",
  "session",
  "managed_instance",
  "wakeup_task",
  "target_app",
  "mixed",
];

const initialTagForm: TagFormState = {
  name: "",
  color: "#3f6f5f",
  description: "",
};

const initialItemTagForm: ItemTagFormState = {
  tag_id: "",
  item_type: "provider",
  item_id: "",
};

const initialPluginForm: PluginFormState = {
  name: "",
  plugin_key: "review.bridge",
  item_type: "provider",
  item_id: "",
  config_json: "{\"mode\":\"metadata\"}",
  enabled: true,
  status: "configured",
  notes: "",
};

const initialBulkForm: BulkFormState = {
  name: "",
  operation_type: "tag_apply",
  target_type: "provider",
  item_ids_json: "[\"provider-1\"]",
  parameters_json: "{}",
  dry_run: true,
  status: "planned",
  summary_json: "{}",
};

export function BulkScreen() {
  const queryClient = useQueryClient();
  const [tagForm, setTagForm] = useState<TagFormState>(initialTagForm);
  const [itemTagForm, setItemTagForm] = useState<ItemTagFormState>(initialItemTagForm);
  const [pluginForm, setPluginForm] = useState<PluginFormState>(initialPluginForm);
  const [bulkForm, setBulkForm] = useState<BulkFormState>(initialBulkForm);
  const [tagError, setTagError] = useState<string | null>(null);
  const [itemTagError, setItemTagError] = useState<string | null>(null);
  const [pluginError, setPluginError] = useState<string | null>(null);
  const [bulkError, setBulkError] = useState<string | null>(null);

  const tagsQuery = useQuery({ queryKey: ["tags"], queryFn: listTags });
  const itemTagsQuery = useQuery({ queryKey: ["item-tags"], queryFn: listItemTags });
  const pluginLinksQuery = useQuery({ queryKey: ["plugin-links"], queryFn: listPluginLinks });
  const bulkOperationsQuery = useQuery({
    queryKey: ["bulk-operations"],
    queryFn: listBulkOperations,
  });

  const tagMutation = useMutation({
    mutationFn: (request: NewTagRecord) => createTag(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["tags"] });
      setTagForm(initialTagForm);
      setTagError(null);
    },
  });
  const itemTagMutation = useMutation({
    mutationFn: (request: NewItemTag) => createItemTag(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["item-tags"] });
      setItemTagForm(initialItemTagForm);
      setItemTagError(null);
    },
  });
  const pluginMutation = useMutation({
    mutationFn: (request: NewPluginLink) => createPluginLink(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["plugin-links"] });
      setPluginForm(initialPluginForm);
      setPluginError(null);
    },
  });
  const pluginEnabledMutation = useMutation({
    mutationFn: (request: { id: string; enabled: boolean }) => setPluginLinkEnabled(request),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["plugin-links"] }),
  });
  const bulkMutation = useMutation({
    mutationFn: (request: NewBulkOperation) => createBulkOperation(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["bulk-operations"] });
      setBulkForm(initialBulkForm);
      setBulkError(null);
    },
  });

  const tags = tagsQuery.data ?? [];
  const itemTags = itemTagsQuery.data ?? [];
  const pluginLinks = pluginLinksQuery.data ?? [];
  const bulkOperations = bulkOperationsQuery.data ?? [];

  if (
    tagsQuery.isLoading ||
    itemTagsQuery.isLoading ||
    pluginLinksQuery.isLoading ||
    bulkOperationsQuery.isLoading
  ) {
    return <p className="text-steel">Loading bulk metadata...</p>;
  }

  if (
    tagsQuery.error ||
    itemTagsQuery.error ||
    pluginLinksQuery.error ||
    bulkOperationsQuery.error
  ) {
    return <p className="text-ember">Could not load bulk metadata.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Bulk</h1>
        <p className="text-steel">
          Manage tags, plugin links, and bulk operation records without executing plugins or
          changing external tools.
        </p>
      </div>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-3">
        <div className="lg:col-span-3">
          <h2 className="font-display text-xl font-semibold text-ink">Tags</h2>
          <p className="text-sm text-steel">Create shared labels and attach them to local records.</p>
        </div>
        <TextField
          label="Tag name"
          value={tagForm.name}
          onChange={(value) => setTagForm((current) => ({ ...current, name: value }))}
          placeholder="review"
        />
        <TextField
          label="Tag color"
          value={tagForm.color}
          onChange={(value) => setTagForm((current) => ({ ...current, color: value }))}
          placeholder="#3f6f5f"
        />
        <TextField
          label="Tag description"
          value={tagForm.description}
          onChange={(value) => setTagForm((current) => ({ ...current, description: value }))}
          placeholder="Shared review items"
        />
        <div className="flex flex-wrap items-center gap-3 lg:col-span-3">
          <Button type="button" disabled={tagMutation.isPending} onClick={submitTag}>
            Create tag
          </Button>
          {tagError && <p className="text-sm text-ember">{tagError}</p>}
          {tagMutation.error && !tagError && <p className="text-sm text-ember">Could not create tag.</p>}
        </div>

        <TextField
          label="Assignment tag ID"
          value={itemTagForm.tag_id}
          onChange={(value) => setItemTagForm((current) => ({ ...current, tag_id: value }))}
          placeholder="tag-1"
        />
        <SelectField
          label="Assignment item type"
          value={itemTagForm.item_type}
          onChange={(value) =>
            setItemTagForm((current) => ({ ...current, item_type: value as AutomationItemType }))
          }
          options={itemTypes}
        />
        <TextField
          label="Assignment item ID"
          value={itemTagForm.item_id}
          onChange={(value) => setItemTagForm((current) => ({ ...current, item_id: value }))}
          placeholder="provider-1"
        />
        <div className="flex flex-wrap items-center gap-3 lg:col-span-3">
          <Button type="button" disabled={itemTagMutation.isPending} onClick={submitItemTag}>
            Assign tag
          </Button>
          {itemTagError && <p className="text-sm text-ember">{itemTagError}</p>}
          {itemTagMutation.error && !itemTagError && (
            <p className="text-sm text-ember">Could not assign tag.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Plugin link</h2>
          <p className="text-sm text-steel">
            Plugin links store integration metadata only; they do not load or execute plugin code.
          </p>
        </div>
        <TextField
          label="Plugin link name"
          value={pluginForm.name}
          onChange={(value) => setPluginForm((current) => ({ ...current, name: value }))}
          placeholder="Review bridge"
        />
        <TextField
          label="Plugin key"
          value={pluginForm.plugin_key}
          onChange={(value) => setPluginForm((current) => ({ ...current, plugin_key: value }))}
          placeholder="review.bridge"
        />
        <SelectField
          label="Plugin item type"
          value={pluginForm.item_type}
          onChange={(value) =>
            setPluginForm((current) => ({ ...current, item_type: value as AutomationItemType }))
          }
          options={itemTypes}
        />
        <TextField
          label="Plugin item ID"
          value={pluginForm.item_id}
          onChange={(value) => setPluginForm((current) => ({ ...current, item_id: value }))}
          placeholder="provider-1"
        />
        <SelectField
          label="Plugin link status"
          value={pluginForm.status}
          onChange={(value) =>
            setPluginForm((current) => ({ ...current, status: value as PluginLinkStatus }))
          }
          options={["configured", "paused", "error"]}
        />
        <label className="flex items-center gap-3 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={pluginForm.enabled}
            onChange={(event) =>
              setPluginForm((current) => ({ ...current, enabled: event.target.checked }))
            }
            className="h-4 w-4"
          />
          Plugin link enabled
        </label>
        <JsonField
          label="Plugin config JSON"
          value={pluginForm.config_json}
          onChange={(value) => setPluginForm((current) => ({ ...current, config_json: value }))}
        />
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Plugin notes</span>
          <textarea
            value={pluginForm.notes}
            onChange={(event) =>
              setPluginForm((current) => ({ ...current, notes: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={pluginMutation.isPending} onClick={submitPluginLink}>
            Create plugin link
          </Button>
          {pluginError && <p className="text-sm text-ember">{pluginError}</p>}
          {pluginMutation.error && !pluginError && (
            <p className="text-sm text-ember">Could not create plugin link.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Bulk operation record</h2>
          <p className="text-sm text-steel">
            Bulk records capture intended item sets and parameters. They do not execute changes.
          </p>
        </div>
        <TextField
          label="Bulk operation name"
          value={bulkForm.name}
          onChange={(value) => setBulkForm((current) => ({ ...current, name: value }))}
          placeholder="Apply review tag"
        />
        <SelectField
          label="Bulk operation type"
          value={bulkForm.operation_type}
          onChange={(value) =>
            setBulkForm((current) => ({ ...current, operation_type: value as BulkOperationType }))
          }
          options={["tag_apply", "tag_remove", "status_record", "export_selection", "plugin_link"]}
        />
        <SelectField
          label="Bulk target type"
          value={bulkForm.target_type}
          onChange={(value) =>
            setBulkForm((current) => ({ ...current, target_type: value as AutomationItemType }))
          }
          options={itemTypes}
        />
        <SelectField
          label="Bulk status"
          value={bulkForm.status}
          onChange={(value) =>
            setBulkForm((current) => ({ ...current, status: value as BulkOperationStatus }))
          }
          options={["planned", "recorded", "cancelled", "error"]}
        />
        <JsonField
          label="Bulk item IDs JSON"
          value={bulkForm.item_ids_json}
          onChange={(value) => setBulkForm((current) => ({ ...current, item_ids_json: value }))}
        />
        <JsonField
          label="Bulk parameters JSON"
          value={bulkForm.parameters_json}
          onChange={(value) => setBulkForm((current) => ({ ...current, parameters_json: value }))}
        />
        <JsonField
          label="Bulk summary JSON"
          value={bulkForm.summary_json}
          onChange={(value) => setBulkForm((current) => ({ ...current, summary_json: value }))}
        />
        <label className="flex items-center gap-3 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={bulkForm.dry_run}
            onChange={(event) =>
              setBulkForm((current) => ({ ...current, dry_run: event.target.checked }))
            }
            className="h-4 w-4"
          />
          Dry-run record
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={bulkMutation.isPending} onClick={submitBulkOperation}>
            Create bulk operation
          </Button>
          {bulkError && <p className="text-sm text-ember">{bulkError}</p>}
          {bulkMutation.error && !bulkError && (
            <p className="text-sm text-ember">Could not create bulk operation.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
        <RecordList title="Tag records" empty="No tags yet.">
          {tags.map((tag) => (
            <article key={tag.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{tag.name}</p>
              {tag.color && <p className="text-sm text-steel">{tag.color}</p>}
              {tag.description && <p className="text-sm text-steel">{tag.description}</p>}
            </article>
          ))}
        </RecordList>

        <RecordList title="Tag assignments" empty="No tag assignments yet.">
          {itemTags.map((itemTag) => (
            <article key={itemTag.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{itemTag.item_type}</p>
              <p className="break-all font-mono text-xs text-steel">{itemTag.item_id}</p>
              <p className="break-all font-mono text-xs text-steel">{itemTag.tag_id}</p>
            </article>
          ))}
        </RecordList>

        <RecordList title="Plugin links" empty="No plugin links yet.">
          {pluginLinks.map((link) => (
            <article key={link.id} className="rounded-2xl bg-paper/70 p-4">
              <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
                <div>
                  <p className="font-semibold text-ink">{link.name}</p>
                  <p className="text-sm text-steel">
                    {link.plugin_key} / {link.status} / {link.enabled ? "enabled" : "disabled"}
                  </p>
                  <p className="break-all font-mono text-xs text-steel">{link.item_id}</p>
                </div>
                <div className="flex gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={`Enable ${link.name}`}
                    disabled={pluginEnabledMutation.isPending}
                    onClick={() => pluginEnabledMutation.mutate({ id: link.id, enabled: true })}
                  >
                    Enable
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={`Disable ${link.name}`}
                    disabled={pluginEnabledMutation.isPending}
                    onClick={() => pluginEnabledMutation.mutate({ id: link.id, enabled: false })}
                  >
                    Disable
                  </Button>
                </div>
              </div>
              <pre className="mt-3 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {link.config_json}
              </pre>
            </article>
          ))}
        </RecordList>

        <RecordList title="Bulk operation records" empty="No bulk operations yet.">
          {bulkOperations.map((operation) => (
            <article key={operation.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{operation.name}</p>
              <p className="text-sm text-steel">
                {operation.operation_type} / {operation.status} /{" "}
                {operation.dry_run ? "dry-run" : "recorded"}
              </p>
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {operation.item_ids_json}
              </pre>
            </article>
          ))}
        </RecordList>
      </section>
    </section>
  );

  function submitTag() {
    setTagError(null);
    if (tagForm.color.trim() && !isHexColor(tagForm.color.trim())) {
      setTagError("Tag color must be a hex color.");
      return;
    }

    tagMutation.mutate({
      name: tagForm.name.trim(),
      color: tagForm.color.trim() || null,
      description: tagForm.description.trim() || null,
    });
  }

  function submitItemTag() {
    setItemTagError(null);
    itemTagMutation.mutate({
      tag_id: itemTagForm.tag_id.trim(),
      item_type: itemTagForm.item_type,
      item_id: itemTagForm.item_id.trim(),
    });
  }

  function submitPluginLink() {
    setPluginError(null);
    const configJson = pluginForm.config_json.trim() || "{}";
    if (!isObjectJson(configJson)) {
      setPluginError("Plugin config JSON must be an object.");
      return;
    }

    pluginMutation.mutate({
      name: pluginForm.name.trim(),
      plugin_key: pluginForm.plugin_key.trim(),
      item_type: pluginForm.item_type,
      item_id: pluginForm.item_id.trim(),
      config_json: configJson,
      enabled: pluginForm.enabled,
      status: pluginForm.status,
      notes: pluginForm.notes.trim() || null,
    });
  }

  function submitBulkOperation() {
    setBulkError(null);
    const itemIdsJson = bulkForm.item_ids_json.trim() || "[]";
    const parametersJson = bulkForm.parameters_json.trim() || "{}";
    const summaryJson = bulkForm.summary_json.trim() || "{}";
    if (!isStringArrayJson(itemIdsJson)) {
      setBulkError("Bulk item IDs JSON must be an array of strings.");
      return;
    }
    if (!isObjectJson(parametersJson)) {
      setBulkError("Bulk parameters JSON must be an object.");
      return;
    }
    if (!isObjectJson(summaryJson)) {
      setBulkError("Bulk summary JSON must be an object.");
      return;
    }

    bulkMutation.mutate({
      name: bulkForm.name.trim(),
      operation_type: bulkForm.operation_type,
      target_type: bulkForm.target_type,
      item_ids_json: itemIdsJson,
      parameters_json: parametersJson,
      dry_run: bulkForm.dry_run,
      status: bulkForm.status,
      summary_json: summaryJson,
    });
  }
}

function isObjectJson(json: string) {
  try {
    const value = JSON.parse(json);
    return Boolean(value) && !Array.isArray(value) && typeof value === "object";
  } catch {
    return false;
  }
}

function isStringArrayJson(json: string) {
  try {
    const value = JSON.parse(json);
    return Array.isArray(value) && value.every((item) => typeof item === "string");
  } catch {
    return false;
  }
}

function isHexColor(value: string) {
  return /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(value);
}

type TextFieldProps = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
};

function TextField({ label, value, onChange, placeholder }: TextFieldProps) {
  return (
    <label className="space-y-2 text-sm font-semibold text-ink">
      <span>{label}</span>
      <input
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
      />
    </label>
  );
}

type SelectFieldProps = {
  label: string;
  value: string;
  options: string[];
  onChange: (value: string) => void;
};

function SelectField({ label, value, options, onChange }: SelectFieldProps) {
  return (
    <label className="space-y-2 text-sm font-semibold text-ink">
      <span>{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    </label>
  );
}

type JsonFieldProps = {
  label: string;
  value: string;
  onChange: (value: string) => void;
};

function JsonField({ label, value, onChange }: JsonFieldProps) {
  return (
    <label className="space-y-2 text-sm font-semibold text-ink">
      <span>{label}</span>
      <textarea
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm outline-none focus:border-moss"
      />
    </label>
  );
}

type RecordListProps = {
  title: string;
  empty: string;
  children: React.ReactNode;
};

function RecordList({ title, empty, children }: RecordListProps) {
  const hasChildren = Array.isArray(children) ? children.length > 0 : Boolean(children);

  return (
    <div className="space-y-3 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm">
      <h2 className="font-display text-xl font-semibold text-ink">{title}</h2>
      {hasChildren ? children : <p className="text-sm text-steel">{empty}</p>}
    </div>
  );
}
