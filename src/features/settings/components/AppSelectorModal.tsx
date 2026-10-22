import { open } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import AppSelector from "./AppSelector";
import AppModal from "../../../shared/components/AppModal";
import type { InstalledAppOption } from "../../app/types";
import { withNativeDialog } from "../../../shared/lib/focus";
import { isMacPlatform } from "../../../shared/lib/platform";

// macOS apps are .app bundles in /Applications; the Windows kinds can't be picked there.
const browseOptions = () => isMacPlatform()
    ? { defaultPath: "/Applications", filters: [{ name: "Applications", extensions: ["app"] }] }
    : { filters: [{ name: "Applications", extensions: ["exe", "cmd", "bat", "lnk"] }] };

interface AppSelectorModalProps {
    show: string | null;
    installedApps: InstalledAppOption[];
    theme: string;
    colorMode: string;
    t: (key: string) => string;
    onClose: () => void;
    onSave: (type: string, val: string) => void;
    /** Label of the button that clears the chosen app; hidden when none is chosen. */
    resetLabel?: string;
}

const AppSelectorModal = ({ show, installedApps, theme, colorMode, t, onClose, onSave, resetLabel }: AppSelectorModalProps) => (
    <AppModal
        open={Boolean(show)}
        onClose={onClose}
        theme={theme}
        panelClassName="modal-panel modal-panel-form modal-panel-wide modal-panel-app-selector"
        ariaLabel={t("select_app_title")}
    >
        <div className="modal-panel-header">
            <h3 className="modal-title">{t("select_app_title")}</h3>
            <button type="button" className="modal-button icon-only" onClick={onClose} aria-label={t("cancel")}>
                <X size={18} />
            </button>
        </div>

        <div className="selector-container">
            <AppSelector
                type={show || ""}
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

        <div className="modal-panel-actions split">
            {resetLabel && (
                <button
                    type="button"
                    className="modal-button app-selector-reset"
                    onClick={() => {
                        // An empty value is "nothing chosen": the backend opens the default.
                        if (show) onSave(show, "");
                        onClose();
                    }}
                >
                    {resetLabel}
                </button>
            )}
            <button
                type="button"
                className="modal-button"
                onClick={async () => {
                    try {
                        const selected = await withNativeDialog(() => open({
                            multiple: false,
                            ...browseOptions()
                        }), "settings:choose-application");
                        if (selected && show) {
                            onSave(show, selected as string);
                            onClose();
                        }
                    } catch (err) {
                        console.error(err);
                    }
                }}
            >
                {t("browse_file")}
            </button>
            <button type="button" className="modal-button" onClick={onClose}>
                {t("cancel")}
            </button>
        </div>
    </AppModal>
);

export default AppSelectorModal;
