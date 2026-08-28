/**
 * Release notes panel.
 *
 * Adapted from xintaofei/codeg (Apache-2.0).
 */
import { useMemo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useI18n } from "../../lib/i18n";
import { localizeReleaseNotes } from "../../lib/releaseNotes";

/**
 * Typography for a GitHub release body rendered inside a compact panel. UnoCSS
 * has no typography preset here, so the element styles are attached with
 * arbitrary-variant selectors rather than a `prose` class.
 */
const NOTES_PROSE = [
  "break-words text-[12px] leading-5 text-stone-700",
  "[&_h1]:text-[13px] [&_h1]:font-semibold [&_h1]:text-stone-950 [&_h1]:mb-2",
  "[&_h2]:text-[13px] [&_h2]:font-semibold [&_h2]:text-stone-950 [&_h2]:mt-3 [&_h2]:mb-2",
  "[&_h3]:text-[12px] [&_h3]:font-semibold [&_h3]:text-stone-950 [&_h3]:mt-2 [&_h3]:mb-1",
  "[&_p]:mb-2 [&_p:last-child]:mb-0",
  "[&_ul]:list-disc [&_ul]:pl-5 [&_ul]:mb-2",
  "[&_ol]:list-decimal [&_ol]:pl-5 [&_ol]:mb-2",
  "[&_li]:mb-1",
  "[&_code]:font-mono [&_code]:text-[11px] [&_code]:rounded [&_code]:bg-stone-200 [&_code]:px-1",
  "[&_pre]:mb-2 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-stone-200 [&_pre]:p-2",
  "[&_pre_code]:bg-transparent [&_pre_code]:px-0",
  "[&_a]:text-amber-700 [&_a]:underline [&_a]:underline-offset-2",
  "[&_blockquote]:border-l-2 [&_blockquote]:border-stone-300 [&_blockquote]:pl-3 [&_blockquote]:text-stone-500",
  "[&_hr]:my-2 [&_hr]:border-stone-200",
  "[&_table]:mb-2 [&_table]:w-full [&_table]:border-collapse",
  "[&_th]:border [&_th]:border-stone-200 [&_th]:px-2 [&_th]:py-1 [&_th]:text-left [&_th]:font-semibold",
  "[&_td]:border [&_td]:border-stone-200 [&_td]:px-2 [&_td]:py-1",
].join(" ");

export type ReleaseNotesProps = {
  /** Raw markdown from the updater manifest, both languages included. */
  notes: string;
  className?: string;
};

/**
 * Release notes panel shared by the update dialog and the updates screen.
 *
 * Releases carry Chinese and English in one body, so the half matching the
 * interface language is picked here rather than at each call site — that way the
 * two surfaces cannot drift apart on which one they show. See
 * {@link localizeReleaseNotes} for what happens to a body outside that shape.
 */
export function ReleaseNotes({ notes, className }: ReleaseNotesProps) {
  const { language } = useI18n();
  // The dialog re-renders on every download-progress event while the notes sit
  // unchanged beside the progress bar, so the scan is not repeated per render.
  const localized = useMemo(() => localizeReleaseNotes(notes, language), [notes, language]);

  if (!localized) {
    return null;
  }

  return (
    <div className={className ? `${NOTES_PROSE} ${className}` : NOTES_PROSE}>
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{localized}</ReactMarkdown>
    </div>
  );
}
