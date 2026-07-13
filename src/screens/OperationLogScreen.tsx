export function OperationLogScreen() {
  return (
    <section className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
      <div className="border-b border-stone-200 px-4 py-3">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Activity</p>
        <h1 className="mt-0.5 text-lg font-semibold text-stone-950">Operation Log</h1>
      </div>
      <p className="px-4 py-3 text-[13px] text-stone-500">
        Import and config write events appear here when services emit them.
      </p>
    </section>
  );
}
