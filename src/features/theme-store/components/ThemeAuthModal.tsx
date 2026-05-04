import { memo, useState, useCallback } from "react";
import { X } from "lucide-react";

interface ThemeAuthModalProps {
  onClose: () => void;
  onLogin: (username: string, password: string) => Promise<unknown>;
  onRegister: (username: string, password: string) => Promise<unknown>;
  t: (key: string) => string;
}

const ThemeAuthModal = ({
  onClose,
  onLogin,
  onRegister,
  t,
}: ThemeAuthModalProps) => {
  const [tab, setTab] = useState<"login" | "register">("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = useCallback(async () => {
    setError("");
    setLoading(true);
    try {
      if (tab === "login") {
        await onLogin(username, password);
      } else {
        await onRegister(username, password);
      }
      onClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed");
    } finally {
      setLoading(false);
    }
  }, [tab, username, password, onLogin, onRegister, onClose]);

  return (
    <div className="theme-auth-overlay" onClick={onClose}>
      <div
        className="theme-auth-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span className="theme-auth-title">{t("theme_store_auth")}</span>
          <button
            type="button"
            className="theme-store-back"
            onClick={onClose}
          >
            <X size={14} />
          </button>
        </div>

        <div className="theme-auth-tabs">
          <button
            type="button"
            className={tab === "login" ? "active" : ""}
            onClick={() => { setTab("login"); setError(""); }}
          >
            {t("theme_store_login")}
          </button>
          <button
            type="button"
            className={tab === "register" ? "active" : ""}
            onClick={() => { setTab("register"); setError(""); }}
          >
            {t("theme_store_register")}
          </button>
        </div>

        <div className="theme-auth-field">
          <label>{t("theme_store_username")}</label>
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            maxLength={20}
            autoFocus
          />
        </div>

        <div className="theme-auth-field">
          <label>{t("theme_store_password")}</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            maxLength={100}
          />
        </div>

        {error && <div className="theme-auth-error">{error}</div>}

        <div className="theme-auth-actions">
          <button type="button" onClick={onClose}>
            {t("cancel")}
          </button>
          <button
            type="button"
            className="primary"
            onClick={handleSubmit}
            disabled={loading || !username || !password}
          >
            {loading
              ? "..."
              : tab === "login"
                ? t("theme_store_login")
                : t("theme_store_register")}
          </button>
        </div>
      </div>
    </div>
  );
};

export default memo(ThemeAuthModal);
