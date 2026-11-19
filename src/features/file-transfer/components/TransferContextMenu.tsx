import { useRef, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
    ChevronDown,
    ChevronRight,
    Folder,
    ExternalLink,
    Download,
    Image as ImageIcon,
    Link as LinkIcon,
    Video,
    Copy,
    ClipboardPaste,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import type { FileTransferContextMenu, FileTransferMessage } from "../types";
import { copyPrivateText, isLocalTransferPath, resolveShareableDownloadUrl } from "../utils/transferHelpers";
import { withNativeDialog } from "../../../shared/lib/focus";
import { isMacPlatform } from "../../../shared/lib/platform";

interface TransferContextMenuProps {
    contextMenu: FileTransferContextMenu | null;
    onClose: () => void;
    expandedBatches: Set<string>;
    toggleBatchExpanded: (batchId: string) => void;
    savingBatchId: string | null;
    saveBatchAs: (messages: FileTransferMessage[]) => void | Promise<void>;
    openTransferContent: (params: {
        filePath?: string;
        content?: string;
        type?: string;
    }) => Promise<void>;
    t: (key: string) => string;
    /** Address phones reach this computer at, for copied links. */
    localIp?: string;
    actualPort?: string;
}

export const TransferContextMenu = ({
    contextMenu,
    onClose,
    expandedBatches,
    toggleBatchExpanded,
    savingBatchId,
    saveBatchAs,
    openTransferContent,
    t,
    localIp,
    actualPort,
}: TransferContextMenuProps) => {
    const contextMenuRef = useRef<HTMLDivElement>(null);
    const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null);

    // Keep context menu position within viewport
    useEffect(() => {
        if (!contextMenu) {
            setMenuPosition(null);
            return;
        }

        setMenuPosition({ x: contextMenu.x, y: contextMenu.y });

        const frame = requestAnimationFrame(() => {
            const menu = contextMenuRef.current;
            if (!menu) return;

            const edge = 8;
            const nextX = Math.max(
                edge,
                Math.min(contextMenu.x, window.innerWidth - menu.offsetWidth - edge)
            );
            const nextY = Math.max(
                edge,
                Math.min(contextMenu.y, window.innerHeight - menu.offsetHeight - edge)
            );

            setMenuPosition({ x: nextX, y: nextY });

            if (!menu.contains(document.activeElement)) {
                menu.focus({ preventScroll: true });
            }
        });

        const handlePointerDown = (event: PointerEvent) => {
            if (!contextMenuRef.current?.contains(event.target as Node)) {
                onClose();
            }
        };

        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                onClose();
                return;
            }

            if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
            const items = Array.from(
                contextMenuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') || []
            );
            if (items.length === 0) return;

            event.preventDefault();
            const activeIndex = items.indexOf(document.activeElement as HTMLElement);

            if (event.key === "Home") {
                items[0].focus();
            } else if (event.key === "End") {
                items[items.length - 1].focus();
            } else if (event.key === "ArrowDown") {
                const nextIndex = activeIndex < items.length - 1 ? activeIndex + 1 : 0;
                items[nextIndex].focus();
            } else if (event.key === "ArrowUp") {
                const prevIndex = activeIndex > 0 ? activeIndex - 1 : items.length - 1;
                items[prevIndex].focus();
            }
        };

        window.addEventListener("pointerdown", handlePointerDown);
        window.addEventListener("keydown", handleKeyDown);

        return () => {
            cancelAnimationFrame(frame);
            window.removeEventListener("pointerdown", handlePointerDown);
            window.removeEventListener("keydown", handleKeyDown);
        };
    }, [contextMenu, onClose]);

    return (
        <AnimatePresence>
            {contextMenu && (
                <motion.div
                    ref={contextMenuRef}
                    className="wt-context-menu"
                    role="menu"
                    aria-label={t("ft_menu_aria")}
                    tabIndex={-1}
                    initial={{ opacity: 0, scale: 0.97 }}
                    animate={{ opacity: 1, scale: 1 }}
                    exit={{ opacity: 0, scale: 0.97 }}
                    transition={{ duration: 0.12 }}
                    style={{
                        top: menuPosition ? menuPosition.y : contextMenu.y,
                        left: menuPosition ? menuPosition.x : contextMenu.x,
                    }}
                    onContextMenu={(event) => event.preventDefault()}
                >
                    {contextMenu.type === "batch" && contextMenu.batchId && (
                        <>
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={() => {
                                    const batchId = contextMenu.batchId!;
                                    onClose();
                                    toggleBatchExpanded(batchId);
                                }}
                            >
                                {expandedBatches.has(contextMenu.batchId) ? (
                                    <ChevronRight size={16} />
                                ) : (
                                    <ChevronDown size={16} />
                                )}
                                <span>
                                    {expandedBatches.has(contextMenu.batchId)
                                        ? t("ft_collapse_batch")
                                        : t("ft_expand_batch")}
                                </span>
                            </button>
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                disabled={
                                    savingBatchId === contextMenu.batchId ||
                                    !contextMenu.batchMessages?.length ||
                                    contextMenu.batchMessages.some(
                                        (message) =>
                                            !message.file_path ||
                                            !isLocalTransferPath(message.file_path)
                                    )
                                }
                                onClick={() => {
                                    const selected = contextMenu.batchMessages || [];
                                    onClose();
                                    void saveBatchAs(selected);
                                }}
                            >
                                <Folder size={16} />
                                <span>
                                    {savingBatchId === contextMenu.batchId
                                        ? t("ft_saving")
                                        : t("ft_save_batch_as_menu")}
                                </span>
                            </button>
                        </>
                    )}

                    {(contextMenu.type === "file" ||
                        contextMenu.type === "image" ||
                        contextMenu.type === "video") && (
                        <>
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={async () => {
                                    const selected = contextMenu;
                                    onClose();
                                    await openTransferContent({
                                        filePath: selected.filePath,
                                        content: selected.content,
                                        type: selected.type,
                                    });
                                }}
                            >
                                <ExternalLink size={16} />
                                <span>{t("open")}</span>
                            </button>
                            {isLocalTransferPath(contextMenu.filePath) && (
                                <button
                                    type="button"
                                    className="wt-context-item"
                                    role="menuitem"
                                    onClick={async () => {
                                        const selected = contextMenu;
                                        onClose();
                                        try {
                                            await invoke("open_file_location", {
                                                filePath: selected.filePath,
                                            });
                                        } catch (error) {
                                            console.error("Failed to reveal file", error);
                                        }
                                    }}
                                >
                                    <Folder size={16} />
                                    <span>
                                        {t(isMacPlatform() ? "show_in_finder" : "show_in_explorer")}
                                    </span>
                                </button>
                            )}
                            <div className="wt-context-separator" role="separator" />
                        </>
                    )}

                    {contextMenu.type === "image" && (
                        <>
                            {isLocalTransferPath(contextMenu.filePath) && (
                                <button
                                    type="button"
                                    className="wt-context-item"
                                    role="menuitem"
                                    onClick={async () => {
                                        const selected = contextMenu;
                                        onClose();
                                        try {
                                            const target = await withNativeDialog(
                                                () =>
                                                    saveDialog({
                                                        defaultPath:
                                                            selected.filePath
                                                                ?.split(/[/\\]/)
                                                                .pop() || "image.png",
                                                        filters: [
                                                            {
                                                                name: t("type_image"),
                                                                extensions: [
                                                                    "png",
                                                                    "jpg",
                                                                    "jpeg",
                                                                    "webp",
                                                                ],
                                                            },
                                                        ],
                                                    }),
                                                "file-transfer:save-image"
                                            );
                                            if (target && selected.filePath) {
                                                await invoke("save_file_copy", {
                                                    sourcePath: selected.filePath,
                                                    targetPath: target,
                                                });
                                            }
                                        } catch (error) {
                                            console.error("Failed to save image", error);
                                        }
                                    }}
                                >
                                    <Download size={16} />
                                    <span>
                                        {t("save_image_as")}
                                    </span>
                                </button>
                            )}
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={async () => {
                                    const selected = contextMenu;
                                    onClose();
                                    if (selected.filePath) {
                                        await invoke("copy_to_clipboard", {
                                            content: selected.filePath,
                                            contentType: "image",
                                            paste: false,
                                            id: 0,
                                            deleteAfterUse: false,
                                        }).catch((error) =>
                                            console.error("Failed to copy image", error)
                                        );
                                    }
                                }}
                            >
                                <ImageIcon size={16} />
                                <span>{t("copy_image")}</span>
                            </button>
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={async () => {
                                    const selected = contextMenu;
                                    onClose();
                                    const link = resolveShareableDownloadUrl(
                                        selected.content || selected.filePath || "",
                                        localIp,
                                        actualPort
                                    );
                                    if (link) await copyPrivateText(link);
                                }}
                            >
                                <LinkIcon size={16} />
                                <span>
                                    {t("copy_image_link")}
                                </span>
                            </button>
                        </>
                    )}

                    {contextMenu.type === "video" && (
                        <>
                            {isLocalTransferPath(contextMenu.filePath) && (
                                <button
                                    type="button"
                                    className="wt-context-item"
                                    role="menuitem"
                                    onClick={async () => {
                                        const selected = contextMenu;
                                        onClose();
                                        try {
                                            const target = await withNativeDialog(
                                                () =>
                                                    saveDialog({
                                                        defaultPath:
                                                            selected.filePath
                                                                ?.split(/[/\\]/)
                                                                .pop() || "video.mp4",
                                                        filters: [
                                                            {
                                                                name: t("type_video"),
                                                                extensions: [
                                                                    "mp4",
                                                                    "mov",
                                                                    "avi",
                                                                    "mkv",
                                                                    "webm",
                                                                ],
                                                            },
                                                        ],
                                                    }),
                                                "file-transfer:save-video"
                                            );
                                            if (target && selected.filePath) {
                                                await invoke("save_file_copy", {
                                                    sourcePath: selected.filePath,
                                                    targetPath: target,
                                                });
                                            }
                                        } catch (error) {
                                            console.error("Failed to save video", error);
                                        }
                                    }}
                                >
                                    <Download size={16} />
                                    <span>
                                        {t("save_video_as")}
                                    </span>
                                </button>
                            )}
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={async () => {
                                    const selected = contextMenu;
                                    onClose();
                                    if (selected.filePath) {
                                        await invoke("copy_to_clipboard", {
                                            content: selected.filePath,
                                            contentType: "video",
                                            paste: false,
                                            id: 0,
                                            deleteAfterUse: false,
                                        }).catch((error) =>
                                            console.error("Failed to copy video", error)
                                        );
                                    }
                                }}
                            >
                                <Video size={16} />
                                <span>{t("copy_video")}</span>
                            </button>
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={async () => {
                                    const selected = contextMenu;
                                    onClose();
                                    const link = resolveShareableDownloadUrl(
                                        selected.content || selected.filePath || "",
                                        localIp,
                                        actualPort
                                    );
                                    if (link) await copyPrivateText(link);
                                }}
                            >
                                <LinkIcon size={16} />
                                <span>
                                    {t("copy_video_link")}
                                </span>
                            </button>
                        </>
                    )}

                    {contextMenu.type === "text" && (
                        <>
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={async () => {
                                    const selected = contextMenu;
                                    onClose();
                                    if (selected.content)
                                        await navigator.clipboard.writeText(selected.content);
                                }}
                            >
                                <Copy size={16} />
                                <span>{t("copy_text")}</span>
                            </button>
                            <button
                                type="button"
                                className="wt-context-item"
                                role="menuitem"
                                onClick={async () => {
                                    const selected = contextMenu;
                                    onClose();
                                    if (selected.content) {
                                        try {
                                            await invoke("copy_to_clipboard", {
                                                content: selected.content,
                                                contentType: "text",
                                                paste: true,
                                                id: 0,
                                                deleteAfterUse: false,
                                            });
                                        } catch (e) {
                                            await navigator.clipboard.writeText(selected.content);
                                        }
                                    }
                                }}
                            >
                                <ClipboardPaste size={16} />
                                <span>{t("ft_copy_and_paste")}</span>
                            </button>
                        </>
                    )}

                    {contextMenu.type === "file" && (
                        <button
                            type="button"
                            className="wt-context-item"
                            role="menuitem"
                            onClick={async () => {
                                const selected = contextMenu;
                                onClose();
                                const link = resolveShareableDownloadUrl(
                                    selected.content || selected.filePath || "",
                                    localIp,
                                    actualPort
                                );
                                if (link) await copyPrivateText(link);
                            }}
                        >
                            <LinkIcon size={16} />
                            <span>{t("copy_link")}</span>
                        </button>
                    )}
                </motion.div>
            )}
        </AnimatePresence>
    );
};
