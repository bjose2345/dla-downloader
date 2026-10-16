/** The interface zoom levels, in percent; the Rust side offers the same. */
export const ZOOM_LEVELS = [80, 90, 100, 110, 125, 150] as const;
export const NO_ZOOM = 100;

/** The next level up or down from `current`, staying within the offered ones. */
export function stepZoom(current: number, step: 1 | -1): number {
  if (step > 0) return ZOOM_LEVELS.find((level) => level > current) ?? Math.max(...ZOOM_LEVELS);
  return [...ZOOM_LEVELS].reverse().find((level) => level < current) ?? Math.min(...ZOOM_LEVELS);
}

type Keys = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "altKey">;

/** Ctrl + and Ctrl − (main keys or keypad) zoom in and out; Ctrl 0 resets. */
export function zoomShortcut(event: Keys): "in" | "out" | "reset" | null {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return null;
  if (event.key === "+" || event.key === "=" || event.code === "NumpadAdd") return "in";
  if (event.key === "-" || event.key === "_" || event.code === "NumpadSubtract") return "out";
  if (event.key === "0" || event.code === "Numpad0") return "reset";
  return null;
}

/** The level a shortcut leads to from `current`. */
export function zoomAfter(current: number, shortcut: "in" | "out" | "reset"): number {
  if (shortcut === "reset") return NO_ZOOM;
  return stepZoom(current, shortcut === "in" ? 1 : -1);
}
