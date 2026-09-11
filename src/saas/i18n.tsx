import { createContext, useContext, useState, type ReactNode } from "react";
import { Languages, Moon, Sun } from "lucide-react";
import type { SaasLocale } from "./types";

function storedPreference(key: string): string | null {
  try { return localStorage.getItem(key); } catch { return null; }
}

const LocaleContext = createContext({ locale: "zh" as SaasLocale, text: (chinese: string, english: string) => chinese || english, toggleLocale: () => {} });

export function SaasFrame({ children, embedded = false }: { children: ReactNode; embedded?: boolean }) {
  const [locale, setLocale] = useState<SaasLocale>(() => storedPreference("saas.locale") === "en" ? "en" : "zh");
  const [theme, setTheme] = useState(() => storedPreference("saas.theme") || (document.documentElement.classList.contains("dark") ? "dark" : "light"));
  function remember(key: string, value: string) { try { localStorage.setItem(key, value); } catch {} }
  const text = (chinese: string, english: string) => locale === "zh" ? chinese : english;
  function toggleLocale() { const next = locale === "zh" ? "en" : "zh"; setLocale(next); remember("saas.locale", next); }
  function toggleTheme() { const next = theme === "light" ? "dark" : "light"; setTheme(next); remember("saas.theme", next); }
  return <LocaleContext.Provider value={{ locale, text, toggleLocale }}>
    <div className={`saas-root${embedded ? " saas-embedded" : ""}`} data-theme={theme} lang={locale === "zh" ? "zh-CN" : "en"}>
      <div className="saas-preferences">
        <button type="button" className="saas-button saas-quiet" onClick={toggleLocale} aria-label={text("切换为英文", "Switch to Chinese")}><Languages size={16} /><span>{locale === "zh" ? "EN" : "中文"}</span></button>
        <button type="button" className="saas-button saas-icon-button saas-quiet" onClick={toggleTheme} aria-label={theme === "light" ? text("深色模式", "Dark mode") : text("浅色模式", "Light mode")}>{theme === "light" ? <Moon size={17} /> : <Sun size={17} />}</button>
      </div>
      {children}
    </div>
  </LocaleContext.Provider>;
}

export function useSaasLocale() { return useContext(LocaleContext); }

export function SaasLocaleProvider({
  children,
  locale,
}: {
  children: ReactNode;
  locale: SaasLocale;
}) {
  const text = (chinese: string, english: string) => (locale === "en" ? english : chinese);
  return (
    <LocaleContext.Provider value={{ locale, text, toggleLocale: () => {} }}>
      {children}
    </LocaleContext.Provider>
  );
}
