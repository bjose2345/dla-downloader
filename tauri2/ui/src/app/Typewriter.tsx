import { useReducedMotion } from "motion/react";
import { useEffect, useMemo, useState } from "react";

import { usePresentation } from "../preferences/PresentationProvider";
import { nextFrame, visibleText, wordsFor, type TypewriterState } from "./welcomeWords";

/** Types "Welcome" in one language after another, with a blinking caret. */
export function Typewriter() {
  const { locale, reduceMotion } = usePresentation();
  const systemReduced = useReducedMotion();
  const calm = reduceMotion || Boolean(systemReduced);
  const words = useMemo(() => wordsFor(locale), [locale]);
  const texts = useMemo(() => words.map((word) => word.text), [words]);
  const [state, setState] = useState<TypewriterState>({ word: 0, length: 0, deleting: false });

  // Start again from the chosen language whenever it changes.
  useEffect(() => setState({ word: 0, length: 0, deleting: false }), [texts]);

  useEffect(() => {
    if (calm) return;
    const { state: next, delay } = nextFrame(state, texts);
    const timer = window.setTimeout(() => setState(next), delay);
    return () => window.clearTimeout(timer);
  }, [state, texts, calm]);

  const current = words[calm ? 0 : state.word];
  const shown = calm ? (current?.text ?? "") : visibleText(state, texts);
  return (
    <h1 className="welcome-title" aria-label={words[0]?.text}>
      <span lang={current?.lang} aria-hidden="true">
        {shown}
      </span>
      <span className="caret" aria-hidden="true" />
    </h1>
  );
}
