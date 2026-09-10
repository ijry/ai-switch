import { useEffect, useMemo, useState } from "react";
import { Archive, ImagePlus, LoaderCircle, Plus, Sparkles, Trash2 } from "lucide-react";
import { useI18n } from "../lib/i18n";
import { imagegenApi } from "./api";
import type { ImageAsset, ImageConversation, ImageModelOption, ImageSession } from "./types";
import "./imagegen.css";

export function ImageGenerationScreen() {
  const { t } = useI18n();
  const [sessions, setSessions] = useState<ImageSession[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [conversation, setConversation] = useState<ImageConversation | null>(null);
  const [platform, setPlatform] = useState<"codex" | "gemini">("codex");
  const [models, setModels] = useState<ImageModelOption[]>([]);
  const [model, setModel] = useState("");
  const [prompt, setPrompt] = useState("");
  const [count, setCount] = useState(1);
  const [size, setSize] = useState("1024x1024");
  const [quality, setQuality] = useState("auto");
  const [assetUrls, setAssetUrls] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refreshSessions = async () => {
    const next = await imagegenApi.listSessions();
    setSessions(next);
    setActiveId((current) => current ?? next[0]?.id ?? null);
  };

  useEffect(() => {
    void refreshSessions().catch((reason) => setError(String(reason)));
  }, []);

  useEffect(() => {
    if (!activeId) {
      setConversation(null);
      return;
    }
    void imagegenApi.conversation(activeId)
      .then((next) => {
        setConversation(next);
        setPlatform(next.session.platform);
      })
      .catch((reason) => setError(String(reason)));
  }, [activeId]);

  useEffect(() => {
    void imagegenApi.models(platform)
      .then((next) => {
        setModels(next);
        setModel((current) => (next.some((item) => item.id === current) ? current : next[0]?.id ?? ""));
      })
      .catch((reason) => {
        setModels([]);
        setModel("");
        setError(String(reason));
      });
  }, [platform]);

  useEffect(() => {
    const missing = conversation?.assets.filter((asset) => !assetUrls[asset.id]) ?? [];
    for (const asset of missing) {
      void imagegenApi.asset(asset.id)
        .then((result) => setAssetUrls((current) => ({
          ...current,
          [asset.id]: "data:" + result.asset.mime_type + ";base64," + result.data_base64,
        })))
        .catch(() => undefined);
    }
  }, [assetUrls, conversation]);

  const assetsByMessage = useMemo(() => {
    const map = new Map<string, ImageAsset[]>();
    for (const asset of conversation?.assets ?? []) {
      map.set(asset.message_id, [...(map.get(asset.message_id) ?? []), asset]);
    }
    return map;
  }, [conversation]);

  const createSession = async () => {
    const title = window.prompt(t("imagegen.sessionName"), t("imagegen.newSessionTitle"))?.trim();
    if (!title) return;
    setBusy(true);
    setError(null);
    try {
      const created = await imagegenApi.createSession({ title, platform });
      await refreshSessions();
      setActiveId(created.id);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const archiveSession = async () => {
    if (!activeId) return;
    setBusy(true);
    setError(null);
    try {
      await imagegenApi.updateSession({ id: activeId, archived: true });
      setActiveId(null);
      setConversation(null);
      await refreshSessions();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const deleteSession = async () => {
    if (!activeId || !window.confirm(t("imagegen.deleteConfirm"))) return;
    setBusy(true);
    setError(null);
    try {
      await imagegenApi.deleteSession(activeId);
      setActiveId(null);
      setConversation(null);
      await refreshSessions();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const generate = async () => {
    if (!activeId || !model || !prompt.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await imagegenApi.generate({ session_id: activeId, model, prompt: prompt.trim(), count, size, quality });
      setPrompt("");
      setConversation(await imagegenApi.conversation(activeId));
      await refreshSessions();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="imagegen-shell">
      <aside className="imagegen-sessions">
        <div className="imagegen-panel-title">
          <div>
            <p>{t("imagegen.kicker")}</p>
            <h1>{t("imagegen.title")}</h1>
          </div>
          <button onClick={() => void createSession()} title={t("imagegen.newSession")} type="button"><Plus /></button>
        </div>
        <label className="imagegen-field">
          <span>{t("imagegen.platform")}</span>
          <select disabled={Boolean(conversation)} onChange={(event) => setPlatform(event.target.value as "codex" | "gemini")} value={platform}>
            <option value="codex">Codex</option>
            <option value="gemini">Gemini</option>
          </select>
        </label>
        <div className="imagegen-session-list">
          {sessions.map((session) => (
            <button className={session.id === activeId ? "active" : ""} key={session.id} onClick={() => setActiveId(session.id)} type="button">
              <span>{session.title}</span>
              <small>{session.platform}</small>
            </button>
          ))}
        </div>
      </aside>
      <main className="imagegen-workspace">
        <header>
          <div>
            <h2>{conversation?.session.title ?? t("imagegen.startTitle")}</h2>
            <p>{t("imagegen.schedule")}</p>
          </div>
          {conversation && (
            <div className="imagegen-actions">
              <button onClick={() => void archiveSession()} title={t("imagegen.archive")} type="button"><Archive /></button>
              <button onClick={() => void deleteSession()} title={t("imagegen.delete")} type="button"><Trash2 /></button>
            </div>
          )}
        </header>
        <div className="imagegen-timeline">
          {conversation?.messages.length ? conversation.messages.map((message) => (
            <article className={message.role} key={message.id}>
              <div className="imagegen-message-meta">
                <strong>{message.role === "user" ? t("imagegen.you") : t("imagegen.assistant")}</strong>
                <span>{message.status}</span>
              </div>
              {message.prompt && <p>{message.prompt}</p>}
              {message.error_message && <p className="imagegen-error">{message.error_message}</p>}
              <div className="imagegen-grid">
                {(assetsByMessage.get(message.id) ?? []).map((asset) => assetUrls[asset.id] ? (
                  <a download key={asset.id} href={assetUrls[asset.id]}>
                    <img alt={t("imagegen.generatedImage")} src={assetUrls[asset.id]} />
                  </a>
                ) : (
                  <div className="imagegen-placeholder" key={asset.id}><LoaderCircle /></div>
                ))}
              </div>
            </article>
          )) : (
            <div className="imagegen-empty">
              <ImagePlus />
              <h3>{t("imagegen.emptyTitle")}</h3>
              <p>{t("imagegen.emptyDescription")}</p>
            </div>
          )}
        </div>
        <footer className="imagegen-composer">
          <textarea disabled={!conversation || busy} onChange={(event) => setPrompt(event.target.value)} placeholder={t("imagegen.promptPlaceholder")} value={prompt} />
          <div className="imagegen-controls">
            <select disabled={!conversation || busy || models.length === 0} onChange={(event) => setModel(event.target.value)} value={model}>
              {models.length ? models.map((item) => <option key={item.id} value={item.id}>{item.id} → {item.upstream_model}</option>) : <option value="">{t("imagegen.noModels")}</option>}
            </select>
            <select onChange={(event) => setSize(event.target.value)} value={size}>
              <option>1024x1024</option>
              <option>1536x1024</option>
              <option>1024x1536</option>
            </select>
            <select onChange={(event) => setQuality(event.target.value)} value={quality}>
              <option value="auto">{t("imagegen.qualityAuto")}</option>
              <option value="high">{t("imagegen.qualityHigh")}</option>
              <option value="medium">{t("imagegen.qualityMedium")}</option>
              <option value="low">{t("imagegen.qualityLow")}</option>
            </select>
            <input aria-label={t("imagegen.count")} max={10} min={1} onChange={(event) => setCount(Number(event.target.value))} type="number" value={count} />
            <button disabled={!conversation || !model || !prompt.trim() || busy} onClick={() => void generate()} type="button">
              {busy ? <LoaderCircle className="animate-spin" /> : <Sparkles />}
              {t("imagegen.generate")}
            </button>
          </div>
          {error && <p className="imagegen-error">{error}</p>}
        </footer>
      </main>
    </section>
  );
}
