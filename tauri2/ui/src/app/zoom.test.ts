import { describe, expect, it } from "vitest";

import { stepZoom, zoomAfter, zoomShortcut } from "./zoom";

const keys = (key: string, extra: Partial<KeyboardEvent> = {}) => ({ key, code: "", ctrlKey: true, metaKey: false, altKey: false, ...extra });

describe("zoom", () => {
  it("steps through the offered levels and stops at the ends", () => {
    expect(stepZoom(100, 1)).toBe(110);
    expect(stepZoom(110, 1)).toBe(125);
    expect(stepZoom(150, 1)).toBe(150);
    expect(stepZoom(100, -1)).toBe(90);
    expect(stepZoom(80, -1)).toBe(80);
    // A level between two offered ones goes to the next one in that direction.
    expect(stepZoom(115, 1)).toBe(125);
    expect(stepZoom(115, -1)).toBe(110);
  });

  it("reads Ctrl +, Ctrl − and Ctrl 0 from the main keys and the keypad", () => {
    expect(zoomShortcut(keys("+"))).toBe("in");
    expect(zoomShortcut(keys("="))).toBe("in");
    expect(zoomShortcut(keys("+", { code: "NumpadAdd" }))).toBe("in");
    expect(zoomShortcut(keys("-"))).toBe("out");
    expect(zoomShortcut(keys("-", { code: "NumpadSubtract" }))).toBe("out");
    expect(zoomShortcut(keys("0"))).toBe("reset");
    expect(zoomShortcut(keys("Insert", { code: "Numpad0" }))).toBe("reset");
    // Not without Ctrl, not with AltGr, and not other shortcuts.
    expect(zoomShortcut(keys("+", { ctrlKey: false }))).toBeNull();
    expect(zoomShortcut(keys("+", { altKey: true }))).toBeNull();
    expect(zoomShortcut(keys("a"))).toBeNull();
    expect(zoomAfter(125, "reset")).toBe(100);
    expect(zoomAfter(125, "out")).toBe(110);
  });
});
