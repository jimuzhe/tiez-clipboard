import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ChevronDown, ChevronRight } from "lucide-react";
import { withNativeDialog } from "../../../shared/lib/focus";
import { formatError } from "../../../shared/lib/errorMessage";

interface SecureTransferProps {
  t: (key: string) => string;
  collapsed: boolean;
  onToggle: () => void;
}

export default function SecureTransfer({ t, collapsed, onToggle }: SecureTransferProps) {
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");

  const run = async (importing: boolean) => {
    setStatus("");
    if (new TextEncoder().encode(password).length < 12) {
      setStatus(t("secure_transfer_password_short"));
      return;
    }
    if (!importing && password !== confirmation) {
      setStatus(t("secure_transfer_password_mismatch"));
      return;
    }
    if (importing && !acknowledged) {
      setStatus(t("secure_transfer_ack_required"));
      return;
    }
    setBusy(true);
    try {
      const filters = [{ name: t("secure_transfer_file_filter"), extensions: ["tiezx"] }];
      const path = await withNativeDialog(
        () => importing ? open({ filters, multiple: false }) : save({ filters, defaultPath: "TieZ.tiezx" }),
        "settings:secure-transfer"
      );
      if (!path || typeof path !== "string") return;
      const result = await invoke<{ transferred: number; excluded: number }>(
        importing ? "import_secure_transfer" : "export_secure_transfer",
        { path, password, acknowledgePlaintext: acknowledged }
      );
      // The list reloads on this event, so the imported records show up at once.
      if (importing) await emit("clipboard-changed").catch(console.error);
      setStatus(
        t(importing ? "secure_transfer_success_import" : "secure_transfer_success_export")
          .replace("{n}", String(result.transferred))
          .replace("{x}", String(result.excluded))
      );
    } catch (error) {
      setStatus(formatError(error, t));
    } finally {
      setBusy(false);
      setPassword("");
      setConfirmation("");
    }
  };

  return (
    <div className={`settings-group ${collapsed ? "collapsed" : ""}`}>
      <button
        type="button"
        className="group-header"
        aria-expanded={!collapsed}
        onClick={onToggle}
      >
        <h3>{t("secure_transfer")}</h3>
        {collapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
      </button>
      {!collapsed && (
        <div className="group-content secure-transfer">
          <p className="secure-transfer-description">{t("secure_transfer_desc").replace(/\n/g, "")}</p>
          <label className="secure-transfer-field">
            <span className="item-label">{t("secure_transfer_password")}</span>
            <input
              className="search-input"
              type="password"
              autoComplete="off"
              value={password}
              disabled={busy}
              onChange={(event) => setPassword(event.target.value)}
            />
          </label>
          <label className="secure-transfer-field">
            <span className="item-label">{t("secure_transfer_password_confirm")}</span>
            <input
              className="search-input"
              type="password"
              autoComplete="off"
              value={confirmation}
              disabled={busy}
              onChange={(event) => setConfirmation(event.target.value)}
            />
          </label>
          <label className="secure-transfer-ack">
            <input
              type="checkbox"
              checked={acknowledged}
              disabled={busy}
              onChange={(event) => setAcknowledged(event.target.checked)}
            />
            <span>{t("secure_transfer_ack").replace(/\n/g, "")}</span>
          </label>
          <div className="secure-transfer-actions">
            <button type="button" className="settings-action-button" disabled={busy} onClick={() => void run(false)}>
              {t("secure_transfer_export")}
            </button>
            <button type="button" className="settings-action-button" disabled={busy} onClick={() => void run(true)}>
              {t("secure_transfer_import")}
            </button>
          </div>
          {(busy || status) && (
            <p className="secure-transfer-status" role="status">
              {busy ? t("secure_transfer_busy") : status}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
