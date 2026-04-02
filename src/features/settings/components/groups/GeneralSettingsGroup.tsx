import type { ComponentType, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight } from "lucide-react";

interface LabelWithHintProps {
    label: string;
    hint?: string | ReactNode;
    hintKey: string;
}

interface GeneralSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    LabelWithHint: ComponentType<LabelWithHintProps>;
    autoStart: boolean;
    setAutoStart: (val: boolean) => void;
    silentStart: boolean;
    setSilentStart: (val: boolean) => void;
    hideTrayIcon: boolean;
    setHideTrayIcon: (val: boolean) => void;
    edgeDocking: boolean;
    setEdgeDocking: (val: boolean) => void;
    followMouse: boolean;
    setFollowMouse: (val: boolean) => void;
    soundEnabled: boolean;
    setSoundEnabled: (val: boolean) => void;
    soundVolume: number;
    setSoundVolume: (val: number) => void;
    pasteSoundEnabled: boolean;
    setPasteSoundEnabled: (val: boolean) => void;
    showSearchBox: boolean;
    setShowSearchBox: (val: boolean) => void;
    scrollTopButtonEnabled: boolean;
    setScrollTopButtonEnabled: (val: boolean) => void;
    emojiPanelEnabled: boolean;
    setEmojiPanelEnabled: (val: boolean) => void;
    tagManagerEnabled: boolean;
    setTagManagerEnabled: (val: boolean) => void;
    arrowKeySelection: boolean;
    setArrowKeySelection: (val: boolean) => void;
    saveAppSetting: (key: string, val: string) => void;
}

const isLinux = navigator.platform.toLowerCase().includes('linux')
    || navigator.userAgent.toLowerCase().includes('linux');

const GeneralSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    LabelWithHint,
    autoStart,
    setAutoStart,
    silentStart,
    setSilentStart,
    hideTrayIcon,
    setHideTrayIcon,
    edgeDocking,
    setEdgeDocking,
    followMouse,
    setFollowMouse,
    soundEnabled,
    setSoundEnabled,
    soundVolume,
    setSoundVolume,
    pasteSoundEnabled,
    setPasteSoundEnabled,
    showSearchBox,
    setShowSearchBox,
    scrollTopButtonEnabled,
    setScrollTopButtonEnabled,
    emojiPanelEnabled,
    setEmojiPanelEnabled,
    tagManagerEnabled,
    setTagManagerEnabled,
    arrowKeySelection,
    setArrowKeySelection,
    saveAppSetting
}: GeneralSettingsGroupProps) => (
    <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
        <div className="group-header" onClick={onToggle}>
            <h3 style={{ margin: 0 }}>{t('general_settings')}</h3>
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
        </div>
        {!collapsed && (
            <div className="group-content">
                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('autostart')}</span>
                    </div>
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={autoStart}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setAutoStart(enabled);
                                invoke("toggle_autostart", { enabled }).catch(console.error);
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('hide_tray_icon')}</span>
                    </div>
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={hideTrayIcon}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setHideTrayIcon(val);
                                invoke("set_tray_visible", { visible: !val }).catch(console.error);
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                <div className="setting-item">
                    <LabelWithHint
                        label={t('edge_docking')}
                        hint={t('edge_docking_hint')}
                        hintKey="edge_docking"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={edgeDocking}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setEdgeDocking(val);
                                invoke("set_edge_docking", { enabled: val }).catch(console.error);
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('follow_mouse')}</span>
                    </div>
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={followMouse}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setFollowMouse(val);
                                invoke("set_follow_mouse", { enabled: val }).catch(console.error);
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('sound_effects') || "Sound Effects"}</span>
                    </div>
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={soundEnabled}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setSoundEnabled(enabled);
                                invoke("set_sound_enabled", { enabled }).catch(console.error);
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>
                {soundEnabled && (
                    <div className="setting-item" style={{ marginLeft: '18px' }}>
                        <div className="item-label-group" style={{ flex: 1 }}>
                            <span className="item-label">{t('sound_volume') || "Sound Volume"}</span>
                            <span className="item-desc">{soundVolume}%</span>
                        </div>
                        <input
                            type="range"
                            min="0"
                            max="100"
                            step="1"
                            value={soundVolume}
                            onChange={(e) => {
                                const volume = Number(e.target.value);
                                setSoundVolume(volume);
                            }}
                            style={{ width: '140px' }}
                        />
                    </div>
                )}
                {soundEnabled && (
                    <div className="setting-item" style={{ marginLeft: '18px' }}>
                        <div className="item-label-group">
                            <span className="item-label">{t('paste_sound') || "Paste Sound"}</span>
                        </div>
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={pasteSoundEnabled}
                                onChange={(e) => {
                                    const enabled = e.target.checked;
                                    setPasteSoundEnabled(enabled);
                                    invoke("save_setting", { key: 'app.sound_paste_enabled', value: String(enabled) }).catch(console.error);
                                }}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                )}


                <div className="setting-item">
                    <LabelWithHint
                        label={t('silent_start')}
                        hint={t('silent_start_hint')}
                        hintKey="silent_start"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={silentStart}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setSilentStart(enabled);
                                invoke("set_silent_start", { enabled }).catch(console.error);
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>
                <div className="setting-item">
                    <LabelWithHint
                        label={t('show_search_box')}
                        hint={t('show_search_box_hint')}
                        hintKey="show_search_box"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={showSearchBox}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setShowSearchBox(enabled);
                                saveAppSetting('show_search_box', String(enabled));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>
                <div className="setting-item">
                    <LabelWithHint
                        label={t('scroll_top_button')}
                        hint={t('scroll_top_button_hint')}
                        hintKey="scroll_top_button"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={scrollTopButtonEnabled}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setScrollTopButtonEnabled(enabled);
                                saveAppSetting('show_scroll_top_button', String(enabled));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>
                <div className="setting-item">
                    <LabelWithHint
                        label={t('emoji_panel_enabled') || '表情包开关'}
                        hint={t('emoji_panel_enabled_hint') || '关闭后隐藏表情包入口'}
                        hintKey="emoji_panel_enabled"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={emojiPanelEnabled}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setEmojiPanelEnabled(enabled);
                                saveAppSetting('emoji_panel_enabled', String(enabled));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>
                <div className="setting-item">
                    <LabelWithHint
                        label={t('tag_manager_enabled') || '标签管理页开关'}
                        hint={t('tag_manager_enabled_hint') || '关闭后隐藏标签管理入口'}
                        hintKey="tag_manager_enabled"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={tagManagerEnabled}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setTagManagerEnabled(enabled);
                                saveAppSetting('tag_manager_enabled', String(enabled));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>
                <div className="setting-item">
                    <LabelWithHint
                        label={t('arrow_key_selection')}
                        hint={t('arrow_key_selection_hint')}
                        hintKey="arrow_key_selection"
                    />
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={arrowKeySelection}
                            onChange={(e) => {
                                const enabled = e.target.checked;
                                setArrowKeySelection(enabled);
                                saveAppSetting('arrow_key_selection', String(enabled));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>

                {/* Restart as Admin button */}
                {!isLinux && (
                    <div className="setting-item">
                        <LabelWithHint
                            label={t('restart_as_admin') || "Restart as Admin"}
                            hint={t('restart_as_admin_hint') || "Restart with administrator privileges to paste into admin terminals"}
                            hintKey="restart_as_admin"
                        />
                        <button
                            className="setting-btn"
                            onClick={() => {
                                invoke("restart_as_admin").catch((err) => {
                                    console.error("Failed to restart as admin:", err);
                                });
                            }}
                            style={{
                                padding: '4px 12px',
                                fontSize: '12px',
                                cursor: 'pointer',
                                borderRadius: '6px',
                                border: '1px solid rgba(128, 128, 128, 0.4)',
                                background: 'var(--bg-secondary)',
                                color: 'var(--text-primary)',
                                boxShadow: '0 1px 2px rgba(0, 0, 0, 0.1)',
                            }}
                        >
                            {t('restart') || "Restart"}
                        </button>
                    </div>
                )}
            </div>
        )}
    </div>
);

export default GeneralSettingsGroup;
