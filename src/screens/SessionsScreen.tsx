import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createSession,
  createSessionEvent,
  listSessionEvents,
  listSessions,
  setSessionStatus,
} from "../lib/api/client";
import type {
  NewSessionEvent,
  NewSessionRecord,
  SessionEventType,
  SessionStatus,
} from "../lib/api/types";

type SessionFormState = {
  title: string;
  target_app_id: string;
  provider_id: string;
  official_account_id: string;
  prompt_asset_id: string;
  mcp_server_ids_json: string;
  tags_json: string;
  status: SessionStatus;
  notes: string;
};

type EventFormState = {
  session_id: string;
  event_type: SessionEventType;
  message: string;
  metadata_json: string;
};

const initialSessionForm: SessionFormState = {
  title: "",
  target_app_id: "",
  provider_id: "",
  official_account_id: "",
  prompt_asset_id: "",
  mcp_server_ids_json: "[]",
  tags_json: "[\"review\"]",
  status: "draft",
  notes: "",
};

const initialEventForm: EventFormState = {
  session_id: "",
  event_type: "note",
  message: "",
  metadata_json: "{}",
};

export function SessionsScreen() {
  const queryClient = useQueryClient();
  const [sessionForm, setSessionForm] = useState<SessionFormState>(initialSessionForm);
  const [eventForm, setEventForm] = useState<EventFormState>(initialEventForm);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [eventError, setEventError] = useState<string | null>(null);

  const sessionsQuery = useQuery({ queryKey: ["sessions"], queryFn: listSessions });
  const eventsQuery = useQuery({
    queryKey: ["session-events"],
    queryFn: () => listSessionEvents({ session_id: null }),
  });

  const createMutation = useMutation({
    mutationFn: (request: NewSessionRecord) => createSession(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["sessions"] });
      setSessionForm(initialSessionForm);
      setSessionError(null);
    },
  });
  const statusMutation = useMutation({
    mutationFn: (request: { id: string; status: SessionStatus }) => setSessionStatus(request),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["sessions"] }),
  });
  const eventMutation = useMutation({
    mutationFn: (request: NewSessionEvent) => createSessionEvent(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["session-events"] });
      setEventForm(initialEventForm);
      setEventError(null);
    },
  });

  const sessions = sessionsQuery.data ?? [];
  const events = eventsQuery.data ?? [];

  if (sessionsQuery.isLoading || eventsQuery.isLoading) {
    return <p className="text-steel">Loading sessions...</p>;
  }

  if (sessionsQuery.error || eventsQuery.error) {
    return <p className="text-ember">Could not load sessions.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Sessions</h1>
        <p className="text-steel">
          Group provider, account, target, prompt, and MCP context without launching tools or
          capturing transcripts automatically.
        </p>
      </div>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Create session</h2>
          <p className="text-sm text-steel">
            References are optional IDs. MCP IDs and tags are stored as JSON arrays.
          </p>
        </div>
        <TextField
          label="Session title"
          value={sessionForm.title}
          onChange={(value) => setSessionForm((current) => ({ ...current, title: value }))}
          placeholder="Release review"
        />
        <SelectField
          label="Session status"
          value={sessionForm.status}
          onChange={(value) =>
            setSessionForm((current) => ({ ...current, status: value as SessionStatus }))
          }
          options={["draft", "active", "archived"]}
        />
        <TextField
          label="Target app ID"
          value={sessionForm.target_app_id}
          onChange={(value) => setSessionForm((current) => ({ ...current, target_app_id: value }))}
          placeholder="target-codex"
        />
        <TextField
          label="Provider ID"
          value={sessionForm.provider_id}
          onChange={(value) => setSessionForm((current) => ({ ...current, provider_id: value }))}
          placeholder="provider-1"
        />
        <TextField
          label="Official account ID"
          value={sessionForm.official_account_id}
          onChange={(value) =>
            setSessionForm((current) => ({ ...current, official_account_id: value }))
          }
          placeholder="account-1"
        />
        <TextField
          label="Prompt asset ID"
          value={sessionForm.prompt_asset_id}
          onChange={(value) =>
            setSessionForm((current) => ({ ...current, prompt_asset_id: value }))
          }
          placeholder="prompt-1"
        />
        <JsonField
          label="MCP server IDs JSON"
          value={sessionForm.mcp_server_ids_json}
          onChange={(value) =>
            setSessionForm((current) => ({ ...current, mcp_server_ids_json: value }))
          }
        />
        <JsonField
          label="Session tags JSON"
          value={sessionForm.tags_json}
          onChange={(value) => setSessionForm((current) => ({ ...current, tags_json: value }))}
        />
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Session notes</span>
          <textarea
            value={sessionForm.notes}
            onChange={(event) =>
              setSessionForm((current) => ({ ...current, notes: event.target.value }))
            }
            className="min-h-20 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={createMutation.isPending} onClick={submitSession}>
            Create session
          </Button>
          {sessionError && <p className="text-sm text-ember">{sessionError}</p>}
          {createMutation.error && !sessionError && (
            <p className="text-sm text-ember">Could not create session.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Add event</h2>
          <p className="text-sm text-steel">
            Use events for notes and breadcrumbs. Sensitive metadata must use references.
          </p>
        </div>
        <TextField
          label="Event session ID"
          value={eventForm.session_id}
          onChange={(value) => setEventForm((current) => ({ ...current, session_id: value }))}
          placeholder="session-1"
        />
        <SelectField
          label="Event type"
          value={eventForm.event_type}
          onChange={(value) =>
            setEventForm((current) => ({ ...current, event_type: value as SessionEventType }))
          }
          options={["note", "status", "usage", "quota", "error", "import", "switch"]}
        />
        <TextField
          label="Event message"
          value={eventForm.message}
          onChange={(value) => setEventForm((current) => ({ ...current, message: value }))}
          placeholder="Started review"
        />
        <JsonField
          label="Event metadata JSON"
          value={eventForm.metadata_json}
          onChange={(value) => setEventForm((current) => ({ ...current, metadata_json: value }))}
        />
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={eventMutation.isPending} onClick={submitEvent}>
            Add session event
          </Button>
          {eventError && <p className="text-sm text-ember">{eventError}</p>}
          {eventMutation.error && !eventError && (
            <p className="text-sm text-ember">Could not add session event.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
        <RecordList title="Session records" empty="No sessions yet.">
          {sessions.map((session) => (
            <article key={session.id} className="rounded-2xl bg-paper/70 p-4">
              <div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
                <div>
                  <p className="font-semibold text-ink">{session.title}</p>
                  <p className="text-sm text-steel">{session.status}</p>
                </div>
                <div className="flex gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={`Activate ${session.title}`}
                    disabled={statusMutation.isPending}
                    onClick={() => statusMutation.mutate({ id: session.id, status: "active" })}
                  >
                    Activate
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    aria-label={`Archive ${session.title}`}
                    disabled={statusMutation.isPending}
                    onClick={() => statusMutation.mutate({ id: session.id, status: "archived" })}
                  >
                    Archive
                  </Button>
                </div>
              </div>
              <pre className="mt-3 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {session.tags_json}
              </pre>
              {session.notes && <p className="mt-2 text-sm text-steel">{session.notes}</p>}
            </article>
          ))}
        </RecordList>

        <RecordList title="Session events" empty="No session events yet.">
          {events.map((event) => (
            <article key={event.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{event.message}</p>
              <p className="text-sm text-steel">{event.event_type}</p>
              <p className="break-all font-mono text-xs text-steel">{event.session_id}</p>
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {event.metadata_json}
              </pre>
            </article>
          ))}
        </RecordList>
      </section>
    </section>
  );

  function submitSession() {
    setSessionError(null);
    const mcpServerIdsJson = sessionForm.mcp_server_ids_json.trim() || "[]";
    const tagsJson = sessionForm.tags_json.trim() || "[]";
    if (!isStringArrayJson(mcpServerIdsJson)) {
      setSessionError("MCP server IDs JSON must be an array of strings.");
      return;
    }
    if (!isStringArrayJson(tagsJson)) {
      setSessionError("Session tags JSON must be an array of strings.");
      return;
    }

    createMutation.mutate({
      title: sessionForm.title.trim(),
      target_app_id: sessionForm.target_app_id.trim() || null,
      provider_id: sessionForm.provider_id.trim() || null,
      official_account_id: sessionForm.official_account_id.trim() || null,
      prompt_asset_id: sessionForm.prompt_asset_id.trim() || null,
      mcp_server_ids_json: mcpServerIdsJson,
      tags_json: tagsJson,
      status: sessionForm.status,
      notes: sessionForm.notes.trim() || null,
    });
  }

  function submitEvent() {
    setEventError(null);
    const metadataJson = eventForm.metadata_json.trim() || "{}";
    try {
      const metadata = JSON.parse(metadataJson);
      if (!metadata || Array.isArray(metadata) || typeof metadata !== "object") {
        setEventError("Event metadata JSON must be an object.");
        return;
      }
    } catch {
      setEventError("Event metadata JSON must be valid JSON.");
      return;
    }

    eventMutation.mutate({
      session_id: eventForm.session_id.trim(),
      event_type: eventForm.event_type,
      message: eventForm.message.trim(),
      metadata_json: metadataJson,
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
