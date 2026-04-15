import { useEffect } from "react";

export const useContextMenuBlock = () => {
  useEffect(() => {
    if (!import.meta.env.PROD) return;
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
    };
    document.addEventListener("contextmenu", handleContextMenu);
    return () => document.removeEventListener("contextmenu", handleContextMenu);
  }, []);
};
