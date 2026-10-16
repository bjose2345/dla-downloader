/** "Welcome" in the app's languages and a few more, typed one after another. */
export const WELCOME_WORDS: readonly { text: string; lang: string }[] = [
  { text: "Welcome", lang: "en" },
  { text: "ようこそ", lang: "ja" },
  { text: "Bienvenido", lang: "es" },
  { text: "Willkommen", lang: "de" },
  { text: "Bienvenue", lang: "fr" },
  { text: "Benvenuto", lang: "it" },
  { text: "Bem-vindo", lang: "pt" },
  { text: "Добро пожаловать", lang: "ru" },
  { text: "환영합니다", lang: "ko" },
  { text: "欢迎", lang: "zh" },
];

/** The word list starting with the chosen language's word. */
export function wordsFor(locale: string): { text: string; lang: string }[] {
  const language = locale.split("-")[0];
  const first = WELCOME_WORDS.findIndex((word) => word.lang === language);
  if (first <= 0) return [...WELCOME_WORDS];
  return [...WELCOME_WORDS.slice(first), ...WELCOME_WORDS.slice(0, first)];
}

export interface TypewriterState {
  word: number;
  /** Characters shown, counted in code points so kana and Hangul stay whole. */
  length: number;
  deleting: boolean;
}

export const TYPE_MS = 95;
export const DELETE_MS = 45;
export const HOLD_MS = 1800;
export const GAP_MS = 380;

/** The next frame and how long to wait before showing it. */
export function nextFrame(state: TypewriterState, words: readonly string[]): { state: TypewriterState; delay: number } {
  const length = Array.from(words[state.word] ?? "").length;
  if (!state.deleting) {
    if (state.length < length) return { state: { ...state, length: state.length + 1 }, delay: TYPE_MS };
    return { state: { ...state, deleting: true }, delay: HOLD_MS };
  }
  if (state.length > 0) return { state: { ...state, length: state.length - 1 }, delay: DELETE_MS };
  return { state: { word: (state.word + 1) % words.length, length: 0, deleting: false }, delay: GAP_MS };
}

export function visibleText(state: TypewriterState, words: readonly string[]): string {
  return Array.from(words[state.word] ?? "").slice(0, state.length).join("");
}
