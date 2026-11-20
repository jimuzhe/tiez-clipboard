import { useEffect, useMemo, useState, useRef } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { motion } from "framer-motion";
import {
    FileText,
    Image as ImageIcon,
    Link as LinkIcon,
    Code,
    File,
    FileQuestion,
    Files,
    ImageOff,
    Video,
    AppWindow,
    Clock
} from "lucide-react";
import HtmlContent from "../../../shared/components/HtmlContent";
import {
    applyNativePalette,
    applyThemeClasses,
    DEFAULT_THEME,
    normalizeThemeId
} from "../../../shared/config/themes";
import { getConciseTime } from "../../../shared/lib/utils";
import { getSourceAppIcon, peekSourceAppIcon } from "../../../shared/lib/sourceAppIcon";
import type { Locale } from "../../../shared/types";
import { translations } from "../../../locales";
import { toTauriLocalImageSrc } from "../../../shared/lib/localImageSrc";
import { stripRichStorageMarkers } from "../../../shared/lib/repairHtmlFragment";
import { getRichTextSnapshotDataUrl } from "../../../shared/lib/richTextSnapshot";
import { ensureInstalledThemeStyle } from "../../../shared/lib/themePackages";
import { preferRichImageFallback } from "../lib/richPreview";

const TYPE_LABEL_KEYS: Record<string, string> = {
    code: "type_code", link: "type_url", url: "type_url", file: "type_file",
    image: "type_image", video: "type_video", rich_text: "type_rich_text"
};

/** The content type in the user's language; it showed the raw id ("rich_text"). */
const typeLabel = (type: string, language: Locale = "zh") => {
    const key = (TYPE_LABEL_KEYS[type] ?? "type_text") as keyof typeof translations["zh"];
    return translations[language]?.[key] || translations.zh[key];
};

type PreviewPayload = {
    requestId: number;
    contentType: string;
    content: string;
    preview?: string;
    htmlContent?: string;
    filePreviewExists?: boolean;
    sourceApp?: string;
    sourceAppPath?: string;
    sourceAppIcon?: string | null;
    timestamp?: number;
    language?: Locale;
    theme?: string;
    colorMode?: "light" | "dark";
    nativePalette?: string;
    richTextSnapshotPreview?: boolean;
    clipboardItemFontSize?: number;
    clipboardTagFontSize?: number;
    maxWidth: number;
    maxHeight: number;
    mediaMaxWidth: number;
    mediaMaxHeight: number;
    minWidth: number;
};

type PreviewResizePayload = {
    requestId: number;
    width: number;
    height: number;
};

/**
 * Transparent gap between the card and the window edge. A border right on the
 * edge loses its right and bottom sides at fractional display scaling (only
 * left and top showed, like the main window's old CSS border).
 */
const CARD_INSET_PX = 1;
/** Card padding and border around rich content (see .compact-popover-portal). */
const RICH_CARD_CHROME_PX = 26;
/** Matches the 64px the card keeps above and below a media preview. */
const RICH_SNAPSHOT_CHROME_HEIGHT_PX = 64;

/** Short rich text gets a compact card; longer text uses the room there is. */
const richCardWidth = (payload: { content?: string; maxWidth: number }) =>
    Math.min(payload.maxWidth, (payload.content || "").trim().length <= 80 ? 380 : 520);
const RICH_IMAGE_FALLBACK_PREFIX = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX = "-->";

const extractRichImageFallback = (html?: string): { cleanHtml?: string; imagePayload?: string } => {
    if (!html) return {};
    const start = html.lastIndexOf(RICH_IMAGE_FALLBACK_PREFIX);
    if (start < 0) return { cleanHtml: html };

    const markerStart = start + RICH_IMAGE_FALLBACK_PREFIX.length;
    const endRel = html.slice(markerStart).indexOf(RICH_IMAGE_FALLBACK_SUFFIX);
    if (endRel < 0) return { cleanHtml: html };

    const markerEnd = markerStart + endRel;
    const payload = html.slice(markerStart, markerEnd).trim();
    const cleanHtml = `${html.slice(0, start)}${html.slice(markerEnd + RICH_IMAGE_FALLBACK_SUFFIX.length)}`.trim();
    return {
        cleanHtml: cleanHtml || html,
        imagePayload: payload || undefined
    };
};

