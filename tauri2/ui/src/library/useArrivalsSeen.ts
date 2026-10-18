import { useCallback, useEffect, useMemo, useState } from "react";

import type { RecentFile } from "../backend/types";
import { LATEST_DAYS, type ArrivalsSeen } from "./library";

const SEEN_KEY = "dla-downloader:arrivals-seen:v1";
const DAYS_KEY = "dla-downloader:latest-days:v1";

function readSeen(): ArrivalsSeen {
  try {
    const saved = JSON.parse(localStorage.getItem(SEEN_KEY) ?? "null") as Partial<ArrivalsSeen> | null;
    return {
      openedAt: typeof saved?.openedAt === "number" ? saved.openedAt : null,
      seen: Array.isArray(saved?.seen) ? saved.seen.filter((path): path is string => typeof path === "string") : [],
    };
  } catch {
    return { openedAt: null, seen: [] };
  }
}

/** Which recent files the user has seen, and when "Latest added" was last open. */
export function useArrivalsSeen(files: RecentFile[] | undefined) {
  const [state, setState] = useState(readSeen);

  useEffect(() => {
    try {
      localStorage.setItem(SEEN_KEY, JSON.stringify(state));
    } catch {
      // Storage may be refused; the marks still apply until the app closes.
    }
  }, [state]);

  // Marks of files no longer listed are dropped, so the record stays small.
  useEffect(() => {
    if (!files) return;
    const listed = new Set(files.map((file) => file.path));
    setState((current) =>
      current.seen.every((path) => listed.has(path)) ? current : { ...current, seen: current.seen.filter((path) => listed.has(path)) },
    );
  }, [files]);

  const seen = useMemo(() => new Set(state.seen), [state.seen]);
  const markOpened = useCallback(() => setState((current) => ({ ...current, openedAt: Date.now() })), []);
  const markSeen = useCallback(
    (paths: string[]) =>
      setState((current) => {
        const known = new Set(current.seen);
        const added = paths.filter((path) => !known.has(path));
        return added.length ? { ...current, seen: [...current.seen, ...added] } : current;
      }),
    [],
  );
  return { state, seen, markOpened, markSeen };
}

/** How many days "Latest added" shows, kept for next time. */
export function useLatestDays(): [number, (days: number) => void] {
  const [days, setDays] = useState<number>(() => {
    try {
      const saved = Number(localStorage.getItem(DAYS_KEY));
      return (LATEST_DAYS as readonly number[]).includes(saved) ? saved : 7;
    } catch {
      return 7;
    }
  });
  const choose = useCallback((next: number) => {
    setDays(next);
    try {
      localStorage.setItem(DAYS_KEY, String(next));
    } catch {
      // Storage may be refused; the choice still applies now.
    }
  }, []);
  return [days, choose];
}
