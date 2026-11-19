import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { toTauriLocalImageSrc } from "../lib/localImageSrc";
import {
  colorToCss,
  compositeColor,
  isReadableOn,
  parseCssColor,
  readableTextColor,
  type Rgba
} from "../lib/readableColor";
import {
  isHtmlishTagText,
  isOfficeStyleDefinitionText,
  stripOfficePreviewNoise
} from "../lib/repairHtmlFragment";

const pickFirstSrcFromSrcset = (srcset?: string | null): string | null => {
  if (!srcset) return null;
  const first = srcset
    .split(",")
    .map((part) => part.trim())
    .find(Boolean);
  if (!first) return null;
  const url = first.split(/\s+/)[0]?.trim();
  return url || null;
};

/** Drop authoring font sizes so list/hover preview follows app typography. */
const stripFontSizeFromCssText = (cssText: string): string => {
  return cssText
    .replace(/font-size\s*:\s*[^;]+;?/gi, "")
    .replace(/;\s*;/g, ";")
    .replace(/^;+|;+$/g, "")
    .trim();
};

/**
 * Page margins, first-line indents and double spacing from Word or a web page
 * push a preview's text off to the side and out of view; drop them, along
 * with fixed sizes.
 */
const stripPreviewLayoutFromCssText = (cssText: string): string => {
  // The trailing ";" is left in place so the next declaration still has the
  // separator these patterns anchor on.
  return cssText
    .replace(/(^|[;{])\s*(?:width|height|min-width|max-width|text-indent|line-height)\s*:[^;}]+/gi, "$1")
    .replace(/(^|[;{])\s*(?:margin|padding)(?:-[a-z-]+)?\s*:[^;}]+/gi, "$1")
    .replace(/;(?:\s*;)+/g, ";")
    .replace(/\{\s*;/g, "{")
    .replace(/^\s*;+|;+\s*$/g, "")
    .trim();
};

const stripUnsafeCssText = (cssText: string): string => {
  return cssText
    .replace(/@import\s+[^;]+;?/gi, "")
    .replace(/expression\s*\([^)]*\)/gi, "")
    .replace(/(^|[;{])\s*(?:position|z-index|inset|top|right|bottom|left)\s*:\s*[^;}]+;?/gi, "$1")
    .replace(
      /url\s*\(\s*(['"]?)(.*?)\1\s*\)/gi,
      (full, _quote: string, rawUrl: string) => {
        const url = rawUrl.trim();
        return /^(?:data:image\/|https?:\/\/asset\.localhost\/|asset:)/i.test(url)
          ? full
          : "none";
      }
    )
    .replace(/;\s*;/g, ";")
    .replace(/^;+|;+$/g, "")
    .trim();
};

const LINK_ATTRS = new Set(["href", "xlink:href", "action", "formaction"]);
const SAFE_LINK_SCHEMES = new Set(["http", "https", "mailto", "tel"]);
const RESOURCE_ATTRS = new Set(["src", "poster", "background", "data"]);
const UNSAFE_RESOURCE_SCHEMES = new Set(["javascript", "vbscript"]);

/** URL parsers ignore control chars and whitespace, so `java&#9;script:` must not hide its scheme. */
const getUrlScheme = (value: string): string | null => {
  const compact = value.replace(/[\u0000- \u007f-\u009f]/g, "").toLowerCase();
  return /^([a-z][a-z0-9+.-]*):/.exec(compact)?.[1] ?? null;
};

const isUnsafeUrlAttr = (name: string, value: string): boolean => {
  const scheme = getUrlScheme(value);
  if (!scheme) return false;
  if (LINK_ATTRS.has(name)) return !SAFE_LINK_SCHEMES.has(scheme);
  if (RESOURCE_ATTRS.has(name)) return UNSAFE_RESOURCE_SCHEMES.has(scheme);
  return false;
};

/** Links inside pasted content must never navigate the app window; card clicks still bubble. */
const preventLinkNavigation = (event: Event) => {
  if (event.target instanceof Element && event.target.closest("a, area")) {
    event.preventDefault();
  }
};

const resolveImgSource = (el: Element): string | null => {
  const src = el.getAttribute("src")?.trim() || "";
  const lazyAttrs = [
    "data-src",
    "data-original",
    "data-lazy-src",
    "data-actualsrc",
    "data-url"
  ];
  const lazySrc =
    lazyAttrs
      .map((name) => el.getAttribute(name)?.trim() || "")
      .find(Boolean) || "";
  const srcsetPrimary = pickFirstSrcFromSrcset(el.getAttribute("srcset"));
  const lazySrcsetPrimary = pickFirstSrcFromSrcset(el.getAttribute("data-srcset"));

  const lowerSrc = src.toLowerCase();
  const isMissingOrPlaceholder =
    !src ||
    src === "about:blank" ||
    src === "#" ||
    lowerSrc.startsWith("blob:");
  // Lazy-loading pages (WeChat articles, most CMSs) put a transparent data:
  // URL or a "loading" image in src and the real picture in data-src.
  const looksLikeLazyPlaceholder =
    lowerSrc.startsWith("data:") ||
    /(?:^|[/_.-])(?:placeholder|lazy|loading|blank|spacer|grey|gray|pixel)(?:[._-]|$)/.test(lowerSrc);

  if (isMissingOrPlaceholder) {
    return lazySrc || srcsetPrimary || lazySrcsetPrimary || null;
  }
  if (looksLikeLazyPlaceholder && (lazySrc || lazySrcsetPrimary)) {
    return lazySrc || lazySrcsetPrimary;
  }
  return src;
};

const SANITIZE_CACHE_LIMIT = 100;
// Keyed by the HTML string itself: engines cache a string's hash, so lookups
// stay cheap for the same item, unlike re-hashing every character per render.
type SanitizedHtml = { html: string; hasRenderable: boolean; hasSourceStyles: boolean };
const sanitizeCaches = {
  preview: new Map<string, SanitizedHtml>(),
  full: new Map<string, SanitizedHtml>()
};

const sanitizeHTML = (html: string, preview?: boolean) => {
  const sanitizeCache = preview ? sanitizeCaches.preview : sanitizeCaches.full;
  const cached = sanitizeCache.get(html);
  if (cached) return cached;

  const parser = new DOMParser();

  const stripLeadingPreviewNoise = (container: HTMLElement) => {
    if (!container.querySelector("table, p, div, img, ul, ol, blockquote, pre")) return;

    for (const node of Array.from(container.childNodes)) {
      if (node.nodeType === Node.TEXT_NODE) {
        const text = node.textContent?.trim() || "";
        if (!text) {
          container.removeChild(node);
          continue;
        }
        if (isHtmlishTagText(text) || isOfficeStyleDefinitionText(text)) {
          container.removeChild(node);
          continue;
        }
      }

      if (node.nodeType === Node.ELEMENT_NODE) {
        const element = node as Element;
        const tagName = element.tagName.toLowerCase();
        if (tagName === "style" && isOfficeStyleDefinitionText(element.textContent || "")) {
          container.removeChild(node);
          continue;
        }
        if (tagName === "meta" || tagName === "link" || tagName === "xml") {
          container.removeChild(node);
          continue;
        }
      }

      break;
    }
  };

  // Heuristic: If it contains table elements but no <table> tag, wrap it.
  let processedHtml = stripOfficePreviewNoise(html);
  if ((processedHtml.includes("<tr") || processedHtml.includes("<td") || processedHtml.includes("<col"))
    && !processedHtml.toLowerCase().includes("<table")) {
    // No width here: unstyled tables get the full width from the shadow style,
    // a spreadsheet's rows keep the column widths they came with.
    processedHtml = `<table>${processedHtml}</table>`;
  }

  const doc = parser.parseFromString(processedHtml, "text/html");

  // Keep formatting styles, but remove active/embedded content. Author styles are
  // rendered inside a ShadowRoot below, so their selectors cannot leak into TieZ.
  doc
    .querySelectorAll(
      "script, iframe, frame, object, embed, form, input, button, textarea, select, option, base, template, svg, math"
    )
    .forEach(el => el.remove());
  doc.querySelectorAll("style").forEach((style) => {
    if (isOfficeStyleDefinitionText(style.textContent || "")) {
      style.remove();
      return;
    }
    if (preview && style.textContent) {
      style.textContent = stripPreviewLayoutFromCssText(stripFontSizeFromCssText(style.textContent));
    }
    if (style.textContent) {
      style.textContent = stripUnsafeCssText(style.textContent);
    }
  });
  doc.querySelectorAll("meta, link, xml").forEach((el) => el.remove());

  // Truncate tables for preview to save performance & normalize widths
  if (preview) {
    doc.querySelectorAll("table").forEach((table) => {
      table.removeAttribute("width");
      table.removeAttribute("height");
      table.querySelectorAll("col, colgroup").forEach((col) => {
        col.removeAttribute("width");
        col.removeAttribute("style");
      });
      table.querySelectorAll("tr, th, td").forEach((cell) => {
        cell.removeAttribute("width");
        cell.removeAttribute("height");
      });

      const rows = table.querySelectorAll("tr");
      if (rows.length > 6) {
        // Keep first 5 rows for preview
        for (let i = 5; i < rows.length; i++) {
          rows[i].remove();
        }
      }
    });
  }

  // Move styles from head to body to ensure they are included in the final innerHTML
  doc.head.querySelectorAll("style").forEach(style => {
    doc.body.prepend(style);
  });
  stripLeadingPreviewNoise(doc.body);

  const all = doc.querySelectorAll("*");
  all.forEach(el => {
    // Basic sanitization of on* attributes and script-capable URLs
    [...el.attributes].forEach(attr => {
      const name = attr.name.toLowerCase();
      if (name.startsWith("on")) {
        el.removeAttribute(attr.name);
      }
      if (isUnsafeUrlAttr(name, attr.value)) {
        el.removeAttribute(attr.name);
      }
      if (name === "style") {
        let cleanedStyle = attr.value
          .replace(/(?:^|;)\s*(?:transform|writing-mode|rotate|scale)\s*:[^;]*/gi, "")
          .trim()
          .replace(/^;+|;+$/g, "");
        cleanedStyle = stripUnsafeCssText(cleanedStyle);
        if (preview) {
          cleanedStyle = stripPreviewLayoutFromCssText(stripFontSizeFromCssText(cleanedStyle));
        }
        if (cleanedStyle) {
          el.setAttribute("style", cleanedStyle);
        } else {
          el.removeAttribute("style");
        }
      }
      // Presentational <font size="..."> from older HTML / Office paste
      if (preview && el.tagName.toLowerCase() === "font" && name === "size") {
        el.removeAttribute(attr.name);
      }
    });

    // Handle local file images (including encoded file:// paths)
    if (el.tagName.toLowerCase() === 'img') {
      // Sites that block hotlinking (WeChat, Zhihu, CSDN…) reject TieZ's own
      // origin as referrer but allow none.
      el.setAttribute("referrerpolicy", "no-referrer");
      const resolvedSrc = resolveImgSource(el);
      if (resolvedSrc) {
        if (el.getAttribute("src") !== resolvedSrc) {
          el.setAttribute("src", resolvedSrc);
        }
        const normalizedSrc = resolvedSrc.startsWith("//") ? `https:${resolvedSrc}` : resolvedSrc;
        if (normalizedSrc !== resolvedSrc) {
          el.setAttribute("src", normalizedSrc);
        }

        const localSrc = toTauriLocalImageSrc(normalizedSrc);
        if (localSrc && localSrc !== normalizedSrc) {
          el.setAttribute("src", localSrc);
        }
      }
    }
  });

  const bodyHtml = (doc.body.innerHTML || "").trim();
  const textContent = (doc.body.textContent || "").trim();
  const hasRenderable = !!(
    doc.body.querySelector("img, video, audio, canvas, table, hr, blockquote, pre, code") ||
    textContent
  );

  // A style sheet, or styling on rows and cells: a table that only sets its
  // own width or outline still gets TieZ's cell look.
  const hasSourceStyles = !!doc.body.querySelector("style, tr[style], th[style], td[style], [bgcolor]");

  const result = { html: bodyHtml, hasRenderable, hasSourceStyles };
  if (sanitizeCache.size >= SANITIZE_CACHE_LIMIT) {
    const firstKey = sanitizeCache.keys().next().value;
    if (firstKey) sanitizeCache.delete(firstKey);
  }
  sanitizeCache.set(html, result);
  return result;
};

type HtmlContentProps = {
  htmlContent: string;
  fallbackText?: string;
  className?: string;
  style?: React.CSSProperties;
  preview?: boolean;
  /**
   * Show at most this many lines, cut at a line boundary with an ellipsis
   * (list rows). Headings and gaps are tightened so the lines carry text.
   */
  clampLines?: number;
  /**
   * Render on mount instead of waiting for the element to scroll into view.
   * For a single always-visible view such as the detail pane: if the
   * observer's first report is missed it would otherwise stay blank.
   */
  eager?: boolean;
};

const SHADOW_BASE_STYLE = `
  :host {
    display: block;
    color: var(--text-primary);
    font-family: var(--font-main);
    font-size: var(--clipboard-item-font-size);
    line-height: 1.4;
  }
  :host([data-clamp]) .tiez-rich-root {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: var(--rich-preview-lines, 4);
    overflow: hidden;
  }
  :host([data-clamp]) .tiez-rich-root :is(h1, h2, h3, h4, h5, h6) {
    font-size: 1em !important;
    margin: 0 !important;
  }
  :host([data-clamp]) .tiez-rich-root :is(p, div, ul, ol, li, blockquote, pre, table) {
    margin-top: 0 !important;
    margin-bottom: 0.15em !important;
  }
  :host([data-clamp]) .tiez-rich-root :is(img, video) {
    max-height: 4.2em;
    width: auto;
  }
  .tiez-rich-root {
    color: inherit;
    font: inherit;
    line-height: inherit;
    min-width: 0;
    width: 100%;
    overflow-wrap: anywhere;
    text-align: start;
  }
  .tiez-rich-root *:not(table):not(thead):not(tbody):not(tfoot):not(tr):not(th):not(td):not(colgroup):not(col) {
    box-sizing: border-box;
    max-width: 100%;
  }
  .tiez-rich-root img, .tiez-rich-root video { max-width: 100%; height: auto; }
  /* Bare element selectors (the shadow root scopes them) so a source's own
     rules, such as Excel's td.xl65, win over these defaults. */
  table {
    border-collapse: collapse !important;
    border-spacing: 0;
    margin: var(--rich-preview-block-gap, 0.5em) 0;
    font-size: inherit;
  }
  th, td {
    padding: 0.36em 0.55em;
    vertical-align: top;
    word-break: break-word;
    box-sizing: border-box;
  }
  /* A styled table sits on white paper, as in the app it came from (and in
     the image of it the list shows): its colors were picked for white, and
     black text or dark red on a dark card was unreadable. */
  .tiez-rich-root.has-source-styles table {
    background-color: #fff;
    color: #000;
  }
  /* TieZ's table look only for tables that bring no styling of their own:
     it painted over the fills, borders and stripes a spreadsheet copied. */
  .tiez-rich-root:not(.has-source-styles) table {
    width: 100%;
    border: 1px solid var(--line-strong, var(--border-dark));
    border-radius: var(--rich-preview-radius, 6px);
    overflow: hidden;
  }
  .tiez-rich-root:not(.has-source-styles) th,
  .tiez-rich-root:not(.has-source-styles) td {
    border: 1px solid var(--line-soft, var(--border-dark));
  }
  .tiez-rich-root:not(.has-source-styles) th {
    background-color: var(--bg-button, var(--bg-element));
    font-weight: 600;
  }
  .tiez-rich-root:not(.has-source-styles) td {
    background-color: var(--bg-element, transparent);
  }
  .tiez-rich-root:not(.has-source-styles) tr:nth-child(even) td {
    background-color: color-mix(in srgb, var(--bg-element) 60%, transparent);
  }
`;

const SHADOW_PREVIEW_STYLE = `
  .tiez-rich-root {
    color: var(--text-primary);
    line-height: var(--rich-preview-line-height, 1.5);
  }
  .tiez-rich-root *:not(style) {
    font-size: inherit !important;
  }
  .tiez-rich-root > :first-child,
  .tiez-rich-root > style:first-child + * {
    margin-block-start: 0 !important;
  }
  .tiez-rich-root > :last-child {
    margin-block-end: 0 !important;
  }
  .tiez-rich-root > :is(p, div):has(> br:only-child) {
    display: none !important;
  }
  .tiez-rich-root p {
    margin: 0 0 var(--rich-preview-block-gap, 0.5em) !important;
    padding: 0 !important;
    line-height: inherit;
    text-wrap: pretty;
  }
  .tiez-rich-root h1,
  .tiez-rich-root h2,
  .tiez-rich-root h3,
  .tiez-rich-root h4,
  .tiez-rich-root h5,
  .tiez-rich-root h6 {
    margin: 0.62em 0 0.28em !important;
    line-height: 1.22 !important;
    font-weight: 700 !important;
    letter-spacing: -0.01em;
    text-wrap: balance;
  }
  .tiez-rich-root h1 { font-size: 1.28em !important; }
  .tiez-rich-root h2 { font-size: 1.18em !important; }
  .tiez-rich-root h3 { font-size: 1.1em !important; }
  .tiez-rich-root h4,
  .tiez-rich-root h5,
  .tiez-rich-root h6 { font-size: 1.03em !important; }
  .tiez-rich-root strong,
  .tiez-rich-root b {
    font-weight: 700;
  }
  .tiez-rich-root small {
    font-size: 0.92em !important;
  }
  .tiez-rich-root sub,
  .tiez-rich-root sup {
    font-size: 0.78em !important;
    line-height: 0;
  }
  .tiez-rich-root table {
    width: 100% !important;
    border-collapse: collapse !important;
    table-layout: auto;
    margin: var(--rich-preview-block-gap, 0.5em) 0 !important;
    border: 1px solid var(--line-strong, var(--border-dark)) !important;
    border-radius: var(--rich-preview-radius, 6px);
    overflow: hidden;
  }
  .tiez-rich-root td, .tiez-rich-root th {
    border: 1px solid var(--line-soft, var(--border-dark)) !important;
    padding: 0.36em 0.55em !important;
    font-size: 0.92em !important;
    line-height: 1.35 !important;
    word-break: break-word;
  }
  .tiez-rich-root th {
    background-color: var(--bg-button, var(--bg-element));
    font-weight: 600 !important;
  }
  .tiez-rich-root td {
    background-color: var(--bg-element, transparent);
  }
  .tiez-rich-root tr:nth-child(even) td:not([style*="background"]):not([bgcolor]) {
    background-color: color-mix(in srgb, var(--bg-element) 68%, transparent);
  }
  .tiez-rich-root ul,
  .tiez-rich-root ol {
    margin: 0.2em 0 var(--rich-preview-block-gap, 0.5em) !important;
    padding-inline-start: 1.45em !important;
  }
  .tiez-rich-root li {
    margin: 0.12em 0;
    padding-inline-start: 0.08em;
    line-height: inherit;
  }
  .tiez-rich-root blockquote {
    margin: 0.3em 0 var(--rich-preview-block-gap, 0.5em) !important;
    padding: 0.3em 0.68em !important;
    border-inline-start: 2px solid color-mix(in srgb, var(--accent-color) 72%, transparent);
    border-radius: 0 var(--rich-preview-radius, 6px) var(--rich-preview-radius, 6px) 0;
    background: color-mix(in srgb, var(--accent-color) 7%, transparent);
    color: var(--text-secondary);
  }
  .tiez-rich-root pre {
    margin: 0.35em 0 var(--rich-preview-block-gap, 0.5em) !important;
    padding: 0.55em 0.68em !important;
    overflow: hidden;
    border: 1px solid var(--line-soft, var(--border-dark));
    border-radius: var(--rich-preview-radius, 6px);
    background: color-mix(in srgb, var(--text-primary) 6%, transparent);
    font-family: var(--font-mono) !important;
    line-height: 1.45 !important;
    white-space: pre-wrap !important;
    tab-size: 4;
  }
  .tiez-rich-root code,
  .tiez-rich-root kbd,
  .tiez-rich-root samp {
    font-family: var(--font-mono) !important;
  }
  .tiez-rich-root :not(pre) > code,
  .tiez-rich-root kbd {
    padding: 0.08em 0.3em;
    border: 1px solid var(--line-soft, var(--border-dark));
    border-radius: 4px;
    background: color-mix(in srgb, var(--text-primary) 6%, transparent);
  }
  .tiez-rich-root hr {
    height: 1px;
    margin: 0.65em 0 !important;
    border: 0;
    background: var(--line-soft, var(--border-dark));
  }
  .tiez-rich-root mark {
    padding: 0 0.12em;
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent-color) 18%, transparent);
    color: inherit;
  }
  .tiez-rich-root a {
    color: var(--accent-color) !important;
    text-decoration-line: underline;
    text-decoration-thickness: from-font;
    text-underline-offset: 0.14em;
    text-decoration-skip-ink: auto;
  }
  .tiez-rich-root figure {
    margin: 0.3em 0 var(--rich-preview-block-gap, 0.5em) !important;
  }
  .tiez-rich-root figcaption {
    margin-block-start: 0.25em;
    color: var(--text-secondary);
    font-size: 0.92em !important;
    line-height: 1.35 !important;
  }
  .tiez-rich-root img,
  .tiez-rich-root video {
    display: block;
    width: auto;
    max-width: 100% !important;
    max-height: var(--rich-preview-image-max-height, 96px) !important;
    margin: 0.25em 0;
    border-radius: var(--rich-preview-radius, 6px);
    object-fit: contain;
    object-position: top left;
  }
  .tiez-rich-root dl {
    margin: 0 0 var(--rich-preview-block-gap, 0.5em) !important;
  }
  .tiez-rich-root dt {
    font-weight: 700;
  }
  .tiez-rich-root dd {
    margin-inline-start: 1.25em;
  }
`;

const repairUnreadableSourceColors = (root: ShadowRoot, host: HTMLElement) => {
  const themedForeground = parseCssColor(
    getComputedStyle(host).getPropertyValue("--text-primary").trim()
  );

  // Every text element walks up through the same ancestors (and the card's
  // ~15 outer layers), so cache each element's own background for this pass.
  const ownBackgrounds = new Map<Element, Rgba | null>();
  const ownBackground = (element: Element) => {
    if (!ownBackgrounds.has(element)) {
      ownBackgrounds.set(element, parseCssColor(getComputedStyle(element).backgroundColor));
    }
    return ownBackgrounds.get(element) ?? null;
  };

  const effectiveBackground = (element: Element): Rgba => {
    let background: Rgba = { r: 0, g: 0, b: 0, a: 0 };
    for (const start of [element, host]) {
      let current: Element | null = start;
      while (current) {
        const parsed = ownBackground(current);
        if (parsed && parsed.a > 0) {
          background = compositeColor(background, parsed);
          if (background.a >= 0.99) return background;
        }
        current = current.parentElement;
      }
    }
    const fallback = parseCssColor(
      getComputedStyle(host).getPropertyValue("--bg-element").trim()
    );
    return fallback ?? { r: 255, g: 255, b: 255, a: 1 };
  };

  // Start from the source colours again: a fix made for the light theme (say,
  // black) stays black on a dark card unless it is undone first.
  root.querySelectorAll<HTMLElement>(`[${COLOR_FIX_ATTR}]`).forEach((element) => {
    const original = element.getAttribute(COLOR_FIX_ATTR) || "";
    element.style.removeProperty("color");
    if (original) element.style.setProperty("color", original);
    element.removeAttribute(COLOR_FIX_ATTR);
  });

  // Read everything first, then write: interleaving setProperty with
  // getComputedStyle forced a style recalculation per element.
  const fixes: Array<[HTMLElement, string]> = [];
  root.querySelectorAll<HTMLElement>(".tiez-rich-root *").forEach((element) => {
    if (!(element.textContent || "").trim()) return;
    const foreground = parseCssColor(getComputedStyle(element).color);
    if (!foreground) return;
    const background = effectiveBackground(element);
    if (isReadableOn(foreground, background)) return;
    fixes.push([element, colorToCss(readableTextColor(background, themedForeground))]);
  });
  for (const [element, color] of fixes) {
    element.setAttribute(COLOR_FIX_ATTR, element.style.getPropertyValue("color"));
    element.style.setProperty("color", color, "important");
  }
};

/** Holds an element's own inline colour while a readability fix replaces it. */
const COLOR_FIX_ATTR = "data-tiez-color-fix";

/**
 * Rendered rich text, re-checked for readability when the theme or light/dark
 * mode changes (both switch classes on <html>); the fixes depend on the card
 * colour behind the text.
 */
const mountedRichRoots = new Map<ShadowRoot, HTMLElement>();
let themeObserver: MutationObserver | null = null;
let themeRepairFrame = 0;

const watchThemeChanges = () => {
  if (themeObserver || typeof MutationObserver === "undefined") return;
  themeObserver = new MutationObserver(() => {
    cancelAnimationFrame(themeRepairFrame);
    themeRepairFrame = requestAnimationFrame(() => {
      mountedRichRoots.forEach((host, root) => {
        if (host.isConnected) repairUnreadableSourceColors(root, host);
        else mountedRichRoots.delete(root);
      });
    });
  });
  for (const target of [document.documentElement, document.body]) {
    themeObserver.observe(target, { attributes: true, attributeFilter: ["class", "data-theme", "style"] });
  }
};

const HtmlContent = ({ htmlContent, fallbackText, className, style, preview, clampLines, eager }: HtmlContentProps) => {
  const contentRef = useRef<HTMLDivElement | null>(null);
  const processedRef = useRef<{ htmlContent: string; preview?: boolean; fallbackText?: string } | null>(null);
  const [isVisible, setIsVisible] = useState(!!preview || !!eager);
  const previewPlaceholderMinHeight = (() => {
    const maxH = style?.maxHeight;
    if (typeof maxH === "number" && Number.isFinite(maxH)) return `${maxH}px`;
    if (typeof maxH === "string" && maxH.trim()) return maxH;
    return "40px";
  })();

  useEffect(() => {
    if (preview || eager) return;
    if (!contentRef.current) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setIsVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "200px" } // Load slightly before coming into view
    );

    observer.observe(contentRef.current);
    return () => observer.disconnect();
  }, [preview, eager]);

  useLayoutEffect(() => {
    if (!isVisible || !contentRef.current) return;
    const prev = processedRef.current;
    const host = contentRef.current;
    const scheduleRepair = () => {
      const root = host.shadowRoot;
      if (!root || !root.querySelector(".tiez-rich-root *")) return;
      mountedRichRoots.set(root, host);
      watchThemeChanges();
      const frame = requestAnimationFrame(() => repairUnreadableSourceColors(root, host));
      return () => cancelAnimationFrame(frame);
    };
    if (prev && prev.htmlContent === htmlContent && prev.preview === preview && prev.fallbackText === fallbackText) {
      // A re-run (React's StrictMode does one on mount) cancelled the frame
      // the first run scheduled; schedule it again or the fix never happens.
      return scheduleRepair();
    }
    processedRef.current = { htmlContent, preview, fallbackText };
    const { html: cleanHTML, hasRenderable, hasSourceStyles } = sanitizeHTML(htmlContent, preview);
    let shadowRoot = host.shadowRoot;
    if (!shadowRoot) {
      shadowRoot = host.attachShadow({ mode: "open" });
      shadowRoot.addEventListener("click", preventLinkNavigation);
      shadowRoot.addEventListener("auxclick", preventLinkNavigation);
    }
    const renderedContent = !hasRenderable && fallbackText
      ? `<div class="tiez-rich-root" dir="auto"></div>`
      : `<div class="tiez-rich-root${hasSourceStyles ? " has-source-styles" : ""}" dir="auto">${cleanHTML}</div>`;
    shadowRoot.innerHTML = `<style>${SHADOW_BASE_STYLE}${preview ? SHADOW_PREVIEW_STYLE : ""}</style>${renderedContent}`;

    if (!hasRenderable && fallbackText) {
      const fallbackRoot = shadowRoot.querySelector(".tiez-rich-root");
      if (fallbackRoot) fallbackRoot.textContent = fallbackText;
      mountedRichRoots.delete(shadowRoot);
      return;
    }
    return scheduleRepair();
  }, [htmlContent, fallbackText, isVisible, preview]);

  useEffect(() => {
    const root = contentRef.current?.shadowRoot;
    return () => {
      if (root) mountedRichRoots.delete(root);
    };
  }, []);

  return (
    <div
      ref={contentRef}
      className={className}
      data-clamp={clampLines ? "" : undefined}
      style={{
        minHeight: isVisible ? undefined : (preview ? previewPlaceholderMinHeight : '100px'),
        ...(clampLines ? { ["--rich-preview-lines" as string]: clampLines } : null),
        ...style
      }}
    />
  );
};

export default HtmlContent;
