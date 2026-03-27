import { AnimatePresence, motion } from "framer-motion";
import { open } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import AppSelector from "./AppSelector";
import type { InstalledAppOption } from "../../app/types";

interface AppSelectorModalProps {
    show: string | null;
    installedApps: InstalledAppOption[];
    t: (key: string) => string;
    onClose: () => void;
    onSave: (type: string, val: string) => void;
}

const AppSelectorModal = ({ show, installedApps, t, onClose, onSave }: AppSelectorModalProps) => (
    <AnimatePresence>
        {show && (
            <div className="modal-overlay" onClick={onClose}>
                <motion.div
                    initial={{ scale: 0.9, opacity: 0 }}
                    animate={{ scale: 1, opacity: 1 }}
                    exit={{ scale: 0.9, opacity: 0 }}
                    className="modal-content app-selector-modal"
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
                    <div className="app-selector-modal-header">
                        <h3 className="modal-title">{t('select_app_title')}</h3>
                        <button className="btn-icon btn-icon-scalable btn-icon-size-header app-selector-close-btn" onClick={onClose}>
                            <X size={18} />
                        </button>
                    </div>

                    <div className="selector-container">
                        <AppSelector
                            type={show}
                            installedApps={installedApps}
                            onSelect={(val) => {
                                if (show) onSave(show, val);
                                onClose();
                            }}
                            t={t}
                        />
                    </div>

                    <div className="app-selector-modal-actions">
                        <button
                            className="btn-icon app-selector-action-btn app-selector-action-btn-browse"
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
                        >
                            {t('browse_file')}
                        </button>
                        <button
                            className="btn-icon app-selector-action-btn app-selector-action-btn-cancel"
                            onClick={onClose}
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
