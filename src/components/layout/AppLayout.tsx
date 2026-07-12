type AppLayoutProps = {
  children: React.ReactNode;
  activeScreen: string;
  onNavigate: (screen: string) => void;
};

const screens = ["Dashboard", "Batches", "Providers", "Accounts", "Imports", "Targets", "Settings", "Log"];

export function AppLayout({ children, activeScreen, onNavigate }: AppLayoutProps) {
  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top_left,#f8ddc8,transparent_32%),linear-gradient(135deg,#f4efe5,#dfe7df)] px-6 py-8 font-body text-ink">
      <div className="mx-auto grid max-w-7xl gap-6 lg:grid-cols-[240px_1fr]">
        <aside className="rounded-3xl border border-ink/10 bg-white/65 p-4 shadow-sm shadow-ink/5">
          <p className="px-3 pb-4 font-display text-2xl font-semibold">AI Switch</p>
          <nav className="space-y-1">
            {screens.map((screen) => (
              <button
                key={screen}
                type="button"
                onClick={() => onNavigate(screen)}
                className={`w-full cursor-pointer rounded-2xl px-3 py-2 text-left text-sm font-medium outline-none transition-colors duration-200 focus-visible:ring-2 focus-visible:ring-moss/50 ${
                  activeScreen === screen ? "bg-ink text-paper" : "text-steel hover:bg-white"
                }`}
              >
                {screen}
              </button>
            ))}
          </nav>
        </aside>
        <div>{children}</div>
      </div>
    </main>
  );
}
