import { useMutation, useQuery } from "@tanstack/react-query";
import { getSettings, saveSettings } from "../lib/api/client";

export function SettingsScreen() {
  const settingsQuery = useQuery({ queryKey: ["settings"], queryFn: getSettings });
  const saveMutation = useMutation({ mutationFn: saveSettings });

  if (settingsQuery.isLoading) {
    return <p className="text-steel">Loading settings...</p>;
  }

  if (!settingsQuery.data) {
    return <p className="text-ember">Could not load settings.</p>;
  }

  const settings = settingsQuery.data;

  return (
    <section className="space-y-4 rounded-3xl border border-ink/10 bg-white/70 p-6 shadow-sm shadow-ink/5">
      <h1 className="font-display text-3xl font-semibold text-ink">Settings</h1>
      <p className="text-sm text-steel">Data directory: {settings.data_dir}</p>
      <button
        type="button"
        className="rounded-full bg-ink px-4 py-2 text-sm font-semibold text-paper transition-colors duration-200 hover:bg-ink/90"
        onClick={() =>
          saveMutation.mutate({
            ...settings,
            theme: settings.theme === "dark" ? "system" : "dark",
          })
        }
      >
        Toggle theme value
      </button>
      {saveMutation.data && <p className="text-moss">Settings saved.</p>}
      {saveMutation.error && <p className="text-ember">Could not save settings.</p>}
    </section>
  );
}
