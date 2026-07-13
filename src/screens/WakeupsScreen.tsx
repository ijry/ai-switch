import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createWakeupRun,
  createWakeupTask,
  listWakeupRuns,
  listWakeupTasks,
  setWakeupTaskEnabled,
} from "../lib/api/client";
import type {
  NewWakeupRun,
  NewWakeupTask,
  WakeupRunOutcome,
  WakeupTaskStatus,
  WakeupTriggerType,
} from "../lib/api/types";

type TaskFormState = {
  name: string;
  managed_instance_id: string;
  target_app_id: string;
  provider_id: string;
  trigger_type: WakeupTriggerType;
  schedule_json: string;
  action_json: string;
  enabled: boolean;
  status: WakeupTaskStatus;
  notes: string;
};

type RunFormState = {
  task_id: string;
  outcome: WakeupRunOutcome;
  message: string;
  metadata_json: string;
};

const initialTaskForm: TaskFormState = {
  name: "",
  managed_instance_id: "",
  target_app_id: "",
  provider_id: "",
  trigger_type: "manual",
  schedule_json: "{\"window\":\"morning\"}",
  action_json: "{\"kind\":\"status_record\"}",
  enabled: true,
  status: "configured",
  notes: "",
};

const initialRunForm: RunFormState = {
  task_id: "",
  outcome: "recorded",
  message: "",
  metadata_json: "{}",
};