const resolveRichImageSrc = (payload: string): string | null => {
    const value = payload.trim();
    if (!value) return null;
    if (value.startsWith("data:image/")) return value;
    if (/^https?:\/\/asset\.localhost\//i.test(value)) return value;
    return toTauriLocalImageSrc(value);
};

const COMPACT_PREVIEW_DEBUG = false;
const RICH_PREVIEW_DEBUG = import.meta.env.DEV;
const compactPreviewLog = (...args: unknown[]) => {
    if (!COMPACT_PREVIEW_DEBUG) return;
    const ts = new Date().toISOString();
    console.log(`[CompactPreview][Preview][${ts}]`, ...args);
};
const richPreviewFailureLog = (stage: string, detail?: Record<string, unknown>) => {
    if (!RICH_PREVIEW_DEBUG) return;
    console.warn("[RichTextPreview][CompactWindow]", stage, detail || {});
};

const getIcon = (type: string) => {
    switch (type) {
        case "text": return <FileText size={14} />;
        case "image": return <ImageIcon size={14} />;
        case "url": return <LinkIcon size={14} />;
        case "code": return <Code size={14} />;
        case "file": return <File size={14} />;
        case "video": return <Video size={14} />;
        default: return <FileText size={14} />;
    }
};

const seekVideoPreviewFrame = (video: HTMLVideoElement | null) => {
    if (!video) return;
    const duration = video.duration;
    if (!Number.isFinite(duration) || duration <= 0) return;
    const maxSeek = Math.max(duration - 0.05, 0);
    if (maxSeek <= 0) return;
    const preferred = Math.min(duration * 0.1, 2);
    const target = Math.min(Math.max(preferred, 0.1), maxSeek);
    if (target <= 0) return;
    try {
        video.currentTime = target;
    } catch {
        // Ignore seek errors; fallback will just show the first frame.
    }
};

const applyTheme = async (payload: PreviewPayload) => {
    const theme = normalizeThemeId(payload.theme || DEFAULT_THEME);
    const colorMode = payload.colorMode || "light";

    const root = document.documentElement;
    const body = document.body;

    const installedThemeReady = await ensureInstalledThemeStyle(theme);
    const effectiveTheme = installedThemeReady ? theme : DEFAULT_THEME;
    applyThemeClasses(effectiveTheme, root, body);
    applyNativePalette(payload.theme || effectiveTheme, payload.nativePalette, root, body);

    root.classList.remove("light-mode", "dark-mode");
    body.classList.remove("light-mode", "dark-mode");
    if (colorMode === "dark") {
        root.classList.add("dark-mode");
        body.classList.add("dark-mode");
    } else {
        root.classList.add("light-mode");
        body.classList.add("light-mode");
    }

    body.classList.add("compact-preview");

    if (payload.clipboardItemFontSize) {
        root.style.setProperty("--clipboard-item-font-size", `${payload.clipboardItemFontSize}px`);
    }
    if (payload.clipboardTagFontSize) {
        root.style.setProperty("--clipboard-tag-font-size", `${payload.clipboardTagFontSize}px`);
    }

    compactPreviewLog("theme applied", {
        theme,
        colorMode,
        itemFontSize: payload.clipboardItemFontSize,
        tagFontSize: payload.clipboardTagFontSize
    });
};

const applyPreviewBounds = (payload: PreviewPayload) => {
    const root = document.documentElement;
    root.style.setProperty("--preview-max-width", `${payload.maxWidth}px`);
    root.style.setProperty("--preview-max-height", `${payload.maxHeight}px`);
    root.style.setProperty("--preview-media-max-width", `${payload.mediaMaxWidth}px`);
    root.style.setProperty("--preview-media-max-height", `${payload.mediaMaxHeight}px`);
    root.style.setProperty("--preview-min-width", `${payload.minWidth}px`);
};

const CompactPreviewWindow = () => {
    const [payload, setPayload] = useState<PreviewPayload | null>(null);
    const [sourceAppIcon, setSourceAppIcon] = useState<string | null>(null);
    const [snapshotFailedRequestId, setSnapshotFailedRequestId] = useState<number | null>(null);
    const [richImageFallbackFailedRequestId, setRichImageFallbackFailedRequestId] = useState<number | null>(null);
    const [mediaFailedRequestId, setMediaFailedRequestId] = useState<number | null>(null);
    const richSnapshotImgRef = useRef<HTMLImageElement | null>(null);
    const richSnapshotFallbackTimerRef = useRef<number | null>(null);
    const containerRef = useRef<HTMLDivElement | null>(null);
    const metaRef = useRef<HTMLDivElement | null>(null);
    const contentRef = useRef<HTMLDivElement | null>(null);
    const dividerRef = useRef<HTMLDivElement | null>(null);
    const requestResizeRef = useRef<() => void>(() => {});
    const previewBoundsRef = useRef({ width: 560, height: 560, mediaWidth: 520, mediaHeight: 360 });
    const latestRequestIdRef = useRef(0);
    const lastSentSizeRef = useRef<PreviewResizePayload | null>(null);
    const richImageFallbackSrc = useMemo(() => {
        if (!payload || payload.contentType !== "rich_text" || !payload.htmlContent) return null;
        const { imagePayload } = extractRichImageFallback(payload.htmlContent);
        if (!imagePayload) return null;
        const src = resolveRichImageSrc(imagePayload);
        if (!src) return null;
        // The source app's bitmap, shrunk to the card, was unreadable for text.
        const useBitmap = preferRichImageFallback({
            imageSrc: src,
            text: payload.content,
            html: payload.htmlContent,
            sourceApp: payload.sourceApp,
            sourceAppPath: payload.sourceAppPath
        });
        return useBitmap ? src : null;
    }, [payload]);
    const richTextSnapshotSrc = useMemo(() => {
        if (!payload || payload.contentType !== "rich_text" || !payload.htmlContent) return null;
        if (!payload.richTextSnapshotPreview) return null;
        const { cleanHtml } = extractRichImageFallback(payload.htmlContent);
        const htmlForSnapshot = stripRichStorageMarkers(cleanHtml || payload.htmlContent);
        // Rendered at the width it is shown at; the card crops what doesn't
        // fit instead of shrinking the whole image until the text is tiny.
        return getRichTextSnapshotDataUrl(htmlForSnapshot, {
            width: richCardWidth(payload) - RICH_CARD_CHROME_PX,
            // The space the card gives it (see the media max-height in
            // compact-preview.css); a taller image was shrunk to fit,
            // text and all.
            maxHeight: payload.maxHeight - RICH_SNAPSHOT_CHROME_HEIGHT_PX
        });
    }, [payload]);
    const snapshotFailed = !!payload && snapshotFailedRequestId === payload.requestId;
    const richImageFallbackFailed = !!payload && richImageFallbackFailedRequestId === payload.requestId;
    const mediaFailed = !!payload && mediaFailedRequestId === payload.requestId;
    const effectiveRichTextSnapshotSrc = snapshotFailed ? null : richTextSnapshotSrc;
    const effectiveRichImageFallbackSrc = richImageFallbackFailed ? null : richImageFallbackSrc;
    const useSnapshotPreviewImage = !effectiveRichImageFallbackSrc && !!effectiveRichTextSnapshotSrc;

    useEffect(() => {
        compactPreviewLog("window mounted", { label: getCurrentWindow().label });
        let disposed = false;
        let stopUpdate: (() => void) | null = null;
        let stopReadyQuery: (() => void) | null = null;

        void (async () => {
            try {
                // Listen first: an update sent before the listener exists is
                // dropped. The list re-sends it once we announce ready below.
                const windowSetup = Promise.allSettled([
                    getCurrentWindow().setFocusable(false),
                    getCurrentWindow().setIgnoreCursorEvents(true)
                ]);

                stopUpdate = await listen<PreviewPayload>("compact-preview-update", (event) => {
                    // A disposed listener (StrictMode remount) must not mark
                    // the request as seen, or the live one skips it as a repeat.
                    if (disposed) return;
                    const next = event.payload;
                    if (latestRequestIdRef.current === next.requestId) {
                        requestResizeRef.current?.();
                        return;
                    }
                    latestRequestIdRef.current = next.requestId;
                    lastSentSizeRef.current = null;
                    applyPreviewBounds(next);
                    previewBoundsRef.current = {
                        width: next.maxWidth,
                        height: next.maxHeight,
                        mediaWidth: next.mediaMaxWidth,
                        mediaHeight: next.mediaMaxHeight
                    };
                    compactPreviewLog("received compact-preview-update", {
                        requestId: next.requestId,
                        contentType: next.contentType,
                        contentLength: next.content?.length ?? 0,
                        previewLength: next.preview?.length ?? 0,
                        hasHtml: !!next.htmlContent,
                        sourceApp: next.sourceApp
                    });
                    const peeked =
                        next.sourceAppIcon ||
                        peekSourceAppIcon(next.sourceAppPath) ||
                        null;
                    setSourceAppIcon(peeked);
                    setPayload(next);
                    if (!peeked && next.sourceAppPath) {
                        void getSourceAppIcon(next.sourceAppPath).then((icon) => {
                            if (!disposed && latestRequestIdRef.current === next.requestId) {
                                setSourceAppIcon(icon);
                                lastSentSizeRef.current = null;
                                requestResizeRef.current?.();
                            }
                        });
                    }
                    void applyTheme(next).then(() => {
                        if (!disposed && latestRequestIdRef.current === next.requestId) {
                            requestResizeRef.current?.();
                        }
                    }).catch((err) => {
                        console.error("Failed to apply compact preview theme:", err);
                    });
                });
                if (disposed) {
                    stopUpdate();
                    stopUpdate = null;
                    return;
                }

                // Broadcast, not emitTo("main"): emit_to is refused for this
                // window in release builds, so the list never heard "ready".
                stopReadyQuery = await listen("compact-preview-ready-query", () => {
                    void emit("compact-preview-ready", true).catch(() => {});
                });
                if (disposed) {
                    stopReadyQuery();
                    stopReadyQuery = null;
                    return;
                }

                await windowSetup;
                if (!disposed) {
                    await emit("compact-preview-ready", true);
                    compactPreviewLog("compact preview listeners ready");
                }
            } catch (err) {
                console.error("Failed to initialize compact preview window:", err);
                compactPreviewLog("compact preview initialization failed", err);
            }
        })();

        return () => {
            disposed = true;
            compactPreviewLog("window unmount cleanup");
            stopUpdate?.();
            stopReadyQuery?.();
        };
    }, []);

    useEffect(() => {
        if (!payload) {
            compactPreviewLog("skip resize effect: payload empty");
            return;
        }
        let raf = 0;
        let timerA: number | null = null;
        let timerB: number | null = null;
        let timerC: number | null = null;
const measurePlainTextWidth = (text: string, contentEl: HTMLElement | null): number => {
    const lines = text.split("\n");
    const style = contentEl ? getComputedStyle(contentEl) : null;
    const fontSize = style?.fontSize || "13px";
    const fontFamily = style?.fontFamily || "sans-serif";
    const fontWeight = style?.fontWeight || "400";
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d");
    if (!ctx) {
        return Math.ceil(Math.max(...lines.map((line) => line.length), 1) * 8.5);
    }
    ctx.font = `${fontWeight} ${fontSize} ${fontFamily}`;
    let max = 0;
    for (const line of lines) {
        max = Math.max(max, ctx.measureText(line || " ").width);
    }
    return Math.ceil(max);
};

/** Measure meta at its unconstrained natural width so type · app · time never clip. */
const measureMetaNaturalWidth = (metaEl: HTMLElement | null): number => {
    if (!metaEl) return 240;

    const clone = metaEl.cloneNode(true) as HTMLElement;
    clone.style.cssText = [
        "position:absolute",
        "left:-10000px",
        "top:0",
        "visibility:hidden",
        "pointer-events:none",
        "display:inline-flex",
        "flex-direction:row",
        "align-items:center",
        "flex-wrap:nowrap",
        "width:max-content",
        "max-width:none",
        "min-width:0",
        "overflow:visible",
        "white-space:nowrap",
        "gap:6px",
        "margin:0",
        "padding:0",
        "border:0",
        "font:inherit",
    ].join(";");

    for (const child of Array.from(clone.querySelectorAll<HTMLElement>("*"))) {
        child.style.flexShrink = "0";
        child.style.maxWidth = "none";
        child.style.overflow = "visible";
        child.style.whiteSpace = "nowrap";
        child.style.width = "max-content";
    }

    document.body.appendChild(clone);
    const width = Math.ceil(
        Math.max(clone.scrollWidth, clone.offsetWidth, clone.getBoundingClientRect().width)
    );
    clone.remove();
    return Math.max(180, width);
};

const calculateStandardCardDimensions = (
    payload: PreviewPayload,
    bounds: { width: number; height: number; mediaWidth: number; mediaHeight: number },
    chromeWidth: number,
    chromeHeight: number,
    container: HTMLElement,
    contentEl: HTMLElement | null,
    metaEl: HTMLElement | null
): { width: number; height: number } => {
    const maxW = Math.min(bounds.width, 540);
    const maxH = bounds.height;
    const applyCardWidth = (width: number) => {
        container.style.setProperty("--preview-card-width", `${width}px`);
        container.style.width = `${width}px`;
    };
    const baseMinW = 168;
    // Meta row drives the floor — keep a little slack for icon/font rounding.
    const metaMinWidth = Math.min(
        maxW,
        Math.max(220, measureMetaNaturalWidth(metaEl) + chromeWidth + 8)
    );

    // 1. Media Preview (Images, Videos, Fallback snapshots)
    const isMedia = payload.contentType === "image" || payload.contentType === "video";
    if (isMedia) {
        const standardMediaWidth = Math.min(maxW, 460);
        const availableMediaHeight = Math.max(120, Math.floor(maxH - chromeHeight - 8));
        const availableMediaWidth = Math.max(160, Math.floor(standardMediaWidth - chromeWidth));

        container.style.setProperty("--preview-media-fit-height", `${availableMediaHeight}px`);
        container.style.setProperty("--preview-media-fit-width", `${availableMediaWidth}px`);

        const mediaEl = contentEl?.querySelector<HTMLImageElement | HTMLVideoElement>("img, video");
        if (mediaEl instanceof HTMLImageElement && mediaEl.naturalWidth > 0 && mediaEl.naturalHeight > 0) {
            const nw = mediaEl.naturalWidth;
            const nh = mediaEl.naturalHeight;
            // Vertical phone screenshot (aspect ratio < 0.68) -> slightly narrower card 360px
            const isTallPortrait = nw / nh < 0.68;
            const chosenTargetWidth = isTallPortrait ? Math.min(maxW, 360) : standardMediaWidth;
            const availW = chosenTargetWidth - chromeWidth;
            const scale = Math.min(1, availW / nw, availableMediaHeight / nh);
            const fitW = Math.round(nw * scale);
            const fitH = Math.round(nh * scale);
            const finalW = Math.min(maxW, Math.max(baseMinW, metaMinWidth, fitW + chromeWidth));
            const finalH = Math.min(maxH, Math.max(80, fitH + chromeHeight));
            applyCardWidth(finalW);
            return { width: finalW, height: finalH };
        } else if (mediaEl && mediaEl.getBoundingClientRect().height > 0) {
            const r = mediaEl.getBoundingClientRect();
            const finalW = Math.max(standardMediaWidth, metaMinWidth);
            applyCardWidth(finalW);
            return {
                width: finalW,
                height: Math.min(maxH, Math.max(80, Math.ceil(r.height + chromeHeight)))
            };
        }
        const finalW = Math.max(standardMediaWidth, metaMinWidth);
        applyCardWidth(finalW);
        return {
            width: finalW,
            height: Math.min(maxH, Math.max(160, Math.floor(availableMediaHeight * 0.7) + chromeHeight))
        };
    }

    // 2. Code Preview
    if (payload.contentType === "code") {
        const standardCodeWidth = Math.max(metaMinWidth, Math.min(maxW, 480));
        applyCardWidth(standardCodeWidth);
        const measuredHeight = Math.ceil(container.scrollHeight || container.offsetHeight || container.getBoundingClientRect().height);
        return {
            width: standardCodeWidth,
            height: Math.min(maxH, Math.max(80, measuredHeight))
        };
    }

    // 3. File List Preview
    if (payload.contentType === "file") {
        const standardFileWidth = Math.max(metaMinWidth, Math.min(maxW, 380));
        applyCardWidth(standardFileWidth);
        const measuredHeight = Math.ceil(container.scrollHeight || container.offsetHeight || container.getBoundingClientRect().height);
        return {
            width: standardFileWidth,
            height: Math.min(maxH, Math.max(80, measuredHeight))
        };
    }

    // 4. Rich Text Preview
    if (payload.contentType === "rich_text") {
        const standardRichWidth = Math.max(metaMinWidth, Math.min(maxW, richCardWidth(payload)));
        applyCardWidth(standardRichWidth);
        const measuredHeight = Math.ceil(container.scrollHeight || container.offsetHeight || container.getBoundingClientRect().height);
        return {
            width: standardRichWidth,
            height: Math.min(maxH, Math.max(80, measuredHeight))
        };
    }

    // 5. Plain text / URL — never narrower than the full meta row.
    const text = (payload.content || payload.preview || "").trim();
    const hasLineBreak = text.includes("\n");
    const charCount = text.length;
    const lineCount = hasLineBreak ? text.split("\n").length : 1;
    const measuredTextWidth = measurePlainTextWidth(text, contentEl);
    const contentPad = 16;
    const contentDriven = measuredTextWidth + chromeWidth + contentPad;

    let targetWidth: number;
    if (!hasLineBreak && charCount <= 24) {
        targetWidth = Math.max(metaMinWidth, Math.min(maxW, contentDriven));
    } else if (!hasLineBreak && charCount <= 56) {
        targetWidth = Math.max(metaMinWidth, Math.min(maxW, Math.max(contentDriven, metaMinWidth)));
    } else if (charCount <= 120 && lineCount <= 3) {
        targetWidth = Math.max(metaMinWidth, Math.min(maxW, Math.max(contentDriven, 300)));
    } else if (charCount <= 220 && lineCount <= 6) {
        targetWidth = Math.max(metaMinWidth, Math.min(maxW, 380));
    } else {
        targetWidth = Math.max(metaMinWidth, Math.min(maxW, 420));
    }

    applyCardWidth(targetWidth);
    contentEl?.classList.toggle(
        "is-short-text",
        !hasLineBreak && charCount <= 56
    );
    const measuredHeight = Math.ceil(container.scrollHeight || container.offsetHeight || container.getBoundingClientRect().height);
    return {
        width: targetWidth,
        height: Math.min(maxH, Math.max(76, measuredHeight))
    };
};

        const measure = () => {
            {
                const container = containerRef.current;
                if (!container) {
                    compactPreviewLog("skip measure: container missing");
                    return;
                }

                const bounds = previewBoundsRef.current;
                const style = getComputedStyle(container);
                const pixels = (value: string) => Number.parseFloat(value) || 0;
                const outerHeight = (element: HTMLElement | null) => {
                    if (!element || getComputedStyle(element).display === "none") return 0;
                    const elementStyle = getComputedStyle(element);
                    return element.getBoundingClientRect().height
                        + pixels(elementStyle.marginTop) + pixels(elementStyle.marginBottom);
                };
                const chromeHeight = pixels(style.paddingTop) + pixels(style.paddingBottom)
                    + pixels(style.borderTopWidth) + pixels(style.borderBottomWidth)
                    + outerHeight(metaRef.current) + outerHeight(dividerRef.current);
                const chromeWidth = pixels(style.paddingLeft) + pixels(style.paddingRight)
                    + pixels(style.borderLeftWidth) + pixels(style.borderRightWidth);

                const { width, height } = calculateStandardCardDimensions(
                    payload,
                    bounds,
                    chromeWidth,
                    chromeHeight,
                    container,
                    contentRef.current,
                    metaRef.current
                );

                // Content taller than the card is cropped; fade it out so the
                // cut doesn't look like the end (the window takes no scrolling).
                const contentEl = contentRef.current;
                if (contentEl) {
                    contentEl.classList.toggle(
                        "is-clipped",
                        contentEl.scrollHeight > contentEl.clientHeight + 1
                    );
                }

                if (width < 40 || height < 40) {
                    compactPreviewLog("skip emit: measured size too small", {
                        width,
                        height
                    });
                    return;
                }

                const requestId = payload.requestId;
                if (latestRequestIdRef.current !== requestId) return;
                const last = lastSentSizeRef.current;
                if (
                    last &&
                    last.requestId === requestId &&
                    Math.abs(last.width - width) <= 1 &&
                    Math.abs(last.height - height) <= 1
                ) {
                    return;
                }
                lastSentSizeRef.current = { requestId, width, height };
                compactPreviewLog("emit compact-preview-resize", {
                    requestId,
                    width,
                    height
                });

                // The window is the card plus its inset on every side (see
                // .compact-preview-root in compact-preview.css).
                const windowSize = {
                    requestId,
                    width: width + 2 * CARD_INSET_PX,
                    height: height + 2 * CARD_INSET_PX
                };
                emit("compact-preview-resize", windowSize).catch((err) => {
                    console.error(err);
                    compactPreviewLog("emit compact-preview-resize failed", err);
                });
            }
        };

        // The window is still hidden when a new entry arrives, and a hidden
        // web view runs no animation frames: measuring only in one meant the
        // window was first shown at a default size, then jumped to its own.
        // Measure right away while hidden, so it opens at the right size.
        const updateSize = () => {
            if (raf) cancelAnimationFrame(raf);
            raf = 0;
            if (document.visibilityState === "hidden") {
                measure();
                return;
            }
            raf = window.requestAnimationFrame(() => {
                raf = 0;
                measure();
            });
        };

        requestResizeRef.current = updateSize;
        compactPreviewLog("resize effect start", { contentType: payload.contentType });
        measure();
        const observer = new ResizeObserver(updateSize);
        if (containerRef.current) observer.observe(containerRef.current);
        if (contentRef.current) observer.observe(contentRef.current);
        if (metaRef.current) observer.observe(metaRef.current);
        if (dividerRef.current) observer.observe(dividerRef.current);
        compactPreviewLog("ResizeObserver attached");

        // Async render safety net: rich text/media/font loading may settle later.
        timerA = window.setTimeout(updateSize, 50);
        timerB = window.setTimeout(updateSize, 180);
        timerC = window.setTimeout(updateSize, 420);

        return () => {
            if (raf) cancelAnimationFrame(raf);
            if (timerA) window.clearTimeout(timerA);
            if (timerB) window.clearTimeout(timerB);
            if (timerC) window.clearTimeout(timerC);
            observer.disconnect();
            compactPreviewLog("resize effect cleanup");
        };
    }, [payload]);

    useEffect(() => {
        if (!payload) return;
        // App icon / meta text can settle after the first measure — reflow once more.
        lastSentSizeRef.current = null;
        const timer = window.setTimeout(() => requestResizeRef.current?.(), 30);
        return () => window.clearTimeout(timer);
    }, [payload, sourceAppIcon]);

    useEffect(() => {
        if (richSnapshotFallbackTimerRef.current) {
            window.clearTimeout(richSnapshotFallbackTimerRef.current);
            richSnapshotFallbackTimerRef.current = null;
        }
        if (!useSnapshotPreviewImage) return;

        // Safety net: in some environments broken SVG data urls don't always emit onError.
        richSnapshotFallbackTimerRef.current = window.setTimeout(() => {
            const img = richSnapshotImgRef.current;
            if (!img || !img.complete || img.naturalWidth <= 0 || img.naturalHeight <= 0) {
                richPreviewFailureLog("snapshot image timeout -> fallback to html", {
                    hasImageElement: !!img,
                    complete: img?.complete ?? false,
                    naturalWidth: img?.naturalWidth ?? 0,
                    naturalHeight: img?.naturalHeight ?? 0,
                    sourceApp: payload?.sourceApp || ""
                });
                if (payload) setSnapshotFailedRequestId(payload.requestId);
            }
        }, 700);

        return () => {
            if (richSnapshotFallbackTimerRef.current) {
                window.clearTimeout(richSnapshotFallbackTimerRef.current);
                richSnapshotFallbackTimerRef.current = null;
            }
        };
    }, [useSnapshotPreviewImage, effectiveRichTextSnapshotSrc, payload?.requestId, payload?.content, payload?.htmlContent]);

    const content = useMemo(() => {
        if (!payload) return null;
        if (payload.contentType === "image") {
            if (payload.filePreviewExists === false || mediaFailed) {
                return (
                    <div className="compact-preview-unavailable">
                        <ImageOff size={22} />
                        <span>Preview unavailable</span>
                    </div>
                );
            }
            const src = payload.content.startsWith("data:")
                ? payload.content
                : (toTauriLocalImageSrc(payload.content) || payload.content);
            return (
                <img
                    src={src}
                    alt="preview"
                    onLoad={() => {
                        compactPreviewLog("image loaded, request resize");
                        requestResizeRef.current?.();
                    }}
                    onError={() => setMediaFailedRequestId(payload.requestId)}
                    style={{
                        maxWidth: "100%",
                        maxHeight: "var(--preview-media-fit-height, 460px)",
                        width: "auto",
                        height: "auto",
                        objectFit: "contain",
                        display: "block",
                        borderRadius: "4px"
                    }}
                />
            );
        }
        if (payload.contentType === "video") {
            if (payload.filePreviewExists === false || mediaFailed) {
                return (
                    <div className="compact-preview-unavailable">
                        <Video size={22} />
                        <span>Preview unavailable</span>
                    </div>
                );
            }
            const src = payload.content.startsWith("data:")
                ? payload.content
                : (toTauriLocalImageSrc(payload.content) || payload.content);
            return (
                <video
                    src={src}
                    preload="metadata"
                    muted
                    playsInline
                    onLoadedMetadata={(e) => {
                        compactPreviewLog("video metadata loaded", {
                            duration: e.currentTarget.duration
                        });
                        seekVideoPreviewFrame(e.currentTarget);
                        requestResizeRef.current?.();
                    }}
                    onError={() => setMediaFailedRequestId(payload.requestId)}
                    style={{
                        maxWidth: "100%",
                        maxHeight: "var(--preview-media-fit-height, 460px)",
                        width: "auto",
                        height: "auto",
                        objectFit: "contain",
                        display: "block",
                        borderRadius: "4px",
                        background: "#000"
                    }}
                />
            );
        }
        if (payload.contentType === "file") {
            if (payload.filePreviewExists === false) {
                return (
                    <div className="compact-preview-unavailable">
                        <FileQuestion size={22} />
                        <span>File unavailable</span>
                    </div>
                );
            }
            const paths = payload.content
                .split(/\r?\n/)
                .map((path) => path.trim())
                .filter(Boolean);
            return (
                <div className="compact-preview-file-list">
                    {paths.length > 1 && (
                        <div className="compact-preview-file-count">
                            <Files size={16} />
                            <span>{paths.length} items</span>
                        </div>
                    )}
                    {(paths.length > 0 ? paths : [payload.preview || payload.content]).slice(0, 12).map((path, index) => (
                        <div className="compact-preview-file-row" key={`${path}-${index}`}>
                            <File size={14} />
                            <span title={path}>{path.split(/[\\/]/).pop() || path}</span>
                        </div>
                    ))}
                    {paths.length > 12 && (
                        <div className="compact-preview-file-more">+{paths.length - 12}</div>
                    )}
                </div>
            );
        }
        if (payload.contentType === "rich_text" && payload.htmlContent) {
            if (effectiveRichImageFallbackSrc) {
                compactPreviewLog("render rich_text as fallback image");
                return (
                    <img
                        src={effectiveRichImageFallbackSrc}
                        alt="rich text preview"
                        onLoad={() => {
                            compactPreviewLog("rich fallback image loaded, request resize");
                            requestResizeRef.current?.();
                        }}
                        onError={() => {
                            richPreviewFailureLog("fallback image load error -> switch to snapshot", {
                                srcLength: (effectiveRichImageFallbackSrc || "").length,
                                srcSample: (effectiveRichImageFallbackSrc || "").slice(0, 140),
                                sourceApp: payload.sourceApp || ""
                            });
                            setRichImageFallbackFailedRequestId(payload.requestId);
                        }}
                        style={{
                            maxWidth: "100%",
                            maxHeight: "var(--preview-media-fit-height, 460px)",
                            width: "auto",
                            height: "auto",
                            objectFit: "contain",
                            display: "block",
                            borderRadius: "4px"
                        }}
                    />
                );
            }
            if (effectiveRichTextSnapshotSrc) {
                compactPreviewLog("render rich_text as html snapshot image");
                return (
                    <img
                        ref={richSnapshotImgRef}
                        src={effectiveRichTextSnapshotSrc}
                        alt="rich text snapshot preview"
                        className="rich-text-snapshot-full"
                        onLoad={() => {
                            if (richSnapshotFallbackTimerRef.current) {
                                window.clearTimeout(richSnapshotFallbackTimerRef.current);
                                richSnapshotFallbackTimerRef.current = null;
                            }
                            compactPreviewLog("rich snapshot image loaded, request resize");
                            requestResizeRef.current?.();
                        }}
                        onError={() => {
                            if (richSnapshotFallbackTimerRef.current) {
                                window.clearTimeout(richSnapshotFallbackTimerRef.current);
                                richSnapshotFallbackTimerRef.current = null;
                            }
                            richPreviewFailureLog("snapshot image load error -> fallback to html", {
                                srcLength: (effectiveRichTextSnapshotSrc || "").length,
                                srcSample: (effectiveRichTextSnapshotSrc || "").slice(0, 140),
                                sourceApp: payload.sourceApp || ""
                            });
                            setSnapshotFailedRequestId(payload.requestId);
                        }}
                        style={{
                            // Full card width, natural height: .popover-content
                            // crops (and fades) the rest.
                            width: "100%",
                            height: "auto",
                            display: "block",
                            borderRadius: "4px"
                        }}
                    />
                );
            }
            const { cleanHtml } = extractRichImageFallback(payload.htmlContent);
            return (
                <HtmlContent
                    className="rich-text-preview"
                    htmlContent={cleanHtml || payload.htmlContent}
                    fallbackText={payload.preview || payload.content}
                    preview={true}
                    style={{
                        // Keep a single scrollbar on .popover-content to avoid nested scrollbars.
                        maxHeight: "none",
                        overflow: "visible",
                        fontSize: "var(--clipboard-item-font-size)",
                        lineHeight: "1.5"
                    }}
                />
            );
        }
        return payload.content || payload.preview || "";
    }, [payload, effectiveRichImageFallbackSrc, effectiveRichTextSnapshotSrc, mediaFailed]);

    return (
        <div className="compact-preview-root">
            <motion.div
                ref={containerRef}
                className={`compact-popover-portal compact-preview-window theme-${normalizeThemeId(payload?.theme || DEFAULT_THEME)} ${payload?.contentType === "image" ? "compact-preview-image" : ""} ${payload?.contentType === "image" || payload?.contentType === "video" || !!effectiveRichImageFallbackSrc || useSnapshotPreviewImage ? "compact-preview-media" : ""} ${payload?.colorMode === "dark" ? "dark-mode" : ""}`}
                initial={{ opacity: 0, scale: 0.95, y: 3 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                transition={{
                    type: "spring",
                    damping: 26,
                    stiffness: 360,
                    mass: 0.8
                }}
                style={{
                    display: "flex",
                    flexDirection: "column",
                    transformOrigin: "center center"
                }}
            >
                <div ref={metaRef} className="popover-meta">
                    <div className="meta-row">
                        {getIcon(payload?.contentType || "text")}
                        <span>{typeLabel(payload?.contentType || "text", payload?.language)}</span>
                    </div>
                    <div className="meta-dot">•</div>
                    <div className="meta-row meta-row-source">
                        {sourceAppIcon || payload?.sourceAppIcon ? (
                            <img
                                src={sourceAppIcon || payload?.sourceAppIcon || ""}
                                alt=""
                                className="source-app-icon"
                                draggable={false}
                            />
                        ) : (
                            <AppWindow size={14} />
                        )}
                        <span>{payload?.sourceApp || "Unknown"}</span>
                    </div>
                    <div className="meta-dot">•</div>
                    <div className="meta-row">
                        <Clock size={14} />
                        <span>
                            {payload?.timestamp && payload?.language
                                ? getConciseTime(payload.timestamp, payload.language)
                                : "-"}
                        </span>
                    </div>
                </div>
                <div ref={dividerRef} className="popover-divider" />
                <motion.div
                    key={payload?.requestId}
                    ref={contentRef}
                    initial={{ opacity: 0.4 }}
                    animate={{ opacity: 1 }}
                    transition={{ duration: 0.15, ease: [0.16, 1, 0.3, 1] }}
                    className={`popover-content${payload?.contentType === "code" ? " compact-preview-code" : ""}`}
                >
                    {content}
                </motion.div>
            </motion.div>
        </div>
    );
};

export default CompactPreviewWindow;
