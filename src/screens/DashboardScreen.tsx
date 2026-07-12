export function DashboardScreen() {
  return (
    <section className="overflow-hidden rounded-3xl border border-ink/10 bg-white/75 p-8 shadow-xl shadow-ink/5">
      <p className="text-sm uppercase tracking-[0.3em] text-moss">Foundation</p>
      <h1 className="mt-3 font-display text-4xl font-semibold text-ink">AI Switch</h1>
      <p className="mt-4 max-w-2xl text-base leading-7 text-steel">
        Batch-first provider and official account switching foundation.
      </p>
      <div className="mt-8 grid gap-3 sm:grid-cols-3">
        {["Batch imports", "Provider metadata", "Target adapters"].map((label) => (
          <div key={label} className="rounded-2xl bg-paper/70 p-4">
            <p className="text-sm font-semibold text-ink">{label}</p>
            <p className="mt-1 text-sm text-steel">Phase A foundation</p>
          </div>
        ))}
      </div>
    </section>
  );
}
