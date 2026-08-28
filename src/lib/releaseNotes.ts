/**
 * Bilingual release notes splitting.
 *
 * Adapted from xintaofei/codeg (Apache-2.0).
 *
 * AI Switch publishes every release with both languages in one markdown body:
 * the Chinese notes, a long rule on its own line, then the English notes.
 * Rendering the whole document means half of it is always a translation the
 * reader cannot use, and in the update dialog — a panel a few lines tall — that
 * half is what they scroll past to reach the buttons. So the UI keeps the half
 * matching the interface language.
 *
 * The boundary is the rule the publishing format puts between the halves, and
 * nothing subtler. Inferring it from where the Han characters start, or from
 * which headings look alike, cannot be made safe: a Chinese-language section at
 * the end of otherwise English notes puts all the Han on one side of a rule
 * exactly like a translation does, and it can carry the same heading and the
 * same version number. Guessing wrong truncates the notes for both readers,
 * which is worse than showing both languages. The rule is something the release
 * author types deliberately, so it is read as what it is: a marker.
 */

/**
 * The rule separating the two languages: at least twelve dashes, asterisks or
 * underscores on a line of its own. Releases are published with twenty-nine
 * dashes and carry no other rule, so length is what tells the marker from
 * ordinary punctuation — a `---` dividing two English sections is not a language
 * boundary, and treating it as one hides everything below it.
 */
const LANGUAGE_SEPARATOR = /^ {0,3}(?:(?:-[ \t]*){12,}|(?:\*[ \t]*){12,}|(?:_[ \t]*){12,})$/;

/** A line opening or closing a fenced code block: the delimiter run plus the rest. */
const CODE_FENCE = /^ {0,3}(`{3,}|~{3,})(.*)$/;

/**
 * Han ideographs plus the CJK punctuation and fullwidth forms Chinese prose is
 * written with (、。「」（）). All of it is absent from the English half, which is
 * what makes it the signal the split is scored on.
 */
const CJK_CHARS = /[\u3000-\u303f\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff\uff01-\uffef]/g;

const LATIN_LETTERS = /[A-Za-z]/g;

/**
 * How much more Chinese one side must read than the other before the break
 * between them counts as the language boundary. A real boundary scores well
 * above this; a rule inside a single-language section scores near zero.
 */
const MIN_LANGUAGE_CONTRAST = 0.3;

export type BilingualReleaseNotes = {
  /** The Chinese half, or null when the body is not bilingual. */
  chinese: string | null;
  /** The English half, or null when the body is not bilingual. */
  english: string | null;
};

function count(text: string, pattern: RegExp) {
  return text.match(pattern)?.length ?? 0;
}

/**
 * Whether `lines[index]` is the separator rather than a line that merely looks
 * like one. A run of dashes written directly under a paragraph is not a rule at
 * all — CommonMark reads it as that paragraph's heading underline — so a line can
 * match {@link LANGUAGE_SEPARATOR} while the document has no break there to split
 * on. Requiring the blank line above tells the two apart.
 */
function isSeparatorAt(lines: string[], index: number) {
  if (!LANGUAGE_SEPARATOR.test(lines[index])) {
    return false;
  }
  return index === 0 || lines[index - 1].trim() === "";
}

/** Share of the text's letters that are Chinese, ignoring digits and markup. */
function cjkRatio(text: string) {
  const cjk = count(text, CJK_CHARS);
  if (cjk === 0) {
    return 0;
  }
  return cjk / (cjk + count(text, LATIN_LETTERS));
}

/**
 * Split a bilingual release body into its two halves, dropping the separator
 * between them. Returns both halves as null when the body carries no separator,
 * or carries one that does not divide two languages.
 */
export function splitBilingualReleaseNotes(body: string): BilingualReleaseNotes {
  const lines = body.replace(/\r\n?/g, "\n").split("\n");
  const section = (start: number, end?: number) => lines.slice(start, end).join("\n").trim();

  let best: { chinese: string; english: string; score: number } | null = null;
  let openFence: { char: string; length: number } | null = null;

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];

    // A separator inside a fenced block is sample text, so fences are tracked by
    // CommonMark's rules rather than by "saw a fence line". A run shorter than
    // the opening one, or one carrying an info string, is content within the
    // block; closing on it would expose the lines below and let one of them
    // split the notes.
    const fence = CODE_FENCE.exec(line);
    if (fence) {
      const [, run, rest] = fence;
      const char = run[0];
      if (openFence === null) {
        // A backtick fence may not carry a backtick in its info string; such a
        // line is ordinary text, and opening a block on it would swallow the
        // rest of the notes.
        if (char !== "`" || !rest.includes("`")) {
          openFence = { char, length: run.length };
        }
      } else if (char === openFence.char && run.length >= openFence.length && rest.trim() === "") {
        openFence = null;
      }
      continue;
    }

    if (openFence !== null || !isSeparatorAt(lines, index)) {
      continue;
    }

    const before = section(0, index);
    const after = section(index + 1);
    if (!before || !after) {
      continue;
    }

    // Which half is which, and a floor under how differently the two read: a
    // separator with the same language on both sides divides sections, not
    // translations, and splitting there would hide one of them.
    const beforeRatio = cjkRatio(before);
    const afterRatio = cjkRatio(after);
    const score = Math.abs(afterRatio - beforeRatio);
    if (score < MIN_LANGUAGE_CONTRAST) {
      continue;
    }

    const chineseIsSecond = afterRatio > beforeRatio;

    // An equal score keeps the later separator, which pushes a stray rule
    // sitting next to the real one into the discarded middle rather than into
    // the half that follows it.
    if (best !== null && score < best.score) {
      continue;
    }
    best = {
      chinese: chineseIsSecond ? after : before,
      english: chineseIsSecond ? before : after,
      score,
    };
  }

  return best ? { chinese: best.chinese, english: best.english } : { chinese: null, english: null };
}

/**
 * Whether `language` reads the Chinese half. Accepts both the app's `zh-CN` tag
 * and the `zh_CN` spelling, plus any other Chinese variant.
 */
export function prefersChineseReleaseNotes(language: string) {
  return language.toLowerCase().split(/[-_]/)[0] === "zh";
}

/**
 * The half of `body` to render for `language` — or the whole body, when it is not
 * the bilingual shape releases are published in.
 */
export function localizeReleaseNotes(body: string, language: string) {
  const trimmed = body.trim();
  if (!trimmed) {
    return "";
  }

  const { chinese, english } = splitBilingualReleaseNotes(trimmed);
  const preferred = prefersChineseReleaseNotes(language) ? chinese : english;
  return preferred ?? trimmed;
}
