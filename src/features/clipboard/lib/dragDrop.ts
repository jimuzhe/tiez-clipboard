import type { ClipboardEntry } from "../../../shared/types";
import { stripRichStorageMarkers } from "../../../shared/lib/repairHtmlFragment";

/** Hide only after an accepted external drop, never on Escape or row reorder. */
export const shouldHideAfterExternalDrop = (effect: DataTransfer["dropEffect"], insideWindow: boolean, pinned: boolean): boolean =>
  effect !== "none" && !insideWindow && !pinned;

const SVG_ICONS: Record<string, string> = {
  file: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>`,
  image: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>`,
  video: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>`,
  code: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>`,
  url: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>`,
  rich_text: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 7 4 4 20 4 20 7"/><line x1="9" y1="20" x2="15" y2="20"/><line x1="12" y1="4" x2="12" y2="20"/></svg>`,
  text: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="21" y1="10" x2="3" y2="10"/><line x1="21" y1="6" x2="3" y2="6"/><line x1="21" y1="14" x2="3" y2="14"/><line x1="17" y1="18" x2="3" y2="18"/></svg>`
};

const normalizeFileUri = (rawPath: string): string => {
  const p = rawPath.trim();
  if (p.startsWith("file://")) return p;
  const normalized = p.replace(/\\/g, "/");
  // encodeURI leaves "#" and "?" alone, which would cut the path short.
  const encoded = encodeURI(normalized).replace(/#/g, "%23").replace(/\?/g, "%3F");
  return `file://${normalized.startsWith("/") ? "" : "/"}${encoded}`;
};

/** The list's copy was shortened (get_clipboard_history), so a drag needs the full entry. */
export const isShortenedForList = (item: ClipboardEntry): boolean =>
  item.content.includes("... [Truncated for speed]")
  || Boolean(item.html_content?.includes("... [HTML Truncated]"));

/**
 * Configure standard dataTransfer payload for dragging items to external windows.
 */
export const setupDragDataTransfer = (
  dt: DataTransfer,
  item: ClipboardEntry,
  filePaths: string[]
): void => {
  dt.effectAllowed = "copyMove";

  if (item.content_type === "file") {
    const paths = filePaths.length > 0 ? filePaths : [item.content];
    const validPaths = paths.map((p) => p.trim()).filter(Boolean);
    const uris = validPaths.map(normalizeFileUri).join("\r\n");
    const plainList = validPaths.join("\n");

    dt.setData("text/uri-list", uris);
    dt.setData("text/plain", plainList);

    const first = validPaths[0];
    if (first) {
      const fileName = first.split(/[\\/]/).pop() || "file";
      const uri = normalizeFileUri(first);
      try {
        dt.setData("DownloadURL", `application/octet-stream:${fileName}:${uri}`);
      } catch {}
    }
  } else if (item.content_type === "url") {
    const trimmed = item.content.trim();
    dt.setData("text/uri-list", trimmed);
    dt.setData("text/plain", trimmed);
    dt.setData("text/html", `<a href="${trimmed}">${trimmed}</a>`);
  } else if (item.content_type === "image") {
    if (item.content.startsWith("data:")) {
      dt.setData("text/html", `<img src="${item.content}" />`);
      dt.setData("text/plain", "[Image]");
    } else {
      const trimmed = item.content.trim();
      const uri = normalizeFileUri(trimmed);
      dt.setData("text/uri-list", uri);
      dt.setData("text/plain", trimmed);
      dt.setData("text/html", `<img src="${uri}" />`);
      const fileName = trimmed.split(/[\\/]/).pop() || "image.png";
      try {
        dt.setData("DownloadURL", `application/octet-stream:${fileName}:${uri}`);
      } catch {}
    }
  } else if (item.content_type === "rich_text") {
    if (item.html_content) {
      dt.setData("text/html", stripRichStorageMarkers(item.html_content));
    }
    dt.setData("text/plain", item.content);
  } else if (item.content_type === "code") {
    dt.setData("text/plain", item.content);
    const escaped = item.content
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
    dt.setData("text/html", `<pre><code>${escaped}</code></pre>`);
  } else {
    dt.setData("text/plain", item.content);
  }
};

/** What a native (macOS) drag carries; null when the entry has no file or text to give. */
export type NativeDragPayload = { files: string[]; text: string | null; html: string | null; url: string | null };

