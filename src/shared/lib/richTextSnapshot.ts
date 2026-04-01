import { toTauriLocalImageSrc } from "./localImageSrc";
import {
  isHtmlishTagText,
  isOfficeStyleDefinitionText,
  stripOfficePreviewNoise
} from "./repairHtmlFragment";

const SNAPSHOT_CACHE_LIMIT = 240;
const SNAPSHOT_CACHE_VERSION = "v4";
const snapshotCache = new Map<string, string>();

const RICH_IMAGE_FALLBACK_PREFIX = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX = "-->";

const trimCache = () => {
  while (snapshotCache.size > SNAPSHOT_CACHE_LIMIT) {
    const first = snapshotCache.keys().next();
    if (first.done) return;
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

const normalizeRichHtml = (html: string): {
  bodyHtml: string;
  estimatedHeight: number;
  imageStats: {
    total: number;
    data: number;
    local: number;
    remote: number;
    unsupported: number;
  };
} | null => {
  const parser = new DOMParser();
  let processed = stripOfficePreviewNoise(stripRichImageFallbackMarker((html || "").trim()));
  if (!processed) return null;

  if (
    (processed.includes("<tr") || processed.includes("<td") || processed.includes("<col")) &&
    !processed.toLowerCase().includes("<table")
  ) {
    processed = `<table style="border-collapse: collapse; min-width: 100%;">${processed}</table>`;
  }

  const doc = parser.parseFromString(processed, "text/html");
  doc.querySelectorAll("script").forEach((el) => el.remove());
  doc.querySelectorAll("style").forEach((style) => {
    if (isOfficeStyleDefinitionText(style.textContent || "")) {
      style.remove();
    }
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

  return { bodyHtml, estimatedHeight, imageStats };
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

export const getRichTextSnapshotDataUrl = (
  html: string,
  options: SnapshotOptions = {}
): string | null => {
  const sourceHtml = html || "";
  const width = Math.max(480, Math.min(1800, Math.round(options.width ?? 960)));
  const maxHeight = Math.max(220, Math.min(3200, Math.round(options.maxHeight ?? 1600)));

  try {
    const trimmedLength = sourceHtml.trim().length;
    if (!trimmedLength) {
      logSnapshotFailure("empty_html", {
        htmlLength: sourceHtml.length,
        width,
        maxHeight
      });
      return null;
    }

    const key = `${SNAPSHOT_CACHE_VERSION}:${hashString(sourceHtml)}:${sourceHtml.length}:${width}:${maxHeight}`;
    const cached = snapshotCache.get(key);
    if (cached) return cached;

    const normalized = normalizeRichHtml(sourceHtml);
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

    const height = Math.max(48, Math.min(maxHeight, normalized.estimatedHeight));
    const snapshotStyle = [
      "box-sizing:border-box",
      "margin:0",
      "padding:8px 10px",
      "width:100%",
      "height:100%",
      "overflow:hidden",
      "background:transparent",
      "font-family:'Segoe UI','Microsoft YaHei',sans-serif",
      "color:#111",
      "line-height:1.35"
    ].join(";");

    const xhtmlBodyHtml = toXhtmlCompatibleFragment(normalized.bodyHtml);
    const xmlSafeBodyHtml = toXmlSafeNamedEntities(xhtmlBodyHtml);

    const svg = [
      `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`,
      `<foreignObject x="0" y="0" width="100%" height="100%">`,
      '<div xmlns="http://www.w3.org/1999/xhtml" style="',
      snapshotStyle,
      '">',
      `<style>
      * { box-sizing: border-box; }
      table { border-collapse: collapse; border-spacing: 0; }
      img, video { max-width: 100%; height: auto; }
      td, th { vertical-align: top; }
    </style>`,
      xmlSafeBodyHtml,
      "</div>",
      "</foreignObject>",
      "</svg>"
    ].join("");

    const safeSvg = stripInvalidXmlChars(svg);
    const parseError = getSvgParseError(safeSvg);
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

    snapshotCache.set(key, dataUrl);
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
