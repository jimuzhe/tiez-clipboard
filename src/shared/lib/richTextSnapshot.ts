import { toTauriLocalImageSrc } from "./localImageSrc";
import {
  colorToCss,
  compositeColor,
  isReadableOn,
  parseCssColor,
  readableTextColor,
  type Rgba
} from "./readableColor";
import {
  isHtmlishTagText,
  isOfficeStyleDefinitionText,
  stripOfficePreviewNoise,
  stripRichStorageMarkers
} from "./repairHtmlFragment";

const SNAPSHOT_CACHE_LIMIT = 120;
const SNAPSHOT_CACHE_BYTE_LIMIT = 24 * 1024 * 1024;
const SNAPSHOT_CACHE_VERSION = "v12";
const snapshotCache = new Map<string, { dataUrl: string; bytes: number }>();
let snapshotCacheBytes = 0;

const RICH_IMAGE_FALLBACK_PREFIX = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX = "-->";

const trimCache = () => {
  while (
    snapshotCache.size > SNAPSHOT_CACHE_LIMIT ||
    snapshotCacheBytes > SNAPSHOT_CACHE_BYTE_LIMIT
  ) {
    const first = snapshotCache.keys().next();
    if (first.done) return;
    snapshotCacheBytes -= snapshotCache.get(first.value)?.bytes ?? 0;
    snapshotCache.delete(first.value);
  }
};

const hashString = (input: string): string => {
  let hash = 2166136261;
  for (let i = 0; i < input.length; i++) {
    hash ^= input.charCodeAt(i);
    hash += (hash << 1) + (hash << 4) + (hash << 7) + (hash << 8) + (hash << 24);
  }
  return (hash >>> 0).toString(16);
};

const stripRichImageFallbackMarker = (html: string): string => {
  const start = html.lastIndexOf(RICH_IMAGE_FALLBACK_PREFIX);
  if (start < 0) return html;
  const markerStart = start + RICH_IMAGE_FALLBACK_PREFIX.length;
  const endRel = html.slice(markerStart).indexOf(RICH_IMAGE_FALLBACK_SUFFIX);
  if (endRel < 0) return html;
  const markerEnd = markerStart + endRel;
  const clean = `${html.slice(0, start)}${html.slice(markerEnd + RICH_IMAGE_FALLBACK_SUFFIX.length)}`.trim();
  return clean || html;
};

type SnapshotOptions = {
  width?: number;
  maxHeight?: number;
};

type SnapshotFailureReason =
  | "empty_html"
  | "normalize_failed"
  | "contains_external_images"
  | "contains_non_data_images"
  | "data_url_too_large"
  | "svg_xml_invalid"
  | "unexpected_error";

const logSnapshotFailure = (
  reason: SnapshotFailureReason,
  context: Record<string, unknown>
) => {
  console.warn("[RichTextSnapshot] generation failed", { reason, ...context });
};

const WHITE: Rgba = { r: 255, g: 255, b: 255, a: 1 };
const BLACK: Rgba = { r: 0, g: 0, b: 0, a: 1 };

let colorProbe: CanvasRenderingContext2D | null | undefined;

/** Any CSS colour (names, hsl(), …), resolved through a canvas; null if unknown. */
const resolveCssColor = (value: string): Rgba | null => {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const direct = parseCssColor(trimmed);
  if (direct) return direct;
  if (/^(inherit|initial|unset|revert|currentcolor)$/i.test(trimmed)) return null;
  if (colorProbe === undefined) {
    try {
      colorProbe = document.createElement("canvas").getContext("2d");
    } catch {
      colorProbe = null;
    }
  }
  if (!colorProbe) return null;
  // An invalid value leaves fillStyle unchanged.
  const sentinel = "#010203";
  colorProbe.fillStyle = sentinel;
  colorProbe.fillStyle = trimmed;
  const resolved = String(colorProbe.fillStyle);
  return resolved === sentinel ? null : parseCssColor(resolved);
};

/**
 * The snapshot is an image, so HtmlContent's readability pass cannot reach it:
 * text copied from an app in dark mode kept its near-white colour and the
 * card came out blank. Same rule here, from the inline colours the copy
 * carries (rules in <style> blocks are not seen).
 */