export const nativeDragPayload = (item: ClipboardEntry, filePaths: string[]): NativeDragPayload | null => {
  const none = { files: [] as string[], text: null, html: null, url: null };
  const type = item.content_type;
  if (type === "file" || type === "video" || type === "image") {
    // Embedded images (data: URLs) have no file to hand over.
    if (item.content.startsWith("data:")) return null;
    const paths = (type === "file" && filePaths.length > 0 ? filePaths : item.content.split("\n"))
      .map((p) => p.trim())
      .filter(Boolean);
    return paths.length > 0 ? { ...none, files: paths } : null;
  }
  if (type === "url") {
    const url = item.content.trim();
    return { ...none, text: url, url };
  }
  if (type === "rich_text") {
    return { ...none, text: item.content, html: item.html_content ? stripRichStorageMarkers(item.html_content) : null };
  }
  if (type === "code") {
    const escaped = item.content.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    return { ...none, text: item.content, html: `<pre><code>${escaped}</code></pre>` };
  }
  return item.content ? { ...none, text: item.content } : null;
};

const roundedRect = (ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, radius: number) => {
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + width, y, x + width, y + height, radius);
  ctx.arcTo(x + width, y + height, x, y + height, radius);
  ctx.arcTo(x, y + height, x, y, radius);
  ctx.arcTo(x, y, x + width, y, radius);
  ctx.closePath();
};

const drawDragIcon = (ctx: CanvasRenderingContext2D, type: string, centerY: number) => {
  const svg = new DOMParser().parseFromString(SVG_ICONS[type] || SVG_ICONS.text, "image/svg+xml");
  ctx.save();
  ctx.translate(12.5, centerY - 6.5);
  ctx.scale(13 / 24, 13 / 24);
  ctx.strokeStyle = "#ffffff";
  ctx.lineWidth = 2.5;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  for (const shape of svg.documentElement.children) {
    const number = (name: string) => Number(shape.getAttribute(name) || 0);
    if (shape.tagName === "path") {
      ctx.stroke(new Path2D(shape.getAttribute("d") || ""));
      continue;
    }
    ctx.beginPath();
    if (shape.tagName === "rect") {
      roundedRect(ctx, number("x"), number("y"), number("width"), number("height"), number("rx"));
    } else if (shape.tagName === "circle") {
      ctx.arc(number("cx"), number("cy"), number("r"), 0, Math.PI * 2);
    } else if (shape.tagName === "line") {
      ctx.moveTo(number("x1"), number("y1"));
      ctx.lineTo(number("x2"), number("y2"));
    } else if (shape.tagName === "polyline" || shape.tagName === "polygon") {
      const points = (shape.getAttribute("points") || "").trim().split(/[\s,]+/).map(Number);
      for (let i = 0; i + 1 < points.length; i += 2) {
        if (i === 0) ctx.moveTo(points[i], points[i + 1]);
        else ctx.lineTo(points[i], points[i + 1]);
      }
      if (shape.tagName === "polygon") ctx.closePath();
    }
    ctx.stroke();
  }
  ctx.restore();
};

/**
 * Drag preview label of a hidden sensitive entry: the masked text the list
 * shows, never the entry's own text, file name or path.
 */
export const hiddenDragLabel = (
  item: ClipboardEntry,
  maskedText: string,
  t: (key: string) => string
): string => {
  if (item.content_type === "image") return t("image");
  if (item.content_type === "file" || item.content_type === "video") return "SENSITIVE";
  return maskedText || "SENSITIVE";
};

