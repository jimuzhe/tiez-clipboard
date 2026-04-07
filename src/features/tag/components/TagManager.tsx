import { useState, useEffect, useRef, useMemo } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen, emit } from '@tauri-apps/api/event';
import {
    Edit2, Trash2, X, ChevronRight, LayoutGrid, List,
    Clock, MousePointer2, ChevronLeft, Plus, Search, ExternalLink, CheckSquare, Copy
} from 'lucide-react';
import { getTagColor } from "../../../shared/lib/utils";
import type { ClipboardEntry } from "../../../shared/types";

interface TagManagerProps {
    t: (key: string) => string;
    theme: string;
}

interface TagInfo {
    name: string;
    count: number;
}

export default function TagManager({ t, theme }: TagManagerProps) {
    const TAG_MANAGER_VIEW_MODE_KEY = "tiez_tag_manager_view_mode";
    const [tags, setTags] = useState<TagInfo[]>([]);
    const [tagSearch, setTagSearch] = useState('');
    const [selectedTag, setSelectedTag] = useState<string | null>(null);
    const [tagItems, setTagItems] = useState<ClipboardEntry[]>([]);
    const [tagColors, setTagColors] = useState<Record<string, string>>({});
    const [editingTag, setEditingTag] = useState<string | null>(null);
    const [newTagName, setNewTagName] = useState('');
    const [loading, setLoading] = useState(false);
    const [viewMode, setViewMode] = useState<'list' | 'grid'>(() => {
        try {
            const saved = window.localStorage.getItem(TAG_MANAGER_VIEW_MODE_KEY);
            return saved === 'list' ? 'list' : 'grid';
        } catch {
            return 'grid';
        }
    });
    const [isDeleting, setIsDeleting] = useState(false);
    const [deleteConfirmation, setDeleteConfirmation] = useState<{ show: boolean, tagName: string | null }>({ show: false, tagName: null });
    const [itemDeleteConfirmation, setItemDeleteConfirmation] = useState<{ show: boolean, id: number | null }>({ show: false, id: null });
    const [isCollapsed, setIsCollapsed] = useState(false);
    const [sortBy, setSortBy] = useState<'time' | 'count'>('time');
    const [isCreatingItem, setIsCreatingItem] = useState(false);
    const [editingItem, setEditingItem] = useState<{ id: number, content: string } | null>(null);
    const [newItemContent, setNewItemContent] = useState('');
    const [sidebarWidth, setSidebarWidth] = useState(160);
    const [sidebarHeight, setSidebarHeight] = useState(180);
    const [isResizing, setIsResizing] = useState(false);
    const [isStacked, setIsStacked] = useState(false);
    const [isManageMode, setIsManageMode] = useState(false);
    const [selectedItemIds, setSelectedItemIds] = useState<Set<number>>(new Set());
    const containerRef = useRef<HTMLDivElement>(null);

    const selectedTagRef = useRef<string | null>(null);
    useEffect(() => { selectedTagRef.current = selectedTag; }, [selectedTag]);

    useEffect(() => {
        try {
            window.localStorage.setItem(TAG_MANAGER_VIEW_MODE_KEY, viewMode);
        } catch {
            // Ignore storage write failures and keep UI functional.
        }
    }, [viewMode]);

    useEffect(() => {
        let unlisteners: (() => void)[] = [];
        const setupListeners = async () => {
            const handleUpdate = () => {
                // Don't refresh if we're in the middle of a delete operation
                if (isDeleting) return;
                fetchTags();
                if (selectedTagRef.current) loadTagItems(selectedTagRef.current);
            };
            unlisteners.push(await listen('clipboard-changed', handleUpdate));
            unlisteners.push(await listen('clipboard-updated', handleUpdate));
            unlisteners.push(await listen('clipboard-removed', handleUpdate));
        };
        setupListeners();
        return () => unlisteners.forEach(f => f());
    }, [isDeleting]);

    useEffect(() => { fetchTags(); }, []);

    useEffect(() => {
        const mediaQuery = window.matchMedia("(max-width: 340px)");
        const updateLayoutMode = () => {
            setIsStacked(mediaQuery.matches);
        };

        updateLayoutMode();
        mediaQuery.addEventListener("change", updateLayoutMode);

        return () => mediaQuery.removeEventListener("change", updateLayoutMode);
    }, []);

    useEffect(() => {
        if (!isResizing) return;

        const handleMouseMove = (event: MouseEvent) => {
            const bounds = containerRef.current?.getBoundingClientRect();
            if (!bounds) return;
            if (isStacked) {
                const maxHeight = Math.max(140, bounds.height - 180);
                const nextHeight = Math.min(Math.max(event.clientY - bounds.top, 120), maxHeight);
                setSidebarHeight(nextHeight);
                return;
            }

            const dragPos = event.clientX - bounds.left;
            
            // Auto collapse threshold: 110px
            if (dragPos < 110) {
                if (!isCollapsed) setIsCollapsed(true);
                setSidebarWidth(48);
            } else {
                if (isCollapsed) setIsCollapsed(false);
                const nextWidth = Math.min(dragPos, 320);
                setSidebarWidth(nextWidth);
            }
        };

        const handleMouseUp = () => {
            setIsResizing(false);
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
        };

        document.body.style.cursor = isStacked ? "row-resize" : "col-resize";
        document.body.style.userSelect = "none";
        window.addEventListener("mousemove", handleMouseMove);
        window.addEventListener("mouseup", handleMouseUp);

        return () => {
            window.removeEventListener("mousemove", handleMouseMove);
            window.removeEventListener("mouseup", handleMouseUp);
            document.body.style.cursor = "";
            document.body.style.userSelect = "";
        };
    }, [isResizing, isStacked, isCollapsed]);

    const fetchTags = async () => {
        try {
            const [tagMap, colors] = await Promise.all([
                invoke<Record<string, number>>('get_all_tags_info'),
                invoke<Record<string, string>>('get_tag_colors')
            ]);

            const tagArray = Object.entries(tagMap).map(([name, count]) => ({ name, count }));
            tagArray.sort((a, b) => b.count - a.count);
            setTags(tagArray);
            setTagColors(colors || {});

            const activeTag = selectedTagRef.current;
            if (tagArray.length === 0) {
                setSelectedTag(null);
                setTagItems([]);
                return;
            }
            if (!activeTag || !tagArray.some(tag => tag.name === activeTag)) {
                loadTagItems(tagArray[0].name);
            }
        } catch (err) { console.error(err); }
    };

    const loadTagItems = async (tagName: string) => {
        setLoading(true);
        setSelectedTag(tagName);
        try {
            const items = await invoke<ClipboardEntry[]>('get_tag_items', { tag: tagName });
            setTagItems(items || []);
        } catch (err) { console.error(err); setTagItems([]); }
        finally { setLoading(false); }
    };

    const createTag = async (rawName: string) => {
        const trimmed = rawName.trim();
        if (!trimmed) return;

        try {
            await invoke('create_new_tag', { tagName: trimmed });
            setNewTagName('');
            setTagSearch('');
            await fetchTags();
            await loadTagItems(trimmed);
        } catch (err) { console.error(err); }
    };

    const handleRenameTag = async (oldName: string) => {
        const trimmed = newTagName.trim();
        if (!trimmed || trimmed === oldName) { setEditingTag(null); return; }

        if (oldName === 'sensitive' || oldName === '密码') {
            setEditingTag(null);
            return;
        }

        try {
            await invoke('rename_tag_globally', { oldName, newName: trimmed });
            if (selectedTag === oldName) setSelectedTag(trimmed);
            await fetchTags();
            await loadTagItems(trimmed);
            setEditingTag(null);
            setNewTagName('');
        } catch (err) { console.error(err); }
    };

    const handleDeleteTag = async (tagName: string) => {
        if (tagName === 'sensitive' || tagName === '密码') return;
        setIsDeleting(true);
        try {
            await invoke('delete_tag_from_all', { tagName });
            await emit('clipboard-changed'); // Notify App.tsx to refresh
            await fetchTags();
        } catch (err) { console.error(err); }
        finally {
            setIsDeleting(false);
        }
    };

    const handleAddManualItem = async () => {
        if (!newItemContent.trim() || !selectedTag) return;
        try {
            await invoke('add_manual_item', {
                content: newItemContent,
                contentType: 'text',
                tags: [selectedTag]
            });
            setNewItemContent('');
            setIsCreatingItem(false);
            await loadTagItems(selectedTag);
        } catch (err) { console.error(err); }
    };

    const handleUpdateItemContent = async () => {
        if (!editingItem || !editingItem.content.trim()) return;
        try {
            await invoke('update_item_content', {
                id: editingItem.id,
                newContent: editingItem.content
            });
            setEditingItem(null);
            if (selectedTag) await loadTagItems(selectedTag);
        } catch (err) { console.error(err); }
    };

    const copyToClipboard = async (id: number, content: string, type: string) => {
        try {
            await invoke('copy_to_clipboard', { content, contentType: type, paste: true, id, deleteAfterUse: false });
        } catch (err) { console.error(err); }
    };

    const filteredTags = useMemo(() => {
        return tags.filter(t => t.name.toLowerCase().includes(tagSearch.toLowerCase()));
    }, [tags, tagSearch]);

    const normalizedTagSearch = tagSearch.trim().toLowerCase();
    const canCreateTag = normalizedTagSearch.length > 0
        && !tags.some(tag => tag.name.toLowerCase() === normalizedTagSearch);

    const sortedItems = [...tagItems].sort((a, b) => {
        if (sortBy === 'count') return (b.use_count || 0) - (a.use_count || 0);
        return b.timestamp - a.timestamp;
    });

    const formatItemDate = (timestamp: number) => {
        const date = new Date(timestamp);
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, '0');
        const day = String(date.getDate()).padStart(2, '0');
        return `${year}-${month}-${day}`;
    };

    return (
        <div
            ref={containerRef}
            className={`themed-tag-manager theme-${theme} ${isCollapsed ? 'sidebar-collapsed' : ''} ${isStacked ? 'stacked-layout' : ''}`}
            style={{ 
                ["--tag-sidebar-width" as any]: isCollapsed ? '48px' : `${sidebarWidth}px`,
                ["--tm-sidebar-height" as any]: `${sidebarHeight}px`
            } as any}
            onMouseDown={() => invoke('activate_window_focus').catch(console.error)}
        >
            {/* Sidebar with CRUD support */}
            {/* Sidebar with Unified Search & Create */}
            <div className="tag-sidebar">
                <div className="sidebar-header">
                    {!isCollapsed && <span className="header-label">{t('tags')}</span>}
                    <button
                        className="collapse-toggle"
                        title={isCollapsed ? (t('open') || '展开') : (t('collapse') || '收起')}
                        onClick={() => {
                            const newCollapsed = !isCollapsed;
                            setIsCollapsed(newCollapsed);
                            if (!newCollapsed && sidebarWidth < 110) {
                                setSidebarWidth(160);
                            }
                        }}
                    >
                        {isCollapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
                    </button>
                </div>

                {!isCollapsed && (
                    <div className="tag-search-box">
                        <Search size={16} className="search-icon-placeholder" />
                        <input
                            placeholder={t('find_or_create')}
                            value={tagSearch}
                            onMouseDown={() => invoke('activate_window_focus').catch(console.error)}
                            onFocus={() => invoke('activate_window_focus').catch(console.error)}
                            onChange={e => setTagSearch(e.target.value)}
                            onKeyDown={async (e) => {
                                if (e.key === 'Enter' && tagSearch.trim()) {
                                    // If exact match exists, select it. If not, create new.
                                    const exactMatch = tags.find(t => t.name.toLowerCase() === normalizedTagSearch);
                                    if (exactMatch) {
                                        loadTagItems(exactMatch.name);
                                    } else {
                                        await createTag(tagSearch);
                                    }
                                }
                            }}
                        />
                        {tagSearch ? (
                            <div className="action-icons">
                                { /* If no exact match, show Plus to indicate creation */}
                                {canCreateTag ? (
                                    <span
                                        title={t('create_new_tag_tooltip')}
                                        className="action-icon create"
                                        onClick={() => createTag(tagSearch)}
                                    >
                                        <Plus size={12} />
                                    </span>
                                ) : null}
                                <X size={12} className="action-icon clear" onClick={() => setTagSearch('')} />
                            </div>
                        ) : null}
                    </div>
                )}

                <div className="tag-scroll custom-scrollbar">
                    {filteredTags.map(tag => (
                        <div
                            key={tag.name}
                            className={`tag-item ${selectedTag === tag.name ? 'active' : ''}`}
                            onClick={() => loadTagItems(tag.name)}
                            title={tag.name}
                        >
                            <div className="tag-color-wrapper" onClick={(e) => e.stopPropagation()}>
                                <div
                                    className="tag-color-dot"
                                    style={{ background: tagColors[tag.name] || getTagColor(tag.name, theme) }}
                                    onClick={() => document.getElementById(`color-picker-${tag.name}`)?.click()}
                                />
                                <input
                                    type="color"
                                    id={`color-picker-${tag.name}`}
                                    style={{ display: 'none' }}
                                    value={tagColors[tag.name] || '#888888'} // Approximation if not set, or maybe convert HSL to Hex?
                                    onChange={async (e) => {
                                        const newColor = e.target.value;
                                        setTagColors(prev => ({ ...prev, [tag.name]: newColor }));
                                        await invoke('set_tag_color', { name: tag.name, color: newColor });
                                        await emit('tag-colors-updated');
                                    }}
                                />
                            </div>
                            {editingTag === tag.name ? (
                                <input
                                    className="inline-tag-edit"
                                    value={newTagName}
                                    onMouseDown={() => invoke('activate_window_focus').catch(console.error)}
                                    onFocus={() => invoke('activate_window_focus').catch(console.error)}
                                    onChange={(e) => setNewTagName(e.target.value)}
                                    autoFocus
                                    onKeyDown={async (e) => {
                                        if (e.key === 'Enter') {
                                            await handleRenameTag(tag.name);
                                        } else if (e.key === 'Escape') {
                                            setEditingTag(null);
                                        }
                                    }}
                                    onBlur={() => setEditingTag(null)}
                                    onClick={(e) => e.stopPropagation()}
                                />
                            ) : (
                                <>
                                    <span className="tag-name">{tag.name}</span>
                                    <div className="tag-hover-actions">
                                        {(tag.name !== 'sensitive' && tag.name !== '密码') && (
                                            <span title={t('rename')} onClick={(e) => {
                                                e.stopPropagation();
                                                setEditingTag(tag.name);
                                                setNewTagName(tag.name);
                                            }} style={{
                                                display: 'flex',
                                                alignItems: 'center',
                                                cursor: 'pointer'
                                            }}>
                                                <Edit2 size={12} />
                                            </span>
                                        )}
                                        {(tag.name !== 'sensitive' && tag.name !== '密码') && (
                                            <span title={t('delete')} onClick={(e) => {
                                                e.stopPropagation();
                                                e.preventDefault();
                                                setDeleteConfirmation({ show: true, tagName: tag.name });
                                            }} style={{ display: 'flex', alignItems: 'center', cursor: 'pointer' }}>
                                                <Trash2 size={12} />
                                            </span>
                                        )}
                                    </div>
                                    <span className="tag-badge">{tag.count}</span>
                                </>
                            )}
                        </div>
                    ))}
                    {filteredTags.length === 0 && !tagSearch.trim() && (
                        <div className="sidebar-status">{t('no_tags')}</div>
                    )}
                    {/* Visual cue for creating new tag when filtering shows no results */}
                    {!isCollapsed && canCreateTag && filteredTags.length === 0 && (
                        <div className="tag-item create-hint" onClick={() => createTag(tagSearch)}>
                            <div className="tag-color-dot" style={{ border: '1px dashed currentColor', background: 'transparent' }} />
                            <span className="tag-name" style={{ opacity: 0.7 }}>{t('create_tag_hint').replace('{tag}', tagSearch.trim())}</span>
                            <Plus size={10} />
                        </div>
                    )}
                </div>
            </div>

            {!isCollapsed && (
                <div 
                    className={`tag-divider ${isResizing ? 'active' : ''} ${isStacked ? 'stacked' : ''}`}
                    onMouseDown={(e) => {
                        e.preventDefault();
                        setIsResizing(true);
                    }}
                >
                    <div className="tag-divider-handle" />
                </div>
            )}

            {/* Right Main Area */}
            <div className="tag-content">
                <div className="content-toolbar">
                    <div className="toolbar-left">
                        <div className="selected-tag-indicator">
                            <span className="breadcrumb-marker">#</span>
                            <span className="breadcrumb-text">{selectedTag || t('tags')}</span>
                        </div>
                        <div className="toolbar-divider" />
                        <div className="sort-group">
                            <button
                                className={`sort-btn ${sortBy === 'time' ? 'active' : ''}`}
                                title={t('sort_time') || '按时间'}
                                onClick={() => setSortBy('time')}
                            >
                                <Clock size={12} />
                                <span>{t('sort_time') || '时间'}</span>
                            </button>
                            <button
                                className={`sort-btn ${sortBy === 'count' ? 'active' : ''}`}
                                title={t('sort_usage') || '按频率'}
                                onClick={() => setSortBy('count')}
                            >
                                <MousePointer2 size={12} />
                                <span>{t('sort_usage') || '频率'}</span>
                            </button>
                        </div>
                    </div>
                    <div className="toolbar-right">
                        {selectedTag && (
                            <div className="toolbar-actions">
                                {isManageMode ? (
                                    <>
                                        <button
                                            className="sort-btn"
                                            onClick={() => {
                                                setIsManageMode(false);
                                                setSelectedItemIds(new Set());
                                            }}
                                        >
                                            {t('cancel') || '取消'}
                                        </button>
                                        <button
                                            className="sort-btn danger"
                                            disabled={selectedItemIds.size === 0}
                                            onClick={() => setItemDeleteConfirmation({ show: true, id: -1 })}
                                        >
                                            <Trash2 size={14} />
                                            <span>{t('delete_selected') || '删除选中'}</span>
                                        </button>
                                        <button
                                            className="sort-btn active"
                                            disabled={selectedItemIds.size === 0}
                                            onClick={async () => {
                                                const selectedItems = tagItems.filter(item => selectedItemIds.has(item.id));
                                                if (selectedItems.length > 0) {
                                                    const combinedContent = selectedItems.map(item => item.content).join('\n');
                                                    await invoke('copy_to_clipboard', {
                                                        content: combinedContent,
                                                        contentType: 'text',
                                                        paste: true,
                                                        id: -1,
                                                        deleteAfterUse: false
                                                    });
                                                    setIsManageMode(false);
                                                    setSelectedItemIds(new Set());
                                                }
                                            }}
                                        >
                                            <Copy size={14} />
                                            <span>{t('copy_selected') || '复制选中'}</span>
                                        </button>
                                    </>
                                ) : (
                                    <>
                                        <button
                                            className={`sort-btn manage-btn ${isManageMode ? 'active' : ''}`}
                                            onClick={() => setIsManageMode(true)}
                                            title={t('manage_items') || '管理条目'}
                                        >
                                            <CheckSquare size={14} />
                                            <span>{t('manage') || '管理'}</span>
                                        </button>
                                    </>
                                )}
                            </div>
                        )}
                    <div className="view-toggle">
                        <button
                            type="button"
                            className={`toggle-btn btn-icon ${viewMode === 'list' ? 'active' : ''}`}
                            title={t('list_view')}
                            onClick={() => setViewMode('list')}
                        ><List size={14} /></button>
                        <button
                            type="button"
                            className={`toggle-btn btn-icon ${viewMode === 'grid' ? 'active' : ''}`}
                            title={t('grid_view')}
                            onClick={() => setViewMode('grid')}
                        ><LayoutGrid size={14} /></button>
                    </div>
                    </div>
                </div>

                <div className="items-area custom-scrollbar">
                    {loading ? <div className="status-msg">{t('processing')}</div> : sortedItems.length === 0 ? (
                        <div className="status-msg">{selectedTag ? t('no_items') : t('select_tag_to_begin')}</div>
                    ) : (
                        <div className={`items-${viewMode} ${isManageMode ? 'manage-mode' : ''}`}>
                            {sortedItems.map(item => (
                                <div
                                    key={item.id}
                                    className={`themed-card ${selectedItemIds.has(item.id) ? 'selected' : ''}`}
                                    onClick={() => {
                                        if (isManageMode) {
                                            setSelectedItemIds(prev => {
                                                const next = new Set(prev);
                                                if (next.has(item.id)) next.delete(item.id);
                                                else next.add(item.id);
                                                return next;
                                            });
                                        } else {
                                            copyToClipboard(item.id, item.content, item.content_type);
                                        }
                                    }}
                                >
                                    <div className="card-top-row">
                                        <div className="card-actions-left">
                                            {isManageMode ? (
                                                <div className={`selection-indicator ${selectedItemIds.has(item.id) ? 'checked' : ''}`}>
                                                    <div className="inner-check" />
                                                </div>
                                            ) : (
                                                <>
                                                    {(item.content_type === 'text' || item.content_type === 'code') && (
                                                        <button className="card-action-btn" title="编辑" onClick={(e) => {
                                                            e.stopPropagation();
                                                            setEditingItem({ id: item.id, content: item.content });
                                                        }}>
                                                            <Edit2 size={10} />
                                                        </button>
                                                    )}
                                                    <button
                                                        className="card-action-btn"
                                                        onClick={(e) => {
                                                            e.stopPropagation();
                                                            invoke('open_content', {
                                                                id: item.id,
                                                                content: item.content,
                                                                contentType: item.content_type
                                                            });
                                                        }}
                                                        title={t('open')}
                                                    >
                                                        <ExternalLink size={10} />
                                                    </button>
                                                </>
                                            )}
                                        </div>
                                        {!isManageMode && (
                                            <button className="del-btn" title="删除" onClick={(e) => {
                                                e.stopPropagation();
                                                setItemDeleteConfirmation({ show: true, id: item.id });
                                            }}>
                                                <X size={10} />
                                            </button>
                                        )}
                                    </div>

                                    {item.content_type === 'image' ? (
                                        <div className="card-media">
                                            <img
                                                src={item.content.startsWith('data:') ? item.content : convertFileSrc(item.content)}
                                                alt=""
                                                className="image-preview"
                                                loading="lazy"
                                            />
                                        </div>
                                    ) : (
                                        <div className="card-body-text">{item.preview || item.content}</div>
                                    )}

                                    <div className="card-divider" />
                                    <div className="card-footer">
                                        <span className="meta-time">{formatItemDate(item.timestamp)}</span>
                                        <div className="meta-usage"><MousePointer2 size={8} /> {item.use_count || 0}</div>
                                    </div>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
                {selectedTag && !isManageMode && (
                    <button
                        className="fab-add-btn"
                        onClick={(e) => {
                            e.stopPropagation();
                            setIsCreatingItem(true);
                        }}
                        title={t('add_item')}
                    >
                        <Plus size={24} />
                    </button>
                )}
            </div>

            {/* Modals for Create (Rename is handled inline now) */}
            {/* Kept minimal if needed for future extensions, but currently inline handles rename */}

            {/* Tag Delete Confirmation Modal */}
            {deleteConfirmation.show && (
                <div className="modal-overlay" onClick={() => setDeleteConfirmation({ show: false, tagName: null })}>
                    <div className={`confirm-dialog tag-manager-dialog theme-${theme}`} onClick={(e) => e.stopPropagation()}>
                        <h3>{t('confirm_delete')}</h3>
                        <p>
                            {t('confirm_delete_tag')}
                            <br />
                            <span className="tag-highlight" style={{ marginTop: '8px', display: 'inline-block' }}>
                                {deleteConfirmation.tagName}
                            </span>
                        </p>
                        <div className="confirm-dialog-buttons">
                            <button className="confirm-dialog-button" onClick={() => setDeleteConfirmation({ show: false, tagName: null })}>
                                {t('cancel')}
                            </button>
                            <button className="confirm-dialog-button primary" onClick={() => {
                                if (deleteConfirmation.tagName) {
                                    handleDeleteTag(deleteConfirmation.tagName);
                                }
                                setDeleteConfirmation({ show: false, tagName: null });
                            }}>
                                {t('delete')}
                            </button>
                        </div>
                    </div>
                </div>
            )}

            {/* Item Delete Confirmation Modal */}
            {itemDeleteConfirmation.show && (
                <div className="modal-overlay" onClick={() => setItemDeleteConfirmation({ show: false, id: null })}>
                    <div className={`confirm-dialog tag-manager-dialog theme-${theme}`} onClick={e => e.stopPropagation()}>
                        <h3>{t('confirm_delete')}</h3>
                        <p>{t('confirm_delete_desc') || "确定要删除这条记录吗？"}</p>
                        <div className="confirm-dialog-buttons">
                            <button className="confirm-dialog-button" onClick={() => setItemDeleteConfirmation({ show: false, id: null })}>
                                {t('cancel')}
                            </button>
                            <button className="confirm-dialog-button primary" onClick={async () => {
                                if (itemDeleteConfirmation.id === -1) {
                                    // Bulk delete
                                    try {
                                        for (const id of Array.from(selectedItemIds)) {
                                            await invoke('delete_clipboard_entry', { id });
                                        }
                                        setIsManageMode(false);
                                        setSelectedItemIds(new Set());
                                        if (selectedTag) await loadTagItems(selectedTag);
                                        emit('clipboard-changed');
                                    } catch (err) { console.error(err); }
                                } else if (itemDeleteConfirmation.id) {
                                    await invoke('delete_clipboard_entry', { id: itemDeleteConfirmation.id });
                                    loadTagItems(selectedTag!);
                                    emit('clipboard-changed');
                                }
                                setItemDeleteConfirmation({ show: false, id: null });
                            }}>
                                {t('delete')}
                            </button>
                        </div>
                    </div>
                </div>
            )}

            {/* Create Item Modal */}
            {isCreatingItem && (
                <div className="modal-overlay" onClick={() => setIsCreatingItem(false)}>
                    <div className={`confirm-dialog tag-manager-dialog theme-${theme}`} onClick={e => e.stopPropagation()}>
                        <h3>{t('add_item')}</h3>
                        <div className="modal-input-field">
                            <textarea
                                className="tag-manager-textarea"
                                value={newItemContent}
                                onChange={e => setNewItemContent(e.target.value)}
                                placeholder={t('input_content_placeholder')}
                                autoFocus
                            />
                        </div>
                        <div className="confirm-dialog-buttons">
                            <button className="confirm-dialog-button" onClick={() => setIsCreatingItem(false)}>
                                {t('cancel')}
                            </button>
                            <button className="confirm-dialog-button primary" onClick={handleAddManualItem}>
                                {t('confirm')}
                            </button>
                        </div>
                    </div>
                </div>
            )}

            {/* Edit Item Modal */}
            {editingItem && (
                <div className="modal-overlay" onClick={() => setEditingItem(null)}>
                    <div className={`confirm-dialog tag-manager-dialog theme-${theme}`} onClick={e => e.stopPropagation()}>
                        <h3>{t('edit_item')}</h3>
                        <div className="modal-input-field">
                            <textarea
                                className="tag-manager-textarea"
                                value={editingItem.content}
                                onChange={e => setEditingItem({ ...editingItem, content: e.target.value })}
                                autoFocus
                            />
                        </div>
                        <div className="confirm-dialog-buttons">
                            <button className="confirm-dialog-button" onClick={() => setEditingItem(null)}>
                                {t('cancel')}
                            </button>
                            <button className="confirm-dialog-button primary" onClick={handleUpdateItemContent}>
                                {t('save')}
                            </button>
                        </div>
                    </div>
                </div>
            )}
            <style>{`
                .themed-tag-manager {
                    box-sizing: border-box;
                    display: grid;
                    grid-template-columns: var(--tag-sidebar-width, 130px) auto 1fr;
                    height: 100%;
                    background: var(--bg-content);
                    font-family: var(--font-main, ui-monospace, monospace);
                    color: var(--text-primary);
                    gap: 0;
                    padding: 0;
                }

                .themed-tag-manager * {
                    box-sizing: border-box;
                }

                /* Sidebar */
                .tag-sidebar {
                    width: var(--tag-sidebar-width, 130px);
                    flex-shrink: 0;
                    display: flex;
                    flex-direction: column;
                    background: var(--bg-panel);
                    border-radius: 0;
                    box-shadow: none;
                    overflow: hidden;
                    border-right: 1px solid var(--panel-divider-color);
                }
                .sidebar-collapsed .tag-sidebar { width: 48px; }
                
                .sidebar-header {
                    padding: 16px 14px;
                    border-bottom: 1px solid var(--panel-divider-color);
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    min-height: auto;
                    background: transparent;
                    color: var(--text-secondary);
                    font-size: 13px;
                    font-weight: 600;
                    text-transform: uppercase;
                    letter-spacing: 0.5px;
                }
                .collapse-toggle { 
                    background: var(--bg-main); 
                    border: none; 
                    color: inherit; 
                    cursor: pointer; 
                    display: flex; 
                    align-items: center;
                    justify-content: center;
                    width: 28px;
                    height: 28px;
                    border-radius: var(--radius-sm);
                    transition: all 0.2s;
                }
                .collapse-toggle:hover { background: var(--border-light); color: var(--text-primary); }

                /* Tag Search Box */
                .tag-search-box {
                    padding: 12px 10px;
                    display: flex; align-items: center; gap: 6px;
                    background: transparent;
                    border-bottom: 1px solid var(--panel-divider-color);
                    margin: 0;
                    min-height: auto;
                    position: relative;
                }
                .tag-search-box .search-icon-placeholder { 
                    position: absolute;
                    left: 22px;
                    opacity: 0.3; 
                    color: var(--text-primary); 
                    pointer-events: none;
                }
                .tag-search-box input {
                    width: 100%;
                    background: var(--bg-main); 
                    border: 1px solid var(--border); 
                    outline: none;
                    font-size: 13px; 
                    font-weight: 500; 
                    color: var(--text-primary);
                    padding: 8px 12px 8px 32px;
                    flex: 1;
                    min-width: 0; 
                    border-radius: var(--radius-sm);
                    transition: all 0.2s;
                }
                .tag-search-box input:focus {
                    border-color: var(--accent-color);
                    background: var(--bg-panel);
                }
                .tag-search-box input::placeholder { color: var(--text-muted); opacity: 0.7; font-style: normal; font-size: 13px; }
                
                .action-icons { display: flex; align-items: center; gap: 4px; margin-left: 4px; }
                .action-icon { cursor: pointer; opacity: 0.5; color: var(--text-primary); transition: all 0.15s; }
                .action-icon:hover { opacity: 1; transform: scale(1.1); }
                .action-icon.create { color: var(--accent-color); opacity: 0.8; }

                .tag-scroll { flex: 1; overflow-y: auto; padding: 8px 6px 8px 10px; overflow-x: hidden; }
                .tag-item {
                    display: flex; 
                    align-items: center; 
                    gap: 10px;
                    padding: 10px 12px; 
                    cursor: pointer;
                    margin-bottom: 4px; 
                    border: 1px solid transparent;
                    border-radius: var(--radius-sm);
                    transition: all 0.15s;
                    position: relative;
                    overflow: hidden;
                }
                .tag-item:hover { background: var(--bg-main); }
                .tag-item.active { 
                    background: var(--accent-light); 
                    color: var(--accent-color);
                }
                .tag-item.create-hint { border: 1px dashed var(--border); opacity: 0.8; }

                .sidebar-collapsed .tag-name,
                .sidebar-collapsed .tag-badge,
                .sidebar-collapsed .tag-hover-actions,
                .sidebar-collapsed .tag-search-box { display: none; }
                .sidebar-collapsed .tag-item { justify-content: center; }

                .tag-color-dot { 
                    width: 10px; 
                    height: 10px; 
                    border-radius: 50%; 
                    flex-shrink: 0; 
                    cursor: pointer; 
                }
                .tag-name { 
                    flex: 1; 
                    font-size: 13px; 
                    font-weight: 500; 
                    white-space: nowrap; 
                    overflow: hidden; 
                    text-overflow: ellipsis; 
                }
                
                .inline-tag-edit {
                    flex: 1; 
                    border: 1px solid var(--border); 
                    background: var(--bg-main); 
                    color: var(--text-primary); 
                    font-size: 12px; 
                    padding: 4px 8px; 
                    border-radius: var(--radius-sm);
                    min-width: 0; 
                    outline: none;
                }

                .tag-hover-actions { 
                    display: none; 
                    gap: 4px; 
                    align-items: center; 
                    margin-left: auto;
                }
                .tag-item:hover .tag-hover-actions { display: flex; }
                .tag-item:hover .tag-badge { display: none; }
                
                .tag-badge { 
                    font-size: 11px; 
                    color: var(--text-secondary); 
                    background: var(--bg-main); 
                    padding: 1px 6px; 
                    border-radius: 10px;
                }

                /* Content Area */
                .tag-content { flex: 1; display: flex; flex-direction: column; overflow: hidden; position: relative; }
                .content-toolbar {
                    height: 48px; border-bottom: 1px solid var(--panel-divider-color);
                    background: var(--bg-panel);
                    display: flex; align-items: center; justify-content: space-between; padding: 0 16px;
                }
                .toolbar-left { display: flex; align-items: center; gap: 12px; }
                .selected-tag-indicator { display: flex; align-items: center; gap: 6px; font-weight: 600; font-size: 14px; }
                .breadcrumb-marker { color: var(--accent-color); }
                .toolbar-divider { width: 1px; height: 16px; background: var(--panel-divider-color); }

                .sort-group { display: flex; gap: 4px; }
                .sort-btn { background: transparent; border: none; color: var(--text-secondary); cursor: pointer; display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-radius: var(--radius-sm); font-size: 12px; }
                .sort-btn:hover { background: var(--bg-main); }
                .sort-btn.active { color: var(--accent-color); font-weight: 600; }
                .sort-btn.danger { color: #ff4d4f; }

                .view-toggle {
                    display: flex;
                    gap: 2px;
                    background: var(--bg-main);
                    padding: 2px;
                    border-radius: var(--radius-sm);
                }
                .toggle-btn {
                    padding: 4px;
                    border-radius: var(--radius-sm);
                    cursor: pointer;
                    display: flex;
                    align-items: center;
                    border: none;
                    background: transparent;
                    color: var(--text-secondary);
                }
                .toggle-btn.active { background: var(--accent-color); color: white; }

                .items-area { 
                    flex: 1; 
                    overflow-y: auto; 
                    padding: 16px; 
                    background: var(--bg-content); 
                }

                .items-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); gap: 12px; }
                .items-list { display: flex; flex-direction: column; gap: 8px; }

                .themed-card {
                    background: var(--bg-panel);
                    border: 1px solid var(--border);
                    padding: 12px; cursor: pointer;
                    position: relative;
                    border-radius: var(--radius-md);
                    transition: all 0.2s;
                }
                .themed-card:hover { border-color: var(--accent-color); box-shadow: 0 4px 12px var(--shadow); }
                .themed-card.selected { border-color: var(--accent-color); background: var(--accent-light); }

                .card-top-row { display: flex; justify-content: space-between; align-items: center; visibility: hidden; margin-bottom: 8px; }
                .themed-card:hover .card-top-row, .manage-mode .card-top-row { visibility: visible; }
                
                .card-actions-left { display: flex; gap: 4px; }
                .card-action-btn, .del-btn { background: transparent; border: none; color: var(--text-secondary); cursor: pointer; padding: 2px; display: flex; align-items: center; }
                .card-action-btn:hover { color: var(--accent-color); }
                .del-btn:hover { color: #ff4d4f; }

                .selection-indicator { width: 16px; height: 16px; border: 1px solid var(--border); border-radius: 4px; position: relative; }
                .selection-indicator.checked { background: var(--accent-color); border-color: var(--accent-color); }
                .inner-check { position: absolute; left: 5px; top: 2px; width: 4px; height: 8px; border: solid white; border-width: 0 2px 2px 0; transform: rotate(45deg); display: none; }
                .selection-indicator.checked .inner-check { display: block; }

                .card-media img { max-width: 100%; max-height: 120px; border-radius: var(--radius-sm); object-fit: contain; }
                .card-body-text { font-size: 13px; line-height: 1.5; color: var(--text-primary); max-height: 4.5em; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; margin: 4px 0; }
                .card-divider { height: 1px; background: var(--panel-divider-color); margin: 8px 0; }
                .card-footer { display: flex; justify-content: space-between; font-size: 11px; color: var(--text-muted); }
                .meta-usage { display: flex; align-items: center; gap: 4px; }

                .fab-add-btn { position: absolute; bottom: 24px; right: 24px; width: 48px; height: 48px; border-radius: 50%; background: var(--accent-color); color: white; border: none; display: flex; align-items: center; justify-content: center; cursor: pointer; box-shadow: 0 4px 12px rgba(0,0,0,0.2); transition: transform 0.2s; z-index: 10; }
                .fab-add-btn:hover { transform: scale(1.1); }

                /* Modals */
                .modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.5); display: flex; align-items: center; justify-content: center; z-index: 1000; }
                .confirm-dialog { background: var(--bg-main); padding: 20px; border-radius: var(--radius-lg); width: 320px; box-shadow: 0 10px 25px rgba(0,0,0,0.2); }
                .tag-manager-dialog h3 { margin: 0 0 12px; font-size: 16px; }
                .modal-input-field { margin: 12px 0; }
                .tag-manager-textarea { width: 100%; height: 120px; background: var(--bg-panel); border: 1px solid var(--border); border-radius: var(--radius-md); padding: 8px; color: var(--text-primary); resize: none; outline: none; }
                .confirm-dialog-buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
                .confirm-dialog-button { padding: 6px 16px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: transparent; color: var(--text-primary); cursor: pointer; }
                .confirm-dialog-button.primary { background: var(--accent-color); color: white; border: none; }

                .tag-divider { width: 6px; cursor: col-resize; position: relative; z-index: 5; background: transparent; transition: background 0.2s; display: flex; align-items: center; justify-content: center; }
                .tag-divider:hover, .tag-divider.active { background: var(--accent-light); }
                .tag-divider-handle { width: 2px; height: 24px; background: var(--text-muted); opacity: 0.3; border-radius: 1px; }

                /* Custom Scrollbar */
                .custom-scrollbar::-webkit-scrollbar { width: 6px; height: 6px; }
                .custom-scrollbar::-webkit-scrollbar-thumb { background: var(--line-strong, rgba(128,128,128,0.3)); border-radius: 3px; }
                .custom-scrollbar::-webkit-scrollbar-track { background: transparent; }
                
                /* Theme Adaptations */
                .theme-mica .tag-sidebar, .theme-acrylic .tag-sidebar { border-radius: 12px; margin: 8px 4px 8px 8px; height: calc(100% - 16px); }
                .theme-mica .tag-content, .theme-acrylic .tag-content { border-radius: 12px; margin: 8px 8px 8px 4px; height: calc(100% - 16px); border: 1px solid var(--panel-divider-color); }

                /* Stacked Layout Support */
                .stacked-layout {
                    grid-template-columns: 1fr !important;
                    grid-template-rows: var(--tm-sidebar-height, 180px) auto 1fr;
                }
                .stacked-layout .tag-sidebar { width: 100%; border-right: none; border-bottom: 1px solid var(--panel-divider-color); }
                .stacked-layout .tag-divider { width: 100%; height: 6px; cursor: row-resize; }
                .stacked-layout .tag-divider-handle { width: 24px; height: 2px; }
                .stacked-layout .theme-mica .tag-sidebar, .stacked-layout .theme-acrylic .tag-sidebar { margin: 8px 8px 4px 8px; height: calc(var(--tm-sidebar-height, 180px) - 12px); width: calc(100% - 16px); }
                .stacked-layout .theme-mica .tag-content, .stacked-layout .theme-acrylic .tag-content { margin: 4px 8px 8px 8px; width: calc(100% - 16px); height: auto; }
            `}</style>
        </div >
    );
}