const repairSnapshotTextColors = (
  body: HTMLElement,
  colors: { background: string; text: string },
  hasSourceStyles: boolean
) => {
  const themedText = resolveCssColor(colors.text);
  let card = resolveCssColor(colors.background) ?? WHITE;
  if (card.a < 1) {
    // A see-through card: assume the backdrop the theme's text was made for.
    const textIsLight = themedText ? isReadableOn(themedText, BLACK) && !isReadableOn(themedText, WHITE) : false;
    card = compositeColor(card, textIsLight ? BLACK : WHITE);
  }

  const walk = (element: Element, inherited: Rgba, background: Rgba) => {
    const tag = element.tagName.toLowerCase();
    if (tag === "style" || tag === "script") return;
    const style = (element as HTMLElement).style as CSSStyleDeclaration | undefined;
    let bg = background;
    let fg = inherited;
    if (hasSourceStyles && tag === "table") {
      // The snapshot puts source-styled tables on white paper.
      bg = WHITE;
      fg = BLACK;
    }
    const ownBackground = resolveCssColor(style?.backgroundColor || element.getAttribute("bgcolor") || "");
    if (ownBackground && ownBackground.a > 0) bg = compositeColor(ownBackground, bg);
    const ownColor = resolveCssColor(
      style?.color || (tag === "font" ? element.getAttribute("color") || "" : "")
    );
    if (ownColor) fg = ownColor;

    const hasOwnText = Array.from(element.childNodes).some(
      (node) => node.nodeType === Node.TEXT_NODE && (node.textContent || "").trim()
    );
    if (hasOwnText && style && !isReadableOn(fg, bg)) {
      fg = readableTextColor(bg, themedText);
      style.setProperty("color", colorToCss(fg), "important");
    }
    for (const child of Array.from(element.children)) walk(child, fg, bg);
  };

  const rootText = themedText ?? readableTextColor(card, null);
  for (const child of Array.from(body.children)) walk(child, rootText, card);
};

