import { useEffect, useState } from "react";
import type { ComponentType, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight } from "lucide-react";

interface LabelWithHintProps {
    label: string;
    hint?: string | ReactNode;
    hintKey: string;
}

interface ClipboardSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    LabelWithHint: ComponentType<LabelWithHintProps>;
    persistent: boolean;
    setPersistent: (val: boolean) => void;
    persistentLimitEnabled: boolean;
    setPersistentLimitEnabled: (val: boolean) => void;
    persistentLimit: number;
    setPersistentLimit: (val: number) => void;
    saveAppSetting: (key: string, val: string) => void;
    deduplicate: boolean;
    setDeduplicate: (val: boolean) => void;
    captureFiles: boolean;
    setCaptureFiles: (val: boolean) => void;
    captureRichText: boolean;
    setCaptureRichText: (val: boolean) => void;
    richTextSnapshotPreview: boolean;
    setRichTextSnapshotPreview: (val: boolean) => void;
    richPasteHotkey: string;
    isRecordingRich: boolean;
    setIsRecordingRich: (val: boolean) => void;
    updateRichPasteHotkey: (key: string) => void;
    searchHotkey: string;
    isRecordingSearch: boolean;
    setIsRecordingSearch: (val: boolean) => void;
    updateSearchHotkey: (key: string) => void;
    deleteAfterPaste: boolean;
    setDeleteAfterPaste: (val: boolean) => void;
    moveToTopAfterPaste: boolean;
    setMoveToTopAfterPaste: (val: boolean) => void;
    pasteMethod: string;
    setPasteMethod: (val: string) => void;
    sequentialMode: boolean;
    setSequentialModeState: (val: boolean) => void;
    sequentialHotkey: string;
    isRecordingSequential: boolean;
    setIsRecordingSequential: (val: boolean) => void;
    updateSequentialHotkey: (key: string) => void;
    checkHotkeyConflict: (newHotkey: string, mode: 'main' | 'sequential' | 'rich' | 'search') => boolean;
    privacyProtection: boolean;
    setPrivacyProtection: (val: boolean) => void;
    privacyProtectionKinds: string[];
    setPrivacyProtectionKinds: (val: string[]) => void;
    privacyProtectionCustomRules: string;
    setPrivacyProtectionCustomRules: (val: string) => void;
    privacyKindsOpen: boolean;
    setPrivacyKindsOpen: (val: boolean) => void;
    privacyRulesOpen: boolean;
    setPrivacyRulesOpen: (val: boolean) => void;
    registryWinVEnabled: boolean;
    setRegistryWinVEnabled: (val: boolean) => void;
    isRecording: boolean;
    setIsRecording: (val: boolean) => void;
    mainHotkeys: string[];
    updateHotkey: (key: string) => void;
    addMainHotkey: (key: string, options?: { skipAvailabilityCheck?: boolean }) => Promise<boolean>;
    removeMainHotkey: (key: string) => Promise<boolean>;
    hotkey: string;
    appSettings: Record<string, string>;
    theme: string;
    colorMode: string;
}

