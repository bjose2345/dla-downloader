import { describe, expect, it } from "vitest";

import { nextFrame, visibleText, wordsFor, type TypewriterState } from "../app/welcomeWords";
import { readPreferences, resolveTheme, systemLocale } from "./preferences";

const storage = (value: string | null) => ({ getItem: () => value });

describe("preferences", () => {
  it("starts from the system language when it is one of ours", () => {
    expect(systemLocale(["ja-JP", "en-US"])).toBe("ja-JP");
    expect(systemLocale(["es-MX"])).toBe("es-ES");
    expect(systemLocale(["pt-BR"])).toBe("pt-PT");
    expect(systemLocale(["nl-NL"])).toBe("en-US");
    expect(systemLocale([])).toBe("en-US");
  });

  it("keeps valid stored choices and ignores anything else", () => {
    const stored = readPreferences(storage(JSON.stringify({ theme: "dark", locale: "ru-RU", reduceMotion: true })));
    expect(stored).toEqual({ theme: "dark", locale: "ru-RU", reduceMotion: true });
    const damaged = readPreferences(storage(JSON.stringify({ theme: "neon", locale: "xx", reduceMotion: "yes" })));
    expect(damaged.theme).toBe("system");
    expect(damaged.reduceMotion).toBe(false);
    expect(readPreferences(storage("{not json")).theme).toBe("system");
    expect(readPreferences(null).theme).toBe("system");
  });

  it("follows the system only when asked to", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
  });
});

describe("typewriter", () => {
  it("starts with the chosen language's welcome", () => {
    expect(wordsFor("ja-JP")[0]?.text).toBe("ようこそ");
    expect(wordsFor("en-US")[0]?.text).toBe("Welcome");
    expect(wordsFor("xx-XX")[0]?.text).toBe("Welcome");
  });

  it("types, holds, deletes and moves to the next word", () => {
    const words = ["ab", "환영"];
    let state: TypewriterState = { word: 0, length: 0, deleting: false };
    const shown: string[] = [];
    for (let step = 0; step < 12; step += 1) {
      state = nextFrame(state, words).state;
      shown.push(visibleText(state, words));
    }
    expect(shown).toEqual(["a", "ab", "ab", "a", "", "", "환", "환영", "환영", "환", "", ""]);
    expect(state.word).toBe(0);
  });

  it("keeps multi-byte characters whole", () => {
    expect(visibleText({ word: 0, length: 1, deleting: false }, ["ようこそ"])).toBe("よ");
  });
});
