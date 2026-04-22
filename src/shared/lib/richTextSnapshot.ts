import { toTauriLocalImageSrc } from "./localImageSrc";
import {
  isHtmlishTagText,
  isOfficeStyleDefinitionText,
  repairHtmlFragment,
  stripOfficePreviewNoise
} from "./repairHtmlFragment";

const SNAPSHOT_CACHE_LIMIT = 240;
const SNAPSHOT_CACHE_VERSION = "v8";
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
  | "tabular_render_failed"
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

const escapeXmlText = (input: string): string =>
  stripInvalidXmlChars(input)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");

const svgDataUrlFromMarkup = (svg: string): string =>
  `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;

type TabularCellSnapshot = {
  text: string;
  colspan: number;
  align: "left" | "center" | "right";
  background: string;
  color: string;
  bold: boolean;
};

type TabularSnapshot = {
  rows: TabularCellSnapshot[][];
  columnCount: number;
};

type ResolvedTabularStyle = {
  background?: string;
  color?: string;
  textAlign?: "left" | "center" | "right";
  bold?: boolean;
};

const TABULAR_TABLE_RE = /<(table|tr|td|th)\b/i;
const CLASS_NAME_RE = /\.([_a-zA-Z][-_a-zA-Z0-9]*)/g;

const normalizeCssColor = (value: string | null | undefined): string | null => {
  const raw = (value || "").trim();
  if (!raw) return null;
  if (/^(transparent|inherit|initial|unset|auto|none)$/i.test(raw)) return null;
  if (/url\(|gradient\(|var\(/i.test(raw)) return null;
  if (!/^(#[0-9a-f]{3,8}|rgba?\([^)]+\)|hsla?\([^)]+\)|[a-z]+)$/i.test(raw)) {
    return null;
  }
  return raw;
};

const readStyleProperty = (styleText: string, propertyName: string): string | null => {
  const match = styleText.match(
    new RegExp(`${propertyName}\\s*:\\s*([^;]+)`, "i")
  );
  const value = match?.[1]?.trim();
  return value || null;
};

const readColorStyleProperty = (styleText: string, propertyName: string): string | null =>
  normalizeCssColor(readStyleProperty(styleText, propertyName));

const parseTextAlign = (value?: string | null): "left" | "center" | "right" | undefined => {
  const normalized = (value || "").trim().toLowerCase();
  if (!normalized) return undefined;
  if (normalized === "center") return "center";
  if (normalized === "right" || normalized === "end") return "right";
  if (normalized === "left" || normalized === "start") return "left";
  return undefined;
};

const parseBold = (value?: string | null): boolean | undefined => {
  const normalized = (value || "").trim().toLowerCase();
  if (!normalized) return undefined;
  if (normalized === "bold") return true;
  const weight = Number.parseInt(normalized, 10);
  if (Number.isFinite(weight)) {
    return weight >= 600;
  }
  return undefined;
};

const mergeResolvedTabularStyle = (
  base: ResolvedTabularStyle | undefined,
  patch: ResolvedTabularStyle
): ResolvedTabularStyle => ({
  background: patch.background ?? base?.background,
  color: patch.color ?? base?.color,
  textAlign: patch.textAlign ?? base?.textAlign,
  bold: patch.bold ?? base?.bold,
});

const parseResolvedTabularStyle = (styleText: string): ResolvedTabularStyle => ({
  background:
    readColorStyleProperty(styleText, "background-color") ||
    readColorStyleProperty(styleText, "background") ||
    undefined,
  color: readColorStyleProperty(styleText, "color") || undefined,
  textAlign: parseTextAlign(readStyleProperty(styleText, "text-align")),
  bold: parseBold(readStyleProperty(styleText, "font-weight")),
});

const sanitizeStyleBlockText = (styleText: string): string =>
  (styleText || "")
    .replace(/<!--|-->/g, " ")
    .replace(/<!\[CDATA\[|\]\]>/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ");

const extractTabularClassStyles = (html: string): Map<string, ResolvedTabularStyle> => {
  const source = repairHtmlFragment(stripRichImageFallbackMarker((html || "").trim()));
  if (!source) return new Map();

  const doc = new DOMParser().parseFromString(source, "text/html");
  const classStyles = new Map<string, ResolvedTabularStyle>();

  for (const styleEl of Array.from(doc.querySelectorAll("style"))) {
    const cssText = sanitizeStyleBlockText(styleEl.textContent || "");
    const ruleRe = /([^{}]+)\{([^{}]*)\}/g;
    let match: RegExpExecArray | null = null;

    while ((match = ruleRe.exec(cssText)) !== null) {
      const selectors = match[1]
        .split(",")
        .map((selector) => selector.trim())
        .filter(Boolean);
      const style = parseResolvedTabularStyle(match[2] || "");
      if (!style.background && !style.color && !style.textAlign && style.bold === undefined) {
        continue;
      }

      for (const selector of selectors) {
        if (selector.includes(":")) continue;

        let classMatch: RegExpExecArray | null = null;
        while ((classMatch = CLASS_NAME_RE.exec(selector)) !== null) {
          const className = classMatch[1];
          if (!className) continue;
          classStyles.set(
            className,
            mergeResolvedTabularStyle(classStyles.get(className), style)
          );
        }
        CLASS_NAME_RE.lastIndex = 0;
      }
    }
  }

  return classStyles;
};

const resolveElementClassStyle = (
  element: Element | null,
  classStyles?: Map<string, ResolvedTabularStyle>
): ResolvedTabularStyle | undefined => {
  if (!element || !classStyles || classStyles.size === 0) return undefined;

  const classNames = (element.getAttribute("class") || "")
    .split(/\s+/)
    .map((name) => name.trim())
    .filter(Boolean);

  return classNames.reduce<ResolvedTabularStyle | undefined>((acc, className) => {
    const style = classStyles.get(className);
    return style ? mergeResolvedTabularStyle(acc, style) : acc;
  }, undefined);
};

const resolveElementStyle = (
  element: Element | null,
  classStyles?: Map<string, ResolvedTabularStyle>
): ResolvedTabularStyle | undefined => {
  if (!element) return undefined;

  const classStyle = resolveElementClassStyle(element, classStyles);
  const inlineStyle = parseResolvedTabularStyle(element.getAttribute("style") || "");
  if (!classStyle) {
    if (!inlineStyle.background && !inlineStyle.color && !inlineStyle.textAlign && inlineStyle.bold === undefined) {
      return undefined;
    }
    return inlineStyle;
  }
  return mergeResolvedTabularStyle(classStyle, inlineStyle);
};

const resolveCellAlign = (styleText: string, cell: Element): "left" | "center" | "right" => {
  const attr = (cell.getAttribute("align") || "").trim().toLowerCase();
  return parseTextAlign(readStyleProperty(styleText, "text-align")) || parseTextAlign(attr) || "left";
};

const truncateCellText = (text: string, pixelWidth: number): string => {
  const compact = text.replace(/\s+/g, " ").trim();
  if (!compact) return " ";
  const maxChars = Math.max(1, Math.floor((pixelWidth - 16) / 7));
  if (compact.length <= maxChars) return compact;
  if (maxChars <= 1) return "…";
  return `${compact.slice(0, maxChars - 1)}…`;
};

const collectTabularSnapshot = (
  html: string,
  maxRows: number,
  maxCols: number,
  classStyles?: Map<string, ResolvedTabularStyle>
): TabularSnapshot | null => {
  if (!TABULAR_TABLE_RE.test(html)) return null;

  const doc = new DOMParser().parseFromString(html, "text/html");
  const table = doc.querySelector("table");
  if (!table) return null;
  const tableStyle = resolveElementStyle(table, classStyles);
  const columnStyles: Array<ResolvedTabularStyle | undefined> = [];
  let columnCursor = 0;

  for (const col of Array.from(table.querySelectorAll("col"))) {
    const spanRaw = Number.parseInt(col.getAttribute("span") || "1", 10);
    const span = Math.max(1, Number.isFinite(spanRaw) ? spanRaw : 1);
    const colStyle = resolveElementStyle(col, classStyles);
    for (let i = 0; i < span; i++) {
      columnStyles[columnCursor++] = mergeResolvedTabularStyle(tableStyle, colStyle || {});
    }
  }

  const rows: TabularCellSnapshot[][] = [];
  let columnCount = 0;

  for (const rowEl of Array.from(table.querySelectorAll("tr")).slice(0, maxRows)) {
    const cells = Array.from(rowEl.children).filter((cell) =>
      /^(td|th)$/i.test(cell.tagName)
    );
    if (!cells.length) continue;

    let usedColumns = 0;
    const row: TabularCellSnapshot[] = [];
    const rowStyle = mergeResolvedTabularStyle(
      tableStyle,
      resolveElementStyle(rowEl, classStyles) || {}
    );

    for (const cell of cells) {
      if (usedColumns >= maxCols) break;

      const colspanRaw = Number.parseInt(cell.getAttribute("colspan") || "1", 10);
      const colspan = Math.max(
        1,
        Math.min(Number.isFinite(colspanRaw) ? colspanRaw : 1, maxCols - usedColumns)
      );
      const styleText = cell.getAttribute("style") || "";
      const columnStyle = columnStyles[usedColumns];
      const cellStyle = mergeResolvedTabularStyle(
        mergeResolvedTabularStyle(rowStyle, columnStyle || {}),
        resolveElementStyle(cell, classStyles) || {}
      );
      const text = (cell.textContent || "").replace(/\s+/g, " ").trim();

      row.push({
        text: text || " ",
        colspan,
        align: cellStyle?.textAlign || resolveCellAlign(styleText, cell) || "left",
        background:
          normalizeCssColor(cell.getAttribute("bgcolor")) ||
          readColorStyleProperty(styleText, "background-color") ||
          readColorStyleProperty(styleText, "background") ||
          cellStyle?.background ||
          (cell.tagName.toLowerCase() === "th" ? "#dbe8ff" : "#ffffff"),
        color:
          normalizeCssColor(cell.getAttribute("color")) ||
          readColorStyleProperty(styleText, "color") ||
          cellStyle?.color ||
          (cell.tagName.toLowerCase() === "th" ? "#22406f" : "#303846"),
        bold:
          cell.tagName.toLowerCase() === "th" ||
          /font-weight\s*:\s*(bold|[6-9]00)/i.test(styleText) ||
          !!cellStyle?.bold,
      });
      usedColumns += colspan;
    }

    if (!row.length) continue;
    columnCount = Math.max(columnCount, usedColumns);
    rows.push(row);
  }

  if (!rows.length || columnCount <= 0) return null;
  return {
    rows,
    columnCount,
  };
};

const buildTabularSnapshotSvg = (
  snapshot: TabularSnapshot,
  width: number,
  maxHeight: number
): string | null => {
  const outerPadding = 2;
  const rowHeight = 26;
  const headerRowHeight = 28;
  const cellPadX = 8;
  const fontSize = 12;
  const borderColor = "#c6cbd1";
  const headerBg = "#f0f3f6";
  const headerColor = "#24292e";
  const evenRowBg = "#ffffff";
  const oddRowBg = "#f8f9fb";
  const defaultTextColor = "#24292e";

  const visibleRowCount = Math.max(
    1,
    Math.min(snapshot.rows.length, Math.floor((maxHeight - outerPadding * 2) / rowHeight))
  );
  const rows = snapshot.rows.slice(0, visibleRowCount);
  if (!rows.length) return null;

  // Detect if first row is a header (all bold or all th-style background)
  const firstRowIsHeader = rows[0].every(
    (cell) => cell.bold || cell.background === "#dbe8ff"
  );

  const gridWidth = Math.max(160, width - outerPadding * 2);
  const weights = Array.from({ length: snapshot.columnCount }, () => 1);

  for (const row of rows) {
    let colIndex = 0;
    for (const cell of row) {
      const estimatedWeight = Math.max(1, Math.min(3.2, cell.text.trim().length / 5 + 0.9));
      for (let i = 0; i < cell.colspan && colIndex + i < weights.length; i++) {
        weights[colIndex + i] = Math.max(
          weights[colIndex + i],
          estimatedWeight / Math.max(1, cell.colspan)
        );
      }
      colIndex += cell.colspan;
    }
  }

  const desiredWidths = weights.map((weight) => Math.max(56, weight * 54));
  const widthScale = gridWidth / desiredWidths.reduce((sum, current) => sum + current, 0);
  const columnWidths = desiredWidths.map((current) => Math.max(30, current * widthScale));
  const accumulatedWidth = columnWidths.reduce((sum, current) => sum + current, 0);
  if (columnWidths.length > 0) {
    columnWidths[columnWidths.length - 1] += gridWidth - accumulatedWidth;
  }

  const colOffsets: number[] = [];
  let runningX = outerPadding;
  for (const colWidth of columnWidths) {
    colOffsets.push(runningX);
    runningX += colWidth;
  }

  const totalRowsHeight = rows.reduce((sum, _, idx) =>
    sum + (idx === 0 && firstRowIsHeader ? headerRowHeight : rowHeight), 0);
  const height = totalRowsHeight + outerPadding * 2;
  const tableRight = outerPadding + gridWidth;
  const parts: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`,
    // Outer background with subtle rounded corners
    `<rect x="${outerPadding}" y="${outerPadding}" width="${gridWidth}" height="${totalRowsHeight}" rx="3" ry="3" fill="${evenRowBg}" />`,
  ];

  let currentY = outerPadding;

  rows.forEach((row, rowIndex) => {
    const isHeader = rowIndex === 0 && firstRowIsHeader;
    const curRowHeight = isHeader ? headerRowHeight : rowHeight;
    const y = currentY;
    let colIndex = 0;

    // Row background
    const rowBg = isHeader
      ? headerBg
      : rowIndex % 2 === 0
        ? evenRowBg
        : oddRowBg;

    // Clip to rounded corners for first and last row
    if (rowIndex === 0) {
      parts.push(
        `<clipPath id="topClip"><rect x="${outerPadding}" y="${y}" width="${gridWidth}" height="${curRowHeight}" rx="3" ry="3" /></clipPath>`,
        `<rect x="${outerPadding}" y="${y}" width="${gridWidth}" height="${curRowHeight}" fill="${rowBg}" clip-path="url(#topClip)" />`
      );
    } else if (rowIndex === rows.length - 1) {
      parts.push(
        `<clipPath id="botClip"><rect x="${outerPadding}" y="${y}" width="${gridWidth}" height="${curRowHeight}" rx="3" ry="3" /></clipPath>`,
        `<rect x="${outerPadding}" y="${y}" width="${gridWidth}" height="${curRowHeight}" fill="${rowBg}" clip-path="url(#botClip)" />`
      );
    } else {
      parts.push(
        `<rect x="${outerPadding}" y="${y}" width="${gridWidth}" height="${curRowHeight}" fill="${rowBg}" />`
      );
    }

    // Cell backgrounds that differ from row bg, and cell text
    row.forEach((cell) => {
      if (colIndex >= columnWidths.length) return;
      const x = colOffsets[colIndex];
      const cellWidth = columnWidths
        .slice(colIndex, colIndex + cell.colspan)
        .reduce((sum, current) => sum + current, 0);

      // Draw individual cell background only if it differs from row background
      const cellBg = cell.background;
      const normalizedCellBg = cellBg?.toLowerCase();
      const normalizedRowBg = rowBg.toLowerCase();
      if (normalizedCellBg && normalizedCellBg !== normalizedRowBg
        && normalizedCellBg !== "#ffffff" || (isHeader && normalizedCellBg !== headerBg.toLowerCase())) {
        // Only draw if the cell has a meaningful custom background
        if (normalizedCellBg !== "#ffffff" && normalizedCellBg !== evenRowBg.toLowerCase()) {
          parts.push(
            `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${cellWidth.toFixed(1)}" height="${curRowHeight}" fill="${escapeXmlText(cellBg)}" />`
          );
        }
      }

      // Cell text
      const text = escapeXmlText(truncateCellText(cell.text, cellWidth));
      const anchor =
        cell.align === "center" ? "middle" : cell.align === "right" ? "end" : "start";
      const textX =
        cell.align === "center"
          ? x + cellWidth / 2
          : cell.align === "right"
            ? x + cellWidth - cellPadX
            : x + cellPadX;

      const textColor = isHeader
        ? headerColor
        : (cell.color && cell.color.toLowerCase() !== "#303846"
            ? cell.color
            : defaultTextColor);
      const fontWeight = isHeader || cell.bold ? 600 : 400;

      parts.push(
        `<text x="${textX.toFixed(1)}" y="${(y + curRowHeight / 2 + 1).toFixed(
          1
        )}" fill="${escapeXmlText(textColor)}" font-family="Segoe UI, Microsoft YaHei, sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" dominant-baseline="middle" text-anchor="${anchor}">${text}</text>`
      );

      colIndex += cell.colspan;
    });

    // Horizontal grid line below each row (except last)
    if (rowIndex < rows.length - 1) {
      const lineY = y + curRowHeight;
      const lineColor = isHeader ? "#b0b8c1" : borderColor;
      parts.push(
        `<line x1="${outerPadding}" y1="${lineY}" x2="${tableRight}" y2="${lineY}" stroke="${lineColor}" stroke-width="${isHeader ? 1.5 : 0.5}" />`
      );
    }

    currentY += curRowHeight;
  });

  // Vertical grid lines between columns
  for (let ci = 1; ci < columnWidths.length; ci++) {
    const lx = colOffsets[ci];
    parts.push(
      `<line x1="${lx}" y1="${outerPadding}" x2="${lx}" y2="${outerPadding + totalRowsHeight}" stroke="${borderColor}" stroke-width="0.5" />`
    );
  }

  // Outer border
  parts.push(
    `<rect x="${outerPadding}" y="${outerPadding}" width="${gridWidth}" height="${totalRowsHeight}" rx="3" ry="3" fill="none" stroke="${borderColor}" stroke-width="1" />`
  );

  parts.push("</svg>");
  return parts.join("");
};

const tryBuildTabularSnapshotDataUrl = (
  html: string,
  originalHtml: string,
  width: number,
  maxHeight: number
): string | null => {
  const classStyles = extractTabularClassStyles(originalHtml);
  const snapshot = collectTabularSnapshot(html, 18, 10, classStyles);
  if (!snapshot) return null;

  const svg = buildTabularSnapshotSvg(snapshot, width, maxHeight);
  if (!svg) return null;
  return svgDataUrlFromMarkup(svg);
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

    const tabularDataUrl = tryBuildTabularSnapshotDataUrl(
      normalized.bodyHtml,
      sourceHtml,
      width,
      maxHeight
    );
    if (tabularDataUrl) {
      snapshotCache.set(key, tabularDataUrl);
      trimCache();
      return tabularDataUrl;
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
