import { createContext, memo, useCallback, useContext, useMemo, useState, type ReactNode } from "react";
import { HelpCircle } from "lucide-react";

interface SettingsHintState {
    openHints: Set<string>;
    toggleHint: (key: string) => void;
}

const SettingsHintContext = createContext<SettingsHintState | null>(null);

export const SettingsHintProvider = ({ children }: { children: ReactNode }) => {
    const [openHints, setOpenHints] = useState<Set<string>>(new Set());
    const toggleHint = useCallback((key: string) => {
        setOpenHints(prev => {
            const next = new Set(prev);
            if (next.has(key)) next.delete(key);
            else next.add(key);
            return next;
        });
    }, []);
    const value = useMemo(() => ({ openHints, toggleHint }), [openHints, toggleHint]);

    return <SettingsHintContext.Provider value={value}>{children}</SettingsHintContext.Provider>;
};

// The copy puts each short sentence on its own line. In a full-width panel that
// left a column of half-empty lines, so sentences flow into one paragraph; a
// line without closing punctuation (a list such as ports) keeps its break.
const flowSentences = (text: string) =>
    text.replace(/([。！？；.!?;])\n(?=\S)/g, (_, end: string) => (/[.!?;]/.test(end) ? `${end} ` : end));

// Keep this component type stable during live slider updates. Remounting labels
// briefly shrinks the scroll pane, making WebKit clamp its scroll position.
export const LabelWithHint = memo(({ label, hint, hintKey }: {
    label: string;
    hint?: string | ReactNode;
    hintKey: string;
}) => {
    const state = useContext(SettingsHintContext);
    const [summary, ...rest] = typeof hint === "string" ? hint.split("\n") : [];
    const details = typeof hint === "string" ? (rest.length ? rest.join("\n") : undefined) : hint;
    const open = state?.openHints.has(hintKey);

    return (
        <>
            <div className="item-label-group">
                <div className="item-label-row">
                    <span className="item-label">{label}</span>
                    {details && (
                        <button
                            type="button"
                            className="hint-icon-btn"
                            aria-expanded={open}
                            aria-label={label}
                            title={typeof details === "string" ? details : undefined}
                            onClick={(e) => {
                                e.stopPropagation();
                                state?.toggleHint(hintKey);
                            }}
                        >
                            <HelpCircle size={13} />
                        </button>
                    )}
                </div>
                {/* The first line is always visible: it says what the setting does or
                    costs. The ? button only opens the rest. */}
                {summary && <span className="item-desc">{summary}</span>}
            </div>
            {/* A sibling of the label column, not inside it: the row wraps it onto
                its own full-width line instead of squeezing it beside the control. */}
            {details && open && (
                <div className="setting-hint-panel">
                    {typeof details === "string" ? <span className="hint">{flowSentences(details)}</span> : details}
                </div>
            )}
        </>
    );
});
