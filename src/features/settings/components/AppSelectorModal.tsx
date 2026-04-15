import { AnimatePresence, motion } from "framer-motion";
import { open } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import AppSelector from "./AppSelector";
import type { InstalledAppOption } from "../../app/types";

interface AppSelectorModalProps {
    show: string | null;
    installedApps: InstalledAppOption[];
    theme: string;
    colorMode: string;
    t: (key: string) => string;
    onClose: () => void;
    onSave: (type: string, val: string) => void;
}

const AppSelectorModal = ({ show, installedApps, theme, colorMode, t, onClose, onSave }: AppSelectorModalProps) => (
    <AnimatePresence>
        {show && (
            <div className="modal-overlay" onClick={onClose}>
                <motion.div
                    initial={{ scale: 0.9, opacity: 0 }}
                    animate={{ scale: 1, opacity: 1 }}
                    exit={{ scale: 0.9, opacity: 0 }}
                    className="modal-content"
                    style={{
                        width: '92%',
                        maxWidth: '500px',
                        gap: '12px',
                        display: 'flex',
                        flexDirection: 'column',
                        maxHeight: '90vh',
                        overflowY: 'auto'
                    }}
                    onClick={(e) => e.stopPropagation()}
                >
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
                        <h3 className="modal-title">{t('select_app_title')}</h3>
                        <button className="btn-icon" onClick={onClose} style={{ border: 'none', background: 'transparent', boxShadow: 'none' }}>
                            <X size={18} />
                        </button>
                    </div>

                    <div className="selector-container">
                        <AppSelector
                            type={show}
                            installedApps={installedApps}
                            theme={theme}
                            colorMode={colorMode}
                            onSelect={(val) => {
                                if (show) onSave(show, val);
                                onClose();
                            }}
                            t={t}
                        />
                    </div>

                    <div style={{ display: 'flex', gap: '12px' }}>
                        <button
                            className="btn-icon"
                            onClick={async () => {
                                try {
                                    const selected = await open({
                                        multiple: false,
                                        filters: [{
                                            name: 'Applications',
                                            extensions: ['exe', 'cmd', 'bat', 'lnk']
                                        }]
                                    });
                                    if (selected && show) {
                                        onSave(show, selected as string);
                                        onClose();
                                    }
                                } catch (err) { console.error(err); }
                            }}
                            style={{ flex: 1, height: '36px', fontSize: '13px', fontWeight: 'bold', textTransform: 'uppercase' }}
                        >
                            {t('browse_file')}
                        </button>
                        <button
                            className="btn-icon"
                            onClick={onClose}
                            style={{ flex: 1, height: '36px', fontSize: '13px', fontWeight: 'bold', textTransform: 'uppercase', background: '#ff4d4f', color: '#fff', border: '2px solid #333' }}
                        >
                            {t('cancel')}
                        </button>
                    </div>
                </motion.div>
            </div>
        )}
    </AnimatePresence>
);

export default AppSelectorModal;
