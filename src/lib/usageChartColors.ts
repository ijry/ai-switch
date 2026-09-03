/**
 * Colours for the usage trend chart.
 *
 * The eight hues are a validated categorical set: on a white surface every
 * adjacent pair clears the colour-vision-deficiency separation floor (worst
 * pair ΔE 9.1 against a target of 8) and the normal-vision floor (worst 19.6
 * against 15). Three of them — aqua, yellow, magenta — sit under 3:1 contrast
 * against white, which is legal here only because the list view of the same
 * numbers is one click away.
 *
 * There is no ninth hue on purpose. The Rust side folds everything past the
 * eighth series into 其他, which takes the neutral below rather than an invented
 * colour that would impersonate one of the eight.
 */
const seriesPalette = [
  "#2a78d6",
  "#eb6834",
  "#1baf7a",
  "#eda100",
  "#e87ba4",
  "#008300",
  "#4a3aa7",
  "#e34948",
] as const;

/** The folded tail row's key, mirroring `TREND_OTHER_KEY` in Rust. */
export const otherSeriesKey = "其他";

/** Neutral for the folded tail: deliberately outside the categorical set. */
const otherSeriesColor = "#a8a29e";

/** FNV-1a, for a stable slot per series key. */
function hashKey(key: string) {
  let hash = 0x811c9dc5;
  for (let index = 0; index < key.length; index += 1) {
    hash ^= key.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/**
 * Map each series key to a colour.
 *
 * The slot comes from a hash of the key rather than from its rank, so switching
 * the period keeps a model the colour the reader just learned even though the
 * ranking reshuffled underneath. Keys whose hashed slot is already taken fall
 * back to the first free one, in the order they appear, so no two visible series
 * ever share a colour.
 */
export function assignSeriesColors(keys: readonly string[]): Map<string, string> {
  const colors = new Map<string, string>();
  const claimed = new Array<string | null>(seriesPalette.length).fill(null);
  const contested: string[] = [];

  for (const key of keys) {
    if (key === otherSeriesKey) {
      colors.set(key, otherSeriesColor);
      continue;
    }
    const slot = hashKey(key) % seriesPalette.length;
    if (claimed[slot] === null) {
      claimed[slot] = key;
      colors.set(key, seriesPalette[slot]);
    } else {
      contested.push(key);
    }
  }

  for (const key of contested) {
    const slot = claimed.indexOf(null);
    if (slot === -1) {
      // Unreachable while the backend caps named series at eight; a repeat is
      // still better than a blank fill if that cap ever moves.
      colors.set(key, otherSeriesColor);
      continue;
    }
    claimed[slot] = key;
    colors.set(key, seriesPalette[slot]);
  }

  return colors;
}
