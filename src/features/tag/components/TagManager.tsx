import { useState, useEffect, useRef, useMemo } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen, emit } from '@tauri-apps/api/event';
import {
    Edit2, Trash2, X, ChevronRight, LayoutGrid, List,
    Clock, MousePointer2, ChevronLeft, Plus, Search, ExternalLink
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
    const [tags, setTags] = useState<TagInfo[]>([]);
    const [tagSearch, setTagSearch] = useState('');
    const [selectedTag, setSelectedTag] = useState<string | null>(null);
    const [tagItems, setTagItems] = useState<ClipboardEntry[]>([]);
    const [tagColors, setTagColors] = useState<Record<string, string>>({});
    const [editingTag, setEditingTag] = useState<string | null>(null);
    const [newTagName, setNewTagName] = useState('');
    const [loading, setLoading] = useState(false);
    const [viewMode, setViewMode] = useState<'list' | 'grid'>('grid');
    const [isDeleting, setIsDeleting] = useState(false);
    const [deleteConfirmation, setDeleteConfirmation] = useState<{ show: boolean, tagName: string | null }>({ show: false, tagName: null });
    const [itemDeleteConfirmation, setItemDeleteConfirmation] = useState<{ show: boolean, id: number | null }>({ show: false, id: null });
    const [isCollapsed, setIsCollapsed] = useState(false);
    const [sortBy, setSortBy] = useState<'time' | 'count'>('time');
    const [isCreatingItem, setIsCreatingItem] = useState(false);
    const [editingItem, setEditingItem] = useState<{ id: number, content: string } | null>(null);
    const [newItemContent, setNewItemContent] = useState('');
    const [isManageMode, setIsManageMode] = useState(false);
    const [selectedItemIds, setSelectedItemIds] = useState<number[]>([]);

    const selectedTagRef = useRef<string | null>(null);
    useEffect(() => { selectedTagRef.current = selectedTag; }, [selectedTag]);
    useEffect(() => {
        setIsManageMode(false);
        setSelectedItemIds([]);
    }, [selectedTag]);
    useEffect(() => {
        setSelectedItemIds(prev => prev.filter(id => tagItems.some(item => item.id === id)));
    }, [tagItems]);

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

            if (!selectedTag && tagArray.length > 0) loadTagItems(tagArray[0].name);
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

    // Helper for color collision detection




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
            if (selectedTag === tagName) { setSelectedTag(null); setTagItems([]); }
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

    const toggleItemSelection = (id: number) => {
        setSelectedItemIds((prev) => (
            prev.includes(id) ? prev.filter(itemId => itemId !== id) : [...prev, id]
        ));
    };

    const handleManageAction = async () => {
        if (!selectedTag) return;

        if (!isManageMode) {
            setIsManageMode(true);
            setSelectedItemIds([]);
            return;
        }

        if (selectedItemIds.length === 0) {
            setIsManageMode(false);
            setSelectedItemIds([]);
            return;
        }

        setLoading(true);
        try {
            const selectedSet = new Set(selectedItemIds);
            const selectedItems = tagItems.filter(item => selectedSet.has(item.id));

            for (const item of selectedItems) {
                const updatedTags = item.tags.filter(tag => tag !== selectedTag);
                await invoke<number>('update_tags', { id: item.id, tags: updatedTags });
            }

            setIsManageMode(false);
            setSelectedItemIds([]);
            await emit('clipboard-changed');
            await fetchTags();
            await loadTagItems(selectedTag);
        } catch (err) {
            console.error(err);
        } finally {
            setLoading(false);
        }
    };

    const filteredTags = useMemo(() => {
        return tags.filter(t => t.name.toLowerCase().includes(tagSearch.toLowerCase()));
    }, [tags, tagSearch]);

    const sortedItems = [...tagItems].sort((a, b) => {
        if (sortBy === 'count') return (b.use_count || 0) - (a.use_count || 0);
        return b.timestamp - a.timestamp;
    });
    const selectedItemIdSet = useMemo(() => new Set(selectedItemIds), [selectedItemIds]);
    const manageButtonLabel = !isManageMode
        ? (t('manage_items') || '管理')
        : selectedItemIds.length > 0
            ? (t('remove_from_tag') || '移出')
            : (t('cancel') || '取消');

    return (
        <div
            className={`themed-tag-manager theme-${theme} ${isCollapsed ? 'sidebar-collapsed' : ''}`}
            onMouseDown={() => invoke('activate_window_focus').catch(console.error)}
        >
            {/* Sidebar with CRUD support */}
            {/* Sidebar with Unified Search & Create */}
            <div className="tag-sidebar">
                <div className="sidebar-header">
                    {!isCollapsed && <span className="header-label">{t('tags')}</span>}
                    <button
                        className="collapse-toggle"
                        title={isCollapsed ? (t('collapse') || '展开') : (t('collapse') || '收起')}
                        onClick={() => setIsCollapsed(!isCollapsed)}
                    >
                        {isCollapsed ? <ChevronRight size={14} /> : <ChevronLeft size={14} />}
                    </button>
                </div>

                {!isCollapsed && (
                    <div className="tag-search-box">
                        <input
                            placeholder={t('find_or_create')}
                            value={tagSearch}
                            onMouseDown={() => invoke('activate_window_focus').catch(console.error)}
                            onFocus={() => invoke('activate_window_focus').catch(console.error)}
                            onChange={e => setTagSearch(e.target.value)}
                            onKeyDown={async (e) => {
                                if (e.key === 'Enter' && tagSearch.trim()) {
                                    // If exact match exists, select it. If not, create new.
                                    const exactMatch = tags.find(t => t.name.toLowerCase() === tagSearch.trim().toLowerCase());
                                    if (exactMatch) {
                                        loadTagItems(exactMatch.name);
                                    } else {
                                        // Create new persistence tag
                                        try {
                                            await invoke('create_new_tag', { tagName: tagSearch.trim() });
                                            setNewTagName('');
                                            setTagSearch('');
                                            await fetchTags();
                                        } catch (err) { console.error(err); }
                                    }
                                }
                            }}
                        />
                        {tagSearch ? (
                            <div className="action-icons">
                                { /* If no exact match, show Plus to indicate creation */}
                                {!tags.some(t => t.name.toLowerCase() === tagSearch.trim().toLowerCase()) ? (
                                    <span
                                        title={t('create_new_tag_tooltip')}
                                        className="action-icon create"
                                        onClick={async () => {
                                            try {
                                                await invoke('create_new_tag', { tagName: tagSearch.trim() });
                                                setNewTagName('');
                                                setTagSearch('');
                                                await fetchTags();
                                            } catch (err) { console.error(err); }
                                        }}
                                    >
                                        <Plus size={12} />
                                    </span>
                                ) : (
                                    <Search size={12} className="action-icon search" />
                                )}
                                <X size={12} className="action-icon clear" onClick={() => setTagSearch('')} />
                            </div>
                        ) : (
                            <Search size={12} className="search-icon-placeholder" />
                        )}
                    </div>
                )}

                <div className="tag-scroll custom-scrollbar">
                    {filteredTags.map(tag => (
                        <div
                            key={tag.name}
                            className={`tag-item ${selectedTag === tag.name ? 'active' : ''}`}
                            onClick={() => loadTagItems(tag.name)}
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
                                        <span title="重命名" onClick={(e) => {
                                            e.stopPropagation();
                                            if (tag.name === 'sensitive' || tag.name === '密码') return;
                                            setEditingTag(tag.name);
                                            setNewTagName(tag.name);
                                        }} style={{
                                            opacity: (tag.name === 'sensitive' || tag.name === '密码') ? 0.2 : 1,
                                            cursor: (tag.name === 'sensitive' || tag.name === '密码') ? 'not-allowed' : 'pointer',
                                            display: 'flex',
                                            alignItems: 'center'
                                        }}>
                                            <Edit2 size={12} />
                                        </span>
                                        {(tag.name !== 'sensitive' && tag.name !== '密码') && (
                                            <span title="删除" onClick={(e) => {
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
                    {/* Visual cue for creating new tag when filtering shows no results */}
                    {!isCollapsed && tagSearch && filteredTags.length === 0 && (
                        <div className="tag-item create-hint" onClick={async () => {
                            try {
                                await invoke('create_new_tag', { tagName: tagSearch.trim() });
                                setNewTagName('');
                                setTagSearch('');
                                await fetchTags();
                            } catch (err) { console.error(err); }
                        }}>
                            <div className="tag-color-dot" style={{ border: '1px dashed currentColor', background: 'transparent' }} />
                            <span className="tag-name" style={{ opacity: 0.7 }}>{t('create_tag_hint').replace('{tag}', tagSearch.trim())}</span>
                            <Plus size={10} />
                        </div>
                    )}
                </div>
            </div>

            {/* Right Main Area */}
            <div className="tag-content">
                <div className="content-toolbar">
                    <div className="toolbar-left">
                        <div className="selected-tag-indicator">
                            <span className="breadcrumb-marker">#</span>
                            <span className="breadcrumb-text">{selectedTag?.toUpperCase()}</span>
                        </div>
                        <div className="sort-group">
                            <button
                                className={`sort-btn ${sortBy === 'time' ? 'active' : ''}`}
                                title={t('sort_time') || '按时间'}
                                onClick={() => setSortBy('time')}
                            >
                                <Clock size={12} />
                            </button>
                            <button
                                className={`sort-btn ${sortBy === 'count' ? 'active' : ''}`}
                                title={t('sort_usage') || '按频率'}
                                onClick={() => setSortBy('count')}
                            >
                                <MousePointer2 size={12} />
                            </button>
                        </div>
                        {selectedTag && (
                            <button
                                className="add-item-btn btn-icon"
                                onClick={() => setIsCreatingItem(true)}
                                title={t('add_item')}
                                disabled={isManageMode}
                            >
                                <Plus size={14} />
                            </button>
                        )}
                        {selectedTag && (
                            <button
                                type="button"
                                className={`manage-action-btn ${isManageMode ? 'active' : ''} ${isManageMode && selectedItemIds.length > 0 ? 'remove-ready' : ''}`}
                                onClick={handleManageAction}
                                title={manageButtonLabel}
                            >
                                {manageButtonLabel}
                            </button>
                        )}
                    </div>
                    <div className="view-toggle">
                        <button
                            type="button"
                            className={`toggle-btn btn-icon ${viewMode === 'list' ? 'active' : ''}`}
                            title="列表视图"
                            onClick={() => setViewMode('list')}
                        ><List size={14} /></button>
                        <button
                            type="button"
                            className={`toggle-btn btn-icon ${viewMode === 'grid' ? 'active' : ''}`}
                            title="卡片视图"
                            onClick={() => setViewMode('grid')}
                        ><LayoutGrid size={14} /></button>
                    </div>
                </div>

                <div className="items-area custom-scrollbar">
                    {loading ? <div className="status-msg">{t('processing')}</div> : (
                        <div className={`items-${viewMode}`}>
                            {sortedItems.map(item => {
                                const isSelected = selectedItemIdSet.has(item.id);

                                return (
                                    <div
                                        key={item.id}
                                        className={`themed-card ${isManageMode ? 'manage-mode' : ''} ${isSelected ? 'selected' : ''}`}
                                        onClick={() => {
                                            if (isManageMode) {
                                                toggleItemSelection(item.id);
                                                return;
                                            }
                                            copyToClipboard(item.id, item.content, item.content_type);
                                        }}
                                    >
                                        {isManageMode && (
                                            <button
                                                type="button"
                                                className={`manage-checkbox ${isSelected ? 'checked' : ''}`}
                                                onClick={(e) => {
                                                    e.stopPropagation();
                                                    toggleItemSelection(item.id);
                                                }}
                                                aria-label={manageButtonLabel}
                                            >
                                                <input
                                                    type="checkbox"
                                                    checked={isSelected}
                                                    readOnly
                                                    tabIndex={-1}
                                                />
                                            </button>
                                        )}

                                        {!isManageMode && (
                                            <div className="card-top-row">
                                                <div className="card-actions-left">
                                                    {item.content_type === 'text' || item.content_type === 'code' ? (
                                                        <button className="card-action-btn" title="编辑" onClick={(e) => {
                                                            e.stopPropagation();
                                                            setEditingItem({ id: item.id, content: item.content });
                                                        }}>
                                                            <Edit2 size={10} />
                                                        </button>
                                                    ) : null}
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
                                                </div>
                                                <button className="del-btn" title="删除" onClick={(e) => {
                                                    e.stopPropagation();
                                                    setItemDeleteConfirmation({ show: true, id: item.id });
                                                }}>
                                                    <X size={10} />
                                                </button>
                                            </div>
                                        )}

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

                                        {!isManageMode && (
                                            <div className="card-footer">
                                                <span className="meta-time">{new Date(item.timestamp).toLocaleDateString()}</span>
                                                <div className="meta-usage"><MousePointer2 size={8} /> {item.use_count || 0}</div>
                                            </div>
                                        )}
                                    </div>
                                );
                            })}
                        </div>
                    )}
                </div>
            </div>

            {/* Modals for Create (Rename is handled inline now) */}
            {/* Kept minimal if needed for future extensions, but currently inline handles rename */}

            {/* Tag Delete Confirmation Modal */}
            {deleteConfirmation.show && (
                <div className="modal-overlay" onClick={() => setDeleteConfirmation({ show: false, tagName: null })}>
                    <div className={`confirm-dialog theme-${theme}`} onClick={(e) => e.stopPropagation()}>
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
                    <div className={`confirm-dialog theme-${theme}`} onClick={e => e.stopPropagation()}>
                        <h3>{t('confirm_delete')}</h3>
                        <p>{t('confirm_delete_desc') || "确定要删除这条记录吗？"}</p>
                        <div className="confirm-dialog-buttons">
                            <button className="confirm-dialog-button" onClick={() => setItemDeleteConfirmation({ show: false, id: null })}>
                                {t('cancel')}
                            </button>
                            <button className="confirm-dialog-button primary" onClick={async () => {
                                if (itemDeleteConfirmation.id) {
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
                    <div className={`confirm-dialog theme-${theme}`} onClick={e => e.stopPropagation()}>
                        <h3>{t('add_item')}</h3>
                        <div className="modal-input-field">
                            <textarea
                                value={newItemContent}
                                onChange={e => setNewItemContent(e.target.value)}
                                placeholder={t('input_content_placeholder')}
                                autoFocus
                                style={{
                                    width: '100%',
                                    minHeight: '120px',
                                    background: 'var(--bg-input)',
                                    border: '2px solid var(--border-dark)',
                                    padding: '12px',
                                    color: 'var(--text-primary)',
                                    fontFamily: 'inherit',
                                    fontSize: '14px',
                                    outline: 'none',
                                    resize: 'vertical',
                                    marginBottom: '20px'
                                }}
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
                    <div className={`confirm-dialog theme-${theme}`} onClick={e => e.stopPropagation()}>
                        <h3>{t('edit_item')}</h3>
                        <div className="modal-input-field">
                            <textarea
                                value={editingItem.content}
                                onChange={e => setEditingItem({ ...editingItem, content: e.target.value })}
                                autoFocus
                                style={{
                                    width: '100%',
                                    minHeight: '150px',
                                    background: 'var(--bg-input)',
                                    border: '2px solid var(--border-dark)',
                                    padding: '12px',
                                    color: 'var(--text-primary)',
                                    fontFamily: 'inherit',
                                    fontSize: '14px',
                                    outline: 'none',
                                    resize: 'vertical',
                                    marginBottom: '20px'
                                }}
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
                    display: flex;
                    height: 100%;
                    background: var(--bg-content);
                    font-family: var(--font-main, ui-monospace, monospace);
                    color: var(--text-primary);
                }

                /* Sidebar */
                .tag-sidebar {
                    width: 140px;
                    border-right: 2px solid var(--border-dark);
                    background: var(--bg-toolbar);
                    display: flex;
                    flex-direction: column;
                    transition: width 0.2s cubic-bezier(0.4, 0, 0.2, 1);
                    flex-shrink: 0;
                }
                .sidebar-collapsed .tag-sidebar { width: 40px; }
                
                .sidebar-header {
                    padding: 8px 10px;
                    background: var(--bg-element);
                    color: var(--text-primary);
                    font-size: 10px; font-weight: 900; text-transform: uppercase;
                    display: flex; justify-content: space-between; align-items: center;
                    min-height: 32px;
                    border-bottom: 2px solid var(--border-dark);
                }
                .header-actions { display: flex; align-items: center; gap: 8px; }
                .action-btn { background: transparent; border: none; color: inherit; cursor: pointer; padding: 2px; opacity: 0.7; transition: opacity 0.2s; }
                .action-btn:hover { opacity: 1; }
                .collapse-toggle { background: transparent; border: none; color: inherit; cursor: pointer; display: flex; align-items: center; }

                /* Tag Search Box */
                .tag-search-box {
                    padding: 6px 10px;
                    display: flex; align-items: center; gap: 6px;
                    background: var(--bg-input);
                    border-bottom: 2px solid var(--border-dark);
                    margin: 0;
                    min-height: 34px;
                    position: relative;
                }
                .tag-search-box .search-icon-placeholder { opacity: 0.3; color: var(--text-primary); flex-shrink: 0; }
                .tag-search-box input {
                    background: transparent; border: none; outline: none;
                    font-size: 11px; font-weight: 700; width: 100%;
                    color: var(--text-primary);
                    padding: 2px 0;
                    flex: 1;
                    min-width: 0; 
                }
                .tag-search-box input::placeholder { color: var(--text-muted); opacity: 0.5; font-style: italic; font-size: 10px; }
                
                .action-icons { display: flex; align-items: center; gap: 4px; }
                .action-icon { cursor: pointer; opacity: 0.5; color: var(--text-primary); transition: all 0.1s; }
                .action-icon:hover { opacity: 1; transform: scale(1.1); }
                .action-icon.create { color: var(--accent-color); opacity: 0.8; }
                .action-icon.create:hover { opacity: 1; }

                .tag-scroll { flex: 1; overflow-y: auto; padding: 4px; overflow-x: hidden; }
                /* Tag Item Layout: [Color] [Name (Flex)] [Actions (Hover)] [Badge] */
                .tag-item {
                    display: flex; align-items: center; gap: 8px;
                    padding: 8px 10px; cursor: pointer;
                    margin-bottom: 2px; border: 1px solid transparent;
                    transition: all 0.1s;
                    position: relative;
                }
                .tag-item.active { background: var(--bg-element); border: 2px solid var(--border-dark); box-shadow: 3px 3px 0 var(--shadow-color); }
                .tag-item.create-hint { border: 1px dashed var(--border-dark); opacity: 0.8; font-style: italic; }
                .tag-item.create-hint:hover { background: var(--bg-input); border-style: solid; }

                .sidebar-collapsed .tag-item { justify-content: center; padding: 10px 0; gap: 0; }
                .sidebar-collapsed .tag-name,
                .sidebar-collapsed .tag-badge,
                .sidebar-collapsed .tag-hover-actions { display: none; }
                .sidebar-collapsed .tag-color-wrapper { width: 100%; justify-content: center; }
                .tag-color-wrapper { display: flex; align-items: center; justify-content: center; }
                .tag-color-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; cursor: pointer; border: 1px solid rgba(0,0,0,0.1); transition: transform 0.2s; }
                .tag-color-dot:hover { transform: scale(1.2); }
                .tag-name { flex: 1; font-size: 11px; font-weight: 800; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
                
                /* Inline Edit Input */
                .inline-tag-edit {
                    flex: 1; border: none; background: var(--bg-input); 
                    color: var(--text-primary); font-size: 11px; font-weight: 800;
                    padding: 0 4px; border-radius: 2px;
                    min-width: 0; outline: 1px solid var(--accent-color);
                }

                /* Actions group: Hidden by default, Flex on hover. Static position (displaces name) */
                .tag-hover-actions { display: none; gap: 8px; align-items: center; }
                .tag-item:hover .tag-hover-actions { display: flex; }
                
                .tag-badge { font-size: 9px; opacity: 0.6; min-width: 16px; text-align: right; }
                
                .tag-hover-actions > *:hover { color: var(--accent-color); }
                .tag-item.active .tag-hover-actions > * { opacity: 0.8; }
                .tag-item.active .tag-hover-actions > *:hover { opacity: 1; color: white; }

                /* Content Area */
                .tag-content { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
                .content-toolbar {
                    height: 32px; border-bottom: 2px solid var(--border-dark);
                    background: var(--bg-toolbar);
                    display: flex; align-items: center; justify-content: space-between; padding: 0 12px;
                }
                .toolbar-left { display: flex; align-items: center; gap: 16px; }
                .selected-tag-indicator { display: flex; align-items: center; gap: 4px; font-weight: 900; font-size: 10px; opacity: 0.7; }
                .breadcrumb-marker { color: var(--accent-color); }

                .sort-group { display: flex; gap: 4px; padding-left: 12px; border-left: 1px dashed var(--border-dark); }
                .sort-btn { background: transparent; border: none; color: var(--text-muted); cursor: pointer; display: flex; align-items: center; padding: 2px; }
                .sort-btn.active { color: var(--accent-color); opacity: 1; }

                .view-toggle {
                    display: flex;
                    align-items: center;
                    gap: 6px;
                    padding: 0;
                    border: none;
                    background: transparent;
                }
                .toggle-btn {
                    padding: 6px;
                }

                .items-area { flex: 1; overflow-y: auto; padding: 8px; background: var(--bg-content); }

                .items-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(130px, 1fr)); gap: 8px; }
                .items-list { display: flex; flex-direction: column; gap: 4px; }

                .themed-card {
                    background: var(--bg-element);
                    border: 2px solid var(--border-dark);
                    padding: 6px; cursor: pointer;
                    position: relative;
                    box-shadow: 3px 3px 0 var(--border-dark);
                    transition: all 0.1s;
                }
                .themed-card:hover { transform: translate(-1px, -1px); box-shadow: 4px 4px 0 var(--border-dark); background: var(--bg-input); }

                .del-btn { background: transparent; border: none; color: var(--text-muted); cursor: pointer; opacity: 0.2; }
                .del-btn:hover { opacity: 1; color: #ff0000; }

                .card-media { min-height: 40px; border: 1px solid var(--border-dark); margin: 4px 0; overflow: hidden; background: transparent; display: flex; justify-content: center; }
                .card-media.image-preview { max-width: 100%; max-height: 120px; object-fit: contain; }
                
                .card-body-text { font-size: 12px; line-height: 1.2; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; word-break: break-all; }
                .card-footer { display: flex; justify-content: space-between; margin-top: 4px; font-size: 8px; font-weight: 900; opacity: 0.4; }
                .meta-usage { display: flex; align-items: center; gap: 2px; }
                
                .add-item-btn {
                    margin-left: 12px;
                }
                .add-item-btn:disabled {
                    opacity: 0.35;
                    cursor: not-allowed;
                }
                .manage-action-btn {
                    margin-left: 6px;
                    height: 24px;
                    padding: 0 10px;
                    border: 2px solid var(--border-dark);
                    background: var(--bg-button);
                    color: var(--text-primary);
                    font-size: 10px;
                    font-weight: 800;
                    cursor: pointer;
                    transition: all 0.1s;
                    box-shadow: 2px 2px 0 var(--border-dark);
                    text-transform: uppercase;
                }
                .manage-action-btn:active {
                    transform: translate(1px, 1px);
                    box-shadow: 1px 1px 0 var(--border-dark);
                }
                .manage-action-btn.active {
                    background: var(--bg-input);
                }
                .manage-action-btn.remove-ready {
                    background: var(--accent-color);
                    color: #fff;
                }

                .card-top-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 4px; }
                .card-actions-left { display: flex; gap: 4px; }
                .card-action-btn {
                    background: transparent;
                    border: none;
                    color: var(--text-muted);
                    cursor: pointer;
                    display: flex;
                    align-items: center;
                    padding: 2px;
                    opacity: 0.3;
                    transition: opacity 0.2s;
                }
                .card-action-btn:hover { opacity: 1; color: var(--accent-color); }
                .themed-card.manage-mode {
                    cursor: pointer;
                }
                .themed-card.selected {
                    border-color: var(--accent-color);
                    box-shadow: 3px 3px 0 var(--accent-color);
                }
                .manage-checkbox {
                    position: absolute;
                    top: 50%;
                    left: 50%;
                    transform: translate(-50%, -50%);
                    width: 30px;
                    height: 30px;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    background: var(--bg-window);
                    border: 2px solid var(--border-dark);
                    box-shadow: 2px 2px 0 var(--border-dark);
                    z-index: 4;
                    cursor: pointer;
                    padding: 0;
                }
                .manage-checkbox.checked {
                    border-color: var(--accent-color);
                }
                .manage-checkbox input {
                    width: 14px;
                    height: 14px;
                    pointer-events: none;
                }
                .items-list .themed-card.manage-mode {
                    padding-left: 34px;
                }
                .items-list .manage-checkbox {
                    left: 10px;
                    transform: translateY(-50%);
                }

                /* Overlay */
                .modal-overlay {
                    position: fixed; top: 0; left: 0; right: 0; bottom: 0;
                    background: rgba(0, 0, 0, 0.4);
                    backdrop-filter: blur(4px);
                    display: flex; align-items: center; justify-content: center;
                    z-index: 2000;
                }


                /* Confirm Dialog - Base Retro (Brutalist) Style */
                .modal-overlay .confirm-dialog {
                    background: var(--bg-window) !important;
                    padding: 24px;
                    border: 3px solid var(--border-dark) !important;
                    box-shadow: 8px 8px 0 var(--shadow-color) !important;
                    border-radius: 0 !important;
                    width: 360px;
                    max-width: 90%;
                    animation: modal-pop 0.15s cubic-bezier(0.17, 0.89, 0.32, 1.28);
                }

                @keyframes modal-pop {
                    0% { transform: scale(0.9); opacity: 0; }
                    100% { transform: scale(1); opacity: 1; }
                }

                .modal-overlay .confirm-dialog h3 {
                    margin: 0 0 12px 0;
                    font-size: 16px;
                    font-weight: 900;
                    text-transform: uppercase;
                    background: var(--border-dark) !important;
                    color: var(--bg-window) !important;
                    padding: 4px 8px;
                    display: inline-block;
                }

                .modal-overlay .confirm-dialog p {
                    margin: 12px 0 24px 0;
                    font-size: 14px;
                    font-weight: 700;
                    line-height: 1.4;
                    color: var(--text-primary);
                }

                .modal-overlay .confirm-dialog-buttons {
                    display: flex;
                    justify-content: flex-end;
                    gap: 12px;
                }

                .modal-overlay .confirm-dialog-button {
                    padding: 6px 20px;
                    font-size: 12px;
                    font-weight: 900;
                    cursor: pointer;
                    background: var(--bg-button) !important;
                    border: 2px solid var(--border-dark) !important;
                    color: var(--text-primary) !important;
                    box-shadow: 3px 3px 0 var(--border-dark) !important;
                    transition: all 0.1s;
                    border-radius: 0;
                }
                .modal-overlay .confirm-dialog-button:active {
                    transform: translate(2px, 2px);
                    box-shadow: 0 0 0 !important;
                }

                .modal-overlay .confirm-dialog-button.primary {
                    background: var(--accent-color) !important;
                    color: #fff !important;
                }

                /* Modern Theme Polishes for Confirm Dialog */
                .theme-mica .confirm-dialog,
                .theme-acrylic .confirm-dialog {
                    background: rgba(255, 255, 255, 0.8) !important;
                    backdrop-filter: blur(20px);
                    padding: 24px !important;
                    border-radius: 16px !important;
                    box-shadow: 0 10px 40px rgba(0,0,0,0.2) !important;
                    border: 1px solid rgba(255,255,255,0.4) !important;
                    animation: modal-pop-modern 0.3s cubic-bezier(0.34, 1.56, 0.64, 1) !important;
                }
                
                @keyframes modal-pop-modern {
                    0% { transform: scale(0.95); opacity: 0; }
                    100% { transform: scale(1); opacity: 1; }
                }

                .theme-mica .confirm-dialog h3,
                .theme-acrylic .confirm-dialog h3 {
                    background: transparent !important;
                    color: var(--text-primary) !important;
                    font-size: 18px !important;
                    font-weight: 700 !important;
                    text-transform: none !important;
                    padding: 0 !important;
                }

                .theme-mica .confirm-dialog-button,
                .theme-acrylic .confirm-dialog-button {
                    border-radius: 10px !important;
                    border: none !important;
                    box-shadow: none !important;
                    font-weight: 600 !important;
                    background: rgba(0,0,0,0.05) !important;
                }
                .theme-mica .confirm-dialog-button:active,
                .theme-acrylic .confirm-dialog-button:active {
                    transform: scale(0.95);
                }

                .theme-mica .confirm-dialog-button.primary,
                .theme-acrylic .confirm-dialog-button.primary {
                    background: var(--accent-color) !important;
                }

                /* Dark Mode Adaptation */
                .dark-mode .modal-overlay .confirm-dialog {
                    background: #1f1f1f !important;
                    border-color: #000 !important;
                }
                .dark-mode .modal-overlay .confirm-dialog h3 {
                    color: #fff !important;
                }
                .dark-mode .modal-overlay .confirm-dialog p {
                    color: #d1d1d1 !important;
                }
                .dark-mode .theme-mica .confirm-dialog,
                .dark-mode .theme-acrylic .confirm-dialog {
                    background: rgba(30,30,30,0.8) !important;
                    border-color: rgba(255,255,255,0.1) !important;
                }

                .modal-input-field input {
                    width: 100%; background: var(--bg-input);
                    border: 2px solid var(--border-dark);
                    padding: 8px; color: var(--text-primary);
                    font-family: inherit; font-size: 12px; font-weight: 700;
                    outline: none; margin-bottom: 20px;
                }
                .modal-buttons { display: flex; gap: 12px; justify-content: flex-end; }
                .modal-buttons button {
                    padding: 6px 16px; cursor: pointer;
                    font-size: 11px; font-weight: 900;
                    border: 2px solid var(--border-dark);
                    background: var(--bg-button);
                    color: var(--text-primary);
                    box-shadow: 3px 3px 0 var(--border-dark);
                    transition: all 0.1s;
                }
                .modal-buttons button:active { transform: translate(2px, 2px); box-shadow: 0 0 0; }
                .btn-save { background: var(--accent-color); color: white; }
                
                /* Modern Theme Polishes */
                .theme-mica, .theme-acrylic { background: transparent !important; }
                .theme-mica .tag-sidebar, .theme-acrylic .tag-sidebar { border-right: 1px solid rgba(128,128,128,0.1); background: transparent; }
                .theme-mica .sidebar-header, .theme-acrylic .sidebar-header { background: transparent; color: var(--text-primary); border-bottom: 1px solid rgba(128,128,128,0.1); margin: 0; padding: 8px 12px; }
                
                .theme-mica .tag-item.active, .theme-acrylic .tag-item.active { background: var(--accent-color); color: white; border: none; box-shadow: 0 4px 12px rgba(0,0,0,0.1); border-radius: 6px; }

                /* Modern theme rounded inputs & buttons */
                .theme-mica .tag-search-box, .theme-acrylic .tag-search-box {
                    border-radius: 10px;
                    margin: 6px;
                    border: 1px solid rgba(128,128,128,0.2);
                    background: rgba(255,255,255,0.35);
                }
                .theme-mica .tag-search-box input, .theme-acrylic .tag-search-box input {
                    border-radius: 8px;
                    padding: 6px 8px;
                }
                .theme-mica .collapse-toggle, .theme-acrylic .collapse-toggle,
                .theme-mica .sort-btn, .theme-acrylic .sort-btn,
                .theme-mica .toggle-btn, .theme-acrylic .toggle-btn,
                .theme-mica .add-item-btn, .theme-acrylic .add-item-btn,
                .theme-mica .manage-action-btn, .theme-acrylic .manage-action-btn,
                .theme-mica .card-action-btn, .theme-acrylic .card-action-btn,
                .theme-mica .del-btn, .theme-acrylic .del-btn {
                    border-radius: 8px;
                }
                
                .theme-mica .themed-card, .theme-acrylic .themed-card { border-radius: 12px; border: 1px solid rgba(255,255,255,0.3); background: rgba(255,255,255,0.45); box-shadow: 0 4px 12px rgba(0,0,0,0.05); }
                .dark-mode .theme-mica .themed-card, .dark-mode .theme-acrylic .themed-card { background: rgba(45,45,45,0.6); border-color: rgba(255,255,255,0.1); }
                
                .custom-scrollbar::-webkit-scrollbar { width: 3px; }
                .custom-scrollbar::-webkit-scrollbar-thumb { background: var(--border-dark); border-radius: 10px; }
            `}</style>
        </div >
    );
}

