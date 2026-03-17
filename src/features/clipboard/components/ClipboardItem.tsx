import { useRef, useEffect, useState, useMemo, memo } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { currentMonitor, getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import {
    Pin,
    PinOff,
    Eye,
    EyeOff,
    ExternalLink,
    Tag,
    X,
    FileText,
    Image as ImageIcon,
    Link as LinkIcon,
    Code,
    File,
    Plus,
    Video,
    Sparkles,
    Loader2,
    FileArchive,
    Music,
    FileCode,
    Cpu,
    Files,
    ImageOff,
    FileQuestion,
    GripVertical
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import type { ClipboardItemProps } from "../types";
import { getConciseTime, getTagColor } from "../../../shared/lib/utils";
import HtmlContent from "../../../shared/components/HtmlContent";
import { toTauriLocalImageSrc } from "../../../shared/lib/localImageSrc";
import { getRichTextSnapshotDataUrl } from "../../../shared/lib/richTextSnapshot";
import { getSourceAppIcon, peekSourceAppIcon } from "../../../shared/lib/sourceAppIcon";

const COMPACT_PREVIEW_LABEL = "compact-preview";
const RICH_IMAGE_FALLBACK_PREFIX = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX = "-->";
const COMPACT_PREVIEW_DEBUG = false;
const RICH_PREVIEW_DEBUG = import.meta.env.DEV;
const compactPreviewLog = (...args: unknown[]) => {
    if (!COMPACT_PREVIEW_DEBUG) return;
    const ts = new Date().toISOString();
    console.log(`[CompactPreview][Main][${ts}]`, ...args);
};
const richPreviewFailureLog = (stage: string, detail?: Record<string, unknown>) => {
    if (!RICH_PREVIEW_DEBUG) return;
    console.warn("[RichTextPreview][MainList]", stage, detail || {});
};
type CompactPreviewAnchor = {
    clientX: number;
    clientY: number;
    screenX: number;
    screenY: number;
};

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

const resolveRichImageSrc = (payload?: string): string | null => {
    if (!payload) return null;
    const value = payload.trim();
    if (!value) return null;
    if (value.startsWith("data:image/")) return value;
    if (/^https?:\/\/asset\.localhost\//i.test(value)) return value;
    return toTauriLocalImageSrc(value);
};

let compactPreviewWindow: WebviewWindow | null = null;
let compactPreviewCreating = false;
let compactPreviewReady: Promise<WebviewWindow | null> | null = null;
let compactPreviewMounted = false;
let compactPreviewMountedPromise: Promise<boolean> | null = null;
let compactPreviewResizeListener: Promise<() => void> | null = null;
let compactPreviewPendingShow = false;
let compactPreviewPendingAnchor: CompactPreviewAnchor | null = null;
let compactPreviewPendingTimer: ReturnType<typeof setTimeout> | null = null;
let compactPreviewLifecycleListenersReady: Promise<void> | null = null;

const loadWebviewWindowModule = async () => import("@tauri-apps/api/webviewWindow");

const setIgnoreBlurSafe = (ignore: boolean) => {
    compactPreviewLog("set_ignore_blur", { ignore });
    invoke("set_ignore_blur", { ignore }).catch(() => {});
};

const clearCompactPreviewPendingState = () => {
    compactPreviewLog("clear pending state");
    if (compactPreviewPendingTimer) {
        clearTimeout(compactPreviewPendingTimer);
        compactPreviewPendingTimer = null;
    }
    compactPreviewPendingShow = false;
    compactPreviewPendingAnchor = null;
};

const resolveAnchorPhysical = async (
    anchor: CompactPreviewAnchor,
    scale: number
): Promise<{ x: number; y: number }> => {
    try {
        const appWindow = getCurrentWindow();
        const outer = await appWindow.outerPosition();
        return {
            x: Math.round(outer.x + anchor.clientX * scale),
            y: Math.round(outer.y + anchor.clientY * scale)
        };
    } catch {
        return {
            x: Math.round(anchor.screenX * scale),
            y: Math.round(anchor.screenY * scale)
        };
    }
};

const pickPreviewPosition = (
    anchorX: number,
    anchorY: number,
    widthPx: number,
    heightPx: number,
    monitorPos: { x: number; y: number },
    monitorSize: { width: number; height: number },
    margin: number,
    offset: number,
    avoidRect?: { left: number; top: number; right: number; bottom: number } | null
) => {
    const left = monitorPos.x + margin;
    const top = monitorPos.y + margin;
    const right = monitorPos.x + monitorSize.width - margin;
    const bottom = monitorPos.y + monitorSize.height - margin;

    const clampPoint = (p: { x: number; y: number }) => ({
        x: Math.min(Math.max(p.x, left), right - widthPx),
        y: Math.min(Math.max(p.y, top), bottom - heightPx)
    });

    const intersectsAvoidRect = (p: { x: number; y: number }) => {
        if (!avoidRect) return false;
        const previewRect = {
            left: p.x,
            top: p.y,
            right: p.x + widthPx,
            bottom: p.y + heightPx
        };
        return !(
            previewRect.right <= avoidRect.left ||
            previewRect.left >= avoidRect.right ||
            previewRect.bottom <= avoidRect.top ||
            previewRect.top >= avoidRect.bottom
        );
    };

    const candidates = [
        { x: anchorX + offset, y: anchorY + offset }, // right-bottom
        { x: anchorX + offset, y: anchorY - heightPx - offset }, // right-top
        { x: anchorX - widthPx - offset, y: anchorY + offset }, // left-bottom
        { x: anchorX - widthPx - offset, y: anchorY - heightPx - offset } // left-top
    ];

    const fits = (p: { x: number; y: number }) =>
        p.x >= left && p.y >= top && p.x + widthPx <= right && p.y + heightPx <= bottom;

    for (const c of candidates) {
        if (fits(c) && !intersectsAvoidRect(c)) return c;
    }

    if (avoidRect) {
        const outsideCandidates = [
            { x: avoidRect.right + offset, y: anchorY - Math.round(heightPx * 0.25) }, // right of main
            { x: avoidRect.left - widthPx - offset, y: anchorY - Math.round(heightPx * 0.25) }, // left of main
            { x: anchorX - Math.round(widthPx * 0.2), y: avoidRect.top - heightPx - offset }, // above main
            { x: anchorX - Math.round(widthPx * 0.2), y: avoidRect.bottom + offset } // below main
        ].map(clampPoint);

        for (const c of outsideCandidates) {
            if (!intersectsAvoidRect(c)) return c;
        }
    }

    for (const c of candidates) {
        const clamped = clampPoint(c);
        if (!intersectsAvoidRect(clamped)) return clamped;
    }

    // Final fallback: clamp the default candidate into monitor bounds.
    return clampPoint(candidates[0]);
};

const placeAndShowPendingCompactPreview = async (
    widthLogical: number,
    heightLogical: number,
    options?: { keepPending?: boolean }
) => {
    if (!compactPreviewPendingShow || !compactPreviewWindow || !compactPreviewPendingAnchor) {
        compactPreviewLog("skip place/show: pending state not ready", {
            pendingShow: compactPreviewPendingShow,
            hasWindow: !!compactPreviewWindow,
            hasAnchor: !!compactPreviewPendingAnchor
        });
        return;
    }

    const appWindow = getCurrentWindow();
    const scale = await appWindow.scaleFactor();
    const monitor = await currentMonitor();
    const monitorPos = monitor?.position || { x: 0, y: 0 };
    const monitorSize = monitor?.size || { width: 1920, height: 1080 };
    const margin = Math.round(10 * scale);
    const offset = Math.round(12 * scale);

    const widthPx = Math.round(widthLogical * scale);
    const heightPx = Math.round(heightLogical * scale);
    const anchorPx = await resolveAnchorPhysical(compactPreviewPendingAnchor, scale);
    const mainOuter = await appWindow.outerPosition().catch(() => null);
    const mainSize = await appWindow.outerSize().catch(() => null);
    const avoidRect =
        mainOuter && mainSize
            ? {
                  left: mainOuter.x,
                  top: mainOuter.y,
                  right: mainOuter.x + mainSize.width,
                  bottom: mainOuter.y + mainSize.height
              }
            : null;

    const target = pickPreviewPosition(
        anchorPx.x,
        anchorPx.y,
        widthPx,
        heightPx,
        monitorPos,
        monitorSize,
        margin,
        offset,
        avoidRect
    );
    compactPreviewLog("place/show target resolved", {
        widthLogical,
        heightLogical,
        widthPx,
        heightPx,
        anchorPx,
        target,
        avoidRect,
        scale
    });

    setIgnoreBlurSafe(true);
    try {
        await compactPreviewWindow.setPosition(new PhysicalPosition(target.x, target.y));
        await compactPreviewWindow.show();
        // Force top-most z-order refresh so preview is not occluded by the main top-most window.
        try {
            await compactPreviewWindow.setAlwaysOnTop(false);
            await compactPreviewWindow.setAlwaysOnTop(true);
            compactPreviewLog("refresh always-on-top stacking done");
        } catch (stackErr) {
            compactPreviewLog("refresh always-on-top stacking failed", stackErr);
        }
        const visible = await compactPreviewWindow.isVisible().catch(() => null);
        compactPreviewLog("preview window shown", { visible, target });
    } catch (err) {
        setIgnoreBlurSafe(false);
        compactPreviewLog("preview show failed", err);
        throw err;
    }
    if (options?.keepPending) {
        compactPreviewLog("keep pending state after place/show", { widthLogical, heightLogical });
    } else {
        clearCompactPreviewPendingState();
    }
};

const hideCompactPreviewGlobal = async () => {
    const previewWindow = compactPreviewWindow;
    compactPreviewLog("hide preview requested", { hasWindow: !!previewWindow });
    clearCompactPreviewPendingState();
    setIgnoreBlurSafe(false);

    if (!previewWindow) return;

    try {
        await previewWindow.hide();
        const visible = await previewWindow.isVisible().catch(() => null);
        compactPreviewLog("preview window hidden", { visible });
    } catch (err) {
        console.error("Failed to hide compact preview window:", err);
        compactPreviewLog("hide preview failed, reset window reference", err);
        compactPreviewWindow = null;
        compactPreviewMounted = false;
        compactPreviewMountedPromise = null;
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

const waitForCompactPreviewMounted = async (): Promise<boolean> => {
    if (compactPreviewMounted) {
        compactPreviewLog("mounted already true, skip wait");
        return true;
    }
    if (!compactPreviewMountedPromise) {
        compactPreviewLog("waiting compact preview mounted event...");
        compactPreviewMountedPromise = new Promise(async (resolve) => {
            const timeout = setTimeout(() => {
                compactPreviewLog("wait compact-preview-mounted timeout");
                resolve(false);
            }, 1200);
            try {
                const unlisten = await listen("compact-preview-mounted", () => {
                    compactPreviewMounted = true;
                    clearTimeout(timeout);
                    unlisten();
                    compactPreviewLog("received compact-preview-mounted");
                    resolve(true);
                });
            } catch (err) {
                clearTimeout(timeout);
                console.error("Failed to listen for compact preview ready:", err);
                compactPreviewLog("listen compact-preview-mounted failed", err);
                resolve(false);
            }
        });
    }
    return compactPreviewMountedPromise;
};

const ensureCompactPreviewResizeListener = async (): Promise<void> => {
    if (compactPreviewResizeListener) {
        await compactPreviewResizeListener;
        return;
    }
    compactPreviewLog("register compact-preview-resize listener");
    compactPreviewResizeListener = listen<{ width: number; height: number }>(
        "compact-preview-resize",
        async (event) => {
            const { width, height } = event.payload || {};
            if (!width || !height) {
                compactPreviewLog("ignore compact-preview-resize with invalid payload", event.payload);
                return;
            }
            compactPreviewLog("received compact-preview-resize", { width, height });

            try {
                await placeAndShowPendingCompactPreview(width, height);
            } catch (err) {
                console.error("Failed to resize compact preview window:", err);
                compactPreviewLog("resize handling failed", err);
            }
        }
    );
    await compactPreviewResizeListener;
};

const ensureCompactPreviewLifecycleListeners = async (): Promise<void> => {
    if (compactPreviewLifecycleListenersReady) {
        await compactPreviewLifecycleListenersReady;
        return;
    }

    compactPreviewLifecycleListenersReady = (async () => {
        const lifecycleEvents = ["tauri://hide", "tauri://close-requested", "tauri://destroyed"];
        await Promise.all(
            lifecycleEvents.map(async (eventName) => {
                try {
                    compactPreviewLog("bind lifecycle listener", eventName);
                    await listen(eventName, () => {
                        compactPreviewLog("lifecycle event -> hide preview", eventName);
                        void hideCompactPreviewGlobal();
                    });
                } catch (err) {
                    console.error(`Failed to bind compact preview lifecycle listener: ${eventName}`, err);
                    compactPreviewLog("bind lifecycle listener failed", { eventName, err });
                }
            })
        );
    })();

    await compactPreviewLifecycleListenersReady;
};

const tryReuseExistingCompactPreviewWindow = async (): Promise<WebviewWindow | null> => {
    try {
        const { WebviewWindow } = await loadWebviewWindowModule();
        const existing = await WebviewWindow.getByLabel(COMPACT_PREVIEW_LABEL);
        if (!existing) {
            compactPreviewLog("no existing compact preview window by label");
            return null;
        }

        const visible = await existing.isVisible().catch(() => null);
        compactPreviewLog("reuse compact preview window by label", { visible });
        compactPreviewWindow = existing;
        compactPreviewMounted = true;
        compactPreviewMountedPromise = Promise.resolve(true);
        try {
            await existing.setIgnoreCursorEvents(true);
        } catch {}
        try {
            await existing.setAlwaysOnTop(true);
        } catch {}
        return existing;
    } catch (err) {
        compactPreviewLog("reuse compact preview window by label failed", err);
        return null;
    }
};

const ensureCompactPreviewWindow = async (): Promise<WebviewWindow | null> => {
    if (compactPreviewWindow) {
        compactPreviewMounted = true;
        compactPreviewMountedPromise = Promise.resolve(true);
        compactPreviewLog("reuse existing compact preview window");
        return compactPreviewWindow;
    }
    if (compactPreviewReady) return compactPreviewReady;
    if (compactPreviewCreating) return null;
    const reusedBeforeCreate = await tryReuseExistingCompactPreviewWindow();
    if (reusedBeforeCreate) {
        return reusedBeforeCreate;
    }
    compactPreviewLog("create compact preview window start");
    compactPreviewCreating = true;
    compactPreviewReady = (async () => {
        try {
            const { WebviewWindow } = await loadWebviewWindowModule();
            const previewWindow = new WebviewWindow(COMPACT_PREVIEW_LABEL, {
                url: "index.html?window=compact-preview",
                decorations: false,
                transparent: true,
                resizable: false,
                skipTaskbar: true,
                alwaysOnTop: true,
                visible: false,
                focus: false,
                focusable: false,
                shadow: false
            });

            compactPreviewMounted = false;
            compactPreviewMountedPromise = null;
            compactPreviewLog("compact preview window instance created, waiting tauri://created");

            const created = await new Promise<boolean>((resolve) => {
                const timeout = setTimeout(() => resolve(false), 1500);
                previewWindow.once("tauri://created", () => {
                    clearTimeout(timeout);
                    compactPreviewLog("compact preview tauri://created");
                    resolve(true);
                });
                previewWindow.once("tauri://error", (event) => {
                    clearTimeout(timeout);
                    compactPreviewLog("compact preview tauri://error", event.payload);
                    resolve(false);
                });
            });

            if (!created) {
                compactPreviewLog("compact preview create timeout/failure, try reuse by label");
                const reusedAfterFailedCreate = await tryReuseExistingCompactPreviewWindow();
                if (reusedAfterFailedCreate) {
                    return reusedAfterFailedCreate;
                }
                return null;
            }

            try {
                await previewWindow.setSize(new PhysicalSize(1, 1));
            } catch (err) {
                console.error("Failed to initialize compact preview size:", err);
            }

            try {
                await previewWindow.setIgnoreCursorEvents(true);
            } catch (err) {
                console.error("Failed to enable ignore cursor events:", err);
            }

            compactPreviewWindow = previewWindow;
            compactPreviewLog("compact preview window ready");
            return previewWindow;
        } catch (err) {
            console.error("Failed to create compact preview window:", err);
            compactPreviewLog("create compact preview window failed", err);
            return null;
        } finally {
            compactPreviewCreating = false;
            compactPreviewReady = null;
        }
    })();
    return compactPreviewReady;
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

const renderSourceAppIcon = (iconSrc: string | null, contentType: string, sourceApp: string) => {
    if (!iconSrc) {
        return getIcon(contentType);
    }

    return (
        <img
            src={iconSrc}
            alt={`${sourceApp} icon`}
            className="source-app-icon"
            loading="lazy"
        />
    );
};

const getFileIcon = (filePath: string) => {
    const ext = filePath.split('.').pop()?.toLowerCase();
    switch (ext) {
        case 'zip':
        case 'rar':
        case '7z':
        case 'tar':
        case 'gz':
            return <FileArchive size={20} />;
        case 'mp3':
        case 'wav':
        case 'flac':
        case 'm4a':
            return <Music size={20} />;
        case 'exe':
        case 'msi':
        case 'bat':
        case 'sh':
            return <Cpu size={20} />;
        case 'pdf':
        case 'doc':
        case 'docx':
        case 'ppt':
        case 'pptx':
        case 'xls':
        case 'xlsx':
            return <FileText size={20} />;
        case 'js':
        case 'ts':
        case 'tsx':
        case 'jsx':
        case 'py':
        case 'rs':
        case 'c':
        case 'cpp':
        case 'go':
        case 'java':
        case 'html':
        case 'css':
        case 'json':
            return <FileCode size={20} />;
        default:
            return <File size={20} />;
    }
};

const ClipboardItem = ({
    item,
    isSelected,
    windowPinned,
    isSensitiveHidden,
    isRevealed,
    isEditingTags,
    tagInput,
    theme,
    language,
    t,
    isAIProcessing,
    onSelect,
    onCopy,
    onToggleReveal,
    onOpen,
    onTogglePin,
    onDelete,
    onToggleTagEditor,
    onTagInput,
    onTagAdd,
    onTagDelete,
    onAIAction,
    onInputSubmit,
    aiEnabled,
    aiOptionsOpen,
    onAIOptionsToggle,
    tagColors,
    richTextSnapshotPreview = false,
    dragControls,
    id,
    compactMode,
    className,
    disableLayout
}: ClipboardItemProps & { compactMode?: boolean, className?: string }) => {
    const tagInputRef = useRef<HTMLInputElement>(null);
    const [localTagInput, setLocalTagInput] = useState(tagInput);
    const [localAiOptionsOpen, setLocalAiOptionsOpen] = useState(!!aiOptionsOpen);
    const [snapshotFailed, setSnapshotFailed] = useState(false);
    const [richImageFallbackFailed, setRichImageFallbackFailed] = useState(false);
    const [sourceAppIcon, setSourceAppIcon] = useState<string | null>(() => peekSourceAppIcon(item.source_app_path) ?? null);
    const isComposing = useRef(false);
    const richSnapshotImgRef = useRef<HTMLImageElement | null>(null);
    const richSnapshotFallbackTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const hoverAnchorRef = useRef<CompactPreviewAnchor | null>(null);
    const richTextFallback = item.content_type === "rich_text" && item.html_content
        ? (() => {
            const { cleanHtml, imagePayload } = extractRichImageFallback(item.html_content);
            return {
                cleanHtml: cleanHtml || item.html_content,
                imageSrc: resolveRichImageSrc(imagePayload)
            };
        })()
        : null;
    const richTextCleanHtml = richTextFallback?.cleanHtml || item.html_content || "";
    const richTextSnapshotDisplayMaxHeight = compactMode ? 40 : 64;
    const richTextSnapshotRenderMaxHeight = compactMode ? 100 : 200;
    const richTextSnapshotSrc = useMemo(() => {
        if (!richTextSnapshotPreview) return null;
        if (item.content_type !== "rich_text" || !item.html_content) return null;
        if (!richTextCleanHtml) return null;
        return getRichTextSnapshotDataUrl(richTextCleanHtml, {
            width: compactMode ? 360 : 560,
            // Keep source snapshot height bounded so list-item preview does not over-shrink text.
            maxHeight: richTextSnapshotRenderMaxHeight
        });
    }, [
        richTextSnapshotPreview,
        item.content_type,
        item.html_content,
        richTextCleanHtml,
        compactMode,
        richTextSnapshotRenderMaxHeight
    ]);
    const snapshotPreviewEnabled = !!richTextSnapshotPreview;
    const effectiveRichTextSnapshotSrc = snapshotPreviewEnabled && !snapshotFailed ? richTextSnapshotSrc : null;
    const useRichImageFallback = snapshotPreviewEnabled && !richImageFallbackFailed && !!richTextFallback?.imageSrc;
    const richTextPreviewSrc = useRichImageFallback
        ? (richTextFallback?.imageSrc || null)
        : effectiveRichTextSnapshotSrc;
    const useSnapshotPreviewImage = snapshotPreviewEnabled && !useRichImageFallback && !!effectiveRichTextSnapshotSrc;

    useEffect(() => {
        let cancelled = false;
        const sourceAppPath = item.source_app_path?.trim();
        const cachedIcon = peekSourceAppIcon(sourceAppPath);

        if (cachedIcon !== undefined) {
            setSourceAppIcon(cachedIcon ?? null);
            return () => {
                cancelled = true;
            };
        }

        setSourceAppIcon(null);
        if (!sourceAppPath) {
            return () => {
                cancelled = true;
            };
        }

        getSourceAppIcon(sourceAppPath).then((icon) => {
            if (!cancelled) {
                setSourceAppIcon(icon);
            }
        });

        return () => {
            cancelled = true;
        };
    }, [item.source_app_path]);

    const hideCompactPreview = async () => {
        await hideCompactPreviewGlobal();
    };

    const showCompactPreview = async (anchor: CompactPreviewAnchor) => {
        compactPreviewLog("show preview requested", {
            itemId: item.id,
            contentType: item.content_type,
            anchor
        });
        let previewWindow = await ensureCompactPreviewWindow();
        if (!previewWindow) {
            compactPreviewLog("show preview aborted: window unavailable");
            return;
        }
        await ensureCompactPreviewLifecycleListeners();
        await ensureCompactPreviewResizeListener();
        compactPreviewLog("preview listeners ready");
        const mounted = await waitForCompactPreviewMounted();
        compactPreviewLog("mounted state before emit", { mounted });
        if (!mounted) {
            compactPreviewLog("mounted wait returned false; continue with fallback timer");
        }

        try {
            const rootStyle = getComputedStyle(document.documentElement);
            const clipboardItemFontSizeRaw = parseInt(
                rootStyle.getPropertyValue("--clipboard-item-font-size")
            );
            const clipboardTagFontSizeRaw = parseInt(
                rootStyle.getPropertyValue("--clipboard-tag-font-size")
            );
            const clipboardItemFontSize = Number.isFinite(clipboardItemFontSizeRaw)
                ? clipboardItemFontSizeRaw
                : undefined;
            const clipboardTagFontSize = Number.isFinite(clipboardTagFontSizeRaw)
                ? clipboardTagFontSizeRaw
                : undefined;
            const colorMode = document.documentElement.classList.contains("dark-mode") ? "dark" : "light";

            compactPreviewPendingShow = true;
            compactPreviewPendingAnchor = anchor;
            compactPreviewLog("emit compact-preview-update", {
                itemId: item.id,
                contentType: item.content_type,
                hasHtml: !!item.html_content
            });
            await previewWindow.emit("compact-preview-update", {
                contentType: item.content_type,
                content: item.content,
                preview: item.preview,
                htmlContent: item.html_content,
                sourceApp: item.source_app,
                timestamp: item.timestamp,
                language,
                theme,
                colorMode,
                richTextSnapshotPreview,
                clipboardItemFontSize,
                clipboardTagFontSize
            });
            compactPreviewLog("emit compact-preview-update done");
            if (compactPreviewPendingTimer) {
                clearTimeout(compactPreviewPendingTimer);
            }
            compactPreviewPendingTimer = setTimeout(async () => {
                if (!compactPreviewPendingShow || !compactPreviewWindow || !compactPreviewPendingAnchor) {
                    compactPreviewLog("fallback timer canceled: pending state changed");
                    return;
                }
                try {
                    compactPreviewLog("fallback timer place/show with default size");
                    await placeAndShowPendingCompactPreview(320, 220, { keepPending: true });
                } catch (fallbackErr) {
                    console.error("Failed to show compact preview window (fallback):", fallbackErr);
                    compactPreviewLog("fallback place/show failed", fallbackErr);
                }
            }, 200);
        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            if (message.includes("window not found")) {
                compactPreviewLog("window not found, recreate flow");
                compactPreviewWindow = null;
                compactPreviewMounted = false;
                compactPreviewMountedPromise = null;
                previewWindow = await ensureCompactPreviewWindow();
                if (!previewWindow) return;
                try {
                    compactPreviewPendingShow = true;
                    compactPreviewPendingAnchor = anchor;
                    compactPreviewLog("emit compact-preview-update after recreate");
                    await previewWindow.emit("compact-preview-update", {
                        contentType: item.content_type,
                        content: item.content,
                        preview: item.preview,
                        htmlContent: item.html_content,
                        sourceApp: item.source_app,
                        timestamp: item.timestamp,
                        language,
                        theme,
                        richTextSnapshotPreview,
                        colorMode: document.documentElement.classList.contains("dark-mode") ? "dark" : "light"
                    });
                    compactPreviewLog("emit compact-preview-update after recreate done");
                    if (compactPreviewPendingTimer) {
                        clearTimeout(compactPreviewPendingTimer);
                    }
                    compactPreviewPendingTimer = setTimeout(async () => {
                        if (!compactPreviewPendingShow || !compactPreviewWindow || !compactPreviewPendingAnchor) {
                            compactPreviewLog("recreate fallback canceled: pending state changed");
                            return;
                        }
                        try {
                            compactPreviewLog("recreate fallback place/show with default size");
                            await placeAndShowPendingCompactPreview(320, 220, { keepPending: true });
                        } catch (fallbackErr) {
                            console.error("Failed to show compact preview window (fallback):", fallbackErr);
                            compactPreviewLog("recreate fallback failed", fallbackErr);
                        }
                    }, 200);
                } catch (retryErr) {
                    console.error("Failed to show compact preview window:", retryErr);
                    compactPreviewLog("recreate flow failed", retryErr);
                }
                return;
            }
            console.error("Failed to show compact preview window:", err);
            compactPreviewLog("show preview failed", err);
        }
    };

    // Sync local state when prop changes (e.g. when editor opens)
    useEffect(() => {
        setLocalTagInput(tagInput);
    }, [tagInput]);

    useEffect(() => {
        setLocalAiOptionsOpen(!!aiOptionsOpen);
    }, [aiOptionsOpen]);

    useEffect(() => {
        setSnapshotFailed(false);
        setRichImageFallbackFailed(false);
    }, [item.id, item.html_content, richTextSnapshotPreview, compactMode]);

    useEffect(() => {
        if (richSnapshotFallbackTimerRef.current) {
            clearTimeout(richSnapshotFallbackTimerRef.current);
            richSnapshotFallbackTimerRef.current = null;
        }
        if (!useSnapshotPreviewImage) return;

        // Safety net: some WebView failures do not reliably fire <img onError>.
        richSnapshotFallbackTimerRef.current = setTimeout(() => {
            const img = richSnapshotImgRef.current;
            if (!img || !img.complete || img.naturalWidth <= 0 || img.naturalHeight <= 0) {
                richPreviewFailureLog("snapshot image timeout -> fallback to html", {
                    itemId: item.id,
                    hasImageElement: !!img,
                    complete: img?.complete ?? false,
                    naturalWidth: img?.naturalWidth ?? 0,
                    naturalHeight: img?.naturalHeight ?? 0
                });
                setSnapshotFailed(true);
            }
        }, 700);

        return () => {
            if (richSnapshotFallbackTimerRef.current) {
                clearTimeout(richSnapshotFallbackTimerRef.current);
                richSnapshotFallbackTimerRef.current = null;
            }
        };
    }, [useSnapshotPreviewImage, effectiveRichTextSnapshotSrc, item.id]);

    const showAIOptions = localAiOptionsOpen;
    const inlineAiVariants = {
        open: { opacity: 1, height: "auto", marginTop: 8, marginBottom: 8 },
        collapsed: { opacity: 0, height: 0, marginTop: 0, marginBottom: 0 }
    };
    useEffect(() => {
        if (isEditingTags && tagInputRef.current) {
            tagInputRef.current.focus();
        }
    }, [isEditingTags]);

    useEffect(() => {
        if (!compactMode) {
            hideCompactPreview();
        }
    }, [compactMode]);

    useEffect(() => {
        return () => {
            if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);
            hoverAnchorRef.current = null;
            void hideCompactPreviewGlobal();
        };
    }, []);

    const renderFilePreview = () => {
        if (item.file_preview_exists === false) {
            return (
                <div className="file-thumbnail-card error-bg" title={t('file_deleted') || "File Deleted"}>
                    <div className="file-icon-wrapper error-icon">
                        <FileQuestion size={24} />
                    </div>
                    <div className="file-info-wrapper">
                        <div className="file-name error-text">{t('file_deleted') || "Deleted"}</div>
                        <div className="file-hint error-text">{item.content}</div>
                    </div>
                </div>
            );
        }

        const paths = item.content.split('\n').filter(p => p.trim());
        if (paths.length > 1) {
            return (
                <div className="file-thumbnail-card" title={item.content}>
                    <div className="file-icon-wrapper">
                        <Files size={24} />
                    </div>
                    <div className="file-info-wrapper">
                        <div className="file-name">{paths.length} {t('items')}</div>
                        <div className="file-hint">{paths[0].split(/[\\/]/).pop()} ...</div>
                    </div>
                </div>
            );
        }

        const filePath = paths[0];
        const fileName = filePath.split(/[\\/]/).pop();
        const dirPath = filePath.split(/[\\/]/).slice(0, -1).join('\\');

        return (
            <div className="file-thumbnail-card" title={item.content}>
                <div className="file-icon-wrapper">
                    {getFileIcon(filePath)}
                </div>
                <div className="file-info-wrapper">
                    <div className="file-name">{fileName}</div>
                    <div className="file-hint">{dirPath}</div>
                </div>
            </div>
        );
    };

    return (
        <motion.div
            id={id}
            data-test-clipboard-item
            layout={!disableLayout}
            initial={false}
            animate={{ marginBottom: 0 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.1 }}
            className={`history-item ${isSelected ? "selected" : ""} ${compactMode ? "compact" : ""} ${item.is_pinned ? "pinned" : ""} ${className || ''}`}
            onMouseDown={(e) => {
                if (!windowPinned) return;
                const target = e.target as HTMLElement;
                if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                    return;
                }
                if (target.closest('a')) {
                    return;
                }
                // Removed e.preventDefault() to allow blur of search box
            }}
            onClick={(e) => {
                const target = e.target as HTMLElement;
                if (target.closest('button') || target.closest('input') || target.closest('textarea')) {
                    return;
                }
                void hideCompactPreviewGlobal();
                // Prevent link navigation - we want to copy, not open links
                if (target.closest('a')) {
                    e.preventDefault();
                }
                onCopy(false); // Plain text by default for click
                onSelect();
            }}
            onContextMenu={(e) => {
                const target = e.target as HTMLElement;
                if (target.closest('button') || target.closest('input') || target.closest('textarea')) {
                    return;
                }
                void hideCompactPreviewGlobal();
                e.preventDefault();
                // Prevent link navigation on right-click too
                if (target.closest('a')) {
                    e.stopPropagation();
                }
                onCopy(true); // Formatted text for right-click
                onSelect();
            }}
            onMouseEnter={(e) => {
                if (!compactMode) return;
                // Don't show preview if AI options are open to avoid interference
                if (showAIOptions) return;
                compactPreviewLog("mouseenter schedule preview", { itemId: item.id });
                hoverAnchorRef.current = {
                    clientX: e.clientX,
                    clientY: e.clientY,
                    screenX: e.screenX,
                    screenY: e.screenY
                };
                const target = e.currentTarget;

                // Clear any pending hide timer
                if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);

                // Set a delay to show
                hoverTimerRef.current = setTimeout(() => {
                    // Double-check AI options are still closed before showing
                    if (showAIOptions) return;
                    if (!target.isConnected) return;
                    const anchor = hoverAnchorRef.current;
                    if (!anchor) return;
                    compactPreviewLog("mouseenter timer fired, show preview", { itemId: item.id });
                    showCompactPreview(anchor);
                }, 1000); // 1s delay
            }}
            onMouseMove={(e) => {
                if (!compactMode) return;
                hoverAnchorRef.current = {
                    clientX: e.clientX,
                    clientY: e.clientY,
                    screenX: e.screenX,
                    screenY: e.screenY
                };
            }}
            onMouseLeave={() => {
                if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);
                hoverAnchorRef.current = null;
                compactPreviewLog("mouseleave hide preview", { itemId: item.id });
                hideCompactPreview();
            }}
        >
            <div className="item-meta">
                <div className="item-meta-left">
                    {dragControls && (
                        <div
                            className="drag-handle"
                            onPointerDown={(e) => dragControls.start(e)}
                            onClick={(e) => e.stopPropagation()}
                            style={{
                                cursor: 'grab',
                                opacity: 0.5,
                                display: 'flex',
                                alignItems: 'center',
                                touchAction: 'none'
                            }}
                        >
                            <GripVertical size={14} />
                        </div>
                    )}
                    <div className="app-info">
                        {item.is_pinned && !dragControls && <Pin size={10} style={{ color: 'var(--accent-color)', marginRight: '-2px' }} />}
                        {renderSourceAppIcon(sourceAppIcon, item.content_type, item.source_app)}
                        <span>{item.source_app}</span>
                    </div>
                </div>

                <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <div className="item-actions">
                        {(item.tags?.includes('sensitive') || item.tags?.includes('密码') || item.tags?.includes('password')) && (
                            <button
                                className={`btn-icon ${isRevealed ? "active" : ""}`}
                                onClick={onToggleReveal}
                                title={isRevealed ? t('hide') : t('reveal')}
                            >
                                {isRevealed ? <EyeOff size={12} /> : <Eye size={12} />}
                            </button>
                        )}
                        <button
                            className="btn-icon"
                            onClick={onOpen}
                            title={t('open')}
                        >
                            <ExternalLink size={12} />
                        </button>
                        <button
                            className={`btn-icon ${item.is_pinned ? "active" : ""}`}
                            onClick={onTogglePin}
                            title={item.is_pinned ? t('unpin') : t('pin')}
                        >
                            {item.is_pinned ? <PinOff size={12} /> : <Pin size={12} />}
                        </button>
                        <button
                            className={`btn-icon ${item.tags && item.tags.length > 0 ? "active" : ""}`}
                            onClick={onToggleTagEditor}
                            title="Tags"
                        >
                            <Tag size={12} />
                        </button>
                        {item.content_type === 'text' && aiEnabled && (
                            <button
                                className={`btn-icon ai-btn ${isAIProcessing || showAIOptions ? 'active' : ''}`}
                                onClick={(e) => {
                                    e.stopPropagation();
                                    if (!isAIProcessing) {
                                        // Close preview window when opening AI options
                                        if (!showAIOptions) {
                                            hideCompactPreview();
                                        }
                                        setLocalAiOptionsOpen(prev => !prev);
                                        onAIOptionsToggle?.();
                                    }
                                }}
                                title={t('ai_assistant')}
                            >
                                {isAIProcessing ? <Loader2 size={12} className="animate-spin" /> : <Sparkles size={12} />}
                            </button>
                        )}
                        <button className="btn-icon" onClick={onDelete} title={t('delete')}>
                            <X size={12} />
                        </button>
                    </div>
                    <div className="app-info" style={{ opacity: 0.6, fontSize: '10px', display: 'flex', gap: '6px', alignItems: 'center' }}>
                        <span>{getConciseTime(item.timestamp, language)}</span>
                    </div>
                </div>
            </div>

            {compactMode && item.is_pinned && (
                <div className="compact-pinned-indicator" title={t('pinned')}>
                    <Pin size={10} fill="currentColor" />
                </div>
            )}
            <div className={`content-preview ${item.content_type === 'rich_text' ? 'rich-text' : ''} ${isSensitiveHidden ? 'sensitive-blur' : ''}`}>
                {item.content_type === "image" ? (
                    <div style={{ position: 'relative' }}>
                        {item.is_external && item.file_preview_exists === false ? (
                            <div className="image-preview error-placeholder" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', background: 'var(--bg-secondary)', color: 'var(--text-secondary)', height: '100px', fontSize: '12px' }}>
                                <ImageOff size={24} style={{ marginBottom: '8px', opacity: 0.5 }} />
                                <span>{t('image_deleted') || 'Image Deleted'}</span>
                            </div>
                        ) : (
                            <img
                                src={
                                    item.content.startsWith("data:")
                                        ? item.content
                                        : (
                                            toTauriLocalImageSrc(item.content) ||
                                            (item.is_external ? convertFileSrc(item.content) : item.content)
                                        )
                                }
                                alt={t('image_preview')}
                                className="image-preview"
                                loading="lazy"
                                style={isSensitiveHidden ? { filter: 'blur(8px)' } : {}}
                                onError={(e) => {
                                    // Fallback for load errors even if backend said it exists (e.g. deleted after fetch)
                                    e.currentTarget.style.display = 'none';
                                    e.currentTarget.parentElement?.classList.add('image-load-error');
                                }}
                            />
                        )}
                        {isSensitiveHidden && (
                            <div style={{ position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', fontWeight: 'bold', opacity: 0.5, fontSize: '10px' }}>
                                SENSITIVE
                            </div>
                        )}
                    </div>
                ) : item.content_type === "video" ? (
                    <div className="video-thumbnail-card">
                        <div className="video-thumbnail-wrapper">
                            <video
                                src={item.content.startsWith("data:")
                                    ? item.content
                                    : (toTauriLocalImageSrc(item.content) || item.content)}
                                preload="metadata"
                                muted
                                playsInline
                                className="video-thumbnail-element"
                                onLoadedMetadata={(e) => seekVideoPreviewFrame(e.currentTarget)}
                            />
                            <div className="video-play-overlay">
                                <Video size={16} />
                            </div>
                        </div>
                        <div className="video-info-wrapper">
                            <div className="video-name">{item.content.split(/[\\/]/).pop()}</div>
                        </div>
                    </div>
                ) : item.content_type === "file" ? (
                    renderFilePreview()
                ) : isAIProcessing ? (
                    <div className="ai-skeleton-wrapper">
                        <div className="ai-skeleton-line" style={{ width: '90%' }}></div>
                        <div className="ai-skeleton-line" style={{ width: '75%' }}></div>
                        <div className="ai-skeleton-line" style={{ width: '85%' }}></div>
                    </div>
                ) : item.isInputting ? (
                    <div className="ai-input-wrapper">
                        <input
                            autoFocus
                            className="search-input"
                            onMouseDown={() => invoke('activate_window_focus').catch(console.error)}
                            onFocus={() => invoke('activate_window_focus').catch(console.error)}
                            style={{ width: '100%', fontSize: '12px', padding: '8px', border: '1px solid var(--accent-color)' }}
                            placeholder={item.content}
                            onKeyDown={(e) => {
                                if (e.key === 'Enter') {
                                    e.preventDefault();
                                    const val = e.currentTarget.value.trim();
                                    if (onInputSubmit) {
                                        onInputSubmit(val);
                                    }
                                }
                            }}
                            onClick={(e) => e.stopPropagation()}
                        />
                        <div style={{ fontSize: '10px', opacity: 0.6, marginTop: '4px' }}>
                            {language === 'zh' ? '输入补充信息后按回车提交' : 'Press Enter to submit supplementary info'}
                        </div>
                    </div>
                ) : item.content_type === "rich_text" && item.html_content && !isSensitiveHidden ? (
                    richTextPreviewSrc ? (
                        <img
                            ref={richSnapshotImgRef}
                            src={richTextPreviewSrc}
                            alt="rich text preview"
                            onLoad={() => {
                                if (useSnapshotPreviewImage && richSnapshotFallbackTimerRef.current) {
                                    clearTimeout(richSnapshotFallbackTimerRef.current);
                                    richSnapshotFallbackTimerRef.current = null;
                                }
                            }}
                            onError={() => {
                                if (useRichImageFallback) {
                                    richPreviewFailureLog("fallback image load error -> switch to snapshot", {
                                        itemId: item.id,
                                        srcLength: (richTextPreviewSrc || "").length,
                                        srcSample: (richTextPreviewSrc || "").slice(0, 140)
                                    });
                                    setRichImageFallbackFailed(true);
                                    return;
                                }
                                if (richSnapshotFallbackTimerRef.current) {
                                    clearTimeout(richSnapshotFallbackTimerRef.current);
                                    richSnapshotFallbackTimerRef.current = null;
                                }
                                if (effectiveRichTextSnapshotSrc) {
                                    richPreviewFailureLog("snapshot image load error -> fallback to html", {
                                        itemId: item.id,
                                        srcLength: (richTextPreviewSrc || "").length,
                                        srcSample: (richTextPreviewSrc || "").slice(0, 140)
                                    });
                                    setSnapshotFailed(true);
                                }
                            }}
                            style={{
                                width: 'auto',
                                maxWidth: '100%',
                                maxHeight: `${richTextSnapshotDisplayMaxHeight}px`,
                                display: 'block',
                                marginRight: 'auto',
                                pointerEvents: 'none',
                                borderRadius: '4px',
                                maskImage: 'linear-gradient(to bottom, black 78%, transparent 100%)',
                                WebkitMaskImage: 'linear-gradient(to bottom, black 78%, transparent 100%)'
                            }}
                        />
                    ) : (
                        <HtmlContent
                            className="rich-text-preview"
                            htmlContent={richTextCleanHtml || item.html_content}
                            fallbackText={item.preview}
                            preview={true}
                            style={{
                                maxHeight: `${richTextSnapshotDisplayMaxHeight}px`,
                                overflow: 'hidden',
                                fontSize: 'var(--clipboard-item-font-size)',
                                lineHeight: '1.4',
                                position: 'relative',
                                pointerEvents: 'none', // Prevent interacting with links in the list
                                maskImage: 'linear-gradient(to bottom, black 70%, transparent 100%)',
                                WebkitMaskImage: 'linear-gradient(to bottom, black 70%, transparent 100%)'
                            }}
                        />
                    )
                ) : (
                    isSensitiveHidden
                        ? (
                            <div style={{ minHeight: '24px', opacity: 0.6, fontStyle: 'italic', display: 'flex', alignItems: 'center', gap: '8px', fontFamily: 'var(--font-mono)' }}>
                                <span style={{ letterSpacing: '1px' }}>
                                    {item.preview.substring(0, 3)}...
                                </span>
                                <span style={{ fontSize: '10px', opacity: 0.7 }}>
                                    ({item.content.length} {t('chars') || 'chars'})
                                </span>
                            </div>
                        )
                        : item.preview
                )}
            </div>

            {/* AI Options - Compact Mode: Dropdown Panel, Normal Mode: Inline */}
            <AnimatePresence>
                {showAIOptions && (
                    <motion.div
                        className={compactMode ? "ai-options-dropdown" : ""}
                        initial={compactMode ? { opacity: 0, y: -10 } : "collapsed"}
                        animate={compactMode ? { opacity: 1, y: 0 } : "open"}
                        exit={compactMode ? { opacity: 0, y: -10 } : "collapsed"}
                        variants={compactMode ? undefined : inlineAiVariants}
                        transition={compactMode ? { duration: 0.16 } : { duration: 0.18 }}
                        style={compactMode ? {
                            position: 'absolute',
                            top: '100%',
                            right: '4px',
                            zIndex: 100000,
                            marginTop: '4px',
                            background: 'var(--bg-element)',
                            border: '2px solid var(--border-dark)',
                            borderRadius: '4px',
                            boxShadow: '4px 4px 0 0 var(--shadow-color)',
                            padding: '6px',
                            minWidth: '140px',
                            maxHeight: '200px',
                            overflowY: 'auto'
                        } : { overflow: 'hidden' }}
                    >
                        <div style={compactMode ? {
                            display: 'flex',
                            flexDirection: 'column',
                            gap: '4px'
                        } : {
                            padding: '8px 10px',
                            background: 'rgba(72, 123, 219, 0.05)',
                            border: '1.5px dashed var(--accent-color)',
                            borderRadius: '4px',
                            display: 'flex',
                            flexWrap: 'wrap',
                            gap: '6px',
                            alignItems: 'center'
                        }}>
                            {['task', 'mouthpiece', 'translate'].map(actionType => (
                                <button
                                    key={actionType}
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onAIAction?.(actionType);
                                        onAIOptionsToggle?.();
                                    }}
                                    className="btn-icon"
                                    style={compactMode ? {
                                        width: '100%',
                                        fontSize: '11px',
                                        height: '32px',
                                        boxShadow: '2px 2px 0 0 var(--shadow-color)',
                                        textTransform: 'none',
                                        justifyContent: 'flex-start',
                                        paddingLeft: '10px'
                                    } : {
                                        flex: 1,
                                        minWidth: '90px',
                                        fontSize: '11px',
                                        height: '32px',
                                        padding: '0 12px',
                                        boxShadow: '2px 2px 0 0 var(--shadow-color)',
                                        textTransform: 'none',
                                        whiteSpace: 'nowrap'
                                    }}
                                >
                                    {t(`ai_${actionType}`)}
                                </button>
                            ))}
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

            {(item.tags?.length > 0 || isEditingTags) && (
                <div
                    className="item-tags-container"
                    style={{
                        marginTop: '2px',
                        display: 'flex',
                        flexWrap: 'wrap',
                        justifyContent: 'flex-end',
                        gap: '4px',
                        paddingTop: '0'
                    }}>
                    {item.tags?.map(tag => (
                        <span
                            key={tag}
                            className="tag-chip"
                            style={{
                                background: tagColors?.[tag] || getTagColor(tag, theme),
                                display: 'flex',
                                alignItems: 'center',
                                gap: '4px'
                            }}
                        >
                            {tag}
                            {isEditingTags && (
                                <button
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onTagDelete(tag);
                                    }}
                                    style={{ background: 'none', border: 'none', padding: 0, color: 'rgba(255,255,255,0.7)', cursor: 'pointer', display: 'flex' }}
                                >
                                    <X size={8} />
                                </button>
                            )}
                        </span>
                    ))}

                    {isEditingTags && (
                        <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                            <input
                                ref={tagInputRef}
                                type="text"
                                value={localTagInput}
                                onCompositionStart={() => {
                                    isComposing.current = true;
                                }}
                                onCompositionEnd={(e) => {
                                    isComposing.current = false;
                                    const val = (e.target as HTMLInputElement).value;
                                    setLocalTagInput(val);
                                    onTagInput(val);
                                }}
                                onMouseDown={() => {
                                    invoke('activate_window_focus').catch(console.error);
                                }}
                                onFocus={() => {
                                    invoke('activate_window_focus').catch(console.error);
                                }}
                                onChange={(e) => {
                                    const val = e.target.value;
                                    setLocalTagInput(val);
                                    if (!isComposing.current) {
                                        onTagInput(val);
                                    }
                                }}
                                onKeyDown={(e) => {
                                    if (e.key === 'Enter' && !isComposing.current) {
                                        onTagAdd();
                                    }
                                }}
                                className="tag-input"
                                placeholder="New tag..."
                                style={{
                                    background: 'var(--bg-input)',
                                    border: 'none',
                                    borderRadius: '0',
                                    padding: '2px 4px',
                                    fontSize: '10px',
                                    color: 'var(--text-primary)',
                                    width: '60px',
                                    outline: 'none'
                                }}
                                onClick={e => e.stopPropagation()}
                            />
                            <button
                                onClick={(e) => {
                                    e.stopPropagation();
                                    onTagAdd();
                                }}
                                className="btn-icon"
                                style={{ padding: '2px', height: '16px', width: '16px' }}
                            >
                                <Plus size={10} />
                            </button>
                        </div>
                    )}
                </div>
            )}
        </motion.div>
    );
};

export default memo(ClipboardItem, (prevProps, nextProps) => {
    return prevProps.isSelected === nextProps.isSelected &&
        prevProps.item.id === nextProps.item.id &&
        prevProps.item.content_type === nextProps.item.content_type &&
        prevProps.item.timestamp === nextProps.item.timestamp &&
        prevProps.item.content === nextProps.item.content &&
        prevProps.item.preview === nextProps.item.preview &&
        prevProps.item.html_content === nextProps.item.html_content &&
        prevProps.item.source_app === nextProps.item.source_app &&
        prevProps.item.source_app_path === nextProps.item.source_app_path &&
        prevProps.item.is_pinned === nextProps.item.is_pinned &&
        prevProps.item.is_external === nextProps.item.is_external &&
        prevProps.item.file_preview_exists === nextProps.item.file_preview_exists &&
        prevProps.item.tags === nextProps.item.tags &&
        prevProps.isRevealed === nextProps.isRevealed &&
        prevProps.isEditingTags === nextProps.isEditingTags &&
        prevProps.isAIProcessing === nextProps.isAIProcessing &&
        prevProps.aiOptionsOpen === nextProps.aiOptionsOpen &&
        prevProps.aiEnabled === nextProps.aiEnabled &&
        prevProps.richTextSnapshotPreview === nextProps.richTextSnapshotPreview &&
        prevProps.compactMode === nextProps.compactMode &&
        prevProps.theme === nextProps.theme &&
        prevProps.language === nextProps.language &&
        prevProps.tagInput === nextProps.tagInput;
});


