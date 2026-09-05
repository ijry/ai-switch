import {
  BadgeCheck,
  Check,
  Copy,
  ExternalLink,
  Github,
  Globe,
  Heart,
  LifeBuoy,
  Link2,
  Rocket,
  ScrollText,
  Share2,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState } from "react";
import {
  ABOUT_LINKS,
  FRIENDLY_LINKS,
  OFFICIAL_SITE_URL,
  OPEN_SOURCE_CREDITS,
} from "../components/about/catalog";
import { AiSwitchLogo } from "../components/brand/AiSwitchLogo";
import { appVersion } from "../lib/appVersion";
import { copyPlainText } from "../lib/copyToClipboard";
import { useI18n, type TranslationKey } from "../lib/i18n";
import { openExternal } from "../lib/openExternal";

type LinkScope = "links" | "credits" | "friendly";
type CopyState = "idle" | "copied" | "failed";

const LINK_ICONS: Partial<Record<TranslationKey, LucideIcon>> = {
  "about.linkWebsite": Globe,
  "about.linkRepository": Github,
  "about.linkReleases": Rocket,
  "about.linkIssues": LifeBuoy,
};

const CARD_CLASS = "rounded-2xl border border-stone-200 bg-white/82 p-4 shadow-sm";
const CHIP_CLASS =
  "inline-flex items-center gap-1.5 rounded-full bg-stone-50 px-2.5 py-1 text-[11px] font-semibold text-stone-600 ring-1 ring-stone-200";
// Separators are drawn with `ring-*`: the app ships no CSS reset, so `border-*`
// only sets a width and stays invisible.
const TILE_CLASS =
  "flex items-center justify-between gap-2 rounded-xl bg-stone-50 px-3 py-2.5 text-left ring-1 ring-stone-200 motion-control hover:bg-white hover:ring-stone-300 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400";
const SECTION_LABEL_CLASS = "text-[11px] font-semibold uppercase tracking-wide text-stone-400";

function ExternalTile({
  label,
  subtitle,
  url,
  openLabel,
  onOpen,
  inline = false,
}: {
  label: string;
  subtitle: string;
  url: string;
  openLabel: string;
  onOpen: (url: string) => void;
  /** Puts the subtitle on the label line, for short values like a license. */
  inline?: boolean;
}) {
  return (
    <button
      aria-label={openLabel}
      className={`${TILE_CLASS} ${inline ? "py-2" : "py-2.5"}`}
      onClick={() => onOpen(url)}
      title={url}
      type="button"
    >
      {inline ? (
        <span className="flex min-w-0 items-baseline gap-2">
          <span className="shrink-0 text-[13px] font-semibold text-stone-950">{label}</span>
          <span className="truncate text-[11px] text-stone-500">{subtitle}</span>
        </span>
      ) : (
        <span className="min-w-0">
          <span className="block truncate text-[13px] font-semibold text-stone-950">{label}</span>
          <span className="mt-0.5 block break-all text-[11px] text-stone-500">{subtitle}</span>
        </span>
      )}
      <ExternalLink aria-hidden="true" className="h-3.5 w-3.5 shrink-0 text-stone-400" />
    </button>
  );
}

