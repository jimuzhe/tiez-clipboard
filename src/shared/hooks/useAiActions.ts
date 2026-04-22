import { useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Dispatch, SetStateAction } from "react";
import type { ClipboardEntry } from "../types";
import type { AiProfile } from "../../features/settings/types";

interface UseAiActionsOptions {
  aiProfiles: AiProfile[];
  language: string;
  pushToast: (msg: string, duration?: number) => number;
  setShowSettings: Dispatch<SetStateAction<boolean>>;
  setProcessingAiId: Dispatch<SetStateAction<number | null>>;
  setHistory: Dispatch<SetStateAction<ClipboardEntry[]>>;
}

export const useAiActions = ({
  aiProfiles,
  language,
  pushToast,
  setShowSettings,
  setProcessingAiId,
  setHistory
}: UseAiActionsOptions) => {
  const handleAIAction = useCallback(
    async (id: number, content: string, actionType: string) => {
      if (aiProfiles.length === 0) {
        pushToast(
          language === "zh"
            ? "请先在设置中添加 AI 模型"
            : "Please add an AI model in settings first",
          3000
        );
        setShowSettings(true);
        return;
      }

      setProcessingAiId(id);
      try {
        const aiResponse = await invoke<string>("call_ai", { id, content, actionType });

        setHistory((prev) =>
          prev.map((item) => {
            if (item.id == id) {
              const trimmedResponse = aiResponse.trim();
              const questionMatch = trimmedResponse.match(/^\[\[QUESTION:(.+?)\]\]$/);

              if (questionMatch) {
                const questionText = questionMatch[1].trim();
                return {
                  ...item,
                  isInputting: true,
                  content: questionText,
                  html_content: undefined, // Clear rich text to show question
                  preview:
                    questionText.length > 100
                      ? questionText.substring(0, 100).replace(/\n/g, " ") + "..."
                      : questionText.replace(/\n/g, " ")
                };
              }
              return {
                ...item,
                content: aiResponse,
                content_type: item.content_type === 'rich_text' ? 'text' : item.content_type,
                html_content: undefined, // Clear rich text to show AI response
                isInputting: false,
                preview:
                  aiResponse.length > 100
                    ? aiResponse.substring(0, 100).replace(/\n/g, " ") + "..."
                    : aiResponse.replace(/\n/g, " ")
              };
            }
            return item;
          })
        );
      } catch (err) {
        const errorMsg = err?.toString() || "AI processing failed";
        pushToast(errorMsg, 5000);
      } finally {
        setProcessingAiId(null);
      }
    },
    [aiProfiles, language, pushToast, setHistory, setProcessingAiId, setShowSettings]
  );

  return { handleAIAction };
};


