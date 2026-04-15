import { AnimatePresence, motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import type { EditableAiProfile } from "../types";

interface AiProfileModalProps {
    editingProfile: EditableAiProfile | null;
    t: (key: string) => string;
    onClose: () => void;
    onSave: (profile: EditableAiProfile) => void;
    setEditingProfile: (val: EditableAiProfile) => void;
}

const AiProfileModal = ({ editingProfile, t, onClose, onSave, setEditingProfile }: AiProfileModalProps) => (
    <AnimatePresence>
        {editingProfile && (
            <div className="modal-overlay" onClick={onClose}>
                <motion.div
                    initial={{ scale: 0.9, opacity: 0 }}
                    animate={{ scale: 1, opacity: 1 }}
                    exit={{ scale: 0.9, opacity: 0 }}
                    className="modal-content"
                    style={{
                        width: '92%',
                        maxWidth: '340px',
                        gap: '12px',
                        display: 'flex',
                        flexDirection: 'column',
                        maxHeight: '90vh',
                        overflowY: 'auto'
                    }}
                    onClick={(e) => e.stopPropagation()}
                >
                    <h3 className="modal-title">{editingProfile.isNew ? t('ai_add_model') : t('ai_edit_model')}</h3>

                    <div>
                        <div style={{ fontSize: '11px', fontWeight: 'bold', marginBottom: '4px', textTransform: 'uppercase' }}>Endpoint URL</div>
                        <input
                            className="search-input"
                            style={{ width: '100%', borderRadius: '0' }}
                            value={editingProfile.baseUrl}
                            onChange={e => setEditingProfile({ ...editingProfile, baseUrl: e.target.value })}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                        />
                    </div>
                    <div>
                        <div style={{ fontSize: '11px', fontWeight: 'bold', marginBottom: '4px', textTransform: 'uppercase' }}>API Key</div>
                        <input
                            className="search-input"
                            type="password"
                            style={{ width: '100%', borderRadius: '0' }}
                            value={editingProfile.apiKey}
                            onChange={e => setEditingProfile({ ...editingProfile, apiKey: e.target.value })}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                        />
                    </div>
                    <div>
                        <div style={{ fontSize: '11px', fontWeight: 'bold', marginBottom: '4px', textTransform: 'uppercase' }}>Model ID</div>
                        <input
                            className="search-input"
                            style={{ width: '100%', borderRadius: '0' }}
                            value={editingProfile.model}
                            onChange={e => setEditingProfile({ ...editingProfile, model: e.target.value })}
                            onFocus={() => invoke("focus_clipboard_window").catch(console.error)}
                            placeholder="gpt-4o"
                        />
                    </div>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                        <span style={{ fontSize: '13px', fontWeight: 'bold' }}>{t('ai_thinking_enabled')}</span>
                        <label className="switch">
                            <input
                                className="cb"
                                type="checkbox"
                                checked={editingProfile.enableThinking}
                                onChange={e => setEditingProfile({ ...editingProfile, enableThinking: e.target.checked })}
                            />
                            <div className="toggle"><div className="left" /><div className="right" /></div>
                        </label>
                    </div>
                    <div style={{ display: 'flex', gap: '10px', marginTop: '8px' }}>
                        <button className="btn-icon" style={{ flex: 1, padding: '10px', textTransform: 'uppercase', fontWeight: 'bold' }} onClick={onClose}>{t('cancel')}</button>
                        <button className="btn-icon active" style={{ flex: 1, padding: '10px', textTransform: 'uppercase', fontWeight: 'bold' }} onClick={() => onSave(editingProfile)}>{t('ai_save_profile')}</button>
                    </div>
                </motion.div>
            </div>
        )}
    </AnimatePresence>
);

export default AiProfileModal;
