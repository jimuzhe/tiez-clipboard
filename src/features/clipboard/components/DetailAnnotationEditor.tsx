import { useEffect, useMemo, useRef, useState } from "react";
import { Plus, X } from "lucide-react";
import type { ClipboardEntry } from "../../../shared/types";
import { activateForInput } from "../../../shared/lib/focus";
import { getNotePlaceholder, isMacPlatform } from "../../../shared/lib/platform";
import type { DetailPaneActions } from "./ClipboardDetailPane";

interface EditorProps {
  item: ClipboardEntry;
  t: (key: string) => string;
  actions: DetailPaneActions;
  isEditingTags: boolean;
  tagInput: string;
  tagSuggestions: string[];
  onTagInput: (value: string) => void;
  noteDraft?: string;
  onNoteDraftChange: (value: string) => void;
}

const DetailTagEditor = ({ item, t, actions, tagInput, tagSuggestions, onTagInput }: EditorProps) => {
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const composingRef = useRef(false);
  const [selectedSuggestion, setSelectedSuggestion] = useState(-1);
  const suggestions = useMemo(() => {
    const query = tagInput.trim().toLowerCase();
    return tagSuggestions.filter((tag) => !item.tags?.includes(tag) && (!query || tag.toLowerCase().includes(query))).slice(0, 14);
  }, [tagInput, tagSuggestions, item.tags]);
  const listId = `detail-tag-suggestions-${item.id}`;

  useEffect(() => {
    let alive = true;
    void activateForInput().catch(() => {}).finally(() => {
      if (alive) inputRef.current?.focus();
    });
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    const onOutside = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) actions.setTagEditor(item, false);
    };
    document.addEventListener("mousedown", onOutside);
    return () => document.removeEventListener("mousedown", onOutside);
  }, [item, actions]);

  useEffect(() => {
    if (selectedSuggestion >= 0) {
      document.getElementById(`${listId}-${selectedSuggestion}`)?.scrollIntoView({ block: "nearest" });
    }
  }, [selectedSuggestion, listId]);

  return (
    <div className="detail-tag-editor" ref={rootRef}>
      <div className="detail-editor-heading">
        <strong>{t("edit_tags")}</strong>
        <button type="button" className="btn-icon" aria-label={t("cancel")} title={t("cancel")} onClick={() => actions.setTagEditor(item, false)}><X size={14} /></button>
      </div>
      {!!item.tags?.length && (
        <div className="detail-editor-tags">
          {item.tags.map((tag) => (
            <span className="detail-editor-tag" key={tag}>
              <span>{tag}</span>
              <button type="button" aria-label={`${t("delete")} ${tag}`} title={`${t("delete")} ${tag}`} onClick={() => actions.deleteTag(item, tag)}><X size={12} /></button>
            </span>
          ))}
        </div>
      )}
      <div className="detail-tag-input-row">
        <input
          ref={inputRef}
          className="tag-input"
          value={tagInput}
          placeholder={t("enter_tag_name")}
          aria-label={t("enter_tag_name")}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={suggestions.length > 0}
          aria-controls={suggestions.length ? listId : undefined}
          aria-activedescendant={selectedSuggestion >= 0 && selectedSuggestion < suggestions.length ? `${listId}-${selectedSuggestion}` : undefined}
          onCompositionStart={() => { composingRef.current = true; }}
          onCompositionEnd={(event) => { composingRef.current = false; onTagInput(event.currentTarget.value); setSelectedSuggestion(-1); }}
          onChange={(event) => { onTagInput(event.target.value); setSelectedSuggestion(-1); }}
          onKeyDown={(event) => {
            if (composingRef.current || event.nativeEvent.isComposing) return;
            if (event.key === "Escape") {
              event.preventDefault(); event.stopPropagation(); actions.setTagEditor(item, false);
            } else if (event.key === "ArrowDown" && suggestions.length) {
              event.preventDefault(); event.stopPropagation(); setSelectedSuggestion((index) => Math.min(index + 1, suggestions.length - 1));
            } else if (event.key === "ArrowUp" && suggestions.length) {
              event.preventDefault(); event.stopPropagation(); setSelectedSuggestion((index) => Math.max(-1, index - 1));
            } else if (event.key === "Enter") {
              event.preventDefault(); event.stopPropagation(); actions.addTag(item, suggestions[selectedSuggestion] ?? tagInput);
            }
          }}
        />
        <button type="button" className="btn-icon" aria-label={t("add_tag")} title={t("add_tag")} onClick={() => actions.addTag(item, tagInput)}><Plus size={14} /></button>
      </div>
      {suggestions.length > 0 && (
        <div className="detail-tag-suggestions" id={listId} role="listbox" aria-label={t("find_tags")}>
          {suggestions.map((tag, index) => (
            <button
              key={tag}
              id={`${listId}-${index}`}
              type="button"
              role="option"
              aria-selected={selectedSuggestion === index}
              onMouseDown={(event) => event.preventDefault()}
              onMouseEnter={() => setSelectedSuggestion(index)}
              onClick={() => actions.addTag(item, tag)}
            >{tag}</button>
          ))}
        </div>
      )}
    </div>
  );
};

