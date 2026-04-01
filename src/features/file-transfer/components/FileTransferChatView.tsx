import { useState, useEffect, useRef } from "react";
import type { CSSProperties, ReactNode } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import {
    Plus,
    Maximize2,
    Minimize2,
    ExternalLink,
    Folder,
    RotateCcw,
    Image as ImageIcon,
    Link as LinkIcon,
    Clipboard,
    Video,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { QRCodeCanvas } from "qrcode.react";
import type {
    FileTransferChatViewProps,
    FileTransferContextMenu,
    FileTransferMessage,
    FileTransferDevice
} from "../types";

// File Transfer Chat View Component
const FileTransferChatView = ({
    t,
    localIp,
    actualPort
}: FileTransferChatViewProps) => {
    const resolveComposerMetrics = (height?: number) => {
        const baseHeight = height ?? (typeof window !== "undefined" ? window.innerHeight : 0);
        const controlHeight = Math.max(46, Math.min(84, Math.round(baseHeight * 0.1)));
        const footerPaddingY = Math.max(8, Math.min(20, Math.round(controlHeight * 0.18)));
        return { controlHeight, footerPaddingY };
    };

    const rootRef = useRef<HTMLDivElement>(null);
    const [composerMetrics, setComposerMetrics] = useState(() => resolveComposerMetrics());
    const composerMinHeight = composerMetrics.controlHeight;
    const composerSideWidth = Math.max(74, Math.round(composerMinHeight * 1.6));
    const [messages, setMessages] = useState<FileTransferMessage[]>([]);
    const [input, setInput] = useState("");
    const [appLogo, setAppLogo] = useState("");
    const messagesEndRef = useRef<HTMLDivElement>(null);
    const chatBoxRef = useRef<HTMLDivElement>(null);
    const [isUserScrolling, setIsUserScrolling] = useState(false);
    const prevMessagesLengthRef = useRef(0);
    const [showFullScreen, setShowFullScreen] = useState(false);
    const [showExpandBtn, setShowExpandBtn] = useState(false);
    const textareaRef = useRef<HTMLTextAreaElement>(null);
    const [contextMenu, setContextMenu] = useState<FileTransferContextMenu | null>(null);
    const [onlineDevices, setOnlineDevices] = useState<FileTransferDevice[]>([]);
    const [isDragging, setIsDragging] = useState(false);
    const lastDropSignatureRef = useRef("");
    const lastDropHandledAtRef = useRef(0);
    const chatViewStyle = {
        position: 'relative',
        ['--wt-control-height']: `${composerMinHeight}px`,
        ['--wt-side-width']: `${composerSideWidth}px`,
        ['--wt-footer-padding-y']: `${composerMetrics.footerPaddingY}px`
    } as CSSProperties;

    const URL_REGEX = /((https?:\/\/|www\.)[^\s<]+|(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+(?:[a-z]{2,})(?:\/[^\s<]*)?)/gi;

    const normalizeUrl = (raw: string) => {
        if (/^https?:\/\//i.test(raw)) return raw;
        return `http://${raw}`;
    };

    const splitTrailingPunctuation = (raw: string) => {
        let url = raw;
        let trailing = '';
        while (url.length > 0 && /[)\]}>.,;!?]$/.test(url)) {
            trailing = url.slice(-1) + trailing;
            url = url.slice(0, -1);
        }
        return { url, trailing };
    };

    const renderTextWithLinks = (text: string) => {
        const parts: ReactNode[] = [];
        let lastIndex = 0;

        text.replace(URL_REGEX, (match, _group, _proto, offset) => {
            if (match.includes('@')) {
                return match;
            }
            const prevChar = offset > 0 ? text[offset - 1] : '';
            if (prevChar && /[a-z0-9@]/i.test(prevChar)) {
                return match;
            }
            if (offset > lastIndex) {
                parts.push(text.slice(lastIndex, offset));
            }

            const { url, trailing } = splitTrailingPunctuation(match);
            const href = normalizeUrl(url);

            parts.push(
                <a
                    key={`link-${offset}`}
                    href={href}
                    className="wt-link"
                    onClick={(e) => {
                        e.preventDefault();
                        if (window.getSelection()?.toString()) return;
                        invoke('open_content', { id: 0, content: href, contentType: 'url' }).catch(console.error);
                    }}
                >
                    {url}
                </a>
            );

            if (trailing) {
                parts.push(trailing);
            }

            lastIndex = offset + match.length;
            return match;
        });

        if (lastIndex < text.length) {
            parts.push(text.slice(lastIndex));
        }

        return parts.length > 0 ? parts : text;
    };

    useEffect(() => {
        invoke("set_navigation_enabled", { enabled: false }).catch(console.error);
        return () => {
            invoke("set_navigation_enabled", { enabled: true }).catch(console.error);
        };
    }, []);

    useEffect(() => {
        const handleCopy = (e: KeyboardEvent) => {
            const isCopy = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === 'c';
            if (!isCopy) return;

            const selection = window.getSelection();
            const text = selection?.toString() || '';
            if (!text) return;

            const root = chatBoxRef.current;
            const anchor = selection?.anchorNode;
            const focus = selection?.focusNode;
            if (!root || !anchor || !focus) return;
            if (!root.contains(anchor) || !root.contains(focus)) return;

            e.preventDefault();
            e.stopPropagation();
            navigator.clipboard.writeText(text).catch(console.error);
        };

        window.addEventListener('keydown', handleCopy, true);
        return () => window.removeEventListener('keydown', handleCopy, true);
    }, []);

    const getAvatarConfig = (m: FileTransferMessage) => {
        if (m.sender_id === 'pc' || m.direction === 'out') {
            return {
                isImg: !!appLogo,
                content: appLogo || 'PC',
                color: 'var(--wt-own-avatar-background)',
                textColor: 'var(--wt-own-avatar-color)',
                initial: 'PC'
            };
        }

        const gradients = [
            'var(--wt-peer-gradient-1)',
            'var(--wt-peer-gradient-2)',
            'var(--wt-peer-gradient-3)',
            'var(--wt-peer-gradient-4)',
            'var(--wt-peer-gradient-5)',
            'var(--wt-peer-gradient-6)',
            'var(--wt-peer-gradient-7)',
            'var(--wt-peer-gradient-8)'
        ];

        const id = m.sender_id || 'mobile';
        let hash = 0;
        for (let i = 0; i < id.length; i++) {
            hash = id.charCodeAt(i) + ((hash << 5) - hash);
        }
        const gradient = gradients[Math.abs(hash) % gradients.length];

        let initial = 'M';
        if (m.sender_name) {
            const name = m.sender_name.toLowerCase();
            if (name.includes('iphone')) initial = 'iP';
            else if (name.includes('ipad')) initial = 'iD';
            else if (name.includes('android')) initial = 'An';
            else if (name.includes('手机')) initial = 'M';
            else initial = m.sender_name.charAt(0).toUpperCase();
        }

        return {
            isImg: false,
            color: gradient,
            textColor: 'var(--wt-peer-avatar-color)',
            initial
        };
    };

    const fetchMessages = async () => {
        try {
            const msgs = await invoke<FileTransferMessage[]>("get_chat_history");
            setMessages(msgs);
        } catch (e) { }
    };

    useEffect(() => {
        console.log("FileTransferChatView Mounted - initializing listeners (Dual Mode)");
        const appWindow = getCurrentWindow();
        console.log("[DEBUG] appWindow label:", appWindow.label);
        fetchMessages();
        invoke<string>("get_app_logo").then(setAppLogo).catch(console.error);



        const resolveDropPaths = (payload: unknown): string[] => {
            if (Array.isArray(payload)) {
                return payload.filter((p): p is string => typeof p === "string");
            }
            if (payload && typeof payload === "object" && "paths" in payload) {
                const maybePaths = (payload as { paths?: unknown }).paths;
                if (Array.isArray(maybePaths)) {
                    return maybePaths.filter((p): p is string => typeof p === "string");
                }
            }
            return [];
        };

        // Define handlers to be reused
        const handleDragDrop = (event: { payload: unknown }) => {
            console.log("[DRAG] Drop event received:", event);
            console.log("[DRAG] Event payload type:", typeof event.payload);
            console.log("[DRAG] Event payload:", JSON.stringify(event.payload, null, 2));
            setIsDragging(false);

            const paths = resolveDropPaths(event.payload);
            const signature = paths.slice().sort().join("|");
            const now = Date.now();

            console.log("[DRAG] Parsed paths:", paths);

            if (
                signature &&
                signature === lastDropSignatureRef.current &&
                now - lastDropHandledAtRef.current < 1500
            ) {
                console.log("[DRAG] Duplicate drop event ignored");
                return;
            }

            if (paths && paths.length > 0) {
                lastDropSignatureRef.current = signature;
                lastDropHandledAtRef.current = now;

                const tempMessages: FileTransferMessage[] = [];
                paths.forEach(path => {
                    const fileName = path.split(/[/\\]/).pop() || 'File';
                    tempMessages.push({
                        id: Date.now() + Math.random(),
                        direction: 'out',
                        msg_type: 'file',
                        content: 'Preparing...',
                        timestamp: Date.now(),
                        _fileName: fileName,
                        _preparing: true
                    });
                });

                if (tempMessages.length > 0) {
                    setMessages(prev => [...prev, ...tempMessages]);
                    setTimeout(() => messagesEndRef.current?.scrollIntoView({ behavior: "smooth" }), 100);
                }

                paths.forEach(path => {
                    invoke("send_file_to_client", { filePath: path }).catch(console.error);
                });
                setTimeout(fetchMessages, 500);
            }
        };

        const handleDragEnter = (event: { payload: unknown }) => {
            console.log("[DRAG] Enter event received:", event);
            setIsDragging(true);
        };

        const handleDragLeave = (event: { payload: unknown }) => {
            console.log("[DRAG] Leave event received:", event);
            setIsDragging(false);
        };

        // Listen to BOTH v1 and v2 events just to be safe
        console.log("[DEBUG] Registering drag-drop event listeners...");

        // v1 event names (some versions still use these)
        const unlistenV1Drop = appWindow.listen("tauri://file-drop", (e) => {
            console.log("[v1] file-drop received");
            handleDragDrop(e);
        });
        const unlistenV1Hover = appWindow.listen("tauri://file-drop-hover", (e) => {
            console.log("[v1] file-drop-hover received");
            handleDragEnter(e);
        });
        const unlistenV1Cancel = appWindow.listen("tauri://file-drop-cancelled", (e) => {
            console.log("[v1] file-drop-cancelled received");
            handleDragLeave(e);
        });

        // v2 event names
        const unlistenV2Drop = appWindow.listen("tauri://drag-drop", (e) => {
            console.log("[v2] drag-drop received");
            handleDragDrop(e);
        });
        const unlistenV2Enter = appWindow.listen("tauri://drag-enter", (e) => {
            console.log("[v2] drag-enter received");
            handleDragEnter(e);
        });
        const unlistenV2Leave = appWindow.listen("tauri://drag-leave", (e) => {
            console.log("[v2] drag-leave received");
            handleDragLeave(e);
        });

        console.log("[DEBUG] All drag-drop listeners registered successfully");

        const unlistenDevices = listen<FileTransferDevice[]>("online-devices-updated", (event) => {
            setOnlineDevices(event.payload || []);
        });

        const unlistenNewMsg = listen<FileTransferMessage>("new-chat-message", () => {
            fetchMessages();
        });

        return () => {

            unlistenV1Drop.then(f => f());
            unlistenV1Hover.then(f => f());
            unlistenV1Cancel.then(f => f());
            unlistenV2Drop.then(f => f());
            unlistenV2Enter.then(f => f());
            unlistenV2Leave.then(f => f());
            unlistenDevices.then(f => f());
            unlistenNewMsg.then(f => f());
        };
    }, []);

    // Detect if user is at bottom
    useEffect(() => {
        const chatBox = chatBoxRef.current;
        if (!chatBox) return;

        const handleScroll = () => {
            const { scrollTop, scrollHeight, clientHeight } = chatBox;
            const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;
            setIsUserScrolling(!isAtBottom);
        };

        chatBox.addEventListener('scroll', handleScroll);
        return () => chatBox.removeEventListener('scroll', handleScroll);
    }, []);

    // Smart Scroll
    useEffect(() => {
        const hasNewMessages = messages.length > prevMessagesLengthRef.current;
        prevMessagesLengthRef.current = messages.length;

        if (hasNewMessages && !isUserScrolling) {
            messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
        }
    }, [messages, isUserScrolling]);

    // Adjust textarea height
    useEffect(() => {
        const element = rootRef.current;
        if (!element || typeof ResizeObserver === "undefined") {
            setComposerMetrics(resolveComposerMetrics());
            return;
        }

        const updateMetrics = () => {
            setComposerMetrics(resolveComposerMetrics(element.clientHeight));
        };

        updateMetrics();
        const observer = new ResizeObserver(() => updateMetrics());
        observer.observe(element);
        return () => observer.disconnect();
    }, []);

    useEffect(() => {
        if (textareaRef.current) {
            textareaRef.current.style.height = `${composerMinHeight}px`;
            const scrollHeight = textareaRef.current.scrollHeight;

            // Check if text is overflowing (more content than fits in max height)
            if (scrollHeight > 120) {
                setShowExpandBtn(true);
            } else {
                setShowExpandBtn(false);
            }

            textareaRef.current.style.height = Math.min(Math.max(composerMinHeight, scrollHeight), 120) + 'px';
        }
    }, [composerMinHeight, input]);

    const send = async () => {
        if (!input.trim()) return;
        try {
            await invoke("send_chat_message", { msgType: "text", content: input });
            setInput("");
            setShowFullScreen(false);
            fetchMessages();
            // Reset height
            if (textareaRef.current) textareaRef.current.style.height = `${composerMinHeight}px`;
        } catch (e) { }
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            send();
        }
    };

    const handlePaste = async (e: React.ClipboardEvent) => {
        if (e.clipboardData.files.length > 0) {
            const files = Array.from(e.clipboardData.files);
            const imageFiles = files.filter(f => f.type.startsWith('image/'));

            if (imageFiles.length > 0) {
                e.preventDefault();

                for (const file of imageFiles) {
                    const reader = new FileReader();
                    reader.onload = async (ev) => {
                        const base64 = ev.target?.result as string;
                        if (base64) {
                            try {
                                const savedPath = await invoke<string>("save_temp_image", { base64Data: base64 });
                                await invoke("send_file_to_client", { filePath: savedPath });
                            } catch (err) {
                                console.error("Failed to paste image", err);
                            }
                        }
                    };
                    reader.readAsDataURL(file);
                }
            }
        }
    };

    // Close context menu on click outside
    useEffect(() => {
        const handleClick = () => setContextMenu(null);
        if (contextMenu) {
            document.addEventListener('click', handleClick);
            return () => document.removeEventListener('click', handleClick);
        }
    }, [contextMenu]);

    return (
        <div
            ref={rootRef}
            className="wt-chat-view"
            style={chatViewStyle}
            onDragOver={(e) => {
                // Critical: Prevent default browser behavior to allow drop
                e.preventDefault();
                if (!isDragging) setIsDragging(true);
            }}
            onDragLeave={(e) => {
                // Check if leaving the main container
                if (e.currentTarget.contains(e.relatedTarget as Node)) return;
                setIsDragging(false);
            }}
            onDrop={(e) => {
                e.preventDefault();
                setIsDragging(false);
                // Note: Web 'drop' event gives Files but no full path. 
                // We rely on Tauri event for paths. 
                // However, preventing default here removes the "prohibited" cursor.
            }}
        >
            {/* Drag Overlay */}
            <AnimatePresence>
                {isDragging && (
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        style={{
                            position: 'absolute',
                            top: 0, left: 0, right: 0, bottom: 0,
                            background: 'var(--wt-overlay-background)',
                            backdropFilter: 'blur(4px)',
                            zIndex: 99999, // Ensure it's on top of everything
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            pointerEvents: 'none' // Allow drop to pass through? No, wait... Tauri handles drop globally. Overlay is just visual.
                        }}
                    >
                        <div style={{
                            border: '4px dashed var(--wt-overlay-border)',
                            borderRadius: '16px',
                            padding: '40px',
                            display: 'flex',
                            flexDirection: 'column',
                            alignItems: 'center',
                            gap: '16px',
                            color: 'var(--wt-overlay-color)'
                        }}>
                            <Folder size={64} color="currentColor" strokeWidth={1.5} />
                            <div style={{ fontSize: '24px', fontWeight: 'bold' }}>Drop to Send</div>
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

            {localIp && actualPort && (
                <div className="wt-header">
                    <div className="wt-info-panel">
                        <div className="wt-qr-container">
                            <QRCodeCanvas value={`http://${localIp}:${actualPort}`} size={64} />
                        </div>
                        <div className="wt-info-list" style={{ flex: 1 }}>
                            <div className="wt-info-row">
                                <span className="wt-info-label">LOCAL IP:</span>
                                <span className="wt-info-value">{localIp}</span>
                            </div>
                            <div className="wt-info-row">
                                <span className="wt-info-label">PORT:</span>
                                <span className="wt-info-value">{actualPort}</span>
                            </div>
                            <div className="wt-info-row">
                                <span className="wt-info-label">ONLINE:</span>
                                <span className="wt-info-value" style={{ color: "var(--accent-color)" }}>
                                    {onlineDevices.length} 个设备已连接
                                </span>
                            </div>
                        </div>
                    </div>
                </div>
            )}

            <div className="wt-chat-box" ref={chatBoxRef}>
                {messages.length === 0 && (
                    <div style={{ opacity: 0.5, textAlign: 'center', marginTop: '40px', fontSize: '12px', fontFamily: 'var(--font-mono)' }}>
                        {t ? (t('waiting_connection') || "Waiting for connection...") : "Waiting for connection..."}
                    </div>
                )}
                {messages.map(m => {
                    const avatar = getAvatarConfig(m);
                    return (
                        <div key={m.id} className={`wt-message ${m.direction === 'out' ? 'sent' : 'received'}`}>
                            <div
                                className="wt-avatar"
                                style={{
                                    overflow: 'hidden',
                                    background: avatar.isImg ? 'transparent' : avatar.color,
                                    color: avatar.textColor,
                                    display: 'flex',
                                    alignItems: 'center',
                                    justifyContent: 'center',
                                    fontWeight: 'bold',
                                    fontSize: '14px'
                                }}
                            >
                                {avatar.isImg ? (
                                    <img src={avatar.content} style={{ width: '100%', height: '100%', objectFit: 'cover' }} loading="lazy" alt="Avatar" />
                                ) : (
                                    avatar.initial
                                )}
                            </div>
                            <div className="wt-bubble">
                                {m.sender_name && m.direction === 'in' && (
                                    <div style={{ fontSize: '10px', opacity: 0.7, marginBottom: '4px', fontWeight: 'bold' }}>
                                        {m.sender_name}
                                    </div>
                                )}
                                {m.msg_type === 'text' && (
                                    <div
                                        style={{ whiteSpace: 'pre-wrap', userSelect: 'text' }}
                                        onContextMenu={(e) => {
                                            e.preventDefault();
                                            setContextMenu({
                                                x: e.clientX,
                                                y: e.clientY,
                                                content: m.content,
                                                type: 'text'
                                            });
                                        }}
                                    >{renderTextWithLinks(m.content)}</div>
                                )}
                                {m.msg_type === 'image' && (
                                    <>
                                        <img
                                            src={
                                                m.content.startsWith('data:') ? m.content :
                                                    m.file_path ? convertFileSrc(m.file_path) :
                                                        m.content.startsWith('/download/') ? (localIp && actualPort ? `http://${localIp}:${actualPort}${m.content}` : m.content) :
                                                            convertFileSrc(m.content)
                                            }
                                            className="wt-img-preview"
                                            loading="lazy"
                                            style={{ cursor: 'pointer' }}
                                            alt="Image"
                                            onClick={async () => {
                                                await invoke('open_content', {
                                                    id: m.id,
                                                    content: m.file_path || m.content,
                                                    contentType: 'image'
                                                });
                                            }}
                                            onError={(e) => {
                                                const target = e.currentTarget;
                                                const filePath = m.file_path || m.content;
                                                if (!filePath || filePath.startsWith('data:') || target.getAttribute('data-tried-fallback') === 'true') return;

                                                target.setAttribute('data-tried-fallback', 'true');
                                                // Try to get a server token URL as fallback
                                                invoke<string>('get_download_url', { filePath }).then((url) => {
                                                    if (url && localIp && actualPort) {
                                                        const fullUrl = `http://${localIp}:${actualPort}${url}`;
                                                        // Force react to re-render with new src or direct DOM manipulation?
                                                        // Direct manipulation is faster for error handling
                                                        target.src = fullUrl;
                                                        // Also update message object to avoid flicker on rerender
                                                        m._fallbackSrc = fullUrl;
                                                    }
                                                }).catch(err => console.error("Fallback image load failed", err));
                                            }}
                                            onContextMenu={(e) => {
                                                e.preventDefault();
                                                setContextMenu({
                                                    x: e.clientX,
                                                    y: e.clientY,
                                                    // For received images, m.content is often the file path
                                                    filePath: m.file_path || m.content,
                                                    content: m.content,
                                                    id: m.id,
                                                    type: 'image'
                                                });
                                            }}
                                        />
                                        <div style={{ fontSize: '11px', opacity: 0.7, display: 'flex', alignItems: 'center', gap: '4px', marginTop: '4px' }}>
                                            <ImageIcon size={12} />
                                            <span>Image</span>
                                        </div>
                                    </>
                                )}
                                {m.msg_type === 'video' && (
                                    <>
                                        <video
                                            src={
                                                m.file_path ? convertFileSrc(m.file_path) :
                                                    m.content.startsWith('/download/') ? (localIp && actualPort ? `http://${localIp}:${actualPort}${m.content}` : m.content) :
                                                        convertFileSrc(m.content)
                                            }
                                            className="wt-video-preview"
                                            controls
                                            style={{ maxWidth: '100%', borderRadius: '8px', maxHeight: '300px' }}
                                            onContextMenu={(e) => {
                                                e.preventDefault();
                                                setContextMenu({
                                                    x: e.clientX,
                                                    y: e.clientY,
                                                    // For received videos, m.content is often the file path
                                                    filePath: m.file_path || m.content,
                                                    content: m.content,
                                                    id: m.id,
                                                    type: 'video'
                                                });
                                            }}
                                        />
                                        <div style={{ fontSize: '11px', opacity: 0.7, display: 'flex', alignItems: 'center', gap: '4px', marginTop: '4px' }}>
                                            <Video size={12} />
                                            <span>Video</span>
                                        </div>
                                    </>
                                )}
                                {(m.msg_type === 'file' || (m.msg_type !== 'text' && m.msg_type !== 'image' && m.msg_type !== 'video')) && (
                                    <>
                                        <div className="wt-file-card"
                                            style={{ cursor: m.direction === 'in' && !m._preparing ? 'pointer' : 'default' }}
                                            onClick={async () => {
                                                if (m.direction === 'in' && !m._preparing) {
                                                    try {
                                                        const targetPath = m.file_path || m.content;
                                                        await invoke('open_content', {
                                                            id: m.id,
                                                            content: targetPath,
                                                            contentType: 'file'
                                                        });
                                                    } catch (e) {
                                                        console.error("Failed to open file", e);
                                                    }
                                                }
                                            }}

                                            onContextMenu={(e) => {
                                                if (!m._preparing) {
                                                    e.preventDefault();
                                                    setContextMenu({
                                                        x: e.clientX,
                                                        y: e.clientY,
                                                        filePath: m.file_path || m.content,
                                                        content: m.content,
                                                        id: m.id,
                                                        type: 'file'
                                                    });
                                                }
                                            }}
                                        >
                                            <div className="wt-file-icon">{m._preparing ? '📤' : (m.direction === 'in' ? '✅' : '📄')}</div>
                                            <div className="wt-file-info">
                                                <div className="wt-file-name">
                                                    {m._preparing
                                                        ? m._fileName
                                                        : (m.content.includes("name=")
                                                            ? decodeURIComponent(m.content.split("name=")[1])
                                                            : (m.direction === 'in' && m.content.includes('\\')
                                                                ? m.content.split(/[/\\]/).pop()
                                                                : "File Transfer"))}
                                                </div>
                                                {!m._preparing && (
                                                    <div style={{ fontSize: '10px', opacity: 0.7 }}>
                                                        {m.direction === 'in' ? 'Saved - Click to open' : 'Ready for download'}
                                                    </div>
                                                )}
                                            </div>
                                        </div>
                                        {m._preparing && (
                                            <div className="progress-wrapper">
                                                <div className="progress-container">
                                                    <div className="progress-bar" style={{ width: '100%', animation: 'pulse 1.5s ease-in-out infinite' }}></div>
                                                </div>
                                                <div className="status-text">
                                                    <span className="status-label">Preparing...</span>
                                                    <span className="percent"></span>
                                                </div>
                                            </div>
                                        )}
                                    </>
                                )}
                            </div>
                        </div>
                    );
                })}
                <div ref={messagesEndRef} />
            </div>

            <div className="wt-footer">
                <div className="wt-composer">
                    <button
                        className="wt-btn wt-btn-add"
                        title="Send File"
                        onClick={async () => {
                            try {
                                const selected = await open({
                                    multiple: true
                                });

                                if (selected) {
                                    const paths = Array.isArray(selected) ? selected : [selected];
                                    const tempMessages: FileTransferMessage[] = [];
                                    paths.forEach(path => {
                                        const fileName = path.split(/[/\\]/).pop() || 'File';
                                        const tempId = Date.now() + Math.random();
                                        tempMessages.push({
                                            id: tempId,
                                            direction: 'out',
                                            msg_type: 'file',
                                            content: 'Preparing...',
                                            timestamp: Date.now(),
                                            _fileName: fileName,
                                            _preparing: true
                                        });
                                    });

                                    setMessages(prev => [...prev, ...tempMessages]);
                                    setTimeout(() => messagesEndRef.current?.scrollIntoView({ behavior: "smooth" }), 100);

                                    const sendPromises = paths.map(path =>
                                        invoke("send_file_to_client", { filePath: path })
                                    );
                                    await Promise.all(sendPromises);
                                    setTimeout(fetchMessages, 300);
                                }
                            } catch (e) {
                                console.error(e);
                            }
                        }}
                    >
                        <Plus size={16} />
                    </button>

                    <div className="wt-input-wrap">
                        <textarea
                            ref={textareaRef}
                            className="wt-input"
                            value={input}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                            onChange={e => setInput(e.target.value)}
                            onKeyDown={handleKeyDown}
                            onPaste={handlePaste}
                            placeholder={t ? (t('type_message') || "Type a message...") : "Type..."}
                            rows={1}
                            style={{
                                resize: 'none',
                                maxHeight: '120px',
                                overflowY: 'hidden'
                            }}
                        />
                        {showExpandBtn && (
                            <button
                                className="wt-expand-btn"
                                onClick={() => setShowFullScreen(true)}
                                title="Full Screen Edit"
                            >
                                <Maximize2 size={14} />
                            </button>
                        )}
                    </div>

                    <button onClick={send} className="wt-btn send">
                        SEND
                    </button>
                </div>
            </div>

            <AnimatePresence>
                {showFullScreen && (
                    <motion.div
                        initial={{ opacity: 0, scale: 0.95 }}
                        animate={{ opacity: 1, scale: 1 }}
                        exit={{ opacity: 0, scale: 0.95 }}
                        className="wt-fullscreen-editor"
                        style={{
                            position: 'fixed',
                            top: 0, left: 0, right: 0, bottom: 0,
                            background: 'var(--bg-body)',
                            zIndex: 2000,
                            padding: '20px',
                            display: 'flex',
                            flexDirection: 'column',
                            gap: '16px'
                        }}
                    >
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                            <div style={{ fontWeight: 900, fontSize: '14px' }}>FULL SCREEN EDIT</div>
                            <button
                                onClick={() => setShowFullScreen(false)}
                                style={{
                                    background: 'transparent',
                                    border: 'none',
                                    cursor: 'pointer',
                                    color: 'var(--text-primary)'
                                }}
                            >
                                <Minimize2 size={20} />
                            </button>
                        </div>

                        <textarea
                            value={input}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                            onChange={e => setInput(e.target.value)}
                            placeholder="Type your message..."
                            style={{
                                flex: 1,
                                width: '100%',
                                background: 'var(--bg-input)',
                                color: 'var(--text-primary)',
                                border: '2px solid var(--border-dark)',
                                padding: '16px',
                                fontSize: '16px',
                                fontFamily: 'var(--font-mono)',
                                resize: 'none',
                                outline: 'none',
                                borderRadius: 'var(--radius-panel)'
                            }}
                            onPaste={handlePaste}
                        />

                        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '12px' }}>
                            <button
                                onClick={() => setShowFullScreen(false)}
                                className="wt-btn"
                                style={{ width: 'auto', padding: '0 20px', fontWeight: 'bold' }}
                            >
                                CANCEL
                            </button>
                            <button
                                onClick={send}
                                className="wt-btn send"
                                style={{ width: 'auto', padding: '0 24px' }}
                            >
                                SEND
                            </button>
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>

            {/* Context Menu */}
            {
                contextMenu && (
                    <motion.div
                        className="wt-context-menu"
                        initial={{ opacity: 0, scale: 0.98, y: -5 }}
                        animate={{ opacity: 1, scale: 1, y: 0 }}
                        style={{
                            top: contextMenu.y,
                            left: contextMenu.x,
                        }}
                        onClick={(e) => e.stopPropagation()}
                    >
                        {/* Custom Actions (Open / Explorer) */}
                        {(contextMenu.type === 'file' || contextMenu.type === 'image' || contextMenu.type === 'video') && (
                            <>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        await invoke('open_content', {
                                            id: contextMenu.id || 0,
                                            content: contextMenu.filePath,
                                            contentType: contextMenu.type || 'file'
                                        });
                                        setContextMenu(null);
                                    }}
                                >
                                    <ExternalLink size={14} />
                                    <span>{t ? (t('open') || '打开') : '打开'}</span>
                                </div>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        await invoke('open_file_location', { filePath: contextMenu.filePath });
                                        setContextMenu(null);
                                    }}
                                >
                                    <Folder size={14} />
                                    <span>{t ? (t('show_in_explorer') || '使用文件资源管理器打开') : '使用文件资源管理器打开'}</span>
                                </div>
                                <div style={{ height: '1px', background: 'var(--border-dark)', margin: '4px 8px', opacity: 0.3 }} />
                            </>
                        )}

                        {/* Standard Actions (Copy / Save As) */}
                        {contextMenu.type === 'image' && (
                            <>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        const target = await open({
                                            save: true,
                                            defaultPath: contextMenu.filePath?.split(/[/\\]/).pop() || 'image.png',
                                            filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }]
                                        });
                                        if (target && contextMenu.filePath) {
                                            await invoke('save_file_copy', { sourcePath: contextMenu.filePath, targetPath: target });
                                        }
                                        setContextMenu(null);
                                    }}
                                >
                                    <RotateCcw size={14} style={{ transform: 'rotate(90deg)' }} />
                                    <span>{t ? (t('save_image_as') || '将图像另存为...') : '将图像另存为...'}</span>
                                </div>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        if (contextMenu.filePath) {
                                            await invoke('copy_to_clipboard', { content: contextMenu.filePath, contentType: 'image', paste: false, id: 0, deleteAfterUse: false });
                                        }
                                        setContextMenu(null);
                                    }}
                                >
                                    <ImageIcon size={14} />
                                    <span>{t ? (t('copy_image') || '复制图像') : '复制图像'}</span>
                                </div>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        if (contextMenu.filePath && contextMenu.filePath.startsWith('http')) {
                                            await navigator.clipboard.writeText(contextMenu.filePath);
                                        } else if (contextMenu.content) {
                                            await navigator.clipboard.writeText(contextMenu.content);
                                        }
                                        setContextMenu(null);
                                    }}
                                >
                                    <LinkIcon size={14} />
                                    <span>{t ? (t('copy_image_link') || '复制图像链接') : '复制图像链接'}</span>
                                </div>
                            </>
                        )}

                        {contextMenu.type === 'video' && (
                            <>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        const target = await open({
                                            save: true,
                                            defaultPath: contextMenu.filePath?.split(/[/\\]/).pop() || 'video.mp4',
                                            filters: [{ name: 'Videos', extensions: ['mp4', 'mov', 'avi', 'mkv', 'webm'] }]
                                        });
                                        if (target && contextMenu.filePath) {
                                            await invoke('save_file_copy', { sourcePath: contextMenu.filePath, targetPath: target });
                                        }
                                        setContextMenu(null);
                                    }}
                                >
                                    <RotateCcw size={14} style={{ transform: 'rotate(90deg)' }} />
                                    <span>{t ? (t('save_video_as') || '将视频另存为...') : '将视频另存为...'}</span>
                                </div>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        if (contextMenu.filePath) {
                                            await invoke('copy_to_clipboard', { content: contextMenu.filePath, contentType: 'video', paste: false, id: 0, deleteAfterUse: false });
                                        }
                                        setContextMenu(null);
                                    }}
                                >
                                    <Video size={14} />
                                    <span>{t ? (t('copy_video') || '复制视频') : '复制视频'}</span>
                                </div>
                                <div
                                    className="context-item"
                                    onClick={async () => {
                                        if (contextMenu.filePath && contextMenu.filePath.startsWith('http')) {
                                            await navigator.clipboard.writeText(contextMenu.filePath);
                                        } else if (contextMenu.filePath) {
                                            await navigator.clipboard.writeText(contextMenu.filePath);
                                        }
                                        setContextMenu(null);
                                    }}
                                >
                                    <LinkIcon size={14} />
                                    <span>{t ? (t('copy_video_link') || '复制视频链接') : '复制视频链接'}</span>
                                </div>
                            </>
                        )}

                        {contextMenu.type === 'text' && (
                            <div
                                className="context-item"
                                onClick={async () => {
                                    if (contextMenu.content) {
                                        await navigator.clipboard.writeText(contextMenu.content);
                                    }
                                    setContextMenu(null);
                                }}
                            >
                                <Clipboard size={14} />
                                <span>{t ? (t('copy_text') || '复制文本') : '复制文本'}</span>
                            </div>
                        )}

                        {contextMenu.type === 'file' && (
                            <div
                                className="context-item"
                                onClick={async () => {
                                    if (contextMenu.content) {
                                        await navigator.clipboard.writeText(contextMenu.content);
                                    }
                                    setContextMenu(null);
                                }}
                            >
                                <LinkIcon size={14} />
                                <span>{t ? (t('copy_link') || '复制链接') : '复制链接'}</span>
                            </div>
                        )}
                    </motion.div>
                )
            }
        </div >
    );
};

export default FileTransferChatView;
