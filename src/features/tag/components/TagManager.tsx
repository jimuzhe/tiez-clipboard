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

    const selectedTagRef = useRef<string | null>(null);
    useEffect(() => { selectedTagRef.current = selectedTag; }, [selectedTag]);

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
                        title={isCollapsed ? (t('open') || '展开') : (t('collapse') || '收起')}
                        onClick={() => setIsCollapsed(!isCollapsed)}
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
                            <button className="add-item-btn btn-icon" onClick={() => setIsCreatingItem(true)} title={t('add_item')}>
                                <Plus size={16} />
                                <span>{t('add_item') || '添加'}</span>
                            </button>
                        )}
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
                </div>

                <div className="items-area custom-scrollbar">
                    {loading ? <div className="status-msg">{t('processing')}</div> : sortedItems.length === 0 ? (
                        <div className="status-msg">{selectedTag ? t('no_items') : t('select_tag_to_begin')}</div>
                    ) : (
                        <div className={`items-${viewMode}`}>
                            {sortedItems.map(item => (
                                <div key={item.id} className="themed-card" onClick={() => copyToClipboard(item.id, item.content, item.content_type)}>
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
                    display: flex;
                    height: 100%;
                    background: var(--bg-content);
                    font-family: var(--font-main, ui-monospace, monospace);
                    color: var(--text-primary);
                    gap: 16px;
                    padding: 16px;
                }

                /* Sidebar */
                .tag-sidebar {
                    width: 220px;
                    flex-shrink: 0;
                    display: flex;
                    flex-direction: column;
                    background: var(--bg-panel);
                    border-radius: var(--radius-lg);
                    box-shadow: 0 2px 12px var(--shadow);
                    overflow: hidden;
                    border: var(--panel-border);
                }
                .sidebar-collapsed .tag-sidebar { width: 56px; }
                
                .sidebar-header {
                    padding: 16px 20px;
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
                .header-actions { display: flex; align-items: center; gap: 8px; }
                .action-btn { background: transparent; border: none; color: inherit; cursor: pointer; padding: 2px; opacity: 0.7; transition: opacity 0.2s; }
                .action-btn:hover { opacity: 1; }
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
                    padding: 12px 16px;
                    display: flex; align-items: center; gap: 6px;
                    background: transparent;
                    border-bottom: 1px solid var(--panel-divider-color);
                    margin: 0;
                    min-height: auto;
                    position: relative;
                }
                .tag-search-box .search-icon-placeholder { opacity: 0.3; color: var(--text-primary); flex-shrink: 0; }
                .tag-search-box input {
                    width: 100%;
                    background: var(--bg-main); 
                    border: 1px solid var(--border); 
                    outline: none;
                    font-size: 13px; 
                    font-weight: 500; 
                    color: var(--text-primary);
                    padding: 10px 12px 10px 36px;
                    flex: 1;
                    min-width: 0; 
                    border-radius: var(--radius-sm);
                    transition: all 0.2s;
                }
                .tag-search-box input:focus {
                    border-color: var(--accent-color);
                    background: var(--bg-panel);
                    box-shadow: 0 0 0 3px var(--accent-light);
                }
                .tag-search-box input::placeholder { color: var(--text-muted); opacity: 0.7; font-style: normal; font-size: 13px; }
                
                .action-icons { display: flex; align-items: center; gap: 4px; }
                .action-icon { cursor: pointer; opacity: 0.5; color: var(--text-primary); transition: all 0.15s; }
                .action-icon:hover { opacity: 1; transform: scale(1.1); }
                .action-icon.create { color: var(--accent-color); opacity: 0.8; }
                .action-icon.create:hover { opacity: 1; }

                .tag-scroll { flex: 1; overflow-y: auto; padding: 8px; overflow-x: hidden; }
                /* Tag Item Layout: [Color] [Name (Flex)] [Actions (Hover)] [Badge] */
                .tag-item {
                    display: flex; 
                    align-items: center; 
                    gap: 10px;
                    padding: 10px 12px; 
                    cursor: pointer;
                    margin-bottom: 2px; 
                    border: 1px solid transparent;
                    border-radius: var(--radius-sm);
                    transition: all 0.15s;
                    position: relative;
                }
                .tag-item:hover { background: var(--bg-main); }
                .tag-item.active { 
                    background: var(--accent-light); 
                    border-color: transparent;
                    box-shadow: none;
                }
                .tag-item.create-hint { border: 1px dashed var(--border); opacity: 0.8; }
                .tag-item.create-hint:hover { background: var(--bg-main); border-style: solid; }

                .sidebar-collapsed .tag-item { justify-content: center; padding: 10px 0; gap: 0; }
                .sidebar-collapsed .tag-name,
                .sidebar-collapsed .tag-badge,
                .sidebar-collapsed .tag-hover-actions { display: none; }
                .sidebar-collapsed .tag-color-wrapper { width: 100%; justify-content: center; }
                .tag-color-wrapper { display: flex; align-items: center; justify-content: center; }
                .tag-color-dot { 
                    width: 10px; 
                    height: 10px; 
                    border-radius: 50%; 
                    flex-shrink: 0; 
                    cursor: pointer; 
                    border: none;
                    transition: transform 0.2s; 
                }
                .tag-color-dot:hover { transform: scale(1.2); }
                .tag-name { 
                    flex: 1; 
                    font-size: 13px; 
                    font-weight: 500; 
                    white-space: nowrap; 
                    overflow: hidden; 
                    text-overflow: ellipsis; 
                    min-width: 0; 
                }
                
                /* Inline Edit Input */
                .inline-tag-edit {
                    flex: 1; 
                    border: 1px solid var(--border); 
                    background: var(--bg-main); 
                    color: var(--text-primary); 
                    font-size: 13px; 
                    font-weight: 500;
                    padding: 6px 10px; 
                    border-radius: var(--radius-sm);
                    min-width: 0; 
                    outline: none;
                    box-shadow: 0 0 0 3px var(--accent-light);
                }

                /* Actions group: Hidden by default, Flex on hover */
                .tag-hover-actions { 
                    display: none; 
                    gap: 4px; 
                    align-items: center; 
                }
                .tag-item:hover .tag-hover-actions { display: flex; }
                
                .tag-badge { 
                    font-size: 11px; 
                    font-weight: 600; 
                    color: var(--text-secondary); 
                    background: var(--bg-main); 
                    padding: 2px 8px; 
                    border-radius: 10px;
                    min-width: auto;
                    text-align: center;
                }
                .tag-item.active .tag-badge {
                    background: var(--accent-color);
                    color: white;
                }
                
                .tag-hover-actions > *:hover { color: var(--accent-color); }
                .tag-item.active .tag-hover-actions > * { opacity: 0.8; }
                .tag-item.active .tag-hover-actions > *:hover { opacity: 1; color: var(--accent-color); }

                /* Content Area */
                .tag-content { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
                .content-toolbar {
                    height: 48px; border-bottom: 1px solid var(--panel-divider-color);
                    background: var(--bg-panel);
                    display: flex; align-items: center; justify-content: space-between; padding: 0 16px;
                }
                .toolbar-left { display: flex; align-items: center; gap: 12px; }
                .selected-tag-indicator { display: flex; align-items: center; gap: 6px; font-weight: 600; font-size: 14px; color: var(--text-primary); }
                .breadcrumb-marker { color: var(--accent-color); }

                .sort-group { display: flex; gap: 6px; padding-left: 12px; border-left: 1px solid var(--panel-divider-color); }
                .sort-btn { background: transparent; border: none; color: var(--text-secondary); cursor: pointer; display: flex; align-items: center; gap: 4px; padding: 4px 8px; border-radius: var(--radius-sm); transition: all 0.15s; }
                .sort-btn:hover { background: var(--bg-main); color: var(--text-primary); }
                .sort-btn.active { background: var(--accent-light); color: var(--accent-color); }

                .view-toggle {
                    display: flex;
                    align-items: center;
                    gap: 4px;
                    padding: 2px;
                    border: 1px solid var(--panel-divider-color);
                    border-radius: var(--radius-sm);
                    background: var(--bg-main);
                }
                .toggle-btn {
                    padding: 4px;
                    border-radius: var(--radius-sm);
                    transition: all 0.15s;
                }
                .toggle-btn:hover { background: var(--bg-input); }
                .toggle-btn.active { background: var(--accent-color); color: white; }

                .items-area { flex: 1; overflow-y: auto; padding: 16px; background: var(--bg-content); }

                .items-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); gap: 12px; }
                .items-list { display: flex; flex-direction: column; gap: 8px; }

                .themed-card {
                    background: var(--bg-element);
                    border: 1px solid var(--border);
                    padding: 12px; cursor: pointer;
                    position: relative;
                    border-radius: var(--radius-md);
                    transition: all 0.15s ease;
                }
                .themed-card:hover { transform: translateY(-1px); box-shadow: 0 4px 12px var(--shadow); border-color: var(--accent-color); }

                .del-btn { background: transparent; border: none; color: var(--text-muted); cursor: pointer; opacity: 0.4; transition: opacity 0.15s; }
                .del-btn:hover { opacity: 1; color: #ff4d4f; }

                .card-media { min-height: 60px; border-radius: var(--radius-sm); margin: 8px 0; overflow: hidden; background: var(--bg-main); display: flex; justify-content: center; align-items: center; }
                .card-media img { max-width: 100%; max-height: 140px; object-fit: contain; border-radius: var(--radius-sm); }
                
                .card-body-text { font-size: 13px; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; word-break: break-word; color: var(--text-primary); }
                .card-footer { display: flex; justify-content: space-between; margin-top: 8px; font-size: 11px; color: var(--text-secondary); opacity: 0.8; }
                .meta-usage { display: flex; align-items: center; gap: 4px; }
                
                .add-item-btn {
                    margin-left: 12px;
                }

                .card-top-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
                .card-actions-left { display: flex; gap: 6px; }
                .card-action-btn {
                    background: transparent;
                    border: none;
                    color: var(--text-secondary);
                    cursor: pointer;
                    display: flex;
                    align-items: center;
                    padding: 4px;
                    border-radius: var(--radius-sm);
                    opacity: 0.6;
                    transition: all 0.15s;
                }
                .card-action-btn:hover { opacity: 1; color: var(--accent-color); background: var(--bg-main); }

                /* Overlay */
                .modal-overlay {
                    position: fixed; top: 0; left: 0; right: 0; bottom: 0;
                    background: rgba(0, 0, 0, 0.4);
                    backdrop-filter: blur(4px);
                    display: flex; align-items: center; justify-content: center;
                    z-index: 2000;
                }


                /* Confirm Dialog - Modern Style */
                .modal-overlay .confirm-dialog {
                    background: var(--bg-panel) !important;
                    padding: 24px;
                    border: 1px solid var(--border) !important;
                    box-shadow: 0 20px 40px rgba(0,0,0,0.15) !important;
                    border-radius: var(--radius-lg) !important;
                    width: 400px;
                    max-width: 90%;
                    animation: modal-pop 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
                }

                @keyframes modal-pop {
                    0% { transform: scale(0.95); opacity: 0; }
                    100% { transform: scale(1); opacity: 1; }
                }

                .modal-overlay .confirm-dialog h3 {
                    margin: 0 0 16px 0;
                    font-size: 17px;
                    font-weight: 600;
                    background: transparent !important;
                    color: var(--text-primary) !important;
                    padding: 0;
                    display: block;
                    text-transform: none;
                }

                .modal-overlay .confirm-dialog p {
                    margin: 12px 0 24px 0;
                    font-size: 14px;
                    font-weight: 400;
                    line-height: 1.5;
                    color: var(--text-secondary);
                }

                .modal-overlay .confirm-dialog-buttons {
                    display: flex;
                    justify-content: flex-end;
                    gap: 8px;
                }

                .modal-overlay .confirm-dialog-button {
                    padding: 8px 16px;
                    font-size: 13px;
                    font-weight: 500;
                    cursor: pointer;
                    background: var(--bg-main) !important;
                    border: 1px solid var(--border) !important;
                    color: var(--text-primary) !important;
                    box-shadow: none !important;
                    transition: all 0.15s;
                    border-radius: var(--radius-sm);
                }
                .modal-overlay .confirm-dialog-button:hover {
                    background: var(--border-light) !important;
                }
                .modal-overlay .confirm-dialog-button:active {
                    transform: scale(0.98);
                    box-shadow: none !important;
                }

                .modal-overlay .confirm-dialog-button.primary {
                    background: var(--accent-color) !important;
                    color: #fff !important;
                    border: none !important;
                }
                .modal-overlay .confirm-dialog-button.primary:hover {
                    background: var(--accent-color-dark) !important;
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
                    width: 100%; 
                    background: var(--bg-main);
                    border: 1px solid var(--border);
                    padding: 12px; 
                    color: var(--text-primary);
                    font-family: inherit; 
                    font-size: 14px; 
                    font-weight: 400;
                    outline: none; 
                    margin-bottom: 20px;
                    border-radius: var(--radius-sm);
                    transition: all 0.2s;
                }
                .modal-input-field input:focus {
                    border-color: var(--accent-color);
                    box-shadow: 0 0 0 3px var(--accent-light);
                }
                .modal-buttons { display: flex; gap: 8px; justify-content: flex-end; }
                .modal-buttons button {
                    padding: 8px 16px; 
                    cursor: pointer;
                    font-size: 13px; 
                    font-weight: 500;
                    border: 1px solid var(--border);
                    background: var(--bg-main);
                    color: var(--text-primary);
                    box-shadow: none;
                    transition: all 0.15s;
                    border-radius: var(--radius-sm);
                }
                .modal-buttons button:active { transform: scale(0.98); }
                .btn-save { background: var(--accent-color); color: white; border: none; }
                .btn-save:hover { background: var(--accent-color-dark); }
                
                /* Modern Theme Polishes */
                .theme-mica.themed-tag-manager,
                .theme-acrylic.themed-tag-manager {
                    gap: 14px;
                    padding: 14px;
                    background: transparent !important;
                    overflow: hidden;
                }

                .theme-mica .tag-sidebar,
                .theme-acrylic .tag-sidebar {
                    width: clamp(196px, 24%, 248px);
                    border: var(--panel-border);
                    border-radius: 24px;
                    background: var(--bg-panel);
                    box-shadow: var(--panel-shadow);
                    overflow: hidden;
                }

                .theme-mica.sidebar-collapsed .tag-sidebar,
                .theme-acrylic.sidebar-collapsed .tag-sidebar {
                    width: 64px;
                }

                .theme-mica .sidebar-header,
                .theme-acrylic .sidebar-header {
                    min-height: 88px;
                    padding: 24px 24px 18px;
                    background: transparent;
                    border-bottom: 1px solid var(--panel-divider-color);
                }

                .theme-mica .header-label,
                .theme-acrylic .header-label {
                    font-size: 18px;
                    font-weight: 700;
                    letter-spacing: 0;
                }

                .theme-mica .collapse-toggle,
                .theme-acrylic .collapse-toggle {
                    width: 40px;
                    height: 40px;
                    border: 1px solid rgba(var(--accent-color-rgb), 0.12);
                    border-radius: 14px;
                    background: var(--bg-input);
                    color: var(--text-secondary);
                    box-shadow: none;
                }

                .theme-mica .collapse-toggle:hover,
                .theme-acrylic .collapse-toggle:hover {
                    background: rgba(var(--accent-color-rgb), 0.08);
                    color: var(--text-primary);
                }

                .theme-mica .tag-search-box,
                .theme-acrylic .tag-search-box {
                    margin: 18px 16px;
                    min-height: 56px;
                    padding: 0 16px;
                    gap: 12px;
                    border: var(--input-border);
                    border-radius: 16px;
                    background: var(--bg-input);
                    box-shadow: var(--input-shadow);
                }

                .theme-mica .tag-search-box .search-icon-placeholder,
                .theme-acrylic .tag-search-box .search-icon-placeholder {
                    opacity: 0.78;
                    color: var(--text-secondary);
                }

                .theme-mica .tag-search-box input,
                .theme-acrylic .tag-search-box input {
                    padding: 0;
                    font-size: 15px;
                    font-weight: 600;
                }

                .theme-mica .tag-search-box input::placeholder,
                .theme-acrylic .tag-search-box input::placeholder {
                    font-size: 15px;
                    font-style: normal;
                    opacity: 0.72;
                }

                .theme-mica .action-icons,
                .theme-acrylic .action-icons {
                    gap: 8px;
                }

                .theme-mica .action-icon,
                .theme-acrylic .action-icon {
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    width: 28px;
                    height: 28px;
                    border-radius: 999px;
                    background: rgba(var(--accent-color-rgb), 0.08);
                    color: var(--text-secondary);
                    opacity: 1;
                }

                .theme-mica .action-icon:hover,
                .theme-acrylic .action-icon:hover {
                    background: rgba(var(--accent-color-rgb), 0.14);
                    color: var(--accent-color);
                    transform: none;
                }

                .theme-mica .tag-scroll,
                .theme-acrylic .tag-scroll {
                    padding: 4px 12px 16px;
                }

                .theme-mica .tag-item,
                .theme-acrylic .tag-item {
                    min-height: 60px;
                    padding: 14px 16px;
                    margin-bottom: 6px;
                    border: 1px solid transparent;
                    border-radius: 16px;
                    background: transparent;
                }

                .theme-mica .tag-item:hover,
                .theme-acrylic .tag-item:hover {
                    background: rgba(var(--accent-color-rgb), 0.06);
                    border-color: rgba(var(--accent-color-rgb), 0.12);
                }

                .theme-mica .tag-item.active,
                .theme-acrylic .tag-item.active {
                    background: rgba(var(--accent-color-rgb), 0.12);
                    border-color: rgba(var(--accent-color-rgb), 0.16);
                    box-shadow: none;
                }

                .theme-mica .tag-color-dot,
                .theme-acrylic .tag-color-dot {
                    width: 14px;
                    height: 14px;
                    border: none;
                    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.28);
                }

                .theme-mica .tag-name,
                .theme-acrylic .tag-name {
                    font-size: 15px;
                    font-weight: 700;
                }

                .theme-mica .tag-hover-actions,
                .theme-acrylic .tag-hover-actions {
                    gap: 6px;
                    color: var(--text-secondary);
                }

                .theme-mica .tag-hover-actions > span,
                .theme-acrylic .tag-hover-actions > span {
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    width: 24px;
                    height: 24px;
                    border-radius: 999px;
                }

                .theme-mica .tag-hover-actions > span:hover,
                .theme-acrylic .tag-hover-actions > span:hover {
                    background: rgba(var(--accent-color-rgb), 0.12);
                    color: var(--accent-color);
                }

                .theme-mica .tag-item.active .tag-hover-actions > span:hover,
                .theme-acrylic .tag-item.active .tag-hover-actions > span:hover {
                    background: rgba(var(--accent-color-rgb), 0.14);
                    color: var(--accent-color);
                }

                .theme-mica .tag-badge,
                .theme-acrylic .tag-badge {
                    margin-left: auto;
                    min-width: 34px;
                    height: 30px;
                    padding: 0 10px;
                    border-radius: 999px;
                    background: rgba(127, 140, 160, 0.1);
                    color: var(--text-secondary);
                    font-size: 14px;
                    font-weight: 700;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    opacity: 1;
                }

                .theme-mica .tag-item.active .tag-badge,
                .theme-acrylic .tag-item.active .tag-badge {
                    background: var(--accent-color);
                    color: #ffffff;
                }

                .theme-mica .tag-content,
                .theme-acrylic .tag-content {
                    min-width: 0;
                    border: var(--panel-border);
                    border-radius: 28px;
                    background: var(--bg-panel);
                    box-shadow: var(--panel-shadow);
                }

                .theme-mica .content-toolbar,
                .theme-acrylic .content-toolbar {
                    min-height: 88px;
                    padding: 20px 28px;
                    border-bottom: 1px solid var(--panel-divider-color);
                    background: transparent;
                }

                .theme-mica .toolbar-left,
                .theme-mica .toolbar-right,
                .theme-acrylic .toolbar-left,
                .theme-acrylic .toolbar-right {
                    display: flex;
                    align-items: center;
                    gap: 14px;
                }

                .theme-mica .toolbar-right,
                .theme-acrylic .toolbar-right {
                    margin-left: auto;
                }

                .theme-mica .toolbar-divider,
                .theme-acrylic .toolbar-divider {
                    width: 1px;
                    height: 28px;
                    background: var(--panel-divider-color);
                }

                .theme-mica .selected-tag-indicator,
                .theme-acrylic .selected-tag-indicator {
                    padding: 10px 18px;
                    border-radius: var(--radius-pill);
                    background: rgba(var(--accent-color-rgb), 0.12);
                    border: 1px solid rgba(var(--accent-color-rgb), 0.16);
                    color: var(--accent-color);
                    font-size: 16px;
                    font-weight: 700;
                    gap: 10px;
                    opacity: 1;
                }

                .theme-mica .breadcrumb-text,
                .theme-acrylic .breadcrumb-text {
                    color: var(--text-primary);
                }

                .theme-mica .sort-group,
                .theme-acrylic .sort-group {
                    gap: 10px;
                    padding-left: 0;
                    border-left: none;
                }

                .theme-mica .sort-btn,
                .theme-acrylic .sort-btn {
                    min-height: 48px;
                    padding: 0 18px;
                    border: 1px solid rgba(var(--accent-color-rgb), 0.14);
                    border-radius: 16px;
                    background: transparent;
                    color: var(--text-secondary);
                    box-shadow: none;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    gap: 8px;
                }

                .theme-mica .sort-btn span,
                .theme-acrylic .sort-btn span {
                    font-size: 15px;
                    font-weight: 700;
                }

                .theme-mica .sort-btn:hover,
                .theme-acrylic .sort-btn:hover {
                    background: rgba(var(--accent-color-rgb), 0.08);
                    color: var(--text-primary);
                }

                .theme-mica .sort-btn.active,
                .theme-acrylic .sort-btn.active {
                    background: var(--accent-color);
                    border-color: var(--accent-color);
                    color: #ffffff;
                    box-shadow: 0 12px 24px rgba(var(--accent-color-rgb), 0.24);
                }

                .theme-mica .add-item-btn,
                .theme-acrylic .add-item-btn {
                    width: auto !important;
                    min-height: 52px;
                    padding: 0 22px;
                    border: none;
                    border-radius: 18px;
                    background: var(--accent-color);
                    color: #ffffff;
                    box-shadow: 0 12px 24px rgba(var(--accent-color-rgb), 0.26);
                    gap: 10px;
                    font-size: 16px;
                    font-weight: 700;
                }

                .theme-mica .add-item-btn span,
                .theme-acrylic .add-item-btn span {
                    display: inline-block;
                }

                .theme-mica .add-item-btn:hover,
                .theme-acrylic .add-item-btn:hover {
                    background: var(--accent-hover);
                    color: #ffffff;
                }

                .theme-mica .view-toggle,
                .theme-acrylic .view-toggle {
                    padding: 4px;
                    gap: 4px;
                    border: 1px solid rgba(var(--accent-color-rgb), 0.12);
                    border-radius: 18px;
                    background: var(--bg-input);
                }

                .theme-mica .toggle-btn,
                .theme-acrylic .toggle-btn {
                    width: 44px;
                    height: 44px;
                    padding: 0;
                    border: none;
                    border-radius: 14px;
                    background: transparent;
                    color: var(--text-secondary);
                    box-shadow: none;
                }

                .theme-mica .toggle-btn:hover,
                .theme-acrylic .toggle-btn:hover {
                    background: rgba(var(--accent-color-rgb), 0.08);
                    color: var(--text-primary);
                }

                .theme-mica .toggle-btn.active,
                .theme-acrylic .toggle-btn.active {
                    background: var(--accent-color);
                    color: #ffffff;
                    box-shadow: 0 10px 20px rgba(var(--accent-color-rgb), 0.2);
                }

                .theme-mica .items-area,
                .theme-acrylic .items-area {
                    padding: 28px;
                    background: transparent;
                }

                .theme-mica .status-msg,
                .theme-acrylic .status-msg {
                    padding: 36px 12px;
                    text-align: center;
                    color: var(--text-secondary);
                    font-size: 14px;
                }

                .theme-mica .items-grid,
                .theme-acrylic .items-grid {
                    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
                    gap: 22px;
                }

                .theme-mica .items-list,
                .theme-acrylic .items-list {
                    display: grid;
                    grid-template-columns: 1fr;
                    gap: 18px;
                }

                .theme-mica .themed-card,
                .theme-acrylic .themed-card {
                    position: relative;
                    min-height: 244px;
                    padding: 24px 22px 18px;
                    border: 1px solid rgba(var(--accent-color-rgb), 0.08);
                    border-radius: 22px;
                    background: var(--bg-input);
                    box-shadow: 0 12px 28px rgba(15, 23, 42, 0.06);
                    display: flex;
                    flex-direction: column;
                    transition: transform 0.18s ease, box-shadow 0.18s ease, border-color 0.18s ease;
                }

                .theme-mica .themed-card:hover,
                .theme-acrylic .themed-card:hover {
                    transform: translateY(-2px);
                    border-color: rgba(var(--accent-color-rgb), 0.14);
                    box-shadow: 0 18px 34px rgba(15, 23, 42, 0.1);
                    background: var(--bg-input);
                }

                .theme-mica .items-list .themed-card,
                .theme-acrylic .items-list .themed-card {
                    min-height: 180px;
                }

                .theme-mica .card-top-row,
                .theme-acrylic .card-top-row {
                    position: absolute;
                    top: 14px;
                    right: 14px;
                    display: flex;
                    align-items: center;
                    gap: 6px;
                    opacity: 0;
                    transition: opacity 0.18s ease;
                    z-index: 1;
                }

                .theme-mica .themed-card:hover .card-top-row,
                .theme-acrylic .themed-card:hover .card-top-row {
                    opacity: 1;
                }

                .theme-mica .card-actions-left,
                .theme-acrylic .card-actions-left {
                    gap: 6px;
                }

                .theme-mica .card-action-btn,
                .theme-mica .del-btn,
                .theme-acrylic .card-action-btn,
                .theme-acrylic .del-btn {
                    width: 28px;
                    height: 28px;
                    padding: 0;
                    border: 1px solid rgba(var(--accent-color-rgb), 0.08);
                    border-radius: 999px;
                    background: rgba(255, 255, 255, 0.88);
                    color: var(--text-secondary);
                    box-shadow: none;
                    opacity: 1;
                }

                .theme-mica .card-action-btn:hover,
                .theme-mica .del-btn:hover,
                .theme-acrylic .card-action-btn:hover,
                .theme-acrylic .del-btn:hover {
                    background: rgba(var(--accent-color-rgb), 0.12);
                    color: var(--accent-color);
                }

                .theme-mica .card-body-text,
                .theme-acrylic .card-body-text {
                    flex: 1;
                    padding-top: 12px;
                    font-size: 15px;
                    line-height: 1.7;
                    font-weight: 500;
                    color: var(--text-primary);
                    -webkit-line-clamp: 5;
                    min-height: 122px;
                }

                .theme-mica .items-list .card-body-text,
                .theme-acrylic .items-list .card-body-text {
                    -webkit-line-clamp: 3;
                    min-height: 84px;
                }

                .theme-mica .card-media,
                .theme-acrylic .card-media {
                    flex: 1;
                    min-height: 190px;
                    margin-top: 14px;
                    border: none;
                    border-radius: 18px;
                    background: rgba(127, 140, 160, 0.12);
                    align-items: center;
                }

                .theme-mica .card-media img,
                .theme-acrylic .card-media img {
                    max-width: 100%;
                    max-height: 190px;
                    object-fit: contain;
                    border-radius: 14px;
                }

                .theme-mica .card-divider,
                .theme-acrylic .card-divider {
                    height: 1px;
                    margin: 18px 0 14px;
                    background: var(--panel-divider-color);
                }

                .theme-mica .card-footer,
                .theme-acrylic .card-footer {
                    margin-top: auto;
                    font-size: 13px;
                    font-weight: 600;
                    color: var(--text-secondary);
                    opacity: 1;
                }

                .theme-mica .meta-usage,
                .theme-acrylic .meta-usage {
                    gap: 4px;
                }

                .theme-mica .inline-tag-edit,
                .theme-acrylic .inline-tag-edit,
                .theme-mica .modal-input-field input,
                .theme-acrylic .modal-input-field input {
                    border: var(--input-border);
                    border-radius: var(--input-radius);
                    box-shadow: var(--input-shadow);
                    padding: 8px 10px;
                    outline: none;
                }

                .theme-mica .modal-input-field textarea,
                .theme-acrylic .modal-input-field textarea {
                    background: var(--bg-input) !important;
                    border: var(--input-border) !important;
                    border-radius: 16px !important;
                    box-shadow: var(--input-shadow) !important;
                    color: var(--text-primary) !important;
                }

                .theme-mica .modal-buttons button,
                .theme-acrylic .modal-buttons button {
                    border: var(--button-border);
                    border-radius: var(--button-radius);
                    box-shadow: var(--button-shadow);
                }

                .dark-mode .theme-mica .tag-search-box,
                .dark-mode .theme-acrylic .tag-search-box,
                .dark-mode .theme-mica .tag-sidebar,
                .dark-mode .theme-acrylic .tag-sidebar,
                .dark-mode .theme-mica .tag-content,
                .dark-mode .theme-acrylic .tag-content {
                    border-color: rgba(255, 255, 255, 0.08);
                }

                .dark-mode .theme-mica .card-action-btn,
                .dark-mode .theme-mica .del-btn,
                .dark-mode .theme-acrylic .card-action-btn,
                .dark-mode .theme-acrylic .del-btn {
                    background: rgba(22, 28, 39, 0.92);
                    border-color: rgba(255, 255, 255, 0.08);
                }

                .dark-mode .theme-mica .tag-badge,
                .dark-mode .theme-acrylic .tag-badge {
                    background: rgba(255, 255, 255, 0.08);
                }
                
                .custom-scrollbar::-webkit-scrollbar { width: var(--scrollbar-size-thin); }
                .custom-scrollbar::-webkit-scrollbar-thumb { background: var(--scrollbar-thumb-color); border-radius: var(--scrollbar-radius); }
            `}</style>
        </div >
    );
}
