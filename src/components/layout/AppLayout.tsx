type AppLayoutProps = {
  children: React.ReactNode;
};

export function AppLayout({ children }: AppLayoutProps) {
  return (
    <main className="min-h-screen bg-[radial-gradient(circle_at_top_left,#f8ddc8,transparent_32%),linear-gradient(135deg,#f4efe5,#dfe7df)] px-6 py-8 font-body text-ink">
      <div className="mx-auto max-w-7xl">{children}</div>
    </main>
  );
}
