import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight, Edit2, RotateCcw, Trash2 } from "lucide-react";
import type { AiProfile, AiProfileStatusMap, EditableAiProfile } from "../../types";

interface AiSettingsGroupProps {
    t: (key: string) => string;
    collapsed: boolean;
    onToggle: () => void;
    aiEnabled: boolean;
    setAiEnabled: (val: boolean) => void;
    saveSetting: (key: string, val: string) => void;
    aiProfiles: AiProfile[];
    profileStatuses: AiProfileStatusMap;
    checkModelStatus: (profile: AiProfile) => void;
    setEditingProfile: (profile: EditableAiProfile) => void;
    handleDeleteProfile: (id: string) => void;
    aiAssignedProfileTask: string;
    setAiAssignedProfileTask: (id: string) => void;
    aiAssignedProfileMouthpiece: string;
    setAiAssignedProfileMouthpiece: (id: string) => void;
    aiAssignedProfileTranslate: string;
    setAiAssignedProfileTranslate: (id: string) => void;
    aiTargetLang: string;
    setAiTargetLang: (val: string) => void;
    aiThinkingBudget: string;
    setAiThinkingBudget: (val: string) => void;
    theme: string;
}

const AiSettingsGroup = ({
    t,
    collapsed,
    onToggle,
    aiEnabled,
    setAiEnabled,
    saveSetting,
    aiProfiles,
    profileStatuses,
    checkModelStatus,
    setEditingProfile,
    handleDeleteProfile,
    aiAssignedProfileTask,
    setAiAssignedProfileTask,
    aiAssignedProfileMouthpiece,
    setAiAssignedProfileMouthpiece,
    aiAssignedProfileTranslate,
    setAiAssignedProfileTranslate,
    aiTargetLang,
    setAiTargetLang,
    aiThinkingBudget,
    setAiThinkingBudget,
    theme
}: AiSettingsGroupProps) => (
    <div className={`settings-group ${collapsed ? 'collapsed' : ''}`}>
        <div className="group-header" onClick={onToggle}>
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <h3 style={{ margin: 0 }}>{t('ai_settings')}</h3>
            </div>
            {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
        </div>
        {!collapsed && (
            <div className="group-content">
                <div className="setting-item">
                    <div className="item-label-group">
                        <span className="item-label">{t('enable_ai')}</span>
                    </div>
                    <label className="switch">
                        <input
                            className="cb"
                            type="checkbox"
                            checked={aiEnabled}
                            onChange={(e) => {
                                const val = e.target.checked;
                                setAiEnabled(val);
                                saveSetting('ai_enabled', String(val));
                            }}
                        />
                        <div className="toggle"><div className="left" /><div className="right" /></div>
                    </label>
                </div>


                {aiEnabled && (
                    <>
                        <span className="ai-sub-label">{t('ai_model_library')}</span>
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '8px', padding: '0 4px' }}>
                            <button
                                className="btn-icon"
                                style={{ padding: '4px 12px', fontSize: '11px', marginLeft: 'auto', height: '24px' }}
                                onClick={() => setEditingProfile({ isNew: true, baseUrl: 'https://api.longcat.chat/openai/v1', apiKey: '', model: '', enableThinking: false })}
                            >
                                {t('ai_add_model')}
                            </button>
                        </div>

                        <div style={{
                            display: 'flex',
                            flexDirection: 'column',
                            padding: '0',
                            marginBottom: '16px',
                            background: 'rgba(0, 0, 0, 0.02)',
                            borderRadius: theme === 'retro' ? '0' : '8px',
                            border: theme === 'retro' ? '2px solid var(--border-dark)' : '1px solid rgba(128, 128, 128, 0.1)',
                            overflow: 'hidden'
                        }}>
                            {aiProfiles.map(profile => (
                                <div key={profile.id} className="ai-profile-card" style={{ display: 'flex', alignItems: 'center', padding: '10px' }}>
                                    <div
                                        style={{
                                            width: '8px',
                                            height: '8px',
                                            borderRadius: '50%',
                                            backgroundColor:
                                                profileStatuses[profile.id] === 'success' ? '#4CAF50' :
                                                    profileStatuses[profile.id] === 'error' ? '#F44336' :
                                                        profileStatuses[profile.id] === 'loading' ? '#FF9800' : '#999',
                                            marginRight: '12px',
                                            flexShrink: 0,
                                            boxShadow: profileStatuses[profile.id] === 'none' ? 'none' : '0 0 4px rgba(0,0,0,0.2)'
                                        }}
                                        title={profileStatuses[profile.id] || 'Unknown'}
                                    />
                                    <div style={{ flex: 1, minWidth: 0 }}>
                                        <div className="ai-profile-name" style={{ fontWeight: '800', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{profile.model}</div>
                                        {profile.enableThinking && <div className="ai-profile-sub" style={{ fontSize: '9px', opacity: 0.7 }}>Thinking Mode Enabled</div>}
                                    </div>
                                    <div style={{ display: 'flex', gap: '4px', marginLeft: '8px' }}>
                                        <button className="btn-icon" onClick={() => checkModelStatus(profile)} title="Check Connection">
                                            <RotateCcw size={12} className={profileStatuses[profile.id] === 'loading' ? 'animate-spin' : ''} />
                                        </button>
                                        <button className="btn-icon" onClick={() => setEditingProfile(profile)}><Edit2 size={12} /></button>
                                        {!['lc_flash_v1', 'lc_think_v1', 'lc_think_2601_v1'].includes(profile.id) && (
                                            <button className="btn-icon" onClick={() => handleDeleteProfile(profile.id)} style={{ color: '#f44336' }}><Trash2 size={12} /></button>
                                        )}
                                    </div>
                                </div>
                            ))}
                        </div>

                        <span className="ai-sub-label">{t('ai_strategy_settings')}</span>

                        {[
                            { label: t('ai_strategy_task'), value: aiAssignedProfileTask, setter: setAiAssignedProfileTask, key: 'ai_assigned_profile_task' },
                            { label: t('ai_strategy_mouthpiece'), value: aiAssignedProfileMouthpiece, setter: setAiAssignedProfileMouthpiece, key: 'ai_assigned_profile_mouthpiece' },
                        ].map(strategy => (
                            <div className="setting-item" key={strategy.key}>
                                <div className="item-label-group">
                                    <span className="item-label">{strategy.label}</span>
                                </div>
                                <select
                                    className="search-input"
                                    style={{ borderRadius: '0', padding: '6px', width: '160px', background: 'var(--bg-input)', border: '2px solid var(--border-dark)', color: 'var(--text-primary)', fontSize: '12px' }}
                                    value={strategy.value}
                                    onChange={e => {
                                        strategy.setter(e.target.value);
                                        saveSetting(strategy.key, e.target.value);
                                    }}
                                >
                                    <option value="none" disabled>{aiProfiles.length > 0 ? t('select_a_profile') : t('add_profile_first')}</option>
                                    {aiProfiles.map(p => (
                                        <option key={p.id} value={p.id}>{p.model}</option>
                                    ))}
                                </select>
                            </div>
                        ))}

                        <div className="setting-item no-border" style={{ paddingBottom: 0 }}>
                            <div className="item-label-group">
                                <span className="item-label">{t('ai_strategy_translate')}</span>
                            </div>
                            <select
                                className="search-input"
                                style={{ borderRadius: '0', padding: '6px', width: '160px', background: 'var(--bg-input)', border: '2px solid var(--border-dark)', color: 'var(--text-primary)', fontSize: '12px' }}
                                value={aiAssignedProfileTranslate}
                                onChange={e => {
                                    setAiAssignedProfileTranslate(e.target.value);
                                    saveSetting('ai_assigned_profile_translate', e.target.value);
                                }}
                            >
                                <option value="none" disabled>{aiProfiles.length > 0 ? t('select_a_profile') : t('add_profile_first')}</option>
                                {aiProfiles.map(p => (
                                    <option key={p.id} value={p.id}>{p.model}</option>
                                ))}
                            </select>
                        </div>

                        <div className="setting-item">
                            <div className="item-label-group">
                                <span className="item-label">{t('ai_target_lang')}</span>
                            </div>
                            <select
                                className="search-input"
                                style={{ borderRadius: '0', padding: '6px', width: '160px', background: 'var(--bg-input)', border: '2px solid var(--border-dark)', color: 'var(--text-primary)', fontSize: '12px' }}
                                value={aiTargetLang}
                                onChange={e => {
                                    setAiTargetLang(e.target.value);
                                    saveSetting('ai_target_lang', e.target.value);
                                }}
                            >
                                <option value="auto_zh_en">{t('lang_auto_zh_en')}</option>
                                <option value="zh">{t('lang_zh')}</option>
                                <option value="en">{t('lang_en')}</option>
                                <option value="ja">{t('lang_ja')}</option>
                                <option value="de">{t('lang_de')}</option>
                                <option value="fr">{t('lang_fr')}</option>
                            </select>
                        </div>

                        <div className="setting-item no-border">
                            <div className="item-label-group">
                                <span className="item-label">{t('ai_thinking_budget')}</span>
                            </div>
                            <input
                                className="search-input"
                                style={{ borderRadius: '0', padding: '6px', width: '160px' }}
                                type="number"
                                min="1024"
                                max="10000"
                                value={aiThinkingBudget}
                                onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                                onChange={e => {
                                    setAiThinkingBudget(e.target.value);
                                    saveSetting('ai_thinking_budget', e.target.value);
                                }}
                            />
                        </div>
                    </>
                )}
            </div>
        )}
    </div>
);

export default AiSettingsGroup;