export function AboutScreen() {
  const { t } = useI18n();
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const [openErrorScope, setOpenErrorScope] = useState<LinkScope | null>(null);
  const shareMessage = t("about.share.message", { url: OFFICIAL_SITE_URL });
  const versionLabel = t("about.version", { version: appVersion });

  useEffect(() => {
    if (copyState !== "copied") {
      return;
    }
    const timeout = window.setTimeout(() => setCopyState("idle"), 2400);
    return () => window.clearTimeout(timeout);
  }, [copyState]);

  const openLink = (url: string, scope: LinkScope) => {
    setOpenErrorScope(null);
    // The opener plugin rejects when the shell refuses the url; a dead link with
    // no explanation looks like the button is broken.
    void openExternal(url).catch(() => setOpenErrorScope(scope));
  };

  const handleCopyShareText = async () => {
    setCopyState((await copyPlainText(shareMessage)) ? "copied" : "failed");
  };

  const openFailure = (scope: LinkScope, className = "mt-2") =>
    openErrorScope === scope ? (
      <p className={`${className} text-[12px] font-medium text-red-700`} role="alert">
        {t("about.openFailed")}
      </p>
    ) : null;

  return (
    <section className="space-y-3">
      <div className="rounded-2xl border border-stone-200 bg-white/82 shadow-sm">
        <div className="px-4 py-3">
          <p className={SECTION_LABEL_CLASS}>{t("about.kicker")}</p>
          <div className="mt-1.5 flex flex-wrap items-center gap-2.5">
            <AiSwitchLogo className="h-10 w-10 shrink-0 rounded-2xl shadow-sm" />
            <h1 className="text-lg font-semibold tracking-tight text-stone-950">AI Switch</h1>
            <span
              className="inline-flex items-center gap-1.5 rounded-full bg-stone-900 px-2.5 py-1 text-[11px] font-semibold text-white"
              title={t("about.versionLabel")}
            >
              <BadgeCheck aria-hidden="true" className="h-3.5 w-3.5" />
              <span className="sr-only">{t("about.versionLabel")}</span>
              {versionLabel}
            </span>
            <span className={CHIP_CLASS}>
              <ScrollText aria-hidden="true" className="h-3.5 w-3.5 text-stone-400" />
              {t("about.licenseLabel")} · {t("about.license")}
            </span>
          </div>
          <p className="mt-2 max-w-3xl text-[13px] leading-6 text-stone-600">{t("about.tagline")}</p>
        </div>
        <div className="grid gap-2 px-3 pb-3 sm:grid-cols-2 xl:grid-cols-4">
          {ABOUT_LINKS.map((link) => {
            const Icon = LINK_ICONS[link.labelKey];
            return (
              <button
                aria-label={t("about.openLink", { name: t(link.labelKey) })}
                className={TILE_CLASS}
                key={link.url}
                onClick={() => openLink(link.url, "links")}
                title={link.url}
                type="button"
              >
                <span className="flex min-w-0 items-center gap-2.5">
                  <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-white text-stone-700 shadow-sm ring-1 ring-stone-200">
                    {Icon ? <Icon aria-hidden="true" className="h-3.5 w-3.5" /> : null}
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate text-[13px] font-semibold text-stone-950">
                      {t(link.labelKey)}
                    </span>
                    <span className="mt-0.5 block break-all text-[11px] text-stone-500">{link.url}</span>
                  </span>
                </span>
                <ExternalLink aria-hidden="true" className="h-3.5 w-3.5 shrink-0 text-stone-400" />
              </button>
            );
          })}
        </div>
        {openFailure("links", "px-4 pb-3")}
      </div>

      <div className="rounded-2xl border border-amber-200 bg-gradient-to-br from-amber-50 via-white to-emerald-50 p-4 shadow-sm ring-1 ring-amber-200">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="grid h-8 w-8 shrink-0 place-items-center rounded-xl bg-amber-500 text-white shadow-sm">
                <Share2 aria-hidden="true" className="h-4 w-4" />
              </span>
              <h2 className="text-[15px] font-semibold text-stone-950">{t("about.share.title")}</h2>
            </div>
            <p className="mt-1.5 max-w-2xl text-[13px] leading-6 text-stone-600">
              {t("about.share.subtitle")}
            </p>
          </div>
          <button
            className={`inline-flex items-center gap-2 rounded-xl px-4 py-2.5 text-[13px] font-semibold text-white shadow-sm motion-control focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 ${
              copyState === "copied" ? "bg-emerald-600" : "bg-stone-900 hover:bg-stone-800"
            }`}
            onClick={() => void handleCopyShareText()}
            type="button"
          >
            {copyState === "copied" ? (
              <Check aria-hidden="true" className="h-4 w-4" />
            ) : (
              <Copy aria-hidden="true" className="h-4 w-4" />
            )}
            {copyState === "copied" ? t("about.share.copied") : t("about.share.copy")}
          </button>
        </div>
        <p className={`mt-3 ${SECTION_LABEL_CLASS}`}>{t("about.share.preview")}</p>
        <p
          className="mt-1 select-all whitespace-pre-wrap break-words rounded-xl bg-white/90 px-3 py-2.5 text-[13px] leading-6 text-stone-700 ring-1 ring-amber-200"
          data-testid="about-share-text"
        >
          {shareMessage}
        </p>
        <p aria-live="polite" className="mt-2 text-[12px] font-medium text-red-700" role="status">
          {copyState === "failed" ? t("about.share.failed") : null}
          {/* The button label already flips to "copied", but a label change is not
              reliably announced, so the confirmation also lands in the live region. */}
          {copyState === "copied" ? <span className="sr-only">{t("about.share.copied")}</span> : null}
        </p>
      </div>

      <div className={CARD_CLASS}>
        <div className="flex items-center gap-2">
          <Heart aria-hidden="true" className="h-4 w-4 text-rose-500" />
          <h2 className="text-[15px] font-semibold text-stone-950">{t("about.credits.title")}</h2>
        </div>
        <p className="mt-1.5 max-w-3xl text-[13px] leading-6 text-stone-600">
          {t("about.credits.subtitle")}
        </p>
        <div className="mt-3 space-y-3">
          {OPEN_SOURCE_CREDITS.map((group) => (
            <section key={group.titleKey}>
              <p className={SECTION_LABEL_CLASS}>{t(group.titleKey)}</p>
              <div className="mt-1.5 grid gap-2 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
                {group.items.map((credit) => (
                  <ExternalTile
                    inline
                    key={credit.url}
                    label={credit.name}
                    onOpen={(url) => openLink(url, "credits")}
                    openLabel={t("about.openLink", { name: credit.name })}
                    subtitle={credit.license}
                    url={credit.url}
                  />
                ))}
              </div>
            </section>
          ))}
        </div>
        <p className="mt-3 rounded-xl bg-stone-50 px-3 py-2 text-[12px] leading-5 text-stone-600 ring-1 ring-stone-200">
          {t("about.credits.derivedNote")}
        </p>
        {openFailure("credits")}
      </div>

      <div className={CARD_CLASS}>
        <div className="flex items-center gap-2">
          <Link2 aria-hidden="true" className="h-4 w-4 text-stone-500" />
          <h2 className="text-[15px] font-semibold text-stone-950">{t("about.friendly.title")}</h2>
        </div>
        <p className="mt-1.5 max-w-3xl text-[13px] leading-6 text-stone-600">
          {t("about.friendly.subtitle")}
        </p>
        <div className="mt-3 grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
          {FRIENDLY_LINKS.map((link) => (
            <ExternalTile
              key={link.url}
              label={link.name}
              onOpen={(url) => openLink(url, "friendly")}
              openLabel={t("about.openLink", { name: link.name })}
              subtitle={link.url}
              url={link.url}
            />
          ))}
        </div>
        {openFailure("friendly")}
      </div>
    </section>
  );
}
