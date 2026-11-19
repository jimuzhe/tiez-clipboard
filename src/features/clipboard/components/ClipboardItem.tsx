import { getAppliedNativePalette } from "../../../shared/config/themes";
import { useRef, useId, useEffect, useLayoutEffect, useState, useMemo, memo } from "react";
import { createPortal } from "react-dom";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { currentMonitor, getCurrentWindow, LogicalSize, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
import { emit, listen } from "@tauri-apps/api/event";
import {
    Pin,
    PinOff,
    Eye,
    EyeOff,
    ExternalLink,
    Tag,
    StickyNote,
    X,
    Copy,
    Trash2,
    MoreHorizontal,
    Check,
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
    GripVertical,
    PictureInPicture2
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { isItemInteractionActive } from "../lib/itemInteraction";
import type { ClipboardItemProps } from "../types";
import {
    formatSensitivePreview,
    getConciseTime,
    getTagColor,
    getTagTextColor
} from "../../../shared/lib/utils";
import HtmlContent from "../../../shared/components/HtmlContent";
import { toTauriLocalImageSrc } from "../../../shared/lib/localImageSrc";
import { originalImageSrc, useImageThumbnail, useVideoThumbnail } from "../../../shared/hooks/useImageThumbnail";

// The wide two-column layout shows the entry in its detail pane, so the
// floating compact hover preview would only duplicate it.
const isSplitViewActive = () => document.body.classList.contains("split-view-active");

// Snapshots that failed or timed out once go straight to HTML rendering on later
// mounts; otherwise each remount retried, waited 700ms and flipped again.
const failedSnapshotSrcs = new Set<string>();
const rememberFailedSnapshot = (src: string) => {
    if (failedSnapshotSrcs.size >= 200) {
        const oldest = failedSnapshotSrcs.values().next().value;
        if (oldest !== undefined) failedSnapshotSrcs.delete(oldest);
    }
    failedSnapshotSrcs.add(src);
};
import { getRichTextSnapshotDataUrl } from "../../../shared/lib/richTextSnapshot";
import { getFileIcon as getSystemFileIcon, peekFileIcon } from "../../../shared/lib/fileIcon";
import { getSourceAppIcon, peekSourceAppIcon } from "../../../shared/lib/sourceAppIcon";
import { stripRichStorageMarkers } from "../../../shared/lib/repairHtmlFragment";
import { registerCompactPreviewControls } from "../lib/compactPreviewControls";
import { isAnimatedGifSrc, isSpreadsheetLikeSource, richHtmlLooksTabular } from "../lib/richPreview";
import {
    compactPreviewFallbackSize,
    compactPreviewPayloadFor,
    forgetCompactPreviewPayload,
    isCompactPreviewHoverArmed,
    noteCompactPreviewHoverPoint,
    noteCompactPreviewPointerMove,
    rememberCompactPreviewPayload,
} from "../lib/compactPreviewControls";
import { openPinStickerWindow } from "../lib/pinStickerControls";
import { activateForInput } from "../../../shared/lib/focus";
import { getNotePlaceholder, isMacPlatform, isWindowsPlatform } from "../../../shared/lib/platform";
import { setupDragDataTransfer, createDragGhostBadge, hiddenDragLabel, shouldHideAfterExternalDrop, isShortenedForList, nativeDragPayload } from "../lib/dragDrop";

const COMPACT_PREVIEW_LABEL = "compact-preview";
const COMPACT_PREVIEW_HOVER_DELAY_MS = 320;
const COMPACT_PREVIEW_READY_TIMEOUT_MS = 400;
const COMPACT_PREVIEW_FALLBACK_MS = 220;
const RICH_IMAGE_FALLBACK_PREFIX = "<!--TIEZ_RICH_IMAGE:";
const RICH_IMAGE_FALLBACK_SUFFIX = "-->";
const STANDALONE_COLOR_RE = /^(#(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})|(?:rgb|hsl)a?\(\s*[^)]+\s*\))$/i;
const COMPACT_PREVIEW_DEBUG = false;
const IS_MACOS =
    typeof navigator !== "undefined" &&
    (/Mac|iPhone|iPad|iPod/i.test(navigator.userAgent) || /Mac/i.test(navigator.platform));
const COMPACT_PREVIEW_WINDOW_SUPPORTED = true;
const COMPACT_PREVIEW_WARMUP_SUPPORTED = !IS_MACOS;
const compactPreviewLog = (...args: unknown[]) => {
    if (!COMPACT_PREVIEW_DEBUG) return;
    const ts = new Date().toISOString();
    console.log(`[CompactPreview][Main][${ts}]`, ...args);
};
const richPreviewFailureLog = (stage: string, detail?: Record<string, unknown>) => {
    console.warn("[RichTextPreview][MainList]", stage, detail || {});
};
type CompactPreviewAnchor = {
    clientX: number;
    clientY: number;
    screenX: number;
    screenY: number;
};

type CompactPreviewBounds = {
    maxWidth: number;
    maxHeight: number;
    mediaMaxWidth: number;
    mediaMaxHeight: number;
    minWidth: number;
};

type CompactPreviewResizePayload = {
    requestId: number;
    width: number;
    height: number;
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


const getStandaloneColorValue = (contentType: string, content: string): string | null => {
    if (contentType !== "text" && contentType !== "code") {
        return null;
    }

    const normalized = content.trim();
    if (!normalized || normalized.includes("\n")) {
        return null;
    }

    return STANDALONE_COLOR_RE.test(normalized) ? normalized : null;
};

let compactPreviewWindow: WebviewWindow | null = null;
let compactPreviewCreating = false;
let compactPreviewReady: Promise<WebviewWindow | null> | null = null;
let compactPreviewMounted = false;
let compactPreviewReadyListener: Promise<void> | null = null;
const compactPreviewReadyWaiters = new Set<(ready: boolean) => void>();
let compactPreviewResizeListener: Promise<() => void> | null = null;
let compactPreviewPendingShow = false;
let compactPreviewPendingAnchor: CompactPreviewAnchor | null = null;
let compactPreviewPendingRequestId: number | null = null;
let compactPreviewPendingTimer: ReturnType<typeof setTimeout> | null = null;
let compactPreviewRetryTimers: ReturnType<typeof setTimeout>[] = [];
let compactPreviewRequestSerial = 0;

const loadWebviewWindowModule = async () => import("@tauri-apps/api/webviewWindow");

const clearCompactPreviewPendingState = () => {
    compactPreviewLog("clear pending state");
    if (compactPreviewPendingTimer) {
        clearTimeout(compactPreviewPendingTimer);
        compactPreviewPendingTimer = null;
    }
    for (const timer of compactPreviewRetryTimers) clearTimeout(timer);
    compactPreviewRetryTimers = [];
    compactPreviewPendingShow = false;
    compactPreviewPendingAnchor = null;
    compactPreviewPendingRequestId = null;
    forgetCompactPreviewPayload();
};

const resolveCompactPreviewBounds = async (): Promise<CompactPreviewBounds> => {
    const appWindow = getCurrentWindow();
    const scale = await appWindow.scaleFactor().catch(() => 1);
    const monitor = await currentMonitor().catch(() => null);
    const monitorSize = monitor?.workArea?.size || monitor?.size || { width: 1280, height: 720 };
    const logicalWidth = monitorSize.width / Math.max(scale, 1);
    const logicalHeight = monitorSize.height / Math.max(scale, 1);
    const maxWidth = Math.max(320, Math.min(600, Math.floor(logicalWidth * 0.6)));
    const maxHeight = Math.max(240, Math.min(600, Math.floor(logicalHeight * 0.6)));

    return {
        maxWidth,
        maxHeight,
        mediaMaxWidth: Math.max(260, Math.min(560, maxWidth - 20)),
        mediaMaxHeight: Math.max(200, Math.min(520, maxHeight - 64)),
        minWidth: Math.max(240, Math.min(320, Math.floor(maxWidth * 0.42)))
    };
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
    requestId: number,
    widthLogical: number,
    heightLogical: number,
    _options?: { keepPending?: boolean }
) => {
    const previewWindow = compactPreviewWindow;
    const isCurrentRequest = () =>
        compactPreviewPendingShow &&
        compactPreviewWindow === previewWindow &&
        !!previewWindow &&
        !!compactPreviewPendingAnchor &&
        compactPreviewPendingRequestId === requestId;

    if (
        !isCurrentRequest() ||
        !previewWindow ||
        !compactPreviewPendingAnchor
    ) {
        compactPreviewLog("skip place/show: pending state not ready", {
            pendingShow: compactPreviewPendingShow,
            hasWindow: !!compactPreviewWindow,
            hasAnchor: !!compactPreviewPendingAnchor,
            requestId,
            pendingRequestId: compactPreviewPendingRequestId
        });
        return;
    }

    const appWindow = getCurrentWindow();
    const scale = await appWindow.scaleFactor();
    const monitor = await currentMonitor();
    const monitorPos = monitor?.workArea?.position || monitor?.position || { x: 0, y: 0 };
    const monitorSize = monitor?.workArea?.size || monitor?.size || { width: 1920, height: 1080 };
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

    if (!isCurrentRequest()) return;
    try {
        // Keep geometry changes in one owner and never let this passive window
        // become eligible for keyboard focus.
        await previewWindow.setFocusable(false).catch(() => { });
        if (!isCurrentRequest()) return;
        await previewWindow.setSize(new LogicalSize(widthLogical, heightLogical));
        if (!isCurrentRequest()) return;
        await previewWindow.setPosition(new PhysicalPosition(target.x, target.y));
        if (!isCurrentRequest()) return;
        await invoke("configure_compact_preview_window").catch(() => false);
        if (!isCurrentRequest()) return;
        await previewWindow.show();
        if (!isCurrentRequest()) {
            await previewWindow.hide().catch(() => { });
            return;
        }
        // Force top-most z-order refresh so preview is not occluded by the main top-most window.
        // macOS skips this toggle because frequent style-mask sync can cause UI stalls.
        if (!IS_MACOS) {
            try {
                await previewWindow.setAlwaysOnTop(false);
                await previewWindow.setAlwaysOnTop(true);
                compactPreviewLog("refresh always-on-top stacking done");
            } catch (stackErr) {
                compactPreviewLog("refresh always-on-top stacking failed", stackErr);
            }
        }
        const visible = await previewWindow.isVisible().catch(() => null);
        compactPreviewLog("preview window shown", { visible, target });
    } catch (err) {
        compactPreviewLog("preview show failed", err);
        throw err;
    }
    // Stop fallback timers since we successfully placed/shown the preview window
    if (compactPreviewPendingTimer) {
        clearTimeout(compactPreviewPendingTimer);
        compactPreviewPendingTimer = null;
    }
    for (const timer of compactPreviewRetryTimers) clearTimeout(timer);
    compactPreviewRetryTimers = [];
};

const hideCompactPreviewGlobal = async () => {
    const previewWindow = compactPreviewWindow;
    compactPreviewLog("hide preview requested", { hasWindow: !!previewWindow });
    clearCompactPreviewPendingState();

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
    }
};

const forceHideCompactPreviewWindow = () => {
    void hideCompactPreviewGlobal();
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

/**
 * Show the pending preview at a default size if it hasn't reported its own
 * size in time (a hidden web view may not run its measuring frame).
 */
const scheduleCompactPreviewFallback = (
    requestId: number,
    bounds: { maxWidth: number; maxHeight: number; minWidth: number }
) => {
    if (compactPreviewPendingTimer) {
        clearTimeout(compactPreviewPendingTimer);
    }
    compactPreviewPendingTimer = setTimeout(async () => {
        compactPreviewPendingTimer = null;
        if (
            !compactPreviewPendingShow ||
            !compactPreviewWindow ||
            !compactPreviewPendingAnchor ||
            compactPreviewPendingRequestId !== requestId
        ) {
            compactPreviewLog("fallback timer canceled: pending state changed");
            return;
        }
        // A preview that is still loading never got the payload; showing it
        // now would show an empty card. It is re-sent (and this timer
        // re-armed) when the preview announces itself ready.
        if (!compactPreviewMounted) {
            compactPreviewLog("fallback timer deferred: preview not mounted yet");
            return;
        }
        const { width, height } = compactPreviewFallbackSize(bounds);
        try {
            compactPreviewLog("fallback timer place/show with default size");
            await placeAndShowPendingCompactPreview(requestId, width, height, { keepPending: true });
        } catch (fallbackErr) {
            console.error("Failed to show compact preview window (fallback):", fallbackErr);
            compactPreviewLog("fallback place/show failed", fallbackErr);
        }
    }, COMPACT_PREVIEW_FALLBACK_MS);
};

type CompactPreviewSentPayload = CompactPreviewBounds & { requestId: number };

/** Give a preview window that just finished loading the payload it missed. */
const resendPendingCompactPreview = () => {
    const requestId = compactPreviewPendingRequestId;
    const payload = compactPreviewPayloadFor<CompactPreviewSentPayload>(requestId);
    if (!compactPreviewPendingShow || requestId === null || !payload) return;
    compactPreviewLog("resend compact-preview-update after ready", { requestId });
    void emit("compact-preview-update", payload).catch(() => {});
    scheduleCompactPreviewFallback(requestId, payload);
};

const ensureCompactPreviewReadyListener = async (): Promise<void> => {
    if (!compactPreviewReadyListener) {
        compactPreviewReadyListener = listen("compact-preview-ready", () => {
            compactPreviewMounted = true;
            compactPreviewLog("received compact-preview-ready");
            for (const resolve of compactPreviewReadyWaiters) resolve(true);
            compactPreviewReadyWaiters.clear();
            resendPendingCompactPreview();
        }).then(() => undefined);
    }
    await compactPreviewReadyListener;
};

const waitForCompactPreviewReady = async (previewWindow: WebviewWindow): Promise<boolean> => {
    await ensureCompactPreviewReadyListener();
    if (compactPreviewMounted) return true;

    return new Promise<boolean>((resolve) => {
        let settled = false;
        const finish = (ready: boolean) => {
            if (settled) return;
            settled = true;
            window.clearTimeout(timeout);
            compactPreviewReadyWaiters.delete(finish);
            resolve(ready);
        };
        const timeout = window.setTimeout(() => {
            compactPreviewLog("compact preview ready handshake timeout");
            finish(false);
        }, COMPACT_PREVIEW_READY_TIMEOUT_MS);

        compactPreviewReadyWaiters.add(finish);
        // Covers reused windows and hot reloads. A newly mounted preview also
        // announces readiness on its own, so the query cannot race creation.
        previewWindow.emit("compact-preview-ready-query", true).catch(() => finish(false));
    });
};

const ensureCompactPreviewResizeListener = async (): Promise<void> => {
    if (compactPreviewResizeListener) {
        await compactPreviewResizeListener;
        return;
    }
    compactPreviewLog("register compact-preview-resize listener");
    compactPreviewResizeListener = listen<CompactPreviewResizePayload>(
        "compact-preview-resize",
        async (event) => {
            const { requestId, width, height } = event.payload || {};
            if (!requestId || !width || !height || requestId !== compactPreviewPendingRequestId) {
                compactPreviewLog("ignore compact-preview-resize with invalid payload", event.payload);
                return;
            }
            compactPreviewLog("received compact-preview-resize", { requestId, width, height });

            try {
                await placeAndShowPendingCompactPreview(requestId, width, height);
            } catch (err) {
                console.error("Failed to resize compact preview window:", err);
                compactPreviewLog("resize handling failed", err);
            }
        }
    );
    await compactPreviewResizeListener;
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
        compactPreviewMounted = false;
        try {
            await existing.setIgnoreCursorEvents(true);
        } catch { }
        try {
            await existing.setFocusable(false);
        } catch { }
        try {
            await existing.setAlwaysOnTop(true);
        } catch { }
        try {
            await existing.setResizable(true);
        } catch { }
        await invoke("configure_compact_preview_window").catch(() => false);
        void waitForCompactPreviewReady(existing);
        return existing;
    } catch (err) {
        compactPreviewLog("reuse compact preview window by label failed", err);
        return null;
    }
};

const ensureCompactPreviewWindow = async (): Promise<WebviewWindow | null> => {
    if (!COMPACT_PREVIEW_WINDOW_SUPPORTED) return null;
    if (compactPreviewReady) return compactPreviewReady;
    compactPreviewCreating = true;
    compactPreviewReady = (async () => {
        try {
            await ensureCompactPreviewReadyListener();
            if (compactPreviewWindow) {
                void waitForCompactPreviewReady(compactPreviewWindow);
                compactPreviewLog("reuse existing compact preview window");
                return compactPreviewWindow;
            }

            const reusedBeforeCreate = await tryReuseExistingCompactPreviewWindow();
            if (reusedBeforeCreate) return reusedBeforeCreate;

            compactPreviewLog("create compact preview window start");
            const { WebviewWindow } = await loadWebviewWindowModule();
            const previewWindow = new WebviewWindow(COMPACT_PREVIEW_LABEL, {
                url: "index.html?window=compact-preview",
                decorations: false,
                transparent: true,
                resizable: true,
                skipTaskbar: true,
                alwaysOnTop: true,
                visible: false,
                focus: false,
                focusable: false,
                shadow: false
            });

            compactPreviewMounted = false;
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
                await invoke("disable_webview_browser_accelerators", {
                    label: COMPACT_PREVIEW_LABEL
                });
            } catch (err) {
                compactPreviewLog("disable webview browser accelerators failed", err);
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

            try {
                await previewWindow.setFocusable(false);
            } catch (err) {
                console.error("Failed to keep compact preview non-focusable:", err);
            }

            await invoke("configure_compact_preview_window").catch((err) => {
                console.error("Failed to raise compact preview window level:", err);
            });

            compactPreviewWindow = previewWindow;
            previewWindow.once("tauri://destroyed", () => {
                if (compactPreviewWindow?.label === previewWindow.label) {
                    compactPreviewWindow = null;
                    compactPreviewMounted = false;
                    clearCompactPreviewPendingState();
                }
            });
            void waitForCompactPreviewReady(previewWindow).then((ready) => {
                compactPreviewLog("compact preview readiness observed", { ready });
            });
            compactPreviewLog("compact preview window created");
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

/**
 * Pre-warm the compact preview window so it's ready before the user hovers.
 * macOS warms on pointer entry instead of app startup to avoid launch stalls.
 */
const warmupCompactPreviewWindow = () => {
    if (!COMPACT_PREVIEW_WINDOW_SUPPORTED || !COMPACT_PREVIEW_WARMUP_SUPPORTED) return;
    // Only warm up if not already created/creating
    if (compactPreviewWindow || compactPreviewCreating || compactPreviewReady) return;
    compactPreviewLog("warmup: pre-creating compact preview window");
    // Fire and forget - creates the window in the background
    ensureCompactPreviewWindow().catch((err) => {
        compactPreviewLog("warmup: failed", err);
    });
};

const isCompactPreviewWindowSupported = () => COMPACT_PREVIEW_WINDOW_SUPPORTED;
const isCompactPreviewWarmupSupported = () => COMPACT_PREVIEW_WARMUP_SUPPORTED;

registerCompactPreviewControls({
    forceHide: forceHideCompactPreviewWindow,
    warmup: warmupCompactPreviewWindow,
    supported: isCompactPreviewWindowSupported,
    warmupSupported: isCompactPreviewWarmupSupported,
});

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
            draggable={false}
        />
    );
};

const getFallbackFileIcon = (filePath: string) => {
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
    windowPinned,
    isSelected,
    isDetailFocused,
    isSensitiveHidden,
    isRevealed,
    canReveal,
    isEditingTags,
    isEditingNote,
    noteDraft,
    onNoteDraftChange,
    tagInput,
    tagSuggestions = [],
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
    onToggleNoteEditor,
    onNoteSave,
    onNoteEditCancel,
    onTagInput,
    onTagAdd,
    onTagPick,
    onTagEditCancel,
    onTagDelete,
    onAIAction,
    onInputSubmit,
    aiQuestion,
    aiPendingResult,
    onAIApply,
    onAIDiscard,
    aiEnabled,
    aiOptionsOpen,
    onAIOptionsToggle,
    tagColors,
    richTextSnapshotPreview = false,
    showSourceAppIcon = true,
    sensitiveMaskPrefixVisible = 3,
    sensitiveMaskSuffixVisible = 3,
    sensitiveMaskEmailDomain = false,
    quickPasteHint,
    dragControls,
    id,
    compactMode,
    className,
    disableLayout: _disableLayout
}: ClipboardItemProps & { compactMode?: boolean, className?: string }) => {
    const itemRef = useRef<HTMLDivElement | null>(null);
    const tagInputRef = useRef<HTMLInputElement>(null);
    const noteInputRef = useRef<HTMLTextAreaElement>(null);
    const ignoreNoteBlurRef = useRef(false);
    const [localTagInput, setLocalTagInput] = useState(tagInput);
    // The note text lives in the textarea's own state. The app-wide draft only
    // carries it across a switch to the detail pane; writing that draft back
    // into the textarea on every keystroke broke input method composition in
    // WebKit (the pinyin letters were committed as text).
    const [localNoteInput, setLocalNoteInput] = useState(item.note || "");
    const noteInput = localNoteInput;
    const noteComposingRef = useRef(false);
    const [localAiOptionsOpen, setLocalAiOptionsOpen] = useState(!!aiOptionsOpen);
    const [snapshotFailed, setSnapshotFailed] = useState(false);
    const [richImageFallbackFailed, setRichImageFallbackFailed] = useState(false);
    const [imageLoadFailed, setImageLoadFailed] = useState(false);
    const imageSrc = useImageThumbnail(
        item.content_type === "image" ? item.content : null,
        item.is_external,
        item.timestamp
    );
    const videoCoverSrc = useVideoThumbnail(
        item.content_type === "video" ? item.content : null,
        item.timestamp
    );
    const [videoCoverFailed, setVideoCoverFailed] = useState(false);
    // A hidden sensitive file or video shows a bare card instead of its own.
    const hidesFileCard = isSensitiveHidden && (item.content_type === "file" || item.content_type === "video");
    // Large GIF/WebP rows show a still first frame (see get_image_thumbnail)
    // and play only while hovered; the detail pane always plays them.
    const [playAnimation, setPlayAnimation] = useState(false);
    const isAnimatableImage =
        item.content_type === "image" && /\.(gif|webp)$/i.test(item.content.trim());
    const listImageSrc =
        playAnimation && isAnimatableImage
            ? originalImageSrc(item.content, item.is_external)
            : imageSrc;
    const [isDragging, setIsDragging] = useState(false);
    const isDraggingRef = useRef(false);
    const [isReordering, setIsReordering] = useState(false);
    const isReorderingRef = useRef(false);
    const canExportDragRef = useRef(false);
    const fullDragEntryRef = useRef<ClipboardItemProps["item"] | null>(null);
    const dragReleaseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const dragGuardOwner = `drag:${useId()}`;
    const dragGuardRef = useRef<{ owner: string; acquired: Promise<unknown> } | null>(null);
    // After a drag out (the web view's or the native one): drop the preview,
    // release the blur guard, and hide once another app took the entry.
    const finishExternalDrag = (dropEffect: DataTransfer["dropEffect"], insideWindow: boolean) => {
        canExportDragRef.current = false;
        setIsDragging(false);
        if (dragGhostFrameRef.current !== null) cancelAnimationFrame(dragGhostFrameRef.current);
        dragGhostFrameRef.current = null;
        dragGhostRef.current?.remove();
        dragGhostRef.current = null;
        const shouldHide = shouldHideAfterExternalDrop(dropEffect, insideWindow, windowPinned);
        const acquired = dragGuardRef.current;
        dragGuardRef.current = null;
        void (async () => {
            if (acquired) {
                await acquired.acquired;
                await invoke("release_blur_guard", { owner: acquired.owner });
            }
            if (shouldHide) await invoke("hide_window_cmd");
        })().catch(console.error);
        dragReleaseTimerRef.current = setTimeout(() => {
            isDraggingRef.current = false;
            pointerDownPosRef.current = null;
            dragReleaseTimerRef.current = null;
        }, 80);
    };
    const dragSequenceRef = useRef(0);
    const dragGhostRef = useRef<HTMLCanvasElement | null>(null);
    const dragGhostFrameRef = useRef<number | null>(null);
    const pointerDownPosRef = useRef<{ x: number; y: number; time: number } | null>(null);
    const lastPasteTriggeredRef = useRef(0);
    // The left button went down on this row. A release only pastes after a
    // press here: one that began on something that then went away (the
    // search box's tag list, a menu) used to paste whatever row it ended on.
    const pressedHereRef = useRef(false);
    const [sourceAppIcon, setSourceAppIcon] = useState<string | null>(() => peekSourceAppIcon(item.source_app_path) ?? null);
    const filePaths = useMemo(
        () => item.content_type === "file" ? item.content.split('\n').filter((p) => p.trim()) : [],
        [item.content, item.content_type]
    );
    const singleFilePath = filePaths.length === 1 ? filePaths[0] : null;
    const [fileIcon, setFileIcon] = useState<string | null>(() => peekFileIcon(singleFilePath) ?? null);
    const isComposing = useRef(false);
    const richSnapshotImgRef = useRef<HTMLImageElement | null>(null);
    const richSnapshotFallbackTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const hoverAnchorRef = useRef<CompactPreviewAnchor | null>(null);
    const hoverRequestIdRef = useRef(0);
    const compactPreviewBlockedRef = useRef(false);
    const moreActionHoverRef = useRef(false);
    const richTextFallback = item.content_type === "rich_text" && item.html_content
        ? (() => {
            const { cleanHtml, imagePayload } = extractRichImageFallback(item.html_content);
            return {
                cleanHtml: cleanHtml || item.html_content,
                imagePayload,
                imageSrc: resolveRichImageSrc(imagePayload)
            };
        })()
        : null;
    const pickableTagSuggestions = useMemo(() => {
        if (!isEditingTags) return [];
        const existing = new Set(item.tags || []);
        const q = localTagInput.trim().toLowerCase();
        return tagSuggestions
            .filter((tag) => !existing.has(tag))
            .filter((tag) => !q || tag.toLowerCase().includes(q))
            .slice(0, 14);
    }, [isEditingTags, item.tags, localTagInput, tagSuggestions]);

    useEffect(() => {
        const finishReorder = () => {
            if (!isReorderingRef.current) return;
            isReorderingRef.current = false;
            setIsReordering(false);
            // A release over the row can produce a click after the sort ends.
            // Keep it suppressed until that gesture's mouse events finish.
            dragReleaseTimerRef.current = setTimeout(() => {
                isDraggingRef.current = false;
                pointerDownPosRef.current = null;
                dragReleaseTimerRef.current = null;
            }, 80);
        };
        window.addEventListener("pointerup", finishReorder, true);
        window.addEventListener("pointercancel", finishReorder, true);
        return () => {
            window.removeEventListener("pointerup", finishReorder, true);
            window.removeEventListener("pointercancel", finishReorder, true);
            if (dragReleaseTimerRef.current) clearTimeout(dragReleaseTimerRef.current);
            if (dragGhostFrameRef.current !== null) cancelAnimationFrame(dragGhostFrameRef.current);
            dragGhostRef.current?.remove();
            const acquired = dragGuardRef.current;
            dragGuardRef.current = null;
            if (acquired) void acquired.acquired.finally(() => invoke("release_blur_guard", { owner: acquired.owner }).catch(console.error));
        };
    }, [dragGuardOwner]);

    const [tagSuggestIndex, setTagSuggestIndex] = useState(-1);
    const tagSuggestListRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        if (!isEditingTags) setTagSuggestIndex(-1);
    }, [isEditingTags]);

    useEffect(() => {
        setTagSuggestIndex((prev) => {
            if (pickableTagSuggestions.length === 0) return -1;
            if (prev < 0) return -1;
            return Math.min(prev, pickableTagSuggestions.length - 1);
        });
    }, [pickableTagSuggestions]);

    useLayoutEffect(() => {
        if (tagSuggestIndex < 0 || !tagSuggestListRef.current) return;
        const row = tagSuggestListRef.current.children[tagSuggestIndex] as HTMLElement | undefined;
        row?.scrollIntoView({ block: "nearest" });
    }, [tagSuggestIndex, pickableTagSuggestions]);

    useEffect(() => {
        if (!isEditingTags || !onTagEditCancel) return;

        const onDocMouseDown = (e: MouseEvent) => {
            if (e.button !== 0) return;
            const root = itemRef.current;
            if (!root) return;
            const t = e.target as HTMLElement;

            if (root.contains(t)) {
                if (t.closest(".tag-edit-anchor")) return;
                if (t.closest(".item-tags-container .tag-chip")) return;
                if (
                    t.closest("button") ||
                    t.closest("input") ||
                    t.closest("textarea") ||
                    t.closest('[role="button"]') ||
                    t.closest(".drag-handle")
                ) {
                    return;
                }
            }

            onTagEditCancel();
            e.preventDefault();
            e.stopPropagation();
        };

        document.addEventListener("mousedown", onDocMouseDown, true);
        return () => document.removeEventListener("mousedown", onDocMouseDown, true);
    }, [isEditingTags, onTagEditCancel]);

    const [isMoreMenuOpen, setIsMoreMenuOpen] = useState(false);
    const [menuPos, setMenuPos] = useState<{ top?: number; bottom?: number; right: number; placement: 'top' | 'bottom' } | null>(null);
    const moreButtonRef = useRef<HTMLButtonElement | null>(null);
    const moreMenuRef = useRef<HTMLDivElement | null>(null);

    const updateMoreMenuPosition = () => {
        if (!moreButtonRef.current) return;
        const rect = moreButtonRef.current.getBoundingClientRect();
        const spaceBelow = window.innerHeight - rect.bottom;
        const openUpwards = spaceBelow < 240 && rect.top > spaceBelow;
        setMenuPos({
            top: openUpwards ? undefined : Math.round(rect.bottom + 4),
            bottom: openUpwards ? Math.round(window.innerHeight - rect.top + 4) : undefined,
            right: Math.max(8, Math.round(window.innerWidth - rect.right)),
            placement: openUpwards ? 'top' : 'bottom',
        });
    };

    useEffect(() => {
        if (!isMoreMenuOpen) return;
        const handleClickOutside = (e: MouseEvent) => {
            const target = e.target as HTMLElement;
            if (moreMenuRef.current?.contains(target) || moreButtonRef.current?.contains(target)) {
                return;
            }
            setIsMoreMenuOpen(false);
        };
        const handleKeyDown = (e: KeyboardEvent) => {
            if (e.key === 'Escape') {
                setIsMoreMenuOpen(false);
                moreButtonRef.current?.focus();
            }
        };
        const handleScrollOrResize = () => {
            setIsMoreMenuOpen(false);
        };
        window.addEventListener('mousedown', handleClickOutside, true);
        window.addEventListener('keydown', handleKeyDown, true);
        window.addEventListener('scroll', handleScrollOrResize, true);
        window.addEventListener('resize', handleScrollOrResize);
        return () => {
            window.removeEventListener('mousedown', handleClickOutside, true);
            window.removeEventListener('keydown', handleKeyDown, true);
            window.removeEventListener('scroll', handleScrollOrResize, true);
            window.removeEventListener('resize', handleScrollOrResize);
        };
    }, [isMoreMenuOpen]);

    useEffect(() => {
        if ((!compactMode || isEditingTags || isEditingNote) && isMoreMenuOpen) {
            setIsMoreMenuOpen(false);
        }
    }, [compactMode, isEditingTags, isEditingNote, isMoreMenuOpen]);

    const sensitivePreview = useMemo(
        () => formatSensitivePreview(item.content, item.content_type, {
            prefixVisible: sensitiveMaskPrefixVisible,
            suffixVisible: sensitiveMaskSuffixVisible,
            maskEmailDomain: sensitiveMaskEmailDomain,
        }),
        [
            item.content,
            item.content_type,
            sensitiveMaskPrefixVisible,
            sensitiveMaskSuffixVisible,
            sensitiveMaskEmailDomain
        ]
    );
    const richTextCleanHtml = stripRichStorageMarkers(richTextFallback?.cleanHtml || item.html_content || "");
    const richTextSnapshotRenderMaxHeight = compactMode ? 100 : 200;
    const spreadsheetLikeRichSource = item.content_type === "rich_text"
        && !!item.html_content
        && isSpreadsheetLikeSource(item.source_app, item.source_app_path);
    const tabularRichHtml = richHtmlLooksTabular(richTextCleanHtml);
    const useSpreadsheetPreviewHeuristics = spreadsheetLikeRichSource && tabularRichHtml;
    const richTextHasAnimatedImageFallback = isAnimatedGifSrc(
        richTextFallback?.imagePayload || richTextFallback?.imageSrc || null
    );
    const preferHtmlRichPreview = item.content_type === "rich_text"
        && !!item.html_content
        && !richTextSnapshotPreview
        && !richTextHasAnimatedImageFallback
        && !tabularRichHtml
        && !useSpreadsheetPreviewHeuristics;
    const preferGeneratedRichPreview = item.content_type === "rich_text"
        && !!item.html_content
        && !preferHtmlRichPreview
        && (
            !!richTextSnapshotPreview
            || tabularRichHtml
            || useSpreadsheetPreviewHeuristics
        );
    const richTextSnapshotSrc = useMemo(() => {
        if (!preferGeneratedRichPreview) return null;
        if (item.content_type !== "rich_text" || !item.html_content) return null;
        if (!richTextCleanHtml) return null;
        return getRichTextSnapshotDataUrl(richTextCleanHtml, {
            // Render close to the list's actual reading width. The viewport clips
            // vertically instead of scaling a whole document down to thumbnail size.
            width: compactMode ? 320 : 420,
            maxHeight: richTextSnapshotRenderMaxHeight
        });
    }, [
        preferGeneratedRichPreview,
        item.content_type,
        item.html_content,
        richTextCleanHtml,
        compactMode,
        richTextSnapshotRenderMaxHeight
    ]);
    const effectiveRichTextSnapshotSrc = !snapshotFailed && !(richTextSnapshotSrc && failedSnapshotSrcs.has(richTextSnapshotSrc))
        ? richTextSnapshotSrc
        : null;
    const effectiveRichImageFallbackSrc = !richImageFallbackFailed
        ? (richTextFallback?.imageSrc || null)
        : null;
    const preferImageFallbackForTabular = useSpreadsheetPreviewHeuristics
        && !!effectiveRichImageFallbackSrc;
    const richTextPreviewSrc = richTextHasAnimatedImageFallback
        ? (effectiveRichImageFallbackSrc || effectiveRichTextSnapshotSrc)
        : preferImageFallbackForTabular
            ? (effectiveRichImageFallbackSrc || effectiveRichTextSnapshotSrc)
            : richTextSnapshotPreview || !item.content.trim()
                ? (effectiveRichTextSnapshotSrc || effectiveRichImageFallbackSrc)
                // Text reads better as text than as the source app's bitmap
                // shrunk to a row.
                : effectiveRichTextSnapshotSrc;
    const useSnapshotPreviewImage = !!richTextPreviewSrc && richTextPreviewSrc === effectiveRichTextSnapshotSrc;
    const useRichImageFallback = !!richTextPreviewSrc && richTextPreviewSrc === effectiveRichImageFallbackSrc;
    const visibleTagCount = item.tags?.length || 0;
    const hasTagsSection = visibleTagCount > 0 || isEditingTags;
    const hasNoteSection = isEditingNote || (!!item.note && !compactMode);
    const hasMetaFooter = hasTagsSection || hasNoteSection;
    // Keep ordinary tags overlaid on the bottom of the preview, but keep notes
    // and editing controls in flow so they never cover user content.
    const overlayMetaInPreview =
        !compactMode && !isEditingTags && !isEditingNote && visibleTagCount > 0 && !item.note;
    const showFlowMetaFooter = !compactMode && hasMetaFooter && !overlayMetaInPreview;
    const standaloneColorValue = useMemo(
        () => getStandaloneColorValue(item.content_type, item.content),
        [item.content, item.content_type]
    );

    useEffect(() => {
        let cancelled = false;
        const sourceAppPath = item.source_app_path?.trim();
        const cachedIcon = peekSourceAppIcon(sourceAppPath);

        if (!showSourceAppIcon) {
            setSourceAppIcon(null);
            return () => {
                cancelled = true;
            };
        }

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
    }, [item.source_app_path, showSourceAppIcon]);

    useEffect(() => {
        let cancelled = false;
        const cachedIcon = peekFileIcon(singleFilePath);

        if (item.content_type !== "file" || item.file_preview_exists === false || !singleFilePath) {
            setFileIcon(null);
            return () => {
                cancelled = true;
            };
        }

        if (cachedIcon !== undefined) {
            setFileIcon(cachedIcon ?? null);
            return () => {
                cancelled = true;
            };
        }

        setFileIcon(null);
        getSystemFileIcon(singleFilePath).then((icon) => {
            if (!cancelled) {
                setFileIcon(icon);
            }
        });

        return () => {
            cancelled = true;
        };
    }, [item.content_type, item.file_preview_exists, singleFilePath]);

    const compactPreviewEnabled =
        compactMode &&
        COMPACT_PREVIEW_WINDOW_SUPPORTED &&
        !isSensitiveHidden;

    const isHoverPreviewRequestCurrent = (requestId: number) => {
        if (compactPreviewBlockedRef.current) return false;
        const node = itemRef.current;
        return (
            hoverRequestIdRef.current === requestId &&
            !!hoverAnchorRef.current &&
            !!node &&
            node.isConnected &&
            node.matches(":hover")
        );
    };

    const cancelHoverPreview = () => {
        hoverRequestIdRef.current = ++compactPreviewRequestSerial;
        if (hoverTimerRef.current) {
            clearTimeout(hoverTimerRef.current);
            hoverTimerRef.current = null;
        }
        hoverAnchorRef.current = null;
    };

    const hideCompactPreview = async () => {
        cancelHoverPreview();
        await hideCompactPreviewGlobal();
    };

    const showCompactPreview = async (anchor: CompactPreviewAnchor, requestId: number) => {
        if (!compactPreviewEnabled || isSplitViewActive()) return;
        if (compactPreviewBlockedRef.current) {
            compactPreviewLog("show preview aborted: blocked by menu/editor", {
                itemId: item.id,
                requestId
            });
            return;
        }
        if (!isHoverPreviewRequestCurrent(requestId)) {
            compactPreviewLog("show preview aborted: stale hover request before start", {
                itemId: item.id,
                requestId
            });
            return;
        }
        compactPreviewLog("show preview requested", {
            itemId: item.id,
            contentType: item.content_type,
            anchor
        });
        let previewWindow = await ensureCompactPreviewWindow();
        if (!isHoverPreviewRequestCurrent(requestId)) {
            compactPreviewLog("show preview aborted: stale hover request after ensure window", {
                itemId: item.id,
                requestId
            });
            return;
        }
        if (!previewWindow) {
            compactPreviewLog("show preview aborted: window unavailable");
            return;
        }
        await ensureCompactPreviewResizeListener();
        if (!isHoverPreviewRequestCurrent(requestId)) {
            compactPreviewLog("show preview aborted: stale hover request after resize listener", {
                itemId: item.id,
                requestId
            });
            return;
        }
        compactPreviewLog("preview listeners ready");

        const emitPreview = async (targetWindow: WebviewWindow) => {
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
            const bounds = await resolveCompactPreviewBounds();

            if (!isHoverPreviewRequestCurrent(requestId)) {
                compactPreviewLog("show preview aborted: stale hover request before emit", {
                    itemId: item.id,
                    requestId
                });
                return;
            }
            compactPreviewPendingShow = true;
            compactPreviewPendingAnchor = anchor;
            compactPreviewPendingRequestId = requestId;
            compactPreviewLog("emit compact-preview-update", {
                itemId: item.id,
                requestId,
                contentType: item.content_type,
                hasHtml: !!item.html_content
            });
            const previewPayload = {
                requestId,
                contentType: item.content_type,
                content: item.content,
                preview: item.preview,
                htmlContent: item.html_content,
                filePreviewExists: item.file_preview_exists,
                sourceApp: item.source_app,
                sourceAppPath: item.source_app_path,
                sourceAppIcon: sourceAppIcon,
                timestamp: item.created_at || item.timestamp,
                language,
                theme,
                colorMode,
                nativePalette: getAppliedNativePalette(document.documentElement),
                richTextSnapshotPreview,
                clipboardItemFontSize,
                clipboardTagFontSize,
                ...bounds
            };
            rememberCompactPreviewPayload(requestId, previewPayload);
            await Promise.allSettled([
                targetWindow.emit("compact-preview-update", previewPayload),
                emit("compact-preview-update", previewPayload)
            ]);
            for (const delay of [60, 150]) {
                const retryTimer = setTimeout(() => {
                    if (
                        compactPreviewPendingShow &&
                        compactPreviewPendingRequestId === requestId &&
                        compactPreviewWindow === targetWindow
                    ) {
                        void targetWindow.emit("compact-preview-update", previewPayload);
                        void emit("compact-preview-update", previewPayload);
                    }
                }, delay);
                compactPreviewRetryTimers.push(retryTimer);
            }
            compactPreviewLog("emit compact-preview-update done");
            scheduleCompactPreviewFallback(requestId, bounds);
        };

        try {
            await emitPreview(previewWindow);
        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            if (message.includes("window not found")) {
                compactPreviewLog("window not found, recreate flow");
                compactPreviewWindow = null;
                compactPreviewMounted = false;
                previewWindow = await ensureCompactPreviewWindow();
                if (!isHoverPreviewRequestCurrent(requestId)) {
                    compactPreviewLog("show preview aborted: stale hover request after recreate", {
                        itemId: item.id,
                        requestId
                    });
                    return;
                }
                if (!previewWindow) return;
                try {
                    await emitPreview(previewWindow);
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
        setImageLoadFailed(false);
    }, [item.id, item.html_content, richTextSnapshotPreview, compactMode, item.content]);

    useEffect(() => {
        if (richSnapshotFallbackTimerRef.current) {
            clearTimeout(richSnapshotFallbackTimerRef.current);
            richSnapshotFallbackTimerRef.current = null;
        }
        if (!useSnapshotPreviewImage) return;

        // Safety net: some WebView failures do not reliably fire the image error event.
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
                if (effectiveRichTextSnapshotSrc) rememberFailedSnapshot(effectiveRichTextSnapshotSrc);
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
    const isInteractionActive = isItemInteractionActive(isSelected, isEditingTags, isEditingNote, showAIOptions) || isMoreMenuOpen;
    // Keep a live gate for async hover timers / in-flight show — stale closures
    // would otherwise re-open the preview while ··· / AI / editors are active.
    compactPreviewBlockedRef.current =
        isMoreMenuOpen ||
        moreActionHoverRef.current ||
        showAIOptions ||
        isEditingNote ||
        isEditingTags;
    const inlineAiVariants = {
        open: { opacity: 1, height: "auto", marginTop: 8, marginBottom: 8 },
        collapsed: { opacity: 0, height: 0, marginTop: 0, marginBottom: 0 }
    };
    useEffect(() => {
        if (isEditingTags && tagInputRef.current) {
            activateForInput()
                .catch(console.error)
                .finally(() => {
                    tagInputRef.current?.focus();
                });
        }
    }, [isEditingTags]);

    useEffect(() => {
        if (!isEditingNote) {
            ignoreNoteBlurRef.current = false;
            return;
        }
        // Opening the editor remounts the footer and may briefly steal focus on
        // the macOS NSPanel — ignore that spurious blur so the editor stays open.
        ignoreNoteBlurRef.current = true;
        setLocalNoteInput(noteDraft ?? item.note ?? "");
        const focusTimer = window.setTimeout(() => {
            activateForInput()
                .catch(console.error)
                .finally(() => {
                    noteInputRef.current?.focus();
                    const el = noteInputRef.current;
                    if (el) {
                        const len = el.value.length;
                        el.setSelectionRange(len, len);
                    }
                });
            window.setTimeout(() => {
                ignoreNoteBlurRef.current = false;
            }, 200);
        }, 0);
        return () => {
            window.clearTimeout(focusTimer);
        };
    }, [isEditingNote, item.note]);

    useEffect(() => {
        if (!compactPreviewEnabled) {
            void hideCompactPreview();
        }
    }, [compactPreviewEnabled]);

    // ··· / AI / editors own the row — never leave a hover preview on top.
    useEffect(() => {
        if (!compactPreviewBlockedRef.current) return;
        void hideCompactPreview();
    }, [isMoreMenuOpen, showAIOptions, isEditingNote, isEditingTags]);

    useEffect(() => {
        return () => {
            const ownedRequestId = hoverRequestIdRef.current;
            const ownedPending =
                ownedRequestId != null &&
                compactPreviewPendingRequestId === ownedRequestId;
            cancelHoverPreview();
            // Virtuoso recycles rows constantly — only hide the shared preview
            // when this row still owns the in-flight / visible request.
            if (ownedPending) {
                void hideCompactPreviewGlobal();
            }
        };
    }, []);

    const renderFilePreview = () => {
        if (item.file_preview_exists === false) {
            return (
                <div className="file-thumbnail-card error-bg" title={t('file_deleted')}>
                    <div className="file-icon-wrapper error-icon">
                        <FileQuestion size={24} />
                    </div>
                    <div className="file-info-wrapper">
                        <div className="file-name error-text">{t('file_deleted')}</div>
                        <div className="file-hint error-text">{item.content}</div>
                    </div>
                </div>
            );
        }

        if (filePaths.length > 1) {
            return (
                <div className="file-thumbnail-card" title={item.content}>
                    <div className="file-icon-wrapper">
                        <Files size={24} />
                    </div>
                    <div className="file-info-wrapper">
                        <div className="file-name">{filePaths.length} {t('items')}</div>
                        <div className="file-hint">{filePaths[0].split(/[\\/]/).pop()} ...</div>
                    </div>
                </div>
            );
        }

        const filePath = filePaths[0];
        if (!filePath) {
            return (
                <div className="file-thumbnail-card" title={item.content}>
                    <div className="file-icon-wrapper">
                        <File size={24} />
                    </div>
                    <div className="file-info-wrapper">
                        <div className="file-name">{t('file')}</div>
                        <div className="file-hint">{item.content}</div>
                    </div>
                </div>
            );
        }

        const fileName = filePath.split(/[\\/]/).pop();
        // Keep the path's own separator: "/" on macOS used to show as "\\".
        const separator = filePath.includes('\\') ? '\\' : '/';
        const dirPath = filePath.split(/[\\/]/).slice(0, -1).join(separator);

        return (
            <div className="file-thumbnail-card" title={item.content}>
                <div className={`file-icon-wrapper${fileIcon ? " file-icon-wrapper-system" : ""}`}>
                    {fileIcon ? (
                        <img
                            src={fileIcon}
                            alt={`${fileName || "file"} icon`}
                            className="file-icon-image"
                            loading="lazy"
                            draggable={false}
                        />
                    ) : (
                        getFallbackFileIcon(filePath)
                    )}
                </div>
                <div className="file-info-wrapper">
                    <div className="file-name">{fileName}</div>
                    <div className="file-hint">{dirPath}</div>
                </div>
            </div>
        );
    };

    const renderTagsContainer = () => (
        <div
            className={`item-tags-container${isEditingTags ? ' tag-edit-active' : ''}`}
            onMouseDown={(e) => {
                if ((e.target as HTMLElement).closest("button, input, textarea, .tag-chip, .tag-edit-anchor")) {
                    e.stopPropagation();
                }
            }}
        >
            {item.tags?.map((tag) => {
                const tagBackground = tagColors?.[tag] || getTagColor(tag, theme);
                const tagTextColor = getTagTextColor(tagBackground, theme);
                return (
                    <span
                        key={tag}
                        className="tag-chip"
                        style={{
                            background: tagBackground,
                            color: tagTextColor,
                            '--tag-color': tagBackground,
                            '--tag-text-color': tagTextColor,
                        } as CSSProperties}
                    >
                        {tag}
                        {isEditingTags && (
                            <button
                                onClick={(e) => {
                                    e.stopPropagation();
                                    onTagDelete(tag);
                                }}
                                style={{ background: 'none', border: 'none', padding: 0, color: 'inherit', opacity: 0.72, cursor: 'pointer', display: 'flex' }}
                            >
                                <X size={8} />
                            </button>
                        )}
                    </span>
                );
            })}

            {isEditingTags && (
                <div className="tag-edit-anchor">
                    <div className="tag-edit-input-row">
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
                            onChange={(e) => {
                                const val = e.target.value;
                                setLocalTagInput(val);
                                if (!isComposing.current) {
                                    onTagInput(val);
                                }
                            }}
                            onKeyDown={(e) => {
                                if (e.key === 'Escape') {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    onTagEditCancel?.();
                                    return;
                                }
                                const suggestionCount = pickableTagSuggestions.length;
                                if (e.key === 'ArrowDown' && suggestionCount > 0 && onTagPick) {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    setTagSuggestIndex((prev) =>
                                        prev < 0 ? 0 : Math.min(prev + 1, suggestionCount - 1)
                                    );
                                    return;
                                }
                                if (e.key === 'ArrowUp' && suggestionCount > 0 && onTagPick) {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    setTagSuggestIndex((prev) => (prev <= 0 ? -1 : prev - 1));
                                    return;
                                }
                                if (e.key === 'Enter' && !isComposing.current) {
                                    e.preventDefault();
                                    e.stopPropagation();
                                    if (
                                        onTagPick &&
                                        tagSuggestIndex >= 0 &&
                                        tagSuggestIndex < suggestionCount
                                    ) {
                                        const picked = pickableTagSuggestions[tagSuggestIndex];
                                        onTagPick(picked);
                                        setLocalTagInput('');
                                        setTagSuggestIndex(-1);
                                    } else {
                                        onTagAdd();
                                    }
                                }
                            }}
                            className="tag-input"
                            aria-autocomplete="list"
                            aria-controls={
                                pickableTagSuggestions.length > 0 && onTagPick
                                    ? `tag-suggest-list-${item.id}`
                                    : undefined
                            }
                            aria-activedescendant={
                                tagSuggestIndex >= 0 && pickableTagSuggestions.length > 0 && onTagPick
                                    ? `tag-suggest-opt-${item.id}-${tagSuggestIndex}`
                                    : undefined
                            }
                            placeholder={t('enter_tag_name')}
                            style={{
                                background: 'var(--bg-input)',
                                border: 'none',
                                borderRadius: '0',
                                padding: '2px 6px',
                                fontSize: 'var(--font-size-2xs)',
                                color: 'var(--text-primary)',
                                outline: 'none'
                            }}
                            onClick={(e) => e.stopPropagation()}
                        />
                        <button
                            type="button"
                            onClick={(e) => {
                                e.stopPropagation();
                                onTagAdd();
                            }}
                            className="btn-icon"
                            style={{ padding: '2px', height: '16px', width: '16px' }}
                            title={t('add_tag')}
                        >
                            <Plus size={10} />
                        </button>
                        {compactMode && (
                            <button
                                type="button"
                                onClick={(e) => {
                                    e.stopPropagation();
                                    if (localTagInput.trim()) {
                                        onTagAdd();
                                    }
                                    onTagEditCancel?.();
                                }}
                                className="btn-icon"
                                title={t('confirm')}
                                style={{ padding: '2px', height: '16px', width: '16px' }}
                            >
                                <Check size={10} />
                            </button>
                        )}
                    </div>
                    {pickableTagSuggestions.length > 0 && onTagPick && (
                        <div
                            ref={tagSuggestListRef}
                            id={`tag-suggest-list-${item.id}`}
                            className="tag-edit-suggestions-popover hide-scrollbar"
                            role="listbox"
                            aria-label={t('find_tags')}
                            onMouseDown={(e) => e.stopPropagation()}
                        >
                            {pickableTagSuggestions.map((sTag, sIdx) => {
                                const bg = tagColors?.[sTag] || getTagColor(sTag, theme);
                                const fg = getTagTextColor(bg, theme);
                                return (
                                    <button
                                        key={sTag}
                                        type="button"
                                        role="option"
                                        id={`tag-suggest-opt-${item.id}-${sIdx}`}
                                        aria-selected={tagSuggestIndex === sIdx}
                                        className={`tag-suggest-item${tagSuggestIndex === sIdx ? ' tag-suggest-item--active' : ''}`}
                                        onMouseEnter={() => setTagSuggestIndex(sIdx)}
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            onTagPick(sTag);
                                            setLocalTagInput('');
                                            setTagSuggestIndex(-1);
                                        }}
                                    >
                                        <span
                                            className="tag-suggest-pill"
                                            style={{
                                                background: bg,
                                                color: fg
                                            }}
                                        >
                                            {sTag}
                                        </span>
                                    </button>
                                );
                            })}
                        </div>
                    )}
                </div>
            )}
        </div>
    );

    const renderNoteBlock = () => (
        <div
            className={`item-note${isEditingNote ? " item-note-editor" : ""}`}
            onMouseDown={isEditingNote ? (e) => e.stopPropagation() : undefined}
            onClick={isEditingNote ? (e) => e.stopPropagation() : undefined}
        >
            {isEditingNote ? (
                <div className="item-note-editor-inner">
                    <textarea
                        ref={noteInputRef}
                        className="item-note-input"
                        value={noteInput}
                        placeholder={compactMode ? `${t('add_note')}…` : getNotePlaceholder(t)}
                        rows={2}
                        onMouseDown={(e) => e.stopPropagation()}
                        onCompositionStart={() => {
                            noteComposingRef.current = true;
                        }}
                        onCompositionEnd={(e) => {
                            noteComposingRef.current = false;
                            const value = (e.target as HTMLTextAreaElement).value;
                            setLocalNoteInput(value);
                            onNoteDraftChange?.(value);
                        }}
                        onChange={(e) => {
                            setLocalNoteInput(e.target.value);
                            if (!noteComposingRef.current) onNoteDraftChange?.(e.target.value);
                        }}
                        onKeyDown={(e) => {
                            if (e.key === 'Escape') {
                                e.preventDefault();
                                e.stopPropagation();
                                onNoteEditCancel?.();
                                return;
                            }
                            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                                e.preventDefault();
                                e.stopPropagation();
                                onNoteSave(noteInput);
                            }
                        }}
                        onBlur={() => {
                            if (ignoreNoteBlurRef.current) return;
                            // Defer so focus moves within the note block don't save/close.
                            window.setTimeout(() => {
                                if (ignoreNoteBlurRef.current) return;
                                const active = document.activeElement;
                                if (active && noteInputRef.current?.closest(".item-note")?.contains(active)) {
                                    return;
                                }
                                onNoteSave(noteInputRef.current?.value ?? noteInput);
                            }, 0);
                        }}
                    />
                    <div className="item-note-editor-footer">
                        <span className="item-note-hint">
                            {isMacPlatform() ? t("note_editor_hint_mac") : t("note_editor_hint_win")}
                        </span>
                        <div className="item-note-actions">
                            <button
                                type="button"
                                className="btn-note-action cancel"
                                onMouseDown={(e) => {
                                    e.stopPropagation();
                                    ignoreNoteBlurRef.current = true;
                                }}
                                onClick={(e) => {
                                    e.stopPropagation();
                                    ignoreNoteBlurRef.current = false;
                                    onNoteEditCancel?.();
                                }}
                            >
                                {t('cancel')}
                            </button>
                            <button
                                type="button"
                                className="btn-note-action save"
                                onMouseDown={(e) => {
                                    e.stopPropagation();
                                    ignoreNoteBlurRef.current = true;
                                }}
                                onClick={(e) => {
                                    e.stopPropagation();
                                    ignoreNoteBlurRef.current = false;
                                    onNoteSave(noteInput);
                                }}
                            >
                                {t('save')}
                            </button>
                        </div>
                    </div>
                </div>
            ) : (
                // Plain text: a click pastes, as anywhere else on the entry.
                // The full-width note was easy to hit by mistake and opened
                // the editor; the note button in the actions edits it.
                <div className="item-note-text" title={item.note}>
                    <StickyNote size={12} className="item-note-icon" aria-hidden />
                    <span>{item.note}</span>
                </div>
            )}
        </div>
    );

    const renderMetaFooter = () => (
        <div
            className={`item-meta-footer${isEditingNote ? " editing-note" : ""}${isEditingTags ? " editing-tags" : ""}`}
            onMouseDown={(e) => {
                if ((e.target as HTMLElement).closest("button, input, textarea, .tag-chip, .tag-edit-anchor")) {
                    e.stopPropagation();
                }
            }}
        >
            {hasNoteSection && renderNoteBlock()}
            {hasTagsSection && renderTagsContainer()}
        </div>
    );

    return (
        <motion.div
            ref={itemRef}
            id={id}
            data-test-clipboard-item
            data-clipboard-item-id={String(item.id)}
            role="listitem"
            aria-current={isSelected ? "true" : undefined}
            initial={false}
            animate={{ marginBottom: 0 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.1 }}
            draggable={!isEditingTags && !isEditingNote && !isReordering}
            className={`history-item ${isInteractionActive ? "selected" : ""} ${isDetailFocused ? "detail-focused" : ""} ${compactMode ? "compact" : ""} ${item.is_pinned ? "pinned" : ""} ${isDragging ? "is-dragging" : ""} ${isEditingTags ? "editing-tags" : ""} ${isEditingNote ? "editing-note" : ""} ${className || ''}`}
            onPointerDownCapture={(e) => {
                if (dragReleaseTimerRef.current) {
                    clearTimeout(dragReleaseTimerRef.current);
                    dragReleaseTimerRef.current = null;
                    isDraggingRef.current = false;
                }
                // Native dragstart targets the draggable row, rather than the
                // handle/button where the pointer went down. Remember origin.
                const target = e.target as HTMLElement;
                canExportDragRef.current = e.button === 0 && !target.closest('button, input, textarea, [role="button"], .drag-handle');
                // A drag can only use data at hand: fetch the full entry now.
                if (canExportDragRef.current && isShortenedForList(item) && fullDragEntryRef.current?.id !== item.id) {
                    invoke<[string, string | null]>("get_clipboard_content_with_html", { id: item.id })
                        .then(([content, html]) => {
                            fullDragEntryRef.current = { ...item, content, html_content: html ?? undefined };
                        })
                        .catch(console.error);
                }
            }}
            onMouseDownCapture={(e) => {
                const target = e.target as HTMLElement;
                if (e.button !== 0) return;
                pressedHereRef.current = true;

                if (typeof document !== "undefined" && document.querySelector(".modal-overlay")) {
                    return;
                }

                if (isEditingTags) {
                    if (target.closest(".tag-edit-anchor")) return;
                    if (target.closest(".item-tags-container .tag-chip")) return;
                    if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                        return;
                    }
                    if (target.closest('a')) return;
                    e.preventDefault();
                    e.stopPropagation();
                    onTagEditCancel?.();
                    return;
                }

                if (isEditingNote) {
                    if (target.closest(".item-note-editor") || target.closest(".item-note")) {
                        return;
                    }
                    if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                        return;
                    }
                    e.preventDefault();
                    e.stopPropagation();
                    onNoteSave(noteInput);
                    return;
                }

                if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                    return;
                }
                if (target.closest('a')) {
                    return;
                }

                // Record pointer down location to distinguish click vs drag
                isDraggingRef.current = false;
                pointerDownPosRef.current = { x: e.clientX, y: e.clientY, time: Date.now() };
            }}
            onDragStartCapture={(e) => {
                if (!canExportDragRef.current || isReorderingRef.current || isEditingTags || isEditingNote) {
                    e.preventDefault();
                    e.stopPropagation();
                    return;
                }
                const target = e.target as HTMLElement;
                if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                    e.preventDefault();
                    return;
                }
                if (!e.dataTransfer) return;
                e.stopPropagation();

                isDraggingRef.current = true;
                setIsDragging(true);
                void hideCompactPreview();

                // Prevent panel from hiding when dragging across other windows
                const owner = `${dragGuardOwner}:${++dragSequenceRef.current}`;
                dragGuardRef.current = { owner, acquired: invoke("acquire_blur_guard", { owner }).catch(console.error) };

                const fullEntry = fullDragEntryRef.current?.id === item.id ? fullDragEntryRef.current : null;
                const dragged = fullEntry ?? item;
                const ghostHiddenLabel = isSensitiveHidden ? hiddenDragLabel(item, sensitivePreview, t) : undefined;

                // A system drag instead of the web view's, which hands other
                // apps no file for a file entry and dims its preview on macOS
                // (native_drag.rs for each platform).
                const mac = isMacPlatform();
                const nativePayload = mac || isWindowsPlatform() ? nativeDragPayload(dragged, filePaths) : null;
                if (nativePayload) {
                    e.preventDefault();
                    // macOS takes the preview in points (2x for Retina), Windows in pixels.
                    const scale = mac ? 2 : window.devicePixelRatio || 1;
                    const badge = createDragGhostBadge(dragged, filePaths, theme, t, scale, ghostHiddenLabel);
                    const request = {
                        ...nativePayload,
                        preview: badge.toDataURL("image/png").split(",")[1] ?? "",
                        previewWidth: parseFloat(badge.style.width),
                        previewHeight: parseFloat(badge.style.height),
                        grabX: 24,
                        grabY: 20,
                        scale,
                    };
                    void (async () => {
                        const unlisten = await listen<{ accepted: boolean; inside: boolean }>("native-drag-ended", (event) => {
                            unlisten();
                            finishExternalDrag(event.payload.accepted ? "copy" : "none", event.payload.inside);
                        });
                        try {
                            await invoke("start_native_drag", { request });
                        } catch (error) {
                            unlisten();
                            console.error(error);
                            finishExternalDrag("none", true);
                        }
                    })();
                    return;
                }

                // Setup payload for external windows (Finder, VSCode, Browser, Slack, etc.)
                setupDragDataTransfer(e.dataTransfer, dragged, filePaths);

                // Set custom drag preview badge
                try {
                    const badge = createDragGhostBadge(item, filePaths, theme, t, 1, ghostHiddenLabel);
                    badge.style.left = `${Math.max(0, e.clientX - 24)}px`;
                    badge.style.top = `${Math.max(0, e.clientY - 20)}px`;
                    dragGhostRef.current?.remove();
                    dragGhostRef.current = badge;
                    document.body.appendChild(badge);
                    e.dataTransfer.setDragImage(badge, 24, 20);
                    // WebKit can snapshot the drag image on the next frame.
                    // Keep the source alive; hide its page copy after capture.
                    dragGhostFrameRef.current = requestAnimationFrame(() => {
                        dragGhostFrameRef.current = requestAnimationFrame(() => {
                            badge.style.visibility = "hidden";
                            dragGhostFrameRef.current = null;
                        });
                    });
                } catch {}
                // Dragging doesn't touch the clipboard: a cancelled drag must
                // not replace what the user had copied.
            }}
            onDragEndCapture={(e) => {
                if (!canExportDragRef.current) return;
                e.stopPropagation();
                const insideWindow = e.clientX >= 0 && e.clientY >= 0
                    && e.clientX < window.innerWidth && e.clientY < window.innerHeight;
                finishExternalDrag(e.dataTransfer.dropEffect, insideWindow);
            }}
            onMouseUpCapture={(e) => {
                if (e.button !== 0) return;
                const pressedHere = pressedHereRef.current;
                // The click right after this release still needs it.
                setTimeout(() => {
                    pressedHereRef.current = false;
                }, 0);
                if (!pressedHere) return;
                if (isEditingTags || isEditingNote) return;
                if (isDraggingRef.current) return;

                const target = e.target as HTMLElement;
                if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                    return;
                }
                if (target.closest('a')) return;

                if (pointerDownPosRef.current) {
                    const dx = Math.abs(e.clientX - pointerDownPosRef.current.x);
                    const dy = Math.abs(e.clientY - pointerDownPosRef.current.y);
                    if (dx > 6 || dy > 6) {
                        return; // Was drag attempt
                    }
                }

                const now = Date.now();
                if (now - lastPasteTriggeredRef.current < 400) return;
                lastPasteTriggeredRef.current = now;

                e.preventDefault();
                void hideCompactPreview();
                onCopy(true);
                onSelect();
            }}
            onClick={(e) => {
                if (!pressedHereRef.current) return;
                if (isEditingTags || isEditingNote) return;
                if (isDraggingRef.current) return;
                const target = e.target as HTMLElement;
                if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                    return;
                }
                if (target.closest('a')) {
                    e.preventDefault();
                }

                if (pointerDownPosRef.current) {
                    const dx = Math.abs(e.clientX - pointerDownPosRef.current.x);
                    const dy = Math.abs(e.clientY - pointerDownPosRef.current.y);
                    if (dx > 6 || dy > 6) return;
                }

                const now = Date.now();
                if (now - lastPasteTriggeredRef.current < 400) return;
                lastPasteTriggeredRef.current = now;

                void hideCompactPreview();
                onCopy(true);
                onSelect();
            }}
            onContextMenu={(e) => {
                const target = e.target as HTMLElement;
                if (isEditingTags) {
                    if (target.closest(".tag-edit-anchor")) return;
                    e.preventDefault();
                    e.stopPropagation();
                    onTagEditCancel?.();
                    return;
                }
                if (target.closest('button, input, textarea, [role="button"], .drag-handle')) {
                    return;
                }
                void hideCompactPreview();
                e.preventDefault();
                if (target.closest('a')) {
                    e.stopPropagation();
                }
                const now = Date.now();
                lastPasteTriggeredRef.current = now;
                onCopy(false); // The secondary paste; which button keeps the format is a setting (clickPaste.ts)
                onSelect();
            }}
            onMouseEnter={(e) => {
                if (!compactPreviewEnabled || isSplitViewActive()) return;
                // Don't show preview if AI options, more menu, note editor, or tag editor are open to avoid interference
                if (compactPreviewBlockedRef.current) return;

                hoverAnchorRef.current = {
                    clientX: e.clientX,
                    clientY: e.clientY,
                    screenX: e.screenX,
                    screenY: e.screenY
                };

                // Clipboard often opens under the cursor and WebView synthesizes
                // mouseenter. Wait for a real pointer move before scheduling.
                if (!isCompactPreviewHoverArmed()) {
                    noteCompactPreviewHoverPoint(e.screenX, e.screenY);
                    compactPreviewLog("mouseenter ignored until pointer moves", {
                        itemId: item.id
                    });
                    void ensureCompactPreviewWindow();
                    return;
                }

                compactPreviewLog("mouseenter schedule preview", { itemId: item.id });
                const requestId = ++compactPreviewRequestSerial;
                hoverRequestIdRef.current = requestId;
                const target = e.currentTarget;

                // Start the non-focusable window handshake during the hover delay,
                // so the first preview is as quick as subsequent previews.
                void ensureCompactPreviewWindow();

                // Clear any pending hide timer
                if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);

                // Set a delay to show
                hoverTimerRef.current = setTimeout(() => {
                    hoverTimerRef.current = null;
                    // Double-check AI options and more menu are still closed before showing
                    if (compactPreviewBlockedRef.current) return;
                    if (!target.isConnected) return;
                    if (!isHoverPreviewRequestCurrent(requestId)) return;
                    const anchor = hoverAnchorRef.current;
                    if (!anchor) return;
                    compactPreviewLog("mouseenter timer fired, show preview", { itemId: item.id });
                    void showCompactPreview(anchor, requestId);
                }, COMPACT_PREVIEW_HOVER_DELAY_MS);
            }}
            onMouseMove={(e) => {
                if (!compactPreviewEnabled) return;
                hoverAnchorRef.current = {
                    clientX: e.clientX,
                    clientY: e.clientY,
                    screenX: e.screenX,
                    screenY: e.screenY
                };

                const justArmed = noteCompactPreviewPointerMove(e.screenX, e.screenY);
                if (!justArmed) return;
                if (compactPreviewBlockedRef.current) return;
                if (hoverTimerRef.current) return;

                // First real move after clipboard show: start the normal hover delay.
                compactPreviewLog("pointer move armed hover, schedule preview", {
                    itemId: item.id
                });
                const requestId = ++compactPreviewRequestSerial;
                hoverRequestIdRef.current = requestId;
                const target = e.currentTarget;
                void ensureCompactPreviewWindow();
                hoverTimerRef.current = setTimeout(() => {
                    hoverTimerRef.current = null;
                    if (compactPreviewBlockedRef.current) return;
                    if (!target.isConnected) return;
                    if (!isHoverPreviewRequestCurrent(requestId)) return;
                    const anchor = hoverAnchorRef.current;
                    if (!anchor) return;
                    void showCompactPreview(anchor, requestId);
                }, COMPACT_PREVIEW_HOVER_DELAY_MS);
            }}
            onMouseLeave={() => {
                compactPreviewLog("mouseleave hide preview", { itemId: item.id });
                void hideCompactPreview();
            }}
        >
            <div className="item-meta">
                <div className="item-meta-left">
                    {dragControls && (
                        <div
                            className="drag-handle"
                            title={t("reorder_pinned")}
                            draggable={false}
                            onPointerDown={(e) => {
                                if (e.button !== 0 || isEditingTags || isEditingNote) return;
                                // Suppress the browser's ancestor drag and start
                                // only the pointer gesture used for reordering.
                                e.preventDefault();
                                e.stopPropagation();
                                canExportDragRef.current = false;
                                isReorderingRef.current = true;
                                isDraggingRef.current = true;
                                pointerDownPosRef.current = null;
                                setIsReordering(true);
                                void hideCompactPreview();
                                dragControls.start(e);
                            }}
                            onClick={(e) => e.stopPropagation()}
                        >
                            <GripVertical size={14} />
                        </div>
                    )}
                    <div className="app-info">
                        {item.is_pinned && !dragControls && <Pin size={10} style={{ color: 'var(--accent-color)', marginRight: '-2px' }} />}
                        {showSourceAppIcon
                            ? renderSourceAppIcon(sourceAppIcon, item.content_type, item.source_app)
                            : getIcon(item.content_type)}
                        <span>{item.source_app}</span>
                    </div>
                </div>

                <div className="item-meta-right">
                    {compactMode && hasTagsSection ? renderTagsContainer() : null}
                    <div className="item-meta-right-slot">
                    <div className="item-actions">
                        {compactMode ? (
                            (!isEditingTags && !isEditingNote) ? (
                                <div
                                    className="compact-more-action-wrap"
                                    onMouseEnter={() => {
                                        moreActionHoverRef.current = true;
                                        compactPreviewBlockedRef.current = true;
                                        // Cancel pending/in-flight show but keep the row hover
                                        // anchor so leaving ··· can resume preview.
                                        hoverRequestIdRef.current = ++compactPreviewRequestSerial;
                                        if (hoverTimerRef.current) {
                                            clearTimeout(hoverTimerRef.current);
                                            hoverTimerRef.current = null;
                                        }
                                        void hideCompactPreviewGlobal();
                                    }}
                                    onMouseLeave={() => {
                                        moreActionHoverRef.current = false;
                                        compactPreviewBlockedRef.current =
                                            isMoreMenuOpen ||
                                            showAIOptions ||
                                            isEditingNote ||
                                            isEditingTags;
                                        // Still over the row: resume normal hover preview delay.
                                        if (
                                            compactPreviewBlockedRef.current ||
                                            !compactPreviewEnabled ||
                                            !itemRef.current?.matches(":hover") ||
                                            !hoverAnchorRef.current
                                        ) {
                                            return;
                                        }
                                        if (hoverTimerRef.current) clearTimeout(hoverTimerRef.current);
                                        const requestId = ++compactPreviewRequestSerial;
                                        hoverRequestIdRef.current = requestId;
                                        const anchor = hoverAnchorRef.current;
                                        hoverTimerRef.current = setTimeout(() => {
                                            hoverTimerRef.current = null;
                                            if (compactPreviewBlockedRef.current) return;
                                            if (!isHoverPreviewRequestCurrent(requestId)) return;
                                            void showCompactPreview(anchor, requestId);
                                        }, COMPACT_PREVIEW_HOVER_DELAY_MS);
                                    }}
                                >
                                    <button
                                        ref={moreButtonRef}
                                        type="button"
                                        className={`btn-icon compact-more-btn ${isMoreMenuOpen ? "active" : ""}`}
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            if (isMoreMenuOpen) {
                                                setIsMoreMenuOpen(false);
                                            } else {
                                                moreActionHoverRef.current = true;
                                                compactPreviewBlockedRef.current = true;
                                                void hideCompactPreview();
                                                updateMoreMenuPosition();
                                                setIsMoreMenuOpen(true);
                                            }
                                        }}
                                        onMouseDown={(e) => e.stopPropagation()}
                                        title={t('more_actions')}
                                        aria-label={t('more_actions')}
                                        aria-haspopup="menu"
                                        aria-expanded={isMoreMenuOpen}
                                    >
                                        <MoreHorizontal size={13} />
                                    </button>
                                    {isMoreMenuOpen && menuPos && createPortal(
                                        <div
                                            ref={moreMenuRef}
                                            className="compact-action-menu"
                                            role="menu"
                                            aria-label={t('more_actions')}
                                            style={{
                                                position: 'fixed',
                                                top: menuPos.top !== undefined ? `${menuPos.top}px` : undefined,
                                                bottom: menuPos.bottom !== undefined ? `${menuPos.bottom}px` : undefined,
                                                right: `${menuPos.right}px`,
                                                zIndex: 100050,
                                            }}
                                            onMouseDown={(e) => e.stopPropagation()}
                                            onClick={(e) => e.stopPropagation()}
                                        >
                                            <button
                                                type="button"
                                                role="menuitem"
                                                className="compact-action-menu-item"
                                                onClick={() => {
                                                    setIsMoreMenuOpen(false);
                                                    onCopy(true);
                                                }}
                                            >
                                                <Copy size={13} />
                                                <span>{t('copy')}</span>
                                            </button>

                                            {canReveal && (
                                                <button
                                                    type="button"
                                                    role="menuitem"
                                                    className={`compact-action-menu-item ${isRevealed ? "is-active" : ""}`}
                                                    onClick={(e) => {
                                                        setIsMoreMenuOpen(false);
                                                        onToggleReveal(e);
                                                    }}
                                                >
                                                    {isRevealed ? <EyeOff size={13} /> : <Eye size={13} />}
                                                    <span>{isRevealed ? t('hide') : t('reveal')}</span>
                                                </button>
                                            )}

                                            <button
                                                type="button"
                                                role="menuitem"
                                                className="compact-action-menu-item"
                                                onClick={(e) => {
                                                    setIsMoreMenuOpen(false);
                                                    onOpen(e);
                                                }}
                                            >
                                                <ExternalLink size={13} />
                                                <span>{t('open')}</span>
                                            </button>

                                            {item.content_type === 'image' && !isSensitiveHidden && (
                                                <button
                                                    type="button"
                                                    role="menuitem"
                                                    className="compact-action-menu-item"
                                                    onClick={(e) => {
                                                        e.stopPropagation();
                                                        setIsMoreMenuOpen(false);
                                                        openPinStickerWindow(item);
                                                    }}
                                                >
                                                    <PictureInPicture2 size={13} />
                                                    <span>{t('pin_sticker')}</span>
                                                </button>
                                            )}

                                            <button
                                                type="button"
                                                role="menuitem"
                                                className={`compact-action-menu-item ${item.is_pinned ? "is-active" : ""}`}
                                                onClick={(e) => {
                                                    setIsMoreMenuOpen(false);
                                                    onTogglePin(e);
                                                }}
                                            >
                                                {item.is_pinned ? <PinOff size={13} /> : <Pin size={13} />}
                                                <span>{item.is_pinned ? t('unpin') : t('pin')}</span>
                                            </button>

                                            <button
                                                type="button"
                                                role="menuitem"
                                                className={`compact-action-menu-item ${item.tags && item.tags.length > 0 ? "is-active" : ""}`}
                                                onClick={(e) => {
                                                    setIsMoreMenuOpen(false);
                                                    onToggleTagEditor(e);
                                                }}
                                            >
                                                <Tag size={13} />
                                                <span>{t('edit_tags')}</span>
                                            </button>

                                            <button
                                                type="button"
                                                role="menuitem"
                                                className={`compact-action-menu-item ${item.note ? "is-active" : ""}`}
                                                onClick={(e) => {
                                                    e.stopPropagation();
                                                    setIsMoreMenuOpen(false);
                                                    onToggleNoteEditor(e);
                                                }}
                                            >
                                                <StickyNote size={13} />
                                                <span>{item.note ? t('edit_note') : t('add_note')}</span>
                                            </button>

                                            {(item.content_type === 'text' || item.content_type === 'rich_text') && aiEnabled && (
                                                <button
                                                    type="button"
                                                    role="menuitem"
                                                    className={`compact-action-menu-item ${isAIProcessing || showAIOptions ? "is-active" : ""}`}
                                                    onClick={(e) => {
                                                        e.stopPropagation();
                                                        setIsMoreMenuOpen(false);
                                                        if (!isAIProcessing) {
                                                            setLocalAiOptionsOpen(prev => !prev);
                                                            onAIOptionsToggle?.();
                                                        }
                                                    }}
                                                >
                                                    {isAIProcessing ? <Loader2 size={13} className="animate-spin" /> : <Sparkles size={13} />}
                                                    <span>{t('ai_assistant')}</span>
                                                </button>
                                            )}

                                            <div className="compact-action-menu-divider" role="separator" />

                                            <button
                                                type="button"
                                                role="menuitem"
                                                className="compact-action-menu-item danger"
                                                onClick={(e) => {
                                                    setIsMoreMenuOpen(false);
                                                    onDelete(e);
                                                }}
                                            >
                                                <Trash2 size={13} />
                                                <span>{t('delete')}</span>
                                            </button>
                                        </div>,
                                        document.body
                                    )}
                                </div>
                            ) : null
                        ) : (
                            <>
                                {canReveal && (
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
                                {item.content_type === 'image' && !isSensitiveHidden && (
                                    <button
                                        className="btn-icon"
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            openPinStickerWindow(item);
                                        }}
                                        title={t('pin_sticker')}
                                    >
                                        <PictureInPicture2 size={12} />
                                    </button>
                                )}
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
                                    title={t('edit_tags')}
                                >
                                    <Tag size={12} />
                                </button>
                                <button
                                    className={`btn-icon ${item.note ? "active" : ""} ${isEditingNote ? "active" : ""}`}
                                    onMouseDown={(e) => e.stopPropagation()}
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        if (isEditingNote) {
                                            onNoteSave(noteInput);
                                        } else {
                                            onToggleNoteEditor(e);
                                        }
                                    }}
                                    title={item.note ? t('edit_note') : t('add_note')}
                                >
                                    <StickyNote size={12} />
                                </button>
                                {(item.content_type === 'text' || item.content_type === 'rich_text') && aiEnabled && (
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
                            </>
                        )}
                    </div>
                    <div className="item-meta-right-info">
                        {quickPasteHint && item.is_pinned && (
                            <span
                                className="quick-paste-hint"
                                title={`${t('quick_paste_modifier')}: ${quickPasteHint.combo}`}
                            >
                                {quickPasteHint.combo}
                            </span>
                        )}
                        <span className="item-compact-time">
                            {getConciseTime(item.created_at || item.timestamp, language)}
                        </span>
                    </div>
                    </div>
                </div>
            </div>

            {
                compactMode && item.is_pinned && (
                    <div className="compact-pinned-indicator" title={t('pinned')}>
                        <Pin size={10} fill="currentColor" />
                    </div>
                )
            }
            <div className={`content-preview-shell${overlayMetaInPreview ? ' has-overlay-meta' : ''}`}>
                <div data-content-type={item.content_type} className={`content-preview ${item.content_type === 'rich_text' ? 'rich-text' : ''} ${hidesFileCard || item.content_type === 'file' ? 'file-preview' : ''} ${isSensitiveHidden ? 'sensitive-blur' : ''}`}>
                {item.content_type === "image" ? (
                    <div
                        className="image-preview-frame"
                        onMouseEnter={isAnimatableImage ? () => setPlayAnimation(true) : undefined}
                        onMouseLeave={isAnimatableImage ? () => setPlayAnimation(false) : undefined}
                    >
                        {(item.is_external && item.file_preview_exists === false) || imageLoadFailed ? (
                            <div className="image-preview error-placeholder" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', background: 'var(--bg-element)', color: 'var(--text-secondary)', height: '100px', fontSize: 'var(--font-size-sm)' }}>
                                <ImageOff size={24} style={{ marginBottom: '8px', opacity: 0.5 }} />
                                <span>{t('image_deleted')}</span>
                            </div>
                        ) : listImageSrc && (
                            // Not loading="lazy": rows are already virtualized, and lazy
                            // loading only delays images that are about to be visible.
                            <img
                                src={listImageSrc}
                                alt={t('image_preview')}
                                className="image-preview"
                                draggable={false}
                                decoding="async"
                                style={isSensitiveHidden ? { filter: 'blur(8px)' } : {}}
                                onError={() => {
                                    setImageLoadFailed(true);
                                }}
                            />
                        )}
                        {isSensitiveHidden && (
                            <div style={{ position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', fontWeight: 'bold', opacity: 0.5, fontSize: 'var(--font-size-2xs)' }}>
                                SENSITIVE
                            </div>
                        )}
                    </div>
                ) : hidesFileCard ? (
                    // No name, folder, path tooltip or cover until revealed.
                    <div className="file-thumbnail-card">
                        <div className="file-icon-wrapper">
                            {item.content_type === "video" ? <Video size={24} /> : <File size={24} />}
                        </div>
                        <div className="file-info-wrapper">
                            <div className="file-name">SENSITIVE</div>
                        </div>
                    </div>
                ) : item.content_type === "video" ? (
                    <div className="video-thumbnail-card">
                        <div className="video-thumbnail-wrapper">
                            {/* The system's cover still when it made one; the video
                                element only as a fallback (it decodes few codecs). */}
                            {videoCoverSrc && !videoCoverFailed ? (
                                <img
                                    src={videoCoverSrc}
                                    alt=""
                                    className="video-thumbnail-element"
                                    draggable={false}
                                    decoding="async"
                                    onError={() => setVideoCoverFailed(true)}
                                />
                            ) : videoCoverSrc === null && !videoCoverFailed ? null : (
                                <video
                                    src={item.content.startsWith("data:")
                                        ? item.content
                                        : (toTauriLocalImageSrc(item.content) || item.content)}
                                    preload="metadata"
                                    muted
                                    playsInline
                                    draggable={false}
                                    className="video-thumbnail-element"
                                    onLoadedMetadata={(e) => seekVideoPreviewFrame(e.currentTarget)}
                                />
                            )}
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
                ) : aiPendingResult ? (
                    <div className="ai-pending-result" onClick={(e) => e.stopPropagation()}>
                        <div className="ai-pending-text">{aiPendingResult}</div>
                        {!onAIApply && <div className="ai-pending-hint">{t("ai_result_truncated")}</div>}
                        <div className="ai-pending-actions">
                            {onAIApply && (
                                <button type="button" className="ai-pending-btn is-primary" onClick={() => onAIApply()}>
                                    {t("ai_apply_result")}
                                </button>
                            )}
                            <button type="button" className="ai-pending-btn" onClick={() => onAIDiscard?.()}>
                                {t("ai_discard_result")}
                            </button>
                        </div>
                    </div>
                ) : aiQuestion ? (
                    <div className="ai-input-wrapper">
                        <input
                            autoFocus
                            className="search-input"
                            style={{ width: '100%', fontSize: 'var(--font-size-sm)', padding: '8px', border: '1px solid var(--accent-color)' }}
                            placeholder={aiQuestion}
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
                        <div style={{ fontSize: 'var(--font-size-2xs)', opacity: 0.6, marginTop: '4px' }}>
                            {t("ai_supplement_hint")}
                        </div>
                    </div>
                ) : item.content_type === "rich_text" && item.html_content && !isSensitiveHidden ? (
                    richTextPreviewSrc ? (
                        <div className="rich-text-snapshot-viewport">
                            <img
                                ref={richSnapshotImgRef}
                                src={richTextPreviewSrc}
                                alt={item.preview || item.content || "rich text preview"}
                                className="rich-text-snapshot-image"
                                draggable={false}
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
                                        rememberFailedSnapshot(effectiveRichTextSnapshotSrc);
                                        setSnapshotFailed(true);
                                    }
                                }}
                            />
                        </div>
                    ) : (
                        <HtmlContent
                            className="rich-text-preview"
                            htmlContent={richTextCleanHtml || item.html_content}
                            fallbackText={item.preview}
                            preview={true}
                            // Same line count as plain text rows, cut between lines:
                            // a fixed height used to slice a line in half.
                            clampLines={compactMode ? 2 : 4}
                            style={{
                                fontSize: 'var(--clipboard-item-font-size)',
                                lineHeight: '1.4',
                                position: 'relative',
                                pointerEvents: 'none' // Prevent interacting with links in the list
                            }}
                        />
                    )
                ) : standaloneColorValue && !isSensitiveHidden ? (
                    <div className="color-code-preview">
                        <span
                            className="color-code-swatch"
                            style={{ background: standaloneColorValue }}
                            aria-hidden="true"
                        />
                        <span className="color-code-value">{standaloneColorValue}</span>
                    </div>
                ) : (
                    isSensitiveHidden
                        ? (
                            <div style={{ minHeight: '24px', opacity: 0.6, fontStyle: 'italic', display: 'flex', alignItems: 'center', gap: '8px', fontFamily: 'var(--font-mono)' }}>
                                <span style={{ letterSpacing: '1px' }}>
                                    {sensitivePreview}
                                </span>
                                <span style={{ fontSize: 'var(--font-size-2xs)', opacity: 0.7 }}>
                                    ({item.content.length} {t('chars')})
                                </span>
                            </div>
                        )
                        : item.preview
                )}
                </div>
                {overlayMetaInPreview && renderMetaFooter()}
            </div>

            {showFlowMetaFooter && renderMetaFooter()}

            {compactMode && !!item.note && !isEditingNote && (
                <button
                    type="button"
                    className="compact-note-indicator"
                    title={item.note}
                    onClick={(e) => {
                        e.stopPropagation();
                        onToggleNoteEditor(e);
                    }}
                    onMouseDown={(e) => e.stopPropagation()}
                >
                    <StickyNote size={10} />
                </button>
            )}

            {compactMode && isEditingNote && renderNoteBlock()}

            {/* AI Options - Compact Mode: Dropdown Panel, Normal Mode: Inline */}
            <AnimatePresence>
                {showAIOptions && (
                    <motion.div
                        className={compactMode ? "ai-options-dropdown" : "ai-options-inline"}
                        initial={compactMode ? { opacity: 0, y: -10 } : "collapsed"}
                        animate={compactMode ? { opacity: 1, y: 0 } : "open"}
                        exit={compactMode ? { opacity: 0, y: -10 } : "collapsed"}
                        variants={compactMode ? undefined : inlineAiVariants}
                        transition={compactMode ? { duration: 0.16 } : { duration: 0.18 }}
                    >
                        <div className={`ai-actions-group ${compactMode ? 'compact' : ''}`}>
                            {['task', 'mouthpiece', 'translate'].map(actionType => (
                                <button
                                    key={actionType}
                                    type="button"
                                    onClick={(e) => {
                                        e.stopPropagation();
                                        onAIAction?.(actionType);
                                        onAIOptionsToggle?.();
                                    }}
                                    className="ai-action-btn"
                                >
                                    {t(`ai_${actionType}`)}
                                </button>
                            ))}
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

        </motion.div >
    );
};