const DetailNoteEditor = ({ item, t, actions, noteDraft, onNoteDraftChange }: EditorProps) => {
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const ignoreBlurRef = useRef(true);
  // Own state, passed up only outside input method composition: writing the
  // app-wide draft back into the textarea mid-composition broke it in WebKit.
  const [value, setValue] = useState(noteDraft ?? item.note ?? "");
  const composingRef = useRef(false);

  useEffect(() => {
    let alive = true;
    let blurTimer: ReturnType<typeof setTimeout> | undefined;
    void activateForInput().catch(() => {}).finally(() => {
      if (!alive) return;
      const input = inputRef.current;
      input?.focus();
      if (input) input.setSelectionRange(input.value.length, input.value.length);
      blurTimer = setTimeout(() => { ignoreBlurRef.current = false; }, 200);
    });
    return () => { alive = false; ignoreBlurRef.current = true; clearTimeout(blurTimer); };
  }, []);

  return (
    <div className="detail-note-editor item-note-editor-inner" ref={rootRef}>
      <textarea
        ref={inputRef}
        className="item-note-input"
        value={value}
        rows={3}
        aria-label={t("edit_note")}
        placeholder={getNotePlaceholder(t)}
        onCompositionStart={() => { composingRef.current = true; }}
        onCompositionEnd={(event) => {
          composingRef.current = false;
          const next = (event.target as HTMLTextAreaElement).value;
          setValue(next);
          onNoteDraftChange(next);
        }}
        onChange={(event) => {
          setValue(event.target.value);
          if (!composingRef.current) onNoteDraftChange(event.target.value);
        }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "Escape") {
            event.preventDefault(); event.stopPropagation(); ignoreBlurRef.current = true; actions.setNoteEditor(item, false);
          } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
            event.preventDefault(); event.stopPropagation(); ignoreBlurRef.current = true; actions.saveNote(item, value);
          }
        }}
        onBlur={() => {
          if (ignoreBlurRef.current) return;
          setTimeout(() => {
            if (!ignoreBlurRef.current && !rootRef.current?.contains(document.activeElement)) {
              actions.saveNote(item, inputRef.current?.value ?? value);
            }
          }, 0);
        }}
      />
      <div className="item-note-editor-footer">
        <span className="item-note-hint">{isMacPlatform() ? t("note_editor_hint_mac") : t("note_editor_hint_win")}</span>
        <div className="item-note-actions">
          <button type="button" className="btn-note-action cancel" onMouseDown={() => { ignoreBlurRef.current = true; }} onClick={() => actions.setNoteEditor(item, false)}>{t("cancel")}</button>
          <button type="button" className="btn-note-action save" onMouseDown={() => { ignoreBlurRef.current = true; }} onClick={() => actions.saveNote(item, value)}>{t("save")}</button>
        </div>
      </div>
    </div>
  );
};

const DetailAnnotationEditor = (props: EditorProps) => props.isEditingTags ? <DetailTagEditor {...props} /> : <DetailNoteEditor {...props} />;

export default DetailAnnotationEditor;
