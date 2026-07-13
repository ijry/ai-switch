import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import { createInstance, listInstances, setInstanceStatus } from "../lib/api/client";
import type { InstanceStatus, NewManagedInstance } from "../lib/api/types";

type InstanceFormState = {
  name: string;
  target_app_id: string;
  provider_id: string;
  launch_args_json: string;
  env_json: string;
  profile_json: string;
  status: InstanceStatus;
  notes: string;
};

const initialInstanceForm: InstanceFormState = {
  name: "",
  target_app_id: "",
  provider_id: "",
  launch_args_json: "[\"--profile\",\"review\"]",
  env_json: "{}",
  profile_json: "{}",
  status: "configured",
  notes: "",
};

export function InstancesScreen() {
  const queryClient = useQueryClient();
  const [form, setForm] = useState<InstanceFormState>(initialInstanceForm);
  const [formError, setFormError] = useState<string | null>(null);

  const instancesQuery = useQuery({ queryKey: ["instances"], queryFn: listInstances });
  const createMutation = useMutation({
    mutationFn: (request: NewManagedInstance) => createInstance(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["instances"] });
      setForm(initialInstanceForm);
      setFormError(null);
    },
  });
  const statusMutation = useMutation({
    mutationFn: (request: { id: string; status: InstanceStatus }) => setInstanceStatus(request),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["instances"] }),
  });

  const instances = instancesQuery.data ?? [];

  if (instancesQuery.isLoading) {
    return <p className="text-steel">Loading instances...</p>;
  }

  if (instancesQuery.error) {
    return <p className="text-ember">Could not load instances.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Instances</h1>
        <p className="text-steel">
          Store multi-instance metadata and manual status records without launching external
          processes.
        </p>
      </div>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Create instance</h2>
          <p className="text-sm text-steel">
            Environment secrets must use `env://` or `secret://` references.
          </p>
        </div>
        <TextField
          label="Instance name"
          value={form.name}
          onChange={(value) => setForm((current) => ({ ...current, name: value }))}
          placeholder="Codex Review"
        />
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Instance status</span>
          <select
            value={form.status}
            onChange={(event) =>
              setForm((current) => ({ ...current, status: event.target.value as InstanceStatus }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          >
            <option value="configured">configured</option>
            <option value="running">running</option>
            <option value="stopped">stopped</option>
            <option value="error">error</option>
          </select>
        </label>
        <TextField
          label="Instance target app ID"
          value={form.target_app_id}
          onChange={(value) => setForm((current) => ({ ...current, target_app_id: value }))}
          placeholder="target-codex"
        />
        <TextField
          label="Instance provider ID"
          value={form.provider_id}
          onChange={(value) => setForm((current) => ({ ...current, provider_id: value }))}
          placeholder="provider-1"
        />
        <JsonField
          label="Launch args JSON"
          value={form.launch_args_json}
          onChange={(value) => setForm((current) => ({ ...current, launch_args_json: value }))}
        />
        <JsonField
          label="Instance env JSON"
          value={form.env_json}
          onChange={(value) => setForm((current) => ({ ...current, env_json: value }))}
        />
        <JsonField
          label="Instance profile JSON"
          value={form.profile_json}
          onChange={(value) => setForm((current) => ({ ...current, profile_json: value }))}
        />
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Instance notes</span>
          <textarea
            value={form.notes}
            onChange={(event) => setForm((current) => ({ ...current, notes: event.target.value }))}
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={createMutation.isPending} onClick={submitInstance}>
            Create instance
          </Button>
          {formError && <p className="text-sm text-ember">{formError}</p>}
          {createMutation.error && !formError && (
            <p className="text-sm text-ember">Could not create instance.</p>
          )}
        </div>
      </section>

      <section className="space-y-3 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm">
        <h2 className="font-display text-xl font-semibold text-ink">Managed instances</h2>
        {instances.length === 0 ? (
          <p className="text-sm text-steel">No instances yet.</p>
        ) : (
          <div className="grid gap-3">
            {instances.map((instance) => (
              <article key={instance.id} className="rounded-2xl bg-paper/70 p-4">
                <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <div>
                    <p className="font-semibold text-ink">{instance.name}</p>
                    <p className="text-sm text-steel">{instance.status}</p>
                    {instance.notes && <p className="text-sm text-steel">{instance.notes}</p>}
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <StatusButton
                      label={`Mark ${instance.name} running`}
                      status="running"
                      disabled={statusMutation.isPending}
                      onClick={() =>
                        statusMutation.mutate({ id: instance.id, status: "running" })
                      }
                    />
                    <StatusButton
                      label={`Mark ${instance.name} stopped`}
                      status="stopped"
                      disabled={statusMutation.isPending}
                      onClick={() =>
                        statusMutation.mutate({ id: instance.id, status: "stopped" })
                      }
                    />
                    <StatusButton
                      label={`Mark ${instance.name} error`}
                      status="error"
                      disabled={statusMutation.isPending}
                      onClick={() => statusMutation.mutate({ id: instance.id, status: "error" })}
                    />
                  </div>
                </div>
                <pre className="mt-3 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                  {instance.launch_args_json}
                </pre>
              </article>
            ))}
          </div>
        )}
      </section>
    </section>
  );

  function submitInstance() {
    setFormError(null);
    if (!isStringArrayJson(form.launch_args_json.trim() || "[]")) {
      setFormError("Launch args JSON must be an array of strings.");
      return;
    }
    if (!isObjectJson(form.env_json.trim() || "{}")) {
      setFormError("Instance env JSON must be an object.");
      return;
    }
    if (!isObjectJson(form.profile_json.trim() || "{}")) {
      setFormError("Instance profile JSON must be an object.");
      return;
    }

    createMutation.mutate({
      name: form.name.trim(),
      target_app_id: form.target_app_id.trim() || null,
      provider_id: form.provider_id.trim() || null,
      launch_args_json: form.launch_args_json.trim() || "[]",
      env_json: form.env_json.trim() || "{}",
      profile_json: form.profile_json.trim() || "{}",
      status: form.status,
      notes: form.notes.trim() || null,
    });
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

function isObjectJson(json: string) {
  try {
    const value = JSON.parse(json);
    return Boolean(value) && !Array.isArray(value) && typeof value === "object";
  } catch {
    return false;
  }
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

type StatusButtonProps = {
  label: string;
  status: string;
  disabled: boolean;
  onClick: () => void;
};

function StatusButton({ label, status, disabled, onClick }: StatusButtonProps) {
  return (
    <Button
      type="button"
      variant="secondary"
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {status}
    </Button>
  );
}