const normalizeRichHtml = (html: string, colors?: { background: string; text: string }): {
  bodyHtml: string;
  estimatedHeight: number;
  hasSourceStyles: boolean;
  imageStats: {
    total: number;
    data: number;
    local: number;
    remote: number;
    unsupported: number;
  };
} | null => {
  const parser = new DOMParser();
  let processed = stripOfficePreviewNoise(stripRichStorageMarkers(stripRichImageFallbackMarker((html || "").trim())));
  if (!processed) return null;

  if (
    (processed.includes("<tr") || processed.includes("<td") || processed.includes("<col")) &&
    !processed.toLowerCase().includes("<table")
  ) {
    processed = `<table>${processed}</table>`;
  }

  const doc = parser.parseFromString(processed, "text/html");
  doc.querySelectorAll("script").forEach((el) => el.remove());
  doc.querySelectorAll("style").forEach((style) => {
    if (isOfficeStyleDefinitionText(style.textContent || "")) {
      style.remove();
      return;
    }
    // Office wraps its rules in <!-- -->. The snapshot is parsed as XML, where
    // that turned the whole sheet into a comment.
    style.textContent = (style.textContent || "").replace(/<!--|-->/g, "");
  });
  doc.querySelectorAll("meta, link, xml").forEach((el) => el.remove());
  doc.head.querySelectorAll("style").forEach((style) => {
    doc.body.prepend(style);
  });

  if (doc.body.querySelector("table")) {
    for (const node of Array.from(doc.body.childNodes)) {
      if (node.nodeType === Node.TEXT_NODE) {
        const text = node.textContent?.trim() || "";
        if (!text) {
          doc.body.removeChild(node);
          continue;
        }
        if (isHtmlishTagText(text) || isOfficeStyleDefinitionText(text)) {
          doc.body.removeChild(node);
          continue;
        }
      }

      if (node.nodeType === Node.ELEMENT_NODE) {
        const element = node as Element;
        const tagName = element.tagName.toLowerCase();
        if (tagName === "style" && isOfficeStyleDefinitionText(element.textContent || "")) {
          doc.body.removeChild(node);
          continue;
        }
        if (tagName === "meta" || tagName === "link" || tagName === "xml") {
          doc.body.removeChild(node);
          continue;
        }
      }

      break;
    }
  }

  const imageStats = {
    total: 0,
    data: 0,
    local: 0,
    remote: 0,
    unsupported: 0
  };

  doc.querySelectorAll("*").forEach((el) => {
    [...el.attributes].forEach((attr) => {
      const name = attr.name.toLowerCase();
      const value = attr.value.toLowerCase();
      if (name.startsWith("on")) {
        el.removeAttribute(attr.name);
      }
      if ((name === "href" || name === "src") && value.startsWith("javascript:")) {
        el.removeAttribute(attr.name);
      }
    });

    if (el.tagName.toLowerCase() === "img") {
      imageStats.total += 1;
      const src = el.getAttribute("src");
      const normalizedSrc = src?.startsWith("//") ? `https:${src}` : src;
      if (normalizedSrc && normalizedSrc !== src) {
        el.setAttribute("src", normalizedSrc);
      }
      const mapped = normalizedSrc ? toTauriLocalImageSrc(normalizedSrc) : null;
      if (mapped) {
        el.setAttribute("src", mapped);
        imageStats.local += 1;
      } else if (normalizedSrc && /^data:image\//i.test(normalizedSrc)) {
        imageStats.data += 1;
      } else if (
        normalizedSrc &&
        (/^https?:\/\/asset\.localhost\//i.test(normalizedSrc) || /^asset:/i.test(normalizedSrc))
      ) {
        imageStats.local += 1;
      } else if (normalizedSrc && /^https?:\/\//i.test(normalizedSrc)) {
        // Remote images often fail inside SVG foreignObject snapshots.
        imageStats.remote += 1;
      } else {
        // blob:, cid:, relative paths, empty src, etc.
        imageStats.unsupported += 1;
      }
    }
  });

  // Same test as HtmlContent: styled tables keep their own look.
  const hasSourceStyles = !!doc.body.querySelector("style, tr[style], th[style], td[style], [bgcolor]");
  if (colors) repairSnapshotTextColors(doc.body, colors, hasSourceStyles);

  const bodyHtml = (doc.body.innerHTML || "").trim();
  if (!bodyHtml) return null;

  const rowCount = doc.querySelectorAll("tr").length;
  const text = doc.body.textContent || "";
  const charCount = text.trim().length;
  const topLevelBlockCount = Array.from(doc.body.children).filter((el) =>
    /^(p|div|li|blockquote|pre|h1|h2|h3|h4|h5|h6)$/i.test(el.tagName)
  ).length;
  const explicitLineCount = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean).length;
  const preLineCount = Array.from(doc.querySelectorAll("pre")).reduce((sum, pre) => {
    const lines = (pre.textContent || "")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean).length;
    return sum + Math.max(1, lines);
  }, 0);
  const brCount = doc.querySelectorAll("br").length;
  const hasEmbeddedMedia = !!doc.querySelector("img,video,svg,canvas");
  const roughTextLines = Math.max(
    1,
    topLevelBlockCount,
    explicitLineCount,
    preLineCount,
    brCount + 1,
    Math.ceil(charCount / 80)
  );
  const estimatedHeight =
    rowCount > 0
      ? Math.max(72, Math.min(2600, rowCount * 28 + 38))
      : hasEmbeddedMedia
        ? Math.max(120, Math.min(2200, roughTextLines * 20 + 72))
        : Math.max(52, Math.min(1800, roughTextLines * 20 + 20));

  return { bodyHtml, estimatedHeight, imageStats, hasSourceStyles };
};

const toXmlSafeNamedEntities = (html: string): string => {
  // XML only supports amp/lt/gt/quot/apos as named entities.
  // Office HTML often contains entities like &nbsp; which can break SVG parsing.
  return html.replace(/&([a-zA-Z][a-zA-Z0-9]+);/g, (full, name: string) => {
    const lower = name.toLowerCase();
    if (lower === "amp" || lower === "lt" || lower === "gt" || lower === "quot" || lower === "apos") {
      return full;
    }

    const probe = document.createElement("textarea");
    probe.innerHTML = full;
    const decoded = probe.value;
    if (!decoded || decoded === full) {
      return `&amp;${name};`;
    }

    return Array.from(decoded)
      .map((ch) => `&#${ch.codePointAt(0)};`)
      .join("");
  });
};

