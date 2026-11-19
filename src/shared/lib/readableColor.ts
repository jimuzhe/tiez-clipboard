/**
 * Colour maths for keeping copied rich text readable on TieZ's cards: text
 * copied from an app in dark mode often carries a near-white colour that
 * vanishes on a light card (and the reverse).
 */

export type Rgba = { r: number; g: number; b: number; a: number };

/** Hex (#rgb, #rgba, #rrggbb, #rrggbbaa) or rgb()/rgba(); anything else is null. */
export const parseCssColor = (value: string): Rgba | null => {
  // Theme tokens such as --text-primary are read raw, often as hex.
  const hex = value.trim().match(/^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i);
  if (hex) {
    const digits = hex[1].length <= 4 ? [...hex[1]].map((d) => d + d).join("") : hex[1];
    const channel = (index: number) => parseInt(digits.slice(index * 2, index * 2 + 2), 16);
    return { r: channel(0), g: channel(1), b: channel(2), a: digits.length === 8 ? channel(3) / 255 : 1 };
  }
  const match = value.trim().match(
    /^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?\s*\)$/i
  );
  if (!match) return null;
  const alpha = match[4] == null
    ? 1
    : match[4].endsWith("%") ? Number(match[4].slice(0, -1)) / 100 : Number(match[4]);
  return { r: Number(match[1]), g: Number(match[2]), b: Number(match[3]), a: alpha };
};

/** `foreground` painted over `background`. */
export const compositeColor = (foreground: Rgba, background: Rgba): Rgba => {
  const alpha = foreground.a + background.a * (1 - foreground.a);
  if (alpha <= 0) return { r: 0, g: 0, b: 0, a: 0 };
  return {
    r: (foreground.r * foreground.a + background.r * background.a * (1 - foreground.a)) / alpha,
    g: (foreground.g * foreground.a + background.g * background.a * (1 - foreground.a)) / alpha,
    b: (foreground.b * foreground.a + background.b * background.a * (1 - foreground.a)) / alpha,
    a: alpha
  };
};

const relativeLuminance = ({ r, g, b }: Rgba) => {
  const linear = [r, g, b].map((channel) => {
    const value = channel / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  });
  return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
};

/** WCAG contrast ratio, 1 to 21. */
export const contrastRatio = (left: Rgba, right: Rgba) => {
  const light = Math.max(relativeLuminance(left), relativeLuminance(right));
  const dark = Math.min(relativeLuminance(left), relativeLuminance(right));
  return (light + 0.05) / (dark + 0.05);
};

/** Below this, copied text counts as unreadable and gets a replacement colour. */
export const READABLE_PREVIEW_CONTRAST = 3;

const DARK_READABLE_TEXT: Rgba = { r: 0, g: 0, b: 0, a: 1 };
const LIGHT_READABLE_TEXT: Rgba = { r: 255, g: 255, b: 255, a: 1 };

/** Whether `foreground` (possibly translucent) reads on the opaque `background`. */
export const isReadableOn = (foreground: Rgba, background: Rgba) =>
  contrastRatio(compositeColor(foreground, background), background) >= READABLE_PREVIEW_CONTRAST;

/** The theme's own text colour when it reads on `background`; else black or white. */
export const readableTextColor = (background: Rgba, themedForeground: Rgba | null): Rgba =>
  themedForeground && contrastRatio(themedForeground, background) >= READABLE_PREVIEW_CONTRAST
    ? themedForeground
    : [DARK_READABLE_TEXT, LIGHT_READABLE_TEXT].reduce((best, candidate) =>
      contrastRatio(candidate, background) > contrastRatio(best, background) ? candidate : best
    );

export const colorToCss = ({ r, g, b }: Rgba) =>
  `rgb(${Math.round(r)} ${Math.round(g)} ${Math.round(b)})`;