export default memo(ClipboardItem, (prevProps, nextProps) => {
    return prevProps.isSelected === nextProps.isSelected &&
        prevProps.isDetailFocused === nextProps.isDetailFocused &&
        prevProps.navigationIndex === nextProps.navigationIndex &&
        prevProps.item.id === nextProps.item.id &&
        prevProps.item.content_type === nextProps.item.content_type &&
        prevProps.item.timestamp === nextProps.item.timestamp &&
        prevProps.item.created_at === nextProps.item.created_at &&
        prevProps.item.last_used_at === nextProps.item.last_used_at &&
        prevProps.item.sort_at === nextProps.item.sort_at &&
        prevProps.item.use_count === nextProps.item.use_count &&
        prevProps.item.pinned_order === nextProps.item.pinned_order &&
        prevProps.item.content === nextProps.item.content &&
        prevProps.item.preview === nextProps.item.preview &&
        prevProps.item.html_content === nextProps.item.html_content &&
        prevProps.item.source_app === nextProps.item.source_app &&
        prevProps.item.source_app_path === nextProps.item.source_app_path &&
        prevProps.item.is_pinned === nextProps.item.is_pinned &&
        prevProps.item.is_external === nextProps.item.is_external &&
        prevProps.item.file_preview_exists === nextProps.item.file_preview_exists &&
        // History refetches create new tag arrays; compare contents, not references.
        (prevProps.item.tags ?? []).join('\u0000') === (nextProps.item.tags ?? []).join('\u0000') &&
        prevProps.item.note === nextProps.item.note &&
        prevProps.isSensitiveHidden === nextProps.isSensitiveHidden &&
        prevProps.isRevealed === nextProps.isRevealed &&
        prevProps.canReveal === nextProps.canReveal &&
        prevProps.isEditingTags === nextProps.isEditingTags &&
        prevProps.isEditingNote === nextProps.isEditingNote &&
        prevProps.noteDraft === nextProps.noteDraft &&
        prevProps.isAIProcessing === nextProps.isAIProcessing &&
        prevProps.aiQuestion === nextProps.aiQuestion &&
        prevProps.aiPendingResult === nextProps.aiPendingResult &&
        prevProps.aiOptionsOpen === nextProps.aiOptionsOpen &&
        prevProps.aiEnabled === nextProps.aiEnabled &&
        prevProps.richTextSnapshotPreview === nextProps.richTextSnapshotPreview &&
        prevProps.showSourceAppIcon === nextProps.showSourceAppIcon &&
        prevProps.sensitiveMaskPrefixVisible === nextProps.sensitiveMaskPrefixVisible &&
        prevProps.sensitiveMaskSuffixVisible === nextProps.sensitiveMaskSuffixVisible &&
        prevProps.sensitiveMaskEmailDomain === nextProps.sensitiveMaskEmailDomain &&
        prevProps.tagColors === nextProps.tagColors &&
        prevProps.quickPasteHint?.slot === nextProps.quickPasteHint?.slot &&
        prevProps.quickPasteHint?.combo === nextProps.quickPasteHint?.combo &&
        prevProps.compactMode === nextProps.compactMode &&
        prevProps.theme === nextProps.theme &&
        prevProps.language === nextProps.language &&
        prevProps.tagInput === nextProps.tagInput &&
        (prevProps.tagSuggestions ?? []).join('\u0000') === (nextProps.tagSuggestions ?? []).join('\u0000');
});
