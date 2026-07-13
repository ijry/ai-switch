export function DashboardScreen() {
  return (
    <section className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
      <div className="border-b border-stone-200 px-4 py-3">
        <p className="text-[11px] font-semibold uppercase tracking-wide text-stone-400">Overview</p>
        <h1 className="mt-0.5 text-lg font-semibold text-stone-950">AI Switch</h1>
      </div>
      <div className="grid gap-2 p-3 sm:grid-cols-3">
        {["Batch imports", "Provider metadata", "Target adapters"].map((label) => (
          <div key={label} className="rounded-xl border border-stone-200 bg-stone-50 px-3 py-2">
            <p className="text-[13px] font-semibold text-stone-950">{label}</p>
            <p className="mt-0.5 text-[12px] text-stone-500">Ready</p>
          </div>
        ))}
      </div>
    </section>
  );
}