const XHTML_VOID_TAG_RE =
  /<\s*(area|base|br|col|embed|hr|img|input|link|meta|param|source|track|wbr)(\b[^<>]*?)?>/gi;

const toXhtmlCompatibleFragment = (html: string): string => {
  return html.replace(XHTML_VOID_TAG_RE, (full, tag: string, attrs: string) => {
    if (/\/\s*>$/.test(full)) return full;
    const attrPart = attrs || "";
    return `<${tag}${attrPart} />`;
  });
};

const isValidXmlCodePoint = (codePoint: number): boolean => {
  return (
    codePoint === 0x9 ||
    codePoint === 0xa ||
    codePoint === 0xd ||
    (codePoint >= 0x20 && codePoint <= 0xd7ff) ||
    (codePoint >= 0xe000 && codePoint <= 0xfffd) ||
    (codePoint >= 0x10000 && codePoint <= 0x10ffff)
  );
};

const stripInvalidXmlChars = (input: string): string => {
  let out = "";
  for (const ch of input) {
    const cp = ch.codePointAt(0);
    if (cp === undefined) continue;
    if (isValidXmlCodePoint(cp)) {
      out += ch;
    }
  }
  return out;
};

const toBase64Utf8 = (input: string): string => {
  const bytes = new TextEncoder().encode(input);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    const part = bytes.subarray(i, i + chunk);
    binary += String.fromCharCode(...part);
  }
  return btoa(binary);
};

const getSvgParseError = (svg: string): string | null => {
  const parser = new DOMParser();
  const doc = parser.parseFromString(svg, "image/svg+xml");
  const parserError = doc.querySelector("parsererror");
  if (!parserError) return null;
  const raw = (parserError.textContent || "").trim().replace(/\s+/g, " ");
  return raw.slice(0, 360) || "unknown svg parse error";
};

let cachedThemeSignature = "";
let cachedThemeValues = {
  background: "#ffffff",
  text: "#111827",
  accent: "#1d4ed8",
  border: "rgba(128,128,128,.25)",
  fontFamily: "'Segoe UI','Microsoft YaHei',sans-serif",
  fontSize: "13px"
};
let lastThemeCheckTime = 0;

