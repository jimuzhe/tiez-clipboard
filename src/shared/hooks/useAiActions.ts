import { useCallback, useEffect, useRef, useState } from "react";
import { failureText } from "../lib/errorMessage";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../lib/tauriRuntime";
import type { Dispatch, SetStateAction } from "react";
import type { ClipboardEntry } from "../types";
import type { AiProfile } from "../../features/settings/types";

/** Room left for the original in a follow-up prompt (backend limit is 2000). */
const SUPPLEMENT_ORIGINAL_LIMIT = 1200;

export interface AiDetail {
  original: Pick<ClipboardEntry, "content" | "content_type" | "html_content">;
  response?: string;
  reasoning?: string;
  question?: string;
  processing: boolean;
  /** The AI saw only the start of a long entry, so the result can't replace it. */
  truncated?: boolean;
}

interface UseAiActionsOptions {
  history: ClipboardEntry[];
  aiProfiles: AiProfile[];
  t: (key: string) => string;
  pushToast: (msg: string, duration?: number) => number;
  setShowSettings: Dispatch<SetStateAction<boolean>>;
  setProcessingAiId: Dispatch<SetStateAction<number | null>>;
  setHistory: Dispatch<SetStateAction<ClipboardEntry[]>>;
}

export const useAiActions = ({
  history,
  aiProfiles,
  t,
  pushToast,
  setShowSettings,
  setProcessingAiId,
  setHistory
}: UseAiActionsOptions) => {
  // Keep the before/after pair across list refreshes and width changes. An AI
  // result only replaces the saved entry after the user confirms it.
  const [aiDetails, setAiDetails] = useState<Record<number, AiDetail>>({});
  const detailsRef = useRef(aiDetails);
  detailsRef.current = aiDetails;
  const historyRef = useRef(history);
  historyRef.current = history;
  const pendingRef = useRef(new Set<number>());
  /** Deleted while a request was in flight: drop its late result. */
  const removedRef = useRef(new Set<number>());

  // A deleted entry's id can be reused by the next one (SQLite rowid), so its
  // before/after pair must not linger and attach to the new entry.
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unlisten = listen<number>("clipboard-removed", ({ payload: id }) => {
      if (pendingRef.current.has(id)) removedRef.current.add(id);
      setAiDetails((prev) => {
        if (!(id in prev)) return prev;
        const next = { ...prev };
        delete next[id];
        return next;
      });
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  const handleAIAction = useCallback(
    async (id: number, content: string, actionType: string, useProvidedContent = false, inputClipped = false) => {
      if (aiProfiles.length === 0) {
        pushToast(t("ai_no_profile_toast"), 3000);
        setShowSettings(true);
        return;
      }
      if (pendingRef.current.has(id)) return;
      const item = historyRef.current.find((entry) => entry.id === id);
      if (!item) return;
      pendingRef.current.add(id);
      const previous = detailsRef.current[id];
      const initial: AiDetail = {
        original: previous?.original ?? {
          content: item.content,
          content_type: item.content_type,
          html_content: item.html_content
        },
        response: previous?.response,
        question: previous?.question,
        processing: true
      };
      setAiDetails((prev) => ({ ...prev, [id]: initial }));
      setProcessingAiId(id);
      try {
        // The list copy may only contain the first 2000 characters, including rich text.
        const original = previous?.original ?? {
          ...initial.original,
          content: await invoke<string>("get_clipboard_content", { id })
        };
        if (removedRef.current.has(id)) return;
        setAiDetails((prev) => ({ ...prev, [id]: { ...initial, original } }));
        const { content: aiResponse, reasoning, truncated } = await invoke<{
          content: string; reasoning?: string; truncated: boolean;
        }>("call_ai", { id, content, actionType, useProvidedContent });
        if (removedRef.current.has(id)) return;
        const question = aiResponse.trim().match(/^\[\[QUESTION:([\s\S]+?)\]\]$/)?.[1].trim();
        setAiDetails((prev) => ({
          ...prev,
          [id]: { original, response: question ? undefined : aiResponse, reasoning, question,
            processing: false, truncated: truncated || inputClipped }
        }));
      } catch (err) {
        if (removedRef.current.has(id)) return;
        setAiDetails((prev) => {
          const next = { ...prev };
          if (previous) next[id] = previous;
          else delete next[id];
          return next;
        });
        pushToast(failureText(t("ai_failed"), err), 5000);
      } finally {
        pendingRef.current.delete(id);
        removedRef.current.delete(id);
        setProcessingAiId((current) => current === id ? null : current);
      }
    },
    [aiProfiles, t, pushToast, setProcessingAiId, setShowSettings]
  );

  const applyAIResult = useCallback(async (id: number) => {
    const detail = detailsRef.current[id];
    const response = detail?.response;
    if (!response || detail.truncated) return;
    try {
      // Only replace the text the AI actually worked on: the entry may have
      // been edited since, or the id reused by a new entry.
      const current = await invoke<string>("get_clipboard_content", { id });
      if (current !== detail.original.content) {
        pushToast(t("ai_source_changed"), 4000);
        return;
      }
      await invoke("apply_ai_result", { id, content: response });
      // Done with the pair: the pane goes back to showing the (updated) entry.
      setAiDetails((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      setHistory((prev) => prev.map((entry) => entry.id === id ? {
        ...entry,
        content: response,
        content_type: entry.content_type === "image" ? "image" : "text",
        html_content: undefined,
        preview: response.substring(0, 100).replace(/\n/g, " ") + (response.length > 100 ? "..." : "")
      } : entry));
      pushToast(t("ai_result_applied"), 1500);
    } catch (err) {
      pushToast(failureText(t("ai_apply_failed"), err), 5000);
    }
  }, [pushToast, setHistory, t]);

  const discardAIResult = useCallback((id: number) => {
    setAiDetails((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  const handleAISupplement = useCallback((id: number, input: string) => {
    const detail = detailsRef.current[id];
    const item = historyRef.current.find((entry) => entry.id === id);
    const answer = input.trim();
    if (!item || !answer) return;
    const original = detail?.original.content ?? item.content;
    const question = detail?.question ?? "";
    // Answer first, original last and capped: the backend keeps only the first
    // 2000 characters, and that cut must never fall on the user's answer.
    const clipped = original.length > SUPPLEMENT_ORIGINAL_LIMIT;
    const source = clipped ? `${original.slice(0, SUPPLEMENT_ORIGINAL_LIMIT)}…` : original;
    const prompt = `你刚才的问题：\n${question}\n\n我的补充：\n${answer}\n\n原始需求：\n${source}\n\n请结合以上信息完成原始需求。`;
    void handleAIAction(id, prompt, "task", true, clipped);
  }, [handleAIAction]);

  const copyAIText = useCallback(async (content: string) => {
    try {
      // ID zero prevents the backend from replacing this explicit text with
      // the saved entry. Copying a source/result does not trigger paste/delete.
      await invoke("copy_to_clipboard", {
        id: 0, content, contentType: "text", paste: false,
        deleteAfterUse: false, pasteWithFormat: false, moveToTop: false
      });
      pushToast(t("ai_copied"), 1500);
    } catch (err) {
      pushToast(failureText(t("copy_failed"), err), 3000);
    }
  }, [pushToast, t]);

  return { handleAIAction, handleAISupplement, applyAIResult, discardAIResult, copyAIText, aiDetails };
};
