import { useEffect, type RefObject } from "react";

/** Closes a popover on Escape or a press outside `root`, while it is open. */
export function useDismiss(open: boolean, root: RefObject<HTMLElement | null>, onClose: () => void): void {
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent) {
        if (event.key !== "Escape") return;
        // Handled here, so a page underneath does not also act on it.
        event.preventDefault();
      } else if (root.current?.contains(event.target as Node)) {
        return;
      }
      onClose();
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open, root, onClose]);
}