const getThemeSnapshotStyles = () => {
  const now = Date.now();
  if (now - lastThemeCheckTime < 1000 && cachedThemeSignature) {
    return { ...cachedThemeValues, signature: cachedThemeSignature };
  }
  lastThemeCheckTime = now;
  try {
    const themeStyles = getComputedStyle(document.body);
    const background = themeStyles.getPropertyValue("--bg-element").trim() || "#ffffff";
    const text = themeStyles.getPropertyValue("--text-primary").trim() || "#111827";
    const accent = themeStyles.getPropertyValue("--accent-color").trim() || "#1d4ed8";
    const border = themeStyles.getPropertyValue("--border-dark").trim() || "rgba(128,128,128,.25)";
    const rawFontFamily = themeStyles.getPropertyValue("--font-main").trim();
    const fontFamily = rawFontFamily && !/[<>&;]/.test(rawFontFamily)
      ? rawFontFamily.replace(/"/g, "'")
      : "'Segoe UI','Microsoft YaHei',sans-serif";
    const rawFontSize = themeStyles.getPropertyValue("--clipboard-item-font-size").trim();
    const fontSize = /^\d+(?:\.\d+)?px$/i.test(rawFontSize) ? rawFontSize : "13px";
    cachedThemeValues = { background, text, accent, border, fontFamily, fontSize };
    cachedThemeSignature = [background, text, accent, border, fontFamily, fontSize].join("|");
    return { ...cachedThemeValues, signature: cachedThemeSignature };
  } catch {
    return { ...cachedThemeValues, signature: cachedThemeSignature || "default" };
  }
};

// Failed snapshots (non-data images, oversized output, …) fall back to HTML
// rendering. Remember them so a card that remounts does not re-parse the
// whole document just to fail again.
const failedSnapshots = new Map<string, Set<string>>();
const FAILED_SNAPSHOT_LIMIT = 200;

export const getRichTextSnapshotDataUrl = (
  html: string,
  options: SnapshotOptions = {}
): string | null => {
  const size = `${options.width ?? ""}x${options.maxHeight ?? ""}`;
  if (failedSnapshots.get(html)?.has(size)) return null;
  const result = buildRichTextSnapshotDataUrl(html, options);
  if (result === null) {
    if (!failedSnapshots.has(html) && failedSnapshots.size >= FAILED_SNAPSHOT_LIMIT) {
      const oldest = failedSnapshots.keys().next().value;
      if (oldest !== undefined) failedSnapshots.delete(oldest);
    }
    const sizes = failedSnapshots.get(html) ?? new Set<string>();
    sizes.add(size);
    failedSnapshots.set(html, sizes);
  }
  return result;
};

/**
 * Lays the snapshot's markup out off screen, in a shadow root so its sheet
 * stays contained, and returns its height; null where there is no layout.
 */
const measureSnapshotHeight = (markup: string, width: number): number | null => {
  if (typeof document === "undefined" || !document.body) return null;
  const host = document.createElement("div");
  host.setAttribute("aria-hidden", "true");
  host.style.cssText = `position:fixed;left:-100000px;top:0;width:${width}px;visibility:hidden;pointer-events:none;contain:layout style`;
  try {
    const shadow = host.attachShadow({ mode: "open" });
    shadow.innerHTML = markup;
    document.body.appendChild(host);
    const root = shadow.firstElementChild as HTMLElement | null;
    const height = root ? Math.ceil(root.getBoundingClientRect().height) : 0;
    // A little slack for font differences between the page and the image.
    return height > 0 ? height + 4 : null;
  } catch {
    return null;
  } finally {
    host.remove();
  }
};

const buildRichTextSnapshotDataUrl = (
  html: string,
  options: SnapshotOptions
): string | null => {
  const sourceHtml = html || "";
  const width = Math.max(280, Math.min(1800, Math.round(options.width ?? 960)));
  const maxHeight = Math.max(80, Math.min(3200, Math.round(options.maxHeight ?? 1600)));

  try {
    const theme = getThemeSnapshotStyles();
    const snapshotBackground = theme.background;
    const snapshotText = theme.text;
    const snapshotAccent = theme.accent;
    const snapshotBorder = theme.border;
    const snapshotFontFamily = theme.fontFamily;
    const snapshotFontSize = theme.fontSize;
    const themeSignature = theme.signature;
    const trimmedLength = sourceHtml.trim().length;
    if (!trimmedLength) {
      logSnapshotFailure("empty_html", {
        htmlLength: sourceHtml.length,
        width,
        maxHeight
      });
      return null;
    }

    const key = `${SNAPSHOT_CACHE_VERSION}:${hashString(sourceHtml)}:${sourceHtml.length}:${width}:${maxHeight}:${hashString(themeSignature)}`;
    const cached = snapshotCache.get(key);
    if (cached) return cached.dataUrl;

    const normalized = normalizeRichHtml(sourceHtml, { background: snapshotBackground, text: snapshotText });
    if (!normalized) {
      logSnapshotFailure("normalize_failed", {
        htmlLength: sourceHtml.length,
        trimmedLength,
        width,
        maxHeight
      });
      return null;
    }

    const nonDataImageCount =
      normalized.imageStats.local +
      normalized.imageStats.remote +
      normalized.imageStats.unsupported;
    if (nonDataImageCount > 0) {
      logSnapshotFailure("contains_non_data_images", {
        htmlLength: sourceHtml.length,
        width,
        maxHeight,
        imageStats: normalized.imageStats,
        note: "Use HtmlContent fallback for better image compatibility"
      });
      return null;
    }

    const snapshotStyle = [
      "box-sizing:border-box",
      "margin:0",
      "padding:8px 10px",
      "width:100%",
      "height:100%",
      "overflow:hidden",
      `background:${snapshotBackground}`,
      `font-family:${snapshotFontFamily}`,
      `font-size:${snapshotFontSize}`,
      `color:${snapshotText}`,
      "line-height:1.5",
      "overflow-wrap:anywhere",
      "text-align:start"
    ].join(";");

    const xhtmlBodyHtml = toXhtmlCompatibleFragment(normalized.bodyHtml);
    const xmlSafeBodyHtml = toXmlSafeNamedEntities(xhtmlBodyHtml);

    const snapshotSheet = `<style>
      * {
        box-sizing: border-box;
        font-size: inherit !important;
        line-height: inherit;
      }
      .tiez-rich-snapshot-root > :first-child,
      .tiez-rich-snapshot-root > style:first-child + * {
        margin-block-start: 0 !important;
      }
      .tiez-rich-snapshot-root > :nth-last-child(2) {
        margin-block-end: 0 !important;
      }
      .tiez-rich-snapshot-root > :is(p, div):has(> br:only-child) {
        display: none !important;
      }
      p {
        margin: 0 0 0.5em !important;
        padding: 0 !important;
        line-height: inherit;
      }
      h1, h2, h3, h4, h5, h6 {
        margin: 0.62em 0 0.28em !important;
        line-height: 1.22 !important;
        font-weight: 700 !important;
        letter-spacing: -0.01em;
      }
      h1 { font-size: 1.28em !important; }
      h2 { font-size: 1.18em !important; }
      h3 { font-size: 1.1em !important; }
      h4, h5, h6 { font-size: 1.03em !important; }
      strong, b { font-weight: 700; }
      small { font-size: 0.92em !important; }
      sub, sup {
        font-size: 0.78em !important;
        line-height: 0;
      }
      table {
        border-collapse: collapse !important;
        border-spacing: 0;
        table-layout: auto;
        margin: 0.5em 0 !important;
      }
      /* Styled tables on white paper, as in HtmlContent. */
      .tiez-rich-snapshot-root.has-source-styles table {
        background-color: #fff;
        color: #000;
      }
      /* TieZ's table look only where the source brought none (see HtmlContent). */
      .tiez-rich-snapshot-root:not(.has-source-styles) table {
        width: 100% !important;
        border: 1px solid ${snapshotBorder} !important;
      }
      img, video {
        display: block;
        width: auto;
        max-width: 100% !important;
        max-height: 320px !important;
        height: auto;
        margin: 0.25em 0;
        border-radius: 6px;
        object-fit: contain;
        object-position: top left;
      }
      td, th {
        vertical-align: top;
        padding: 0.36em 0.55em !important;
        font-size: 0.92em !important;
        line-height: 1.35 !important;
        word-break: break-word;
      }
      .tiez-rich-snapshot-root:not(.has-source-styles) td,
      .tiez-rich-snapshot-root:not(.has-source-styles) th {
        border: 1px solid ${snapshotBorder} !important;
      }
      .tiez-rich-snapshot-root:not(.has-source-styles) th {
        font-weight: 600 !important;
        background-color: color-mix(in srgb, ${snapshotText} 8%, ${snapshotBackground});
      }
      .tiez-rich-snapshot-root:not(.has-source-styles) tr:nth-child(even) td:not([style*="background"]):not([bgcolor]) {
        background-color: color-mix(in srgb, ${snapshotText} 4%, ${snapshotBackground});
      }
      ul, ol {
        margin: 0.2em 0 0.5em !important;
        padding-inline-start: 1.45em !important;
      }
      li {
        margin: 0.12em 0;
        padding-inline-start: 0.08em;
      }
      blockquote {
        margin: 0.3em 0 0.5em !important;
        padding: 0.3em 0.68em !important;
        border-inline-start: 2px solid ${snapshotAccent};
        border-radius: 0 6px 6px 0;
        background: color-mix(in srgb, ${snapshotAccent} 7%, transparent);
      }
      pre {
        margin: 0.35em 0 0.5em !important;
        padding: 0.55em 0.68em !important;
        overflow: hidden;
        border: 1px solid ${snapshotBorder};
        border-radius: 6px;
        background: color-mix(in srgb, ${snapshotText} 6%, transparent);
        font-family: ui-monospace, 'SFMono-Regular', Consolas, monospace !important;
        line-height: 1.45 !important;
        white-space: pre-wrap !important;
        tab-size: 4;
      }
      code, kbd, samp {
        font-family: ui-monospace, 'SFMono-Regular', Consolas, monospace !important;
      }
      :not(pre) > code, kbd {
        padding: 0.08em 0.3em;
        border: 1px solid ${snapshotBorder};
        border-radius: 4px;
        background: color-mix(in srgb, ${snapshotText} 6%, transparent);
      }
      hr {
        height: 1px;
        margin: 0.65em 0 !important;
        border: 0;
        background: ${snapshotBorder};
      }
      mark {
        padding: 0 0.12em;
        border-radius: 3px;
      }
      a {
        color: ${snapshotAccent} !important;
        text-decoration-line: underline;
        text-decoration-thickness: from-font;
        text-underline-offset: 0.14em;
        text-decoration-skip-ink: auto;
      }
    </style>`;

    // Measured, not guessed: the line estimate assumed ~80 characters a line,
    // so Chinese text (about 20 a line at card width) lost everything after
    // its first lines. Images may still be loading, so keep the estimate as
    // a floor when there are any.
    const measuredHeight = measureSnapshotHeight(
      `<div class="${normalized.hasSourceStyles ? "tiez-rich-snapshot-root has-source-styles" : "tiez-rich-snapshot-root"}" style="${snapshotStyle};height:auto">${normalized.bodyHtml}${snapshotSheet}</div>`,
      width
    );
    const contentHeight = measuredHeight == null
      ? normalized.estimatedHeight
      : normalized.imageStats.total > 0
        ? Math.max(measuredHeight, normalized.estimatedHeight)
        : measuredHeight;
    const height = Math.max(48, Math.min(maxHeight, contentHeight));

    const svg = [
      `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`,
      `<foreignObject x="0" y="0" width="100%" height="100%">`,
      `<div xmlns="http://www.w3.org/1999/xhtml" class="tiez-rich-snapshot-root${normalized.hasSourceStyles ? " has-source-styles" : ""}" style="`,
      snapshotStyle,
      '">',
      xmlSafeBodyHtml,
      snapshotSheet,
      "</div>",
      "</foreignObject>",
      "</svg>"
    ].join("");

    const safeSvg = stripInvalidXmlChars(svg);
    // Diagnostics only (the snapshot renders either way): re-parsing the whole
    // SVG costs a full DOMParser pass, so skip it in production builds.
    const parseError = import.meta.env.DEV ? getSvgParseError(safeSvg) : null;
    if (parseError) {
      // Non-blocking diagnostics: some engines report parser warnings but still render.
      logSnapshotFailure("svg_xml_invalid", {
        htmlLength: sourceHtml.length,
        bodyHtmlLength: normalized.bodyHtml.length,
        width,
        maxHeight,
        estimatedHeight: normalized.estimatedHeight,
        parseError
      });
    }
    const svgBase64 = toBase64Utf8(safeSvg);
    const dataUrl = `data:image/svg+xml;base64,${svgBase64}`;
    if (dataUrl.length > 2_000_000) {
      logSnapshotFailure("data_url_too_large", {
        htmlLength: sourceHtml.length,
        bodyHtmlLength: normalized.bodyHtml.length,
        estimatedHeight: normalized.estimatedHeight,
        finalHeight: height,
        width,
        maxHeight,
        encoding: "base64",
        dataUrlLength: dataUrl.length
      });
      return null;
    }

    const cacheBytes = dataUrl.length * 2;
    snapshotCache.set(key, { dataUrl, bytes: cacheBytes });
    snapshotCacheBytes += cacheBytes;
    trimCache();
    return dataUrl;
  } catch (error) {
    logSnapshotFailure("unexpected_error", {
      htmlLength: sourceHtml.length,
      width,
      maxHeight,
      error: error instanceof Error ? error.message : String(error)
    });
    return null;
  }
};