export function WakeupsScreen() {
  const queryClient = useQueryClient();
  const [taskForm, setTaskForm] = useState<TaskFormState>(initialTaskForm);
  const [runForm, setRunForm] = useState<RunFormState>(initialRunForm);
  const [taskError, setTaskError] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);

  const tasksQuery = useQuery({ queryKey: ["wakeup-tasks"], queryFn: listWakeupTasks });
  const runsQuery = useQuery({
    queryKey: ["wakeup-runs"],
    queryFn: () => listWakeupRuns({ task_id: null }),
  });

  const taskMutation = useMutation({
    mutationFn: (request: NewWakeupTask) => createWakeupTask(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["wakeup-tasks"] });
      setTaskForm(initialTaskForm);
      setTaskError(null);
    },
  });
  const enabledMutation = useMutation({
    mutationFn: (request: { id: string; enabled: boolean }) => setWakeupTaskEnabled(request),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["wakeup-tasks"] }),
  });
  const runMutation = useMutation({
    mutationFn: (request: NewWakeupRun) => createWakeupRun(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["wakeup-runs"] });
      queryClient.invalidateQueries({ queryKey: ["wakeup-tasks"] });
      setRunForm(initialRunForm);
      setRunError(null);
    },
  });

  const tasks = tasksQuery.data ?? [];
  const runs = runsQuery.data ?? [];

  if (tasksQuery.isLoading || runsQuery.isLoading) {
    return <p className="text-steel">Loading wakeup tasks...</p>;
  }

  if (tasksQuery.error || runsQuery.error) {
    return <p className="text-ember">Could not load wakeup tasks.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Wakeups</h1>
        <p className="text-steel">
          Record wakeup task intent and manual run notes without scheduling jobs, launching tools,
          or waking sleeping processes.
        </p>
      </div>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Create wakeup task</h2>
          <p className="text-sm text-steel">
            Schedule and action JSON are metadata only. Sensitive action fields must use
            references.
          </p>
        </div>
        <TextField
          label="Wakeup task name"
          value={taskForm.name}
          onChange={(value) => setTaskForm((current) => ({ ...current, name: value }))}
          placeholder="Morning review"
        />
        <SelectField
          label="Wakeup trigger type"
          value={taskForm.trigger_type}
          onChange={(value) =>
            setTaskForm((current) => ({ ...current, trigger_type: value as WakeupTriggerType }))
          }
          options={["manual", "scheduled", "interval"]}
        />
        <TextField
          label="Wakeup instance ID"
          value={taskForm.managed_instance_id}
          onChange={(value) =>
            setTaskForm((current) => ({ ...current, managed_instance_id: value }))
          }
          placeholder="instance-1"
        />
        <TextField
          label="Wakeup target app ID"
          value={taskForm.target_app_id}
          onChange={(value) => setTaskForm((current) => ({ ...current, target_app_id: value }))}
          placeholder="target-codex"
        />
        <TextField
          label="Wakeup provider ID"
          value={taskForm.provider_id}
          onChange={(value) => setTaskForm((current) => ({ ...current, provider_id: value }))}
          placeholder="provider-1"
        />
        <SelectField
          label="Wakeup status"
          value={taskForm.status}
          onChange={(value) =>
            setTaskForm((current) => ({ ...current, status: value as WakeupTaskStatus }))
          }
          options={["configured", "paused", "error"]}
        />
        <JsonField
          label="Wakeup schedule JSON"
          value={taskForm.schedule_json}
          onChange={(value) => setTaskForm((current) => ({ ...current, schedule_json: value }))}
        />
        <JsonField
          label="Wakeup action JSON"
          value={taskForm.action_json}
          onChange={(value) => setTaskForm((current) => ({ ...current, action_json: value }))}
        />
        <label className="flex items-center gap-3 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={taskForm.enabled}
            onChange={(event) =>
              setTaskForm((current) => ({ ...current, enabled: event.target.checked }))
            }
            className="h-4 w-4"
          />
          Wakeup task enabled
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Wakeup notes</span>
          <textarea
            value={taskForm.notes}
            onChange={(event) =>
              setTaskForm((current) => ({ ...current, notes: event.target.value }))
            }
            className="min-h-20 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={taskMutation.isPending} onClick={submitTask}>
            Create wakeup task
          </Button>
          {taskError && <p className="text-sm text-ember">{taskError}</p>}
          {taskMutation.error && !taskError && (
            <p className="text-sm text-ember">Could not create wakeup task.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Record wakeup run</h2>
          <p className="text-sm text-steel">
            This records a manual readiness event only; it does not start the task.
          </p>
        </div>
        <TextField
          label="Wakeup run task ID"
          value={runForm.task_id}
          onChange={(value) => setRunForm((current) => ({ ...current, task_id: value }))}
          placeholder="wakeup-task-1"
        />
        <SelectField
          label="Wakeup run outcome"
          value={runForm.outcome}
          onChange={(value) =>
            setRunForm((current) => ({ ...current, outcome: value as WakeupRunOutcome }))
          }
          options={["recorded", "skipped", "failed"]}
        />
        <TextField
          label="Wakeup run message"
          value={runForm.message}
          onChange={(value) => setRunForm((current) => ({ ...current, message: value }))}
          placeholder="Ready for manual start"
        />
        <JsonField
          label="Wakeup run metadata JSON"
          value={runForm.metadata_json}
          onChange={(value) => setRunForm((current) => ({ ...current, metadata_json: value }))}
        />
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={runMutation.isPending} onClick={submitRun}>
            Record wakeup run
          </Button>
          {runError && <p className="text-sm text-ember">{runError}</p>}
          {runMutation.error && !runError && (
            <p className="text-sm text-ember">Could not record wakeup run.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
        <RecordList title="Wakeup tasks" empty="No wakeup tasks yet.">
          {tasks.map((task) => (
            <article key={task.id} className="rounded-2xl bg-paper/70 p-4">
              <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
                <div>
                  <p className="font-semibold text-ink">{task.name}</p>
                  <p className="text-sm text-steel">
                    {task.trigger_type} / {task.status} / {task.enabled ? "enabled" : "disabled"}
                  </p>
                  {task.last_run_at && (
                    <p className="text-xs text-steel">Last record: {task.last_run_at}</p>
                  )}
                </div>
                <div className="flex gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={`Enable ${task.name}`}
                    disabled={enabledMutation.isPending}
                    onClick={() => enabledMutation.mutate({ id: task.id, enabled: true })}
                  >
                    Enable
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={`Disable ${task.name}`}
                    disabled={enabledMutation.isPending}
                    onClick={() => enabledMutation.mutate({ id: task.id, enabled: false })}
                  >
                    Disable
                  </Button>
                </div>
              </div>
              <pre className="mt-3 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {task.action_json}
              </pre>
              {task.notes && <p className="mt-2 text-sm text-steel">{task.notes}</p>}
            </article>
          ))}
        </RecordList>

        <RecordList title="Wakeup run records" empty="No wakeup runs yet.">
          {runs.map((run) => (
            <article key={run.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{run.message}</p>
              <p className="text-sm text-steel">{run.outcome}</p>
              <p className="break-all font-mono text-xs text-steel">{run.task_id}</p>
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {run.metadata_json}
              </pre>
            </article>
          ))}
        </RecordList>
      </section>
    </section>
  );

  function submitTask() {
    setTaskError(null);
    const scheduleJson = taskForm.schedule_json.trim() || "{}";
    const actionJson = taskForm.action_json.trim() || "{}";
    if (!isObjectJson(scheduleJson)) {
      setTaskError("Wakeup schedule JSON must be an object.");
      return;
    }
    if (!isObjectJson(actionJson)) {
      setTaskError("Wakeup action JSON must be an object.");
      return;
    }

    taskMutation.mutate({
      name: taskForm.name.trim(),
      managed_instance_id: taskForm.managed_instance_id.trim() || null,
      target_app_id: taskForm.target_app_id.trim() || null,
      provider_id: taskForm.provider_id.trim() || null,
      trigger_type: taskForm.trigger_type,
      schedule_json: scheduleJson,
      action_json: actionJson,
      enabled: taskForm.enabled,
      status: taskForm.status,
      notes: taskForm.notes.trim() || null,
    });
  }

  function submitRun() {
    setRunError(null);
    const metadataJson = runForm.metadata_json.trim() || "{}";
    if (!isObjectJson(metadataJson)) {
      setRunError("Wakeup run metadata JSON must be an object.");
      return;
    }

    runMutation.mutate({
      task_id: runForm.task_id.trim(),
      outcome: runForm.outcome,
      message: runForm.message.trim(),
      metadata_json: metadataJson,
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