const ClipboardSettingsGroup = (props: ClipboardSettingsGroupProps) => {
    const sequentialHotkeyParts = props.sequentialHotkey ? props.sequentialHotkey.split('+') : [];
    const searchHotkeyParts = props.searchHotkey ? props.searchHotkey.split('+') : [];
    const [persistentLimitDraft, setPersistentLimitDraft] = useState(
        props.persistentLimit.toString()
    );
    const isWinVHotkey = (value: string) => {
        const parts = value
            .split('+')
            .map((item) => item.trim().toUpperCase())
            .filter((item) => !!item);
        if (parts.length !== 2) return false;
        const hasWin = parts.includes('WIN') || parts.includes('SUPER') || parts.includes('META') || parts.includes('COMMAND');
        const hasV = parts.includes('V');
        return hasWin && hasV;
    };

    useEffect(() => {
        setPersistentLimitDraft(props.persistentLimit.toString());
    }, [props.persistentLimit, props.persistentLimitEnabled]);

    const commitPersistentLimit = (rawValue?: string) => {
        const source = rawValue ?? persistentLimitDraft;
        const parsed = parseInt(source, 10);
        if (!Number.isFinite(parsed)) {
            setPersistentLimitDraft(props.persistentLimit.toString());
            return;
        }
        const clamped = Math.max(50, Math.min(99999, parsed));
        props.setPersistentLimit(clamped);
        props.saveAppSetting('persistent_limit', clamped.toString());
        if (clamped.toString() !== source) {
            setPersistentLimitDraft(clamped.toString());
        }
    };

    return (
        <div className={`settings-group ${props.collapsed ? 'collapsed' : ''}`}>
            <div className="group-header" onClick={props.onToggle}>
                <h3 style={{ margin: 0 }}>{props.t('clipboard_settings')}</h3>
                {props.collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
            </div>
            {!props.collapsed && (
                <div className="group-content">
                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('persistent_storage')}
                            hint={props.t('persistent_hint')}
                            hintKey="persistent_storage"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.persistent}
                                onChange={(e) => props.setPersistent(e.target.checked)}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                    {props.persistent && (
                        <>
                            <div className="setting-item">
                                <props.LabelWithHint
                                    label={props.t('persistent_limit_enabled')}
                                    hint={props.t('persistent_limit_enabled_hint')}
                                    hintKey="persistent_limit_enabled"
                                />
                                <label className="switch">
                                    <input
                                        className="cb"
                                        type="checkbox"
                                        checked={props.persistentLimitEnabled}
                                        onChange={(e) => {
                                            props.setPersistentLimitEnabled(e.target.checked);
                                            props.saveAppSetting('persistent_limit_enabled', e.target.checked.toString());
                                        }}
                                    />
                                    <div className="toggle"><div className="left" /><div className="right" /></div>
                                </label>
                            </div>
                            {props.persistentLimitEnabled && (
                                <div className="setting-item">
                                    <props.LabelWithHint
                                        label={props.t('persistent_limit')}
                                        hint={props.t('persistent_limit_hint')}
                                        hintKey="persistent_limit"
                                    />
                                    <input
                                        type="number"
                                        value={persistentLimitDraft}
                                        onFocus={(e) => {
                                            e.target.select();
                                            invoke("focus_clipboard_window").catch(console.error);
                                        }}
                                        onChange={(e) => {
                                            const next = e.target.value;
                                            if (next === "") {
                                                setPersistentLimitDraft("");
                                                return;
                                            }
                                            if (!/^\d+$/.test(next)) return;
                                            setPersistentLimitDraft(next);
                                        }}
                                        onBlur={() => {
                                            commitPersistentLimit();
                                        }}
                                        onKeyDown={(e) => {
                                            if (e.key === 'Enter') {
                                                commitPersistentLimit(e.currentTarget.value);
                                                e.currentTarget.blur();
                                            }
                                        }}
                                        style={{
                                            width: '90px',
                                            padding: '4px 8px',
                                            borderRadius: '4px',
                                            border: '1px solid var(--border-color)',
                                            background: 'var(--input-bg)',
                                            color: 'var(--text-color)',
                                            fontSize: '14px'
                                        }}
                                    />
                                </div>
                            )}
                        </>
                    )}
                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('merge_duplicates')}
                            hint={props.t('merge_duplicates_hint') || "Time limit to prevent accidental multiple copies"}
                            hintKey="merge_duplicates"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.deduplicate}
                                onChange={(e) => props.setDeduplicate(e.target.checked)}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{props.t('capture_files')}</span>
                        </div>
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.captureFiles}
                                onChange={(e) => props.setCaptureFiles(e.target.checked)}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('capture_rich_text') || '捕获富文本'}
                            hint={props.t('capture_rich_text_hint') || '开启后可记录富文本并支持双击带格式粘贴'}
                            hintKey="capture_rich_text"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.captureRichText}
                                onChange={(e) => {
                                    const val = e.target.checked;
                                    props.setCaptureRichText(val);
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('rich_text_snapshot_preview') || '富文本快照预览'}
                            hint={props.t('rich_text_snapshot_preview_hint') || '开启后将富文本转换为内存快照图用于条目与悬浮预览'}
                            hintKey="rich_text_snapshot_preview"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.richTextSnapshotPreview}
                                onChange={(e) => {
                                    const val = e.target.checked;
                                    props.setRichTextSnapshotPreview(val);
                                    props.saveAppSetting('rich_text_snapshot_preview', String(val));
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>


                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{props.t('rich_paste_hotkey_label')}</span>
                            <span className="hint">
                                {props.isRecordingRich ? (
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
                                        <span style={{ color: '#ff9800', fontWeight: 'bold' }}>
                                            {props.t('win_key_not_recommended')}
                                        </span>
                                        <span style={{ fontSize: '11px', opacity: 0.8 }}>
                                            {props.t('hotkey_recording_esc')}
                                        </span>
                                    </div>
                                ) : props.t('hotkey_click_hint')}
                            </span>
                        </div>
                        <div
                            className={`key-group ${props.isRecordingRich ? 'recording' : ''}`}
                            onClick={() => props.setIsRecordingRich(true)}
                            tabIndex={0}
                            onKeyDown={(e) => {
                                if (!props.isRecordingRich) return;
                                e.preventDefault();
                                e.stopPropagation();

                                if (e.key === 'Escape') {
                                    props.setIsRecordingRich(false);
                                    return;
                                }

                                const modifiers = [];
                                if (e.ctrlKey) modifiers.push('Ctrl');
                                if (e.shiftKey) modifiers.push('Shift');
                                if (e.altKey) modifiers.push('Alt');

                                const key = e.key.toUpperCase();
                                if (['CONTROL', 'SHIFT', 'ALT', 'META'].includes(key)) return;

                                const newHotkey = [...modifiers, key].join('+');
                                props.updateRichPasteHotkey(newHotkey);
                            }}
                        >
                            {props.isRecordingRich ? (
                                <div className="key-cap" style={{ width: '8em' }}>{props.t('waiting_for_input')}</div>
                            ) : (
                                (props.richPasteHotkey || '').split('+').filter(Boolean).map((k, i) => (
                                    <div key={i} className="key-cap">{k}</div>
                                ))
                            )}
                            {!props.isRecordingRich && !props.richPasteHotkey && (
                                <div className="key-cap" style={{ opacity: 0.5 }}>{props.t('not_set')}</div>
                            )}
                        </div>
                    </div>
                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{props.t('search_hotkey_label')}</span>
                            <span className="hint">
                                {props.isRecordingSearch ? (
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
                                        <span style={{ color: '#ff9800', fontWeight: 'bold' }}>
                                            {props.t('win_key_not_recommended')}
                                        </span>
                                        <span style={{ fontSize: '11px', opacity: 0.8 }}>
                                            {props.t('hotkey_recording_esc')}
                                        </span>
                                    </div>
                                ) : props.t('hotkey_click_hint')}
                            </span>
                        </div>
                        <div
                            className={`key-group ${props.isRecordingSearch ? 'recording' : ''}`}
                            onClick={() => props.setIsRecordingSearch(true)}
                            tabIndex={0}
                            onKeyDown={(e) => {
                                if (!props.isRecordingSearch) return;
                                e.preventDefault();
                                e.stopPropagation();

                                if (e.key === 'Escape') {
                                    props.setIsRecordingSearch(false);
                                    return;
                                }

                                const modifiers = [];
                                if (e.ctrlKey) modifiers.push('Ctrl');
                                if (e.shiftKey) modifiers.push('Shift');
                                if (e.altKey) modifiers.push('Alt');

                                const key = e.key.toUpperCase();
                                if (['CONTROL', 'SHIFT', 'ALT', 'META'].includes(key)) return;

                                const newHotkey = [...modifiers, key].join('+');
                                props.updateSearchHotkey(newHotkey);
                            }}
                        >
                            {props.isRecordingSearch ? (
                                <div className="key-cap" style={{ width: '8em' }}>{props.t('waiting_for_input')}</div>
                            ) : (
                                searchHotkeyParts.length > 0 ? (
                                    searchHotkeyParts.map((k, i) => (
                                        <div key={i} className="key-cap">{k}</div>
                                    ))
                                ) : (
                                    <div className="key-cap" style={{ width: '8em', opacity: 0.5 }}>{props.t('not_set')}</div>
                                )
                            )}
                        </div>
                    </div>
                    <div className="setting-item">
                        <div className="item-label-group">
                            <span className="item-label">{props.t('delete_after_paste')}</span>
                        </div>
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.deleteAfterPaste}
                                onChange={(e) => {
                                    const val = e.target.checked;
                                    props.setDeleteAfterPaste(val);
                                    props.saveAppSetting('delete_after_paste', String(val));
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('move_to_top_after_paste')}
                            hint={props.t('move_to_top_after_paste_hint')}
                            hintKey="move_to_top_after_paste"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.moveToTopAfterPaste}
                                onChange={(e) => {
                                    const val = e.target.checked;
                                    props.setMoveToTopAfterPaste(val);
                                    props.saveAppSetting('move_to_top_after_paste', String(val));
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('paste_method')}
                            hint={props.t(`paste_method_${props.pasteMethod}_hint`)}
                            hintKey="paste_method"
                        />
                        <select
                            className="search-input"
                            style={{ borderRadius: '0', padding: '6px', width: '110px', background: 'var(--bg-input)', border: '2px solid var(--border-dark)', color: 'var(--text-primary)', fontSize: '12px' }}
                            value={props.pasteMethod}
                            onChange={async (e) => {
                                const val = e.target.value;

                                if (val === 'game_mode') {
                                    try {
                                        const isAdmin = await invoke<boolean>("check_is_admin");
                                        if (!isAdmin) {
                                            const confirmed = await ask(
                                                props.t('game_mode_admin_required') || "Game Mode requires Administrator privileges to work correctly with games (especially for IME/Input handling). Restart as Admin now?",
                                                {
                                                    title: props.t('admin_required') || "Administrator Required",
                                                    kind: 'warning'
                                                }
                                            );

                                            if (confirmed) {
                                                // Save the setting BEFORE restarting so it persists
                                                await invoke("save_setting", { key: 'app.paste_method', value: 'game_mode' });
                                                await invoke("restart_as_admin");
                                                return; // App will restart, no need to set state
                                            } else {
                                                // User declined, do not change setting
                                                return;
                                            }
                                        }
                                    } catch (err) {
                                        console.error("Failed to check admin status:", err);
                                    }
                                }

                                props.setPasteMethod(val);
                                invoke("save_setting", { key: 'app.paste_method', value: val }).catch(console.error);
                            }}
                        >
                            <option value="shift_insert">{props.t('paste_method_shift_insert')}</option>
                            <option value="ctrl_v">{props.t('paste_method_ctrl_v')}</option>
                            <option value="game_mode">{props.t('paste_method_game_mode')}</option>
                        </select>
                    </div>
                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('sequential_paste_mode')}
                            hint={props.t('sequential_paste_hint').replace('{hotkey}', props.sequentialHotkey || 'Alt+V')}
                            hintKey="sequential_paste_mode"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.sequentialMode}
                                onChange={(e) => {
                                    const val = e.target.checked;
                                    props.setSequentialModeState(val);
                                    invoke('set_sequential_mode', { enabled: val }).catch(console.error);
                                    if (val) {
                                        if (props.checkHotkeyConflict(props.sequentialHotkey, 'sequential')) {
                                            props.updateSequentialHotkey("");
                                        }
                                    }
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>

                    {props.sequentialMode && (
                        <div className="setting-item">
                            <div className="item-label-group">
                                <span className="item-label">{props.t('sequential_paste_hotkey_label')}</span>
                                <span className="hint">
                                    {props.isRecordingSequential ? (
                                        <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
                                            <span style={{ color: '#ff9800', fontWeight: 'bold' }}>
                                                {props.t('win_key_not_recommended')}
                                            </span>
                                            <span style={{ fontSize: '11px', opacity: 0.8 }}>
                                                {props.t('hotkey_recording_esc')}
                                            </span>
                                        </div>
                                    ) : props.t('hotkey_click_hint')}
                                </span>
                            </div>
                            <div
                                className={`key-group ${props.isRecordingSequential ? 'recording' : ''}`}
                                onClick={() => props.setIsRecordingSequential(true)}
                                tabIndex={0}
                                onKeyDown={(e) => {
                                    if (!props.isRecordingSequential) return;
                                    e.preventDefault();
                                    e.stopPropagation();

                                    if (e.key === 'Escape') {
                                        props.setIsRecordingSequential(false);
                                        return;
                                    }

                                    const modifiers = [];
                                    if (e.ctrlKey) modifiers.push('Ctrl');
                                    if (e.shiftKey) modifiers.push('Shift');
                                    if (e.altKey) modifiers.push('Alt');
                                    if (e.metaKey) return;

                                    const key = e.key.toUpperCase();
                                    if (['CONTROL', 'SHIFT', 'ALT', 'META'].includes(key)) return;

                                    const newHotkey = [...modifiers, key].join('+');
                                    props.updateSequentialHotkey(newHotkey);
                                }}
                            >
                                {props.isRecordingSequential ? (
                                    <div className="key-cap" style={{ width: '8em' }}>{props.t('waiting_for_input')}</div>
                                ) : (
                                    sequentialHotkeyParts.length > 0 ? (
                                        sequentialHotkeyParts.map((k, i) => (
                                            <div key={i} className="key-cap">{k}</div>
                                        ))
                                    ) : (
                                        <div className="key-cap" style={{ width: '8em', opacity: 0.5 }}>{props.t('not_set')}</div>
                                    )
                                )}
                            </div>
                        </div>
                    )}

                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('privacy_protection')}
                            hint={props.t('privacy_protection_hint')}
                            hintKey="privacy_protection"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.privacyProtection}
                                onChange={(e) => {
                                    const val = e.target.checked;
                                    props.setPrivacyProtection(val);
                                    invoke('set_privacy_protection', { enabled: val }).catch(console.error);
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>

                    <div className="setting-item" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '6px' }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                            <button
                                type="button"
                                className="btn-icon"
                                onClick={() => props.setPrivacyKindsOpen(!props.privacyKindsOpen)}
                                style={{ width: '24px', height: '24px' }}
                            >
                                {props.privacyKindsOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                            </button>
                            <span style={{ fontSize: '13px', fontWeight: '500', color: 'var(--text-primary)' }}>{props.t('privacy_protection_kinds')}</span>
                        </div>
                        {props.privacyKindsOpen && (
                            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px', marginLeft: '30px' }}>
                                {[
                                    { id: 'phone', label: props.t('privacy_kind_phone') },
                                    { id: 'idcard', label: props.t('privacy_kind_idcard') },
                                    { id: 'email', label: props.t('privacy_kind_email') },
                                    { id: 'secret', label: props.t('privacy_kind_secret') },
                                    { id: 'password', label: props.t('privacy_kind_password') || "Strong Password" },
                                ].map(opt => {
                                    const checked = props.privacyProtectionKinds.includes(opt.id);
                                    return (
                                        <label key={opt.id} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                                            <input
                                                className="cb"
                                                type="checkbox"
                                                checked={checked}
                                                onChange={(e) => {
                                                    const next = e.target.checked
                                                        ? [...props.privacyProtectionKinds, opt.id]
                                                        : props.privacyProtectionKinds.filter(t => t !== opt.id);
                                                    props.setPrivacyProtectionKinds(next);
                                                    invoke('set_privacy_protection_kinds', { kinds: next }).catch(console.error);
                                                }}
                                            />
                                            <span style={{ fontSize: '12px', color: 'var(--text-primary)' }}>{opt.label}</span>
                                        </label>
                                    );
                                })}
                            </div>
                        )}
                    </div>

                    <div className="setting-item" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: '6px' }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                            <button
                                type="button"
                                className="btn-icon"
                                onClick={() => props.setPrivacyRulesOpen(!props.privacyRulesOpen)}
                                style={{ width: '24px', height: '24px' }}
                            >
                                {props.privacyRulesOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                            </button>
                            <props.LabelWithHint
                                label={props.t('privacy_protection_custom_rules')}
                                hint={props.t('privacy_protection_custom_rules_hint')}
                                hintKey="privacy_protection_custom_rules"
                            />
                        </div>
                        {props.privacyRulesOpen && (
                            <textarea
                                className="search-input"
                                style={{ width: 'calc(100% - 30px)', maxWidth: '100%', minHeight: '80px', padding: '8px', borderRadius: '0', marginLeft: '30px', boxSizing: 'border-box' }}
                                placeholder={props.t('privacy_protection_custom_rules_placeholder')}
                                value={props.privacyProtectionCustomRules}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={(e) => {
                                    const val = e.target.value;
                                    props.setPrivacyProtectionCustomRules(val);
                                    invoke('set_privacy_protection_custom_rules', { rules: val }).catch(console.error);
                                }}
                            />
                        )}
                    </div>

                    <div className="setting-item no-border column">
                        <div className="item-label-group">
                            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                                <span className="item-label">{props.t('global_hotkey')}</span>
                            </div>
                            <span className="hint">
                                {props.isRecording ? (
                                    <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
                                        <span style={{ color: '#ff9800', fontWeight: 'bold' }}>
                                            {props.t('win_key_not_recommended')}
                                        </span>
                                        <span style={{ fontSize: '11px', opacity: 0.8 }}>
                                            {props.t('hotkey_recording_esc')}
                                        </span>
                                    </div>
                                ) : props.t('hotkey_click_hint')}
                            </span>
                        </div>

                        <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                            {props.mainHotkeys.map((item, idx) => (
                                <div key={`${item}-${idx}`} style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                                    <div className="key-group" style={{ flex: 1, cursor: 'default' }}>
                                        {item.split('+').map((k, i) => (
                                            <div key={i} className="key-cap">{k}</div>
                                        ))}
                                    </div>
                                    {!isWinVHotkey(item) && (
                                        <button
                                            type="button"
                                            className="hotkey-delete-btn"
                                            onClick={() => props.removeMainHotkey(item)}
                                            aria-label={`${props.t('delete')} ${item}`}
                                            title={`${props.t('delete')} ${item}`}
                                        >
                                            ×
                                        </button>
                                    )}
                                </div>
                            ))}

                            {props.mainHotkeys.length === 0 && !props.isRecording && (
                                <div className="key-group" style={{ cursor: 'default' }}>
                                    <div className="key-cap" style={{ width: '8em', opacity: 0.5 }}>{props.t('not_set')}</div>
                                </div>
                            )}

                            <div
                                className={`key-group ${props.isRecording ? 'recording' : ''}`}
                                onClick={() => props.setIsRecording(true)}
                                tabIndex={0}
                                onKeyDown={(e) => {
                                    if (!props.isRecording) return;
                                    e.preventDefault();
                                    e.stopPropagation();

                                    if (e.key === 'Escape') {
                                        props.setIsRecording(false);
                                        return;
                                    }

                                    const modifiers = [];
                                    if (e.ctrlKey) modifiers.push('Ctrl');
                                    if (e.shiftKey) modifiers.push('Shift');
                                    if (e.altKey) modifiers.push('Alt');
                                    if (e.metaKey) modifiers.push('Win');

                                    const key = e.key.toUpperCase();
                                    if (['CONTROL', 'SHIFT', 'ALT', 'META'].includes(key)) return;

                                    const newHotkey = [...modifiers, key].join('+');
                                    props.addMainHotkey(newHotkey);
                                }}
                            >
                                <div className="key-cap" style={{ width: '12em' }}>
                                    {props.isRecording ? props.t('waiting_for_input') : `+ ${props.t('global_hotkey')}`}
                                </div>
                            </div>
                        </div>
                    </div>

                    <div className="setting-item">
                        <props.LabelWithHint
                            label={props.t('use_win_v_shortcut')}
                            hint={props.t('use_win_v_shortcut_hint')}
                            hintKey="use_win_v_shortcut"
                        />
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={props.registryWinVEnabled}
                                onChange={async (e) => {
                                    const enabled = e.target.checked;
                                    const previousEnabled = !enabled;
                                    const matchedWinV = props.mainHotkeys.find((item) => isWinVHotkey(item));
                                    const hasWinV = !!matchedWinV;
                                    props.setRegistryWinVEnabled(enabled);
                                    try {
                                        if (enabled && !hasWinV) {
                                            const added = await props.addMainHotkey("Win+V", { skipAvailabilityCheck: true });
                                            if (!added) {
                                                props.setRegistryWinVEnabled(previousEnabled);
                                                return;
                                            }
                                        }
                                        if (!enabled && matchedWinV) {
                                            const removed = await props.removeMainHotkey(matchedWinV);
                                            if (!removed) {
                                                props.setRegistryWinVEnabled(previousEnabled);
                                                return;
                                            }
                                        }

                                        await invoke("save_setting", { key: 'app.use_win_v_shortcut', value: String(enabled) });
                                        const changed = await invoke("trigger_registry_win_v_optimization", { enable: enabled });

                                        if (changed) {
                                            const confirmed = await ask(
                                                props.t('restart_explorer_confirm'),
                                                { title: props.t('restart_explorer_title'), kind: 'warning' }
                                            );
                                            if (confirmed) {
                                                await invoke("restart_explorer");
                                                if (enabled) {
                                                    setTimeout(() => {
                                                        props.addMainHotkey("Win+V", { skipAvailabilityCheck: true });
                                                    }, 1500);
                                                }

                                                setTimeout(async () => {
                                                    try {
                                                        await invoke("set_theme", {
                                                            theme: props.theme,
                                                            color_mode: props.colorMode,
                                                            show_app_border: props.appSettings["app.show_app_border"] !== "false"
                                                        });
                                                    } catch (e) {
                                                        console.error("Failed to restore theme:", e);
                                                    }
                                                }, 2500);
                                            }
                                        }
                                    } catch (err) {
                                        if (enabled && !hasWinV) {
                                            await props.removeMainHotkey("Win+V");
                                        }
                                        if (!enabled && matchedWinV) {
                                            await props.addMainHotkey(matchedWinV, { skipAvailabilityCheck: true });
                                        }
                                        props.setRegistryWinVEnabled(previousEnabled);
                                        console.error(err);
                                        message(props.t('error') + `: ${err}`, { kind: 'error' });
                                    }
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                </div>
            )}
        </div>
    );
};

export default ClipboardSettingsGroup;
