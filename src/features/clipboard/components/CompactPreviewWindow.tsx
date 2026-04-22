import { useEffect, useMemo, useState, useRef } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow, LogicalSize, currentMonitor } from "@tauri-apps/api/window";
import {
    FileText,
    Image as ImageIcon,
    Link as LinkIcon,
    Code,
    File,
    Video,
    AppWindow,
    Clock
} from "lucide-react";
import HtmlContent from "../../../shared/components/HtmlContent";
import {
    applyThemeClasses,
    DEFAULT_THEME,
    normalizeThemeId
} from "../../../shared/config/themes";
import { getConciseTime } from "../../../shared/lib/utils";
import type { Locale } from "../../../shared/types";
import { toTauriLocalImageSrc } from "../../../shared/lib/localImageSrc";
import { getRichTextSnapshotDataUrl } from "../../../shared/lib/richTextSnapshot";

type PreviewPayload = {
    contentType: string;
    content: string;
    preview?: string;
    htmlContent?: string;
    sourceApp?: string;
    timestamp?: number;
    language?: Locale;
    theme?: string;
    colorMode?: "light" | "dark";
    richTextSnapshotPreview?: boolean;
    clipboardItemFontSize?: number;
    clipboardTagFontSize?: number;
};

const RICH_IMAGE_FALLBACK_PREFIX = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX = "-->";
const TABULAR_RICH_HTML_RE = /<(table|tr|td|th|thead|tbody|tfoot|colgroup|col)\b/i;
const SPREADSHEET_SOURCE_RE = /\b(excel|et|wps|sheet|spreadsheet|calc)\b/i;

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

