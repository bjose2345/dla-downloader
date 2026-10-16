import { useCallback, useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { gateway } from "./gateway";
import type { AppUpdateView, ConnectionView, LibraryView, SessionView } from "./types";

/**
 * Rust owns each view. It arrives both as a command's result and as an
 * event, in either order, so only the newest revision is kept.
 */
function useBackendView<View extends { revision: number }>(fetch: () => Promise<View>, subscribe: (handler: (view: View) => void) => Promise<UnlistenFn>) {
  const [view, setView] = useState<View | null>(null);
  const accept = useCallback((next: View) => {
    setView((current) => (current && current.revision > next.revision ? current : next));
  }, []);

  useEffect(() => {
    const unlisten = subscribe(accept);
    void fetch().then(accept);
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [accept, fetch, subscribe]);

  return { view, accept };
}

/** The signed-in account. */
export const useSession = () => useBackendView<SessionView>(gateway.session, gateway.onSession);
/** The archive catalog and downloads. */
export const useLibrary = () => useBackendView<LibraryView>(gateway.library, gateway.onLibrary);
/** Updates for the app itself. */
export const useAppUpdate = () => useBackendView<AppUpdateView>(gateway.appUpdate, gateway.onAppUpdate);

export function useConnection(): ConnectionView | null {
  const [view, setView] = useState<ConnectionView | null>(null);
  useEffect(() => {
    const unlisten = gateway.onConnection(setView);
    void gateway.connectionStatus().then((current) => setView((latest) => latest ?? current));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);
  return view;
}
