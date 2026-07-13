import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Button } from "../components/ui/Button";
import {
  createSyncProfile,
  createSyncSnapshot,
  listSyncProfiles,
  listSyncSnapshots,
} from "../lib/api/client";
import type { NewSyncProfile, CreateSyncSnapshotRequest } from "../lib/api/types";

type SyncProfileFormState = {
  name: string;
  provider: NewSyncProfile["provider"];
  endpoint_url: string;
  auth_ref: string;
  scope_json: string;
  enabled: boolean;
  notes: string;
};

type SyncSnapshotFormState = {
  profile_id: string;
  direction: CreateSyncSnapshotRequest["direction"];
  artifact_ref: string;
};

const initialSyncProfileForm: SyncProfileFormState = {
  name: "",
  provider: "webdav",
  endpoint_url: "https://sync.example.com/ai-switch",
  auth_ref: "",
  scope_json: "{\"providers\":true,\"accounts\":true,\"routing\":true}",
  enabled: true,
  notes: "",
};

const initialSyncSnapshotForm: SyncSnapshotFormState = {
  profile_id: "",
  direction: "export",
  artifact_ref: "",
};

export function SyncScreen() {
  const queryClient = useQueryClient();
  const [profileForm, setProfileForm] = useState<SyncProfileFormState>(initialSyncProfileForm);
  const [snapshotForm, setSnapshotForm] =
    useState<SyncSnapshotFormState>(initialSyncSnapshotForm);
  const [profileError, setProfileError] = useState<string | null>(null);
  const [snapshotError, setSnapshotError] = useState<string | null>(null);

  const profilesQuery = useQuery({
    queryKey: ["sync-profiles"],
    queryFn: listSyncProfiles,
  });
  const snapshotsQuery = useQuery({
    queryKey: ["sync-snapshots"],
    queryFn: listSyncSnapshots,
  });

  const profileMutation = useMutation({
    mutationFn: (request: NewSyncProfile) => createSyncProfile(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["sync-profiles"] });
      setProfileForm(initialSyncProfileForm);
      setProfileError(null);
    },
  });
  const snapshotMutation = useMutation({
    mutationFn: (request: CreateSyncSnapshotRequest) => createSyncSnapshot(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["sync-snapshots"] });
      setSnapshotForm(initialSyncSnapshotForm);
      setSnapshotError(null);
    },
  });

  const profiles = profilesQuery.data ?? [];
  const snapshots = snapshotsQuery.data ?? [];

  if (profilesQuery.isLoading || snapshotsQuery.isLoading) {
    return <p className="text-steel">Loading sync records...</p>;
  }

  if (profilesQuery.error || snapshotsQuery.error) {
    return <p className="text-ember">Could not load sync records.</p>;
  }

  return (
    <section className="space-y-4">
      <div>
        <h1 className="font-display text-3xl font-semibold text-ink">Sync</h1>
        <p className="text-steel">
          Store sync profile metadata and local snapshot manifests without contacting remote
          services.
        </p>
      </div>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Sync profile</h2>
          <p className="text-sm text-steel">
            D5 stores profile metadata only. Credentials must use `env://` or `secret://`
            references.
          </p>
        </div>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Profile name</span>
          <input
            value={profileForm.name}
            onChange={(event) =>
              setProfileForm((current) => ({ ...current, name: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
            placeholder="Team WebDAV"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Provider</span>
          <select
            value={profileForm.provider}
            onChange={(event) =>
              setProfileForm((current) => ({
                ...current,
                provider: event.target.value as NewSyncProfile["provider"],
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          >
            <option value="webdav">webdav</option>
            <option value="local_folder">local_folder</option>
            <option value="s3">s3</option>
            <option value="git">git</option>
          </select>
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Endpoint URL</span>
          <input
            value={profileForm.endpoint_url}
            onChange={(event) =>
              setProfileForm((current) => ({ ...current, endpoint_url: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Auth ref</span>
          <input
            value={profileForm.auth_ref}
            onChange={(event) =>
              setProfileForm((current) => ({ ...current, auth_ref: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
            placeholder="env://WEBDAV_TOKEN"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Scope JSON</span>
          <textarea
            value={profileForm.scope_json}
            onChange={(event) =>
              setProfileForm((current) => ({ ...current, scope_json: event.target.value }))
            }
            className="min-h-24 w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 font-mono text-sm outline-none focus:border-moss"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Profile notes</span>
          <input
            value={profileForm.notes}
            onChange={(event) =>
              setProfileForm((current) => ({ ...current, notes: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          />
        </label>
        <label className="flex items-center gap-2 text-sm font-semibold text-ink">
          <input
            type="checkbox"
            checked={profileForm.enabled}
            onChange={(event) =>
              setProfileForm((current) => ({ ...current, enabled: event.target.checked }))
            }
          />
          <span>Enabled</span>
        </label>
        <div className="flex items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={profileMutation.isPending} onClick={submitProfile}>
            Create sync profile
          </Button>
          {profileError && <p className="text-sm text-ember">{profileError}</p>}
          {profileMutation.error && !profileError && (
            <p className="text-sm text-ember">Could not create sync profile.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 rounded-3xl border border-ink/10 bg-white/75 p-5 shadow-sm lg:grid-cols-2">
        <div className="lg:col-span-2">
          <h2 className="font-display text-xl font-semibold text-ink">Snapshot manifest</h2>
          <p className="text-sm text-steel">
            Capture current local counts as a snapshot record. No upload occurs in D5.
          </p>
        </div>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Profile ID</span>
          <input
            value={snapshotForm.profile_id}
            onChange={(event) =>
              setSnapshotForm((current) => ({ ...current, profile_id: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
            placeholder="sync-1"
          />
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink">
          <span>Direction</span>
          <select
            value={snapshotForm.direction}
            onChange={(event) =>
              setSnapshotForm((current) => ({
                ...current,
                direction: event.target.value as CreateSyncSnapshotRequest["direction"],
              }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
          >
            <option value="export">export</option>
            <option value="import">import</option>
          </select>
        </label>
        <label className="space-y-2 text-sm font-semibold text-ink lg:col-span-2">
          <span>Artifact ref</span>
          <input
            value={snapshotForm.artifact_ref}
            onChange={(event) =>
              setSnapshotForm((current) => ({ ...current, artifact_ref: event.target.value }))
            }
            className="w-full rounded-2xl border border-ink/10 bg-white px-4 py-3 outline-none focus:border-moss"
            placeholder="sync://snapshot/2026-07-13"
          />
        </label>
        <div className="flex items-center gap-3 lg:col-span-2">
          <Button type="button" disabled={snapshotMutation.isPending} onClick={submitSnapshot}>
            Record snapshot manifest
          </Button>
          {snapshotError && <p className="text-sm text-ember">{snapshotError}</p>}
          {snapshotMutation.error && !snapshotError && (
            <p className="text-sm text-ember">Could not record snapshot.</p>
          )}
        </div>
      </section>

      <section className="grid gap-4 xl:grid-cols-2">
        <RecordList title="Sync profiles" empty="No sync profiles yet.">
          {profiles.map((profile) => (
            <article key={profile.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{profile.name}</p>
              <p className="text-sm text-steel">{profile.provider}</p>
              {profile.endpoint_url && (
                <p className="break-all font-mono text-xs text-steel">{profile.endpoint_url}</p>
              )}
              {profile.notes && <p className="mt-2 text-sm text-steel">{profile.notes}</p>}
            </article>
          ))}
        </RecordList>

        <RecordList title="Snapshot manifests" empty="No sync snapshots yet.">
          {snapshots.map((snapshot) => (
            <article key={snapshot.id} className="rounded-2xl bg-paper/70 p-4">
              <p className="font-semibold text-ink">{snapshot.direction}</p>
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {snapshot.item_counts_json}
              </pre>
              <pre className="mt-2 overflow-x-auto rounded-xl bg-white/70 p-2 font-mono text-xs text-ink">
                {snapshot.manifest_json}
              </pre>
            </article>
          ))}
        </RecordList>
      </section>
    </section>
  );

  function submitProfile() {
    setProfileError(null);
    const scopeJson = profileForm.scope_json.trim() || "{}";
    try {
      const scope = JSON.parse(scopeJson);
      if (!scope || Array.isArray(scope) || typeof scope !== "object") {
        setProfileError("Scope JSON must be an object.");
        return;
      }
    } catch {
      setProfileError("Scope JSON must be valid JSON.");
      return;
    }

    profileMutation.mutate({
      name: profileForm.name.trim(),
      provider: profileForm.provider,
      endpoint_url: profileForm.endpoint_url.trim() || null,
      auth_ref: profileForm.auth_ref.trim() || null,
      scope_json: scopeJson,
      enabled: profileForm.enabled,
      notes: profileForm.notes.trim() || null,
    });
  }

  function submitSnapshot() {
    setSnapshotError(null);
    snapshotMutation.mutate({
      profile_id: snapshotForm.profile_id.trim() || null,
      direction: snapshotForm.direction,
      artifact_ref: snapshotForm.artifact_ref.trim() || null,
    });
  }
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