const isAnimatedGifSrc = (src?: string | null): boolean => {
    const value = (src || "").trim().toLowerCase();
    if (!value) return false;
    return value.startsWith("data:image/gif") || /\.gif(?:$|[?#])/i.test(value);
};

const richHtmlLooksTabular = (html?: string): boolean => {
    if (!html) return false;
    return TABULAR_RICH_HTML_RE.test(html);
};

const isSpreadsheetLikeSource = (sourceApp?: string): boolean => {
    const value = (sourceApp || "").trim();
    if (!value) return false;
    return SPREADSHEET_SOURCE_RE.test(value);
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

const applyTheme = (payload: PreviewPayload) => {
    const theme = normalizeThemeId(payload.theme || DEFAULT_THEME);
    const colorMode = payload.colorMode || "light";

    const root = document.documentElement;
    const body = document.body;

    applyThemeClasses(theme, root, body);

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

const CompactPreviewWindow = () => {
    const [payload, setPayload] = useState<PreviewPayload | null>(null);
    const [snapshotFailed, setSnapshotFailed] = useState(false);
    const [richImageFallbackFailed, setRichImageFallbackFailed] = useState(false);
    const richSnapshotImgRef = useRef<HTMLImageElement | null>(null);
    const richSnapshotFallbackTimerRef = useRef<number | null>(null);
    const containerRef = useRef<HTMLDivElement | null>(null);
    const metaRef = useRef<HTMLDivElement | null>(null);
    const contentRef = useRef<HTMLDivElement | null>(null);
    const dividerRef = useRef<HTMLDivElement | null>(null);
    const requestResizeRef = useRef<() => void>(() => {});
    const previewBoundsRef = useRef({ width: 560, height: 560, mediaWidth: 520, mediaHeight: 360 });
    const lastSentSizeRef = useRef<{ width: number; height: number } | null>(null);
    const richImageFallback = useMemo(() => {
        if (!payload || payload.contentType !== "rich_text" || !payload.htmlContent) return null;
        const { imagePayload } = extractRichImageFallback(payload.htmlContent);
        if (!imagePayload) return null;
        const src = resolveRichImageSrc(imagePayload);
        if (!src) return null;
        return {
            payload: imagePayload,
            src
        };
    }, [payload]);
    const richTextCleanHtml = useMemo(() => {
        if (!payload || payload.contentType !== "rich_text" || !payload.htmlContent) return "";
        const { cleanHtml } = extractRichImageFallback(payload.htmlContent);
        return cleanHtml || payload.htmlContent;
    }, [payload]);
    const richTextHasAnimatedImageFallback = useMemo(() => (
        isAnimatedGifSrc(richImageFallback?.payload || richImageFallback?.src || null)
    ), [richImageFallback]);
    const preferHtmlRichPreview = useMemo(() => {
        if (!payload || payload.contentType !== "rich_text" || !payload.htmlContent) return false;
        return (
            !richTextHasAnimatedImageFallback
            && !richHtmlLooksTabular(richTextCleanHtml)
            && !isSpreadsheetLikeSource(payload.sourceApp)
        );
    }, [payload, richTextCleanHtml, richTextHasAnimatedImageFallback]);
    const preferGeneratedRichPreview = useMemo(() => {
        if (!payload || payload.contentType !== "rich_text" || !payload.htmlContent) return false;
        return (
            !preferHtmlRichPreview
            && (
                !!payload.richTextSnapshotPreview
                || richHtmlLooksTabular(richTextCleanHtml)
                || isSpreadsheetLikeSource(payload.sourceApp)
            )
        );
    }, [payload, preferHtmlRichPreview, richTextCleanHtml]);
    const richTextSnapshotSrc = useMemo(() => {
        if (!payload || payload.contentType !== "rich_text" || !payload.htmlContent) return null;
        if (!preferGeneratedRichPreview) return null;
        if (!richTextCleanHtml) return null;
        return getRichTextSnapshotDataUrl(richTextCleanHtml, {
            width: 560,
            maxHeight: 1200
        });
    }, [payload, preferGeneratedRichPreview, richTextCleanHtml]);
    const effectiveRichTextSnapshotSrc = snapshotFailed ? null : richTextSnapshotSrc;
    const effectiveRichImageFallbackSrc = richImageFallbackFailed ? null : (richImageFallback?.src || null);
    // For tabular/spreadsheet sources, prefer the real clipboard bitmap image
    // (same image you see when pasting Excel into WeChat) over the generated SVG snapshot.
    const preferImageFallbackForTabular = (
        richHtmlLooksTabular(richTextCleanHtml) || isSpreadsheetLikeSource(payload?.sourceApp)
    ) && !!effectiveRichImageFallbackSrc;
    const richTextPreviewSrc = richTextHasAnimatedImageFallback
        ? (effectiveRichImageFallbackSrc || effectiveRichTextSnapshotSrc)
        : preferImageFallbackForTabular
            ? (effectiveRichImageFallbackSrc || effectiveRichTextSnapshotSrc)
            : (effectiveRichTextSnapshotSrc || null);
    const useSnapshotPreviewImage = !!richTextPreviewSrc && richTextPreviewSrc === effectiveRichTextSnapshotSrc;
    const useRichImageFallback = !!richTextPreviewSrc && richTextPreviewSrc === effectiveRichImageFallbackSrc;

    useEffect(() => {
        compactPreviewLog("window mounted", { label: getCurrentWindow().label });
        getCurrentWindow()
            .setAlwaysOnTop(true)
            .then(() => compactPreviewLog("setAlwaysOnTop(true) success"))
            .catch((err) => {
                console.error(err);
                compactPreviewLog("setAlwaysOnTop(true) failed", err);
            });
        (async () => {
            try {
                const monitor = await currentMonitor();
                const fallbackWidth = 1280;
                const fallbackHeight = 720;
                const monitorWidth = monitor?.size.width ?? fallbackWidth;
                const monitorHeight = monitor?.size.height ?? fallbackHeight;
                const maxWidth = Math.max(320, Math.min(560, Math.floor(monitorWidth * 0.6)));
                const maxHeight = Math.max(240, Math.min(560, Math.floor(monitorHeight * 0.6)));
                const mediaMaxWidth = Math.max(260, Math.min(520, maxWidth));
                const mediaMaxHeight = Math.max(200, Math.min(360, maxHeight - 120));
                const minWidth = Math.max(300, Math.min(380, Math.floor(maxWidth * 0.62)));
                compactPreviewLog("resolved monitor bounds", {
                    monitorPos: monitor?.position,
                    monitorSize: monitor?.size,
                    maxWidth,
                    maxHeight,
                    mediaMaxWidth,
                    mediaMaxHeight,
                    minWidth
                });

                previewBoundsRef.current = {
                    width: maxWidth,
                    height: maxHeight,
                    mediaWidth: mediaMaxWidth,
                    mediaHeight: mediaMaxHeight
                };

                const root = document.documentElement;
                root.style.setProperty("--preview-max-width", `${maxWidth}px`);
                root.style.setProperty("--preview-max-height", `${maxHeight}px`);
                root.style.setProperty("--preview-media-max-width", `${mediaMaxWidth}px`);
                root.style.setProperty("--preview-media-max-height", `${mediaMaxHeight}px`);
                root.style.setProperty("--preview-min-width", `${minWidth}px`);

                await getCurrentWindow().setSize(new LogicalSize(maxWidth, maxHeight));
                compactPreviewLog("initial size set", { width: maxWidth, height: maxHeight });
            } catch (err) {
                console.error("Failed to initialize preview bounds:", err);
                compactPreviewLog("initialize preview bounds failed", err);
            } finally {
                compactPreviewLog("trigger initial resize request");
                requestResizeRef.current?.();
            }
        })();
        compactPreviewLog("listen compact-preview-update");
        const unlisten = listen<PreviewPayload>("compact-preview-update", (event) => {
            compactPreviewLog("received compact-preview-update", {
                contentType: event.payload.contentType,
                contentLength: event.payload.content?.length ?? 0,
                previewLength: event.payload.preview?.length ?? 0,
                hasHtml: !!event.payload.htmlContent,
                sourceApp: event.payload.sourceApp
            });
            setPayload(event.payload);
            applyTheme(event.payload);
        });
        emitTo("main", "compact-preview-mounted", true)
            .then(() => compactPreviewLog("emit compact-preview-mounted"))
            .catch((err) => {
                console.error(err);
                compactPreviewLog("emit compact-preview-mounted failed", err);
            });
        return () => {
            compactPreviewLog("window unmount cleanup");
            unlisten.then((f) => f());
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
        const updateSize = () => {
            raf = window.requestAnimationFrame(() => {
                const container = containerRef.current;
                if (!container) {
                    compactPreviewLog("skip measure: container missing");
                    return;
                }

                const bounds = previewBoundsRef.current;
                const maxWidth = bounds.width;
                const maxHeight = bounds.height;
                const minWidth =
                    payload.contentType === "image" || payload.contentType === "video"
                        ? 260
                        : Math.min(bounds.width, 320);

                const measuredWidth = Math.max(container.offsetWidth, container.scrollWidth);
                const measuredHeight = Math.max(container.offsetHeight, container.scrollHeight);
                const width = Math.min(Math.max(Math.ceil(measuredWidth), minWidth), maxWidth);
                const height = Math.min(Math.max(Math.ceil(measuredHeight), 80), maxHeight);
                if (width < 40 || height < 40) {
                    compactPreviewLog("skip emit: measured size too small", {
                        measuredWidth,
                        measuredHeight,
                        width,
                        height
                    });
                    return;
                }

                const last = lastSentSizeRef.current;
                if (last && Math.abs(last.width - width) <= 1 && Math.abs(last.height - height) <= 1) {
                    return;
                }
                lastSentSizeRef.current = { width, height };
                compactPreviewLog("emit compact-preview-resize", {
                    measuredWidth,
                    measuredHeight,
                    width,
                    height,
                    minWidth,
                    maxWidth,
                    maxHeight
                });

                emitTo("main", "compact-preview-resize", { width, height }).catch((err) => {
                    console.error(err);
                    compactPreviewLog("emit compact-preview-resize failed", err);
                });
                getCurrentWindow()
                    .setSize(new LogicalSize(width, height))
                    .catch((err) => {
                        console.error(err);
                        compactPreviewLog("setSize in preview failed", err);
                    });
            });
        };

        requestResizeRef.current = updateSize;
        compactPreviewLog("resize effect start", { contentType: payload.contentType });
        updateSize();
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
        setSnapshotFailed(false);
        setRichImageFallbackFailed(false);
    }, [payload?.content, payload?.htmlContent, payload?.richTextSnapshotPreview]);

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
                setSnapshotFailed(true);
            }
        }, 700);

        return () => {
            if (richSnapshotFallbackTimerRef.current) {
                window.clearTimeout(richSnapshotFallbackTimerRef.current);
                richSnapshotFallbackTimerRef.current = null;
            }
        };
    }, [useSnapshotPreviewImage, effectiveRichTextSnapshotSrc, payload?.content, payload?.htmlContent]);

    const content = useMemo(() => {
        if (!payload) return null;
        if (payload.contentType === "image") {
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
                    style={{
                        width: "auto",
                        height: "auto",
                        borderRadius: "4px"
                    }}
                />
            );
        }
        if (payload.contentType === "video") {
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
                    style={{
                        width: "auto",
                        height: "auto",
                        borderRadius: "4px",
                        background: "#000"
                    }}
                />
            );
        }
        if (payload.contentType === "rich_text" && payload.htmlContent) {
            if (useSnapshotPreviewImage && effectiveRichTextSnapshotSrc) {
                compactPreviewLog("render rich_text as html snapshot image");
                return (
                    <img
                        ref={richSnapshotImgRef}
                        src={effectiveRichTextSnapshotSrc}
                        alt="rich text snapshot preview"
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
                            setSnapshotFailed(true);
                        }}
                        style={{
                            width: "100%",
                            maxWidth: "100%",
                            height: "auto",
                            display: "block",
                            borderRadius: "4px"
                        }}
                    />
                );
            }
            if (useRichImageFallback && effectiveRichImageFallbackSrc) {
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
                            richPreviewFailureLog("fallback image load error -> switch to html", {
                                srcLength: (effectiveRichImageFallbackSrc || "").length,
                                srcSample: (effectiveRichImageFallbackSrc || "").slice(0, 140),
                                sourceApp: payload.sourceApp || ""
                            });
                            setRichImageFallbackFailed(true);
                        }}
                        style={{
                            width: "auto",
                            height: "auto",
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
                    preview={false}
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
    }, [payload, effectiveRichImageFallbackSrc, effectiveRichTextSnapshotSrc]);

    return (
        <div className="compact-preview-root">
            <div
                ref={containerRef}
                className={`compact-popover-portal compact-preview-window theme-${normalizeThemeId(payload?.theme || DEFAULT_THEME)} ${payload?.contentType === "image" ? "compact-preview-image" : ""} ${payload?.contentType === "image" || payload?.contentType === "video" || !!richTextPreviewSrc ? "compact-preview-media" : ""} ${payload?.colorMode === "dark" ? "dark-mode" : ""}`}
                style={{
                    display: "flex",
                    flexDirection: "column"
                }}
            >
                <div ref={metaRef} className="popover-meta">
                    <div className="meta-row">
                        {getIcon(payload?.contentType || "text")}
                        <span>{payload?.contentType || "text"}</span>
                    </div>
                    <div className="meta-dot">•</div>
                    <div className="meta-row">
                        <AppWindow size={14} />
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
                <div ref={contentRef} className="popover-content">{content}</div>
            </div>
        </div>
    );
};

export default CompactPreviewWindow;
