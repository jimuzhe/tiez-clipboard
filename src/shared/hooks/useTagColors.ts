import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const useTagColors = () => {
  const [tagColors, setTagColors] = useState<Record<string, string>>({});

  useEffect(() => {
    const fetchTagColors = () => {
      invoke<Record<string, string>>("get_tag_colors")
        .then(setTagColors)
        .catch(console.error);
    };

    fetchTagColors();
    const unlistenColors = listen("tag-colors-updated", fetchTagColors);

    return () => {
      unlistenColors.then((f) => f());
    };
  }, []);

  return tagColors;
};
