import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createUpdateChannel,
  createUpdateCheck,
  listUpdateChannels,
  listUpdateChecks,
} from "../lib/api/client";
import type { NewUpdateChannel, NewUpdateCheck, UpdateCheckStatus } from "../lib/api/types";

type ChannelFormState = {
  name: string;
  channel: NewUpdateChannel["channel"];
  feed_url: string;
  enabled: boolean;
  notes: string;
};

type CheckFormState = {
  channel_id: string;
  current_version: string;
  latest_version: string;
  status: UpdateCheckStatus;
  release_notes_url: string;
  details_json: string;
};

const initialChannelForm: ChannelFormState = {
  name: "",
  channel: "stable",
  feed_url: "https://updates.example.com/stable.json",
  enabled: true,
  notes: "",
};

const initialCheckForm: CheckFormState = {
  channel_id: "",
  current_version: "0.1.0",
  latest_version: "",
  status: "unknown",
  release_notes_url: "",
  details_json: "{}",
};

export function UpdatesScreen() {
  const queryClient = useQueryClient();
  const [channelForm, setChannelForm] = useState<ChannelFormState>(initialChannelForm);
  const [checkForm, setCheckForm] = useState<CheckFormState>(initialCheckForm);
  const [checkError, setCheckError] = useState<string | null>(null);

  const channelsQuery = useQuery({
    queryKey: ["update-channels"],
    queryFn: listUpdateChannels,
  });
  const checksQuery = useQuery({
    queryKey: ["update-checks"],
    queryFn: listUpdateChecks,
  });

  const channelMutation = useMutation({
    mutationFn: (request: NewUpdateChannel) => createUpdateChannel(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["update-channels"] });
      setChannelForm(initialChannelForm);
    },
  });
  const checkMutation = useMutation({
    mutationFn: (request: NewUpdateCheck) => createUpdateCheck(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["update-checks"] });
      setCheckForm(initialCheckForm);
      setCheckError(null);
    },
  });

  const channels = channelsQuery.data ?? [];
  const checks = checksQuery.data ?? [];

  if (channelsQuery.isLoading || checksQuery.isLoading) {
    return <p className="text-steel">Loading update records...</p>;
  }

  if (channelsQuery.error || checksQuery.error) {
    return <p className="text-ember">Could not load update records.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Updates</h1>
        <p className="text-steel">
          Store updater metadata and manual check results without downloading packages or running
          installers.
        </p>
      </div>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Update channel</h2>
          <p className="text-sm text-steel">
            Feed URLs are metadata only in D7. No network checks run from this screen.
          </p>
        </div>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Channel name</span>
          <input
            value={channelForm.name}
            onChange={(event) =>
              setChannelForm((current) => ({ ...current, name: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
            placeholder="Stable"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Channel</span>
          <select
            value={channelForm.channel}
            onChange={(event) =>
              setChannelForm((current) => ({
                ...current,
                channel: event.target.value as NewUpdateChannel["channel"],
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          >
            <option value="stable">stable</option>
            <option value="beta">beta</option>
            <option value="nightly">nightly</option>
          </select>
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Feed URL</span>
          <input
            value={channelForm.feed_url}
            onChange={(event) =>
              setChannelForm((current) => ({ ...current, feed_url: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Channel notes</span>
          <input
            value={channelForm.notes}
            onChange={(event) =>
              setChannelForm((current) => ({ ...current, notes: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <label className="flex items-center gap-2 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={channelForm.enabled}
            onChange={(event) =>
              setChannelForm((current) => ({ ...current, enabled: event.target.checked }))
            }
          />
          <span>Enabled</span>
        </label>
        <div className="flex items-center gap-3 lg:col-span-2">
          <Button
            type="button"
            disabled={channelMutation.isPending}
            onClick={() =>
              channelMutation.mutate({
                name: channelForm.name.trim(),
                channel: channelForm.channel,
                feed_url: channelForm.feed_url.trim() || null,
                enabled: channelForm.enabled,
                notes: channelForm.notes.trim() || null,
              })
            }
          >
            Create update channel
          </Button>
          {channelMutation.error && (
            <p className="text-sm text-ember">Could not create update channel.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Manual check result</h2>
          <p className="text-sm text-steel">
            Record what an external check found. D7 does not query update feeds itself.
          </p>
        </div>
        <TextField
          label="Update channel ID"
          value={checkForm.channel_id}
          onChange={(value) => setCheckForm((current) => ({ ...current, channel_id: value }))}
          placeholder="update-channel-1"
        />
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Check status</span>
          <select
            value={checkForm.status}
            onChange={(event) =>
              setCheckForm((current) => ({
                ...current,
                status: event.target.value as UpdateCheckStatus,
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          >
            <option value="unknown">unknown</option>
            <option value="up_to_date">up_to_date</option>
            <option value="available">available</option>
            <option value="error">error</option>
          </select>
        </label>
        <TextField
          label="Current version"
          value={checkForm.current_version}
          onChange={(value) => setCheckForm((current) => ({ ...current, current_version: value }))}
        />
        <TextField
          label="Latest version"
          value={checkForm.latest_version}
          onChange={(value) => setCheckForm((current) => ({ ...current, latest_version: value }))}
          placeholder="0.1.1"
        />
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Release notes URL</span>
          <input
            value={checkForm.release_notes_url}
            onChange={(event) =>
              setCheckForm((current) => ({ ...current, release_notes_url: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
            placeholder="https://updates.example.com/releases/0.1.1"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Details JSON</span>
          <textarea
            value={checkForm.details_json}
            onChange={(event) =>
              setCheckForm((current) => ({ ...current, details_json: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm outline-none focus:border-moss"
          />
        </label>
        <div className="flex flex-wrap items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={checkMutation.isPending} onClick={submitCheck}>
            Record update check
          </Button>
          {checkError && <p className="text-sm text-ember">{checkError}</p>}
          {checkMutation.error && !checkError && (
            <p className="text-sm text-ember">Could not record update check.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
        <RecordList title="Update channels" empty="No update channels yet.">
          {channels.map((channel) => (
            <article key={channel.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{channel.name}</p>
              <p className="text-sm text-steel">{channel.channel}</p>
              {channel.feed_url && (
                <p className="break-all font-mono text-xs text-steel">{channel.feed_url}</p>
              )}
              {channel.notes && <p className="mt-2 text-sm text-steel">{channel.notes}</p>}
            </article>
          ))}
        </RecordList>

        <RecordList title="Update checks" empty="No update checks yet.">
          {checks.map((check) => (
            <article key={check.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{check.status}</p>
              <p className="text-sm text-steel">
                {check.current_version} to {check.latest_version ?? "unknown"}
              </p>
              {check.release_notes_url && (
                <p className="break-all font-mono text-xs text-steel">
                  {check.release_notes_url}
                </p>
              )}
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {check.details_json}
              </pre>
            </article>
          ))}
        </RecordList>
      </section>
    </section>
  );

  function submitCheck() {
    setCheckError(null);
    if (checkForm.status === "available" && checkForm.latest_version.trim().length === 0) {
      setCheckError("Available updates require a latest version.");
      return;
    }

    const detailsJson = checkForm.details_json.trim() || "{}";
    try {
      const details = JSON.parse(detailsJson);
      if (!details || Array.isArray(details) || typeof details !== "object") {
        setCheckError("Details JSON must be an object.");
        return;
      }
    } catch {
      setCheckError("Details JSON must be valid JSON.");
      return;
    }

    checkMutation.mutate({
      channel_id: checkForm.channel_id.trim() || null,
      current_version: checkForm.current_version.trim(),
      latest_version: checkForm.latest_version.trim() || null,
      status: checkForm.status,
      release_notes_url: checkForm.release_notes_url.trim() || null,
      details_json: detailsJson,
    });
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
