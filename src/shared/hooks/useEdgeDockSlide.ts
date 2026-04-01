import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

interface EdgeDockSlideEvent {
  action: "hide" | "show";
  dock: "top" | "left" | "right" | "none";
}

/**
 * Listens for edge-dock-slide events and applies directional CSS transforms
 * to the app container, creating a slide-in/slide-out animation.
 */
export const useEdgeDockSlide = () => {
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    (async () => {
      unlisten = await listen<EdgeDockSlideEvent>(
        "edge-dock-slide",
        (event) => {
          const el = containerRef.current;
          if (!el) return;

          const { action, dock } = event.payload;

          if (action === "hide") {
            // Slide content towards the docked edge
            el.classList.remove(
              "slide-show-top",
              "slide-show-left",
              "slide-show-right"
            );
            el.classList.add(`slide-hide-${dock}`);
          } else if (action === "show") {
            // Start from edge position, then slide in
            el.classList.remove(
              "slide-hide-top",
              "slide-hide-left",
              "slide-hide-right"
            );
            el.classList.add(`slide-show-${dock}`);
          }
        }
      );
    })();

    return () => {
      unlisten?.();
    };
  }, []);

  return containerRef;
};