/** A transparent bitmap keeps the native drag preview free of a DOM backdrop. */
export const createDragGhostBadge = (
  item: ClipboardEntry,
  filePaths: string[],
  theme: string,
  t: (key: string) => string,
  /** Pixels per point: 2 for a crisp native (Retina) drag image. */
  scale = 1,
  /** Shown instead of the entry's content (see hiddenDragLabel). */
  hiddenLabel?: string
): HTMLCanvasElement => {
  let labelText = "";
  if (hiddenLabel !== undefined) {
    labelText = hiddenLabel;
  } else if (item.content_type === "file") {
    const paths = filePaths.length > 0 ? filePaths : [item.content];
    const valid = paths.map((p) => p.trim()).filter(Boolean);
    if (valid.length > 1) {
      labelText = `${valid.length} ${t("items")}`;
    } else if (valid[0]) {
      labelText = valid[0].split(/[\\/]/).pop() || "File";
    } else {
      labelText = item.content.split(/[\\/]/).pop() || "File";
    }
  } else if (item.content_type === "image") {
    labelText = t("image");
  } else if (item.content_type === "url") {
    labelText = item.content.replace(/^https?:\/\//, "");
  } else {
    labelText = item.content.trim().replace(/\s+/g, " ");
  }
  // Invisible direction controls and line separators should not shape the badge.
  labelText = labelText.replace(/[\u200e\u200f\u202a-\u202e\u2066-\u2069]/g, "").replace(/\s+/g, " ");
  const characters = Array.from(labelText);
  if (characters.length > 24) labelText = characters.slice(0, 22).join("") + "…";

  const badge = document.createElement("canvas");
  const ctx = badge.getContext("2d");
  if (!ctx) throw new Error("Unable to create drag preview");
  const font = '500 12px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';
  ctx.font = font;
  ctx.direction = "ltr";
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
  const textWidth = (text: string) => {
    const metrics = ctx.measureText(text);
    return Math.max(metrics.width, metrics.actualBoundingBoxRight) + Math.max(0, metrics.actualBoundingBoxLeft);
  };
  // Fit the actual glyph bounds, including emoji and font fallback, while
  // preserving whole Unicode code points when adding the ellipsis.
  while (textWidth(labelText) > 170) {
    labelText = Array.from(labelText.replace(/…$/, "")).slice(0, -1).join("") + "…";
  }
  const metrics = ctx.measureText(labelText);
  const labelLeft = 38;
  const labelWidth = Math.ceil(textWidth(labelText));
  const ascent = Math.max(0, metrics.actualBoundingBoxAscent);
  const descent = Math.max(0, metrics.actualBoundingBoxDescent);
  const width = labelLeft + labelWidth + 18;
  const height = 34;
  badge.width = Math.round(width * scale);
  badge.height = Math.round(height * scale);
  const centerY = height / 2;
  badge.className = "drag-ghost-badge";
  Object.assign(badge.style, {
    position: "fixed",
    top: "0",
    left: "0",
    zIndex: "999999",
    pointerEvents: "none",
    overflow: "hidden",
    borderRadius: "17px",
    contain: "paint",
    background: "transparent",
    border: "none",
    boxShadow: "none",
    width: `${width}px`,
    height: `${height}px`
  });
  // Resizing the canvas reset the context; draw in points from here on.
  ctx.setTransform(scale, 0, 0, scale, 0, 0);

  const isDark = theme === "dark" || document.documentElement.classList.contains("dark-mode");
  // Canvas starts transparent. Paint only the rounded pill; no outer shadow
  // or rectangular element background is included in the drag image.
  roundedRect(ctx, 0.5, 0.5, width - 1, height - 1, centerY - 0.5);
  ctx.fillStyle = isDark ? "#18181c" : "#ffffff";
  ctx.fill();
  ctx.strokeStyle = isDark ? "rgba(255,255,255,0.16)" : "rgba(0,0,0,0.12)";
  ctx.lineWidth = 1;
  ctx.stroke();

  ctx.beginPath();
  ctx.arc(19, centerY, 11, 0, Math.PI * 2);
  ctx.fillStyle = getComputedStyle(document.documentElement).getPropertyValue("--accent-color").trim() || "#3b82f6";
  ctx.fill();
  drawDragIcon(ctx, item.content_type, centerY);
  // Resizing the canvas resets its context. Set the same font/alignment used
  // for measurement explicitly, then clip all text to the pill's safe area.
  ctx.save();
  roundedRect(ctx, 1, 1, width - 2, height - 2, centerY - 1);
  ctx.clip();
  ctx.beginPath();
  ctx.rect(labelLeft, 6, labelWidth, height - 12);
  ctx.clip();
  ctx.font = font;
  ctx.direction = "ltr";
  ctx.textAlign = "left";
  ctx.fillStyle = isDark ? "#f4f4f5" : "#18181b";
  ctx.textBaseline = "alphabetic";
  ctx.fillText(labelText, labelLeft + Math.max(0, metrics.actualBoundingBoxLeft), centerY + (ascent - descent) / 2);
  ctx.restore();
  return badge;
};
