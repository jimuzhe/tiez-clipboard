/** The category tabs under the search box, in display order; null is 全部. */
export const TYPE_FILTERS: readonly (string | null)[] = [
  null,
  "text",
  "image",
  "file",
  "url",
  "code",
  "video",
  "rich_text"
];

/** The tab `step` places from `current`, wrapping around (Tab / Shift+Tab). */
export const cycleTypeFilter = (current: string | null, step: 1 | -1): string | null => {
  const count = TYPE_FILTERS.length;
  const index = Math.max(0, TYPE_FILTERS.indexOf(current));
  return TYPE_FILTERS[(index + step + count) % count];
};
