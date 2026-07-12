import { AppLayout } from "./components/layout/AppLayout";

export function App() {
  return (
    <AppLayout>
      <section className="rounded-3xl border border-ink/10 bg-white/70 p-8 shadow-xl shadow-ink/5">
        <p className="text-sm uppercase tracking-[0.3em] text-moss">Foundation</p>
        <h1 className="mt-3 font-display text-4xl font-semibold text-ink">AI Switch</h1>
        <p className="mt-4 max-w-2xl text-base leading-7 text-steel">
          Batch-first provider and official account switching foundation.
        </p>
      </section>
    </AppLayout>
  );
}
