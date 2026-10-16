import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";

import { AppShell } from "./AppShell";
import { SettingsPanel } from "./SettingsPanel";
import { WelcomeScreen } from "./WelcomeScreen";
import { gateway } from "../backend/gateway";
import { usePresentation } from "../preferences/PresentationProvider";
import { UPDATE_REQUIRED, type AppInfo, type AppUpdateView, type LibraryView, type SessionView } from "../backend/types";
import { useAppUpdate, useConnection, useLibrary, useSession } from "../backend/views";
import { UpdateRequiredBanner } from "./AppUpdate";
import { NO_ZOOM, zoomAfter, zoomShortcut } from "./zoom";

export function App() {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const info = useQuery({ queryKey: ["app-info"], queryFn: gateway.appInfo });
  const { view, accept } = useSession();
  const library = useLibrary();
  const appUpdate = useAppUpdate();
  const connection = useConnection();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const account = view?.account ?? null;

  // The window keeps the zoom across starts; the page only shows and sets it.
  const zoom = useQuery({ queryKey: ["zoom"], queryFn: gateway.windowZoom });
  const setZoom = (percent: number) => {
    queryClient.setQueryData(["zoom"], percent);
    void gateway.setWindowZoom(percent).then(
      (level) => queryClient.setQueryData(["zoom"], level),
      () => void queryClient.invalidateQueries({ queryKey: ["zoom"] }),
    );
  };
  const zoomTo = useRef(setZoom);
  zoomTo.current = setZoom;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const shortcut = zoomShortcut(event);
      if (!shortcut) return;
      event.preventDefault();
      const current = queryClient.getQueryData<number>(["zoom"]) ?? NO_ZOOM;
      const next = zoomAfter(current, shortcut);
      if (next !== current) zoomTo.current(next);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [queryClient]);

  if (info.isError) {
    return <p className="notice">{t("app.startFailed")}</p>;
  }
  if (!info.data || !view) {
    return null;
  }

  // A download refused while an update is required: the banner says why.
  const report = (error: unknown) => setFailure(String(error) === UPDATE_REQUIRED ? null : String(error));
  const run = (command: Promise<SessionView>) =>
    command.then((next) => {
      setFailure(null);
      accept(next);
    }, report);
  const runLibrary = (command: Promise<LibraryView>) =>
    void command.then((next) => {
      setFailure(null);
      library.accept(next);
    }, report);
  const runUpdate = (command: Promise<AppUpdateView>) =>
    void command.then((next) => {
      setFailure(null);
      appUpdate.accept(next);
    }, report);
  const updateReady = Boolean(appUpdate.view?.ready && appUpdate.view.installable);
  const setInfo = (next: AppInfo) => queryClient.setQueryData(["app-info"], next);
  const openSettings = () => setSettingsOpen(true);

  return (
    <>
      {info.data.selfTest && (
        <div className="demo-banner">
          <strong>{t("banner.selfTestTitle")}</strong>
          <span>{t("banner.selfTest")}</span>
        </div>
      )}
      {appUpdate.view?.required && <UpdateRequiredBanner update={appUpdate.view} onInstall={() => runUpdate(gateway.appUpdateInstall())} />}
      <AnimatePresence mode="wait" initial={false}>
        {account ? (
          <motion.div key="shell" initial={{ opacity: 0, scale: 0.99 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.25 }}>
            <AppShell
              info={info.data}
              account={account}
              library={library.view}
              connection={connection}
              failure={failure}
              run={(command) => void run(command)}
              runLibrary={runLibrary}
              onOpenSettings={openSettings}
              updateReady={updateReady}
            />
          </motion.div>
        ) : (
          <motion.div key="welcome" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0, y: -12 }} transition={{ duration: 0.25 }}>
            <WelcomeScreen
              info={info.data}
              connection={connection}
              notice={view.notice}
              onSignIn={(username, password) => run(gateway.signIn(username, password)).then(() => queryClient.invalidateQueries({ queryKey: ["app-info"] }))}
              onOpenSettings={openSettings}
              updateReady={updateReady}
            />
          </motion.div>
        )}
      </AnimatePresence>
      <SettingsPanel
        open={settingsOpen}
        info={info.data}
        library={library.view}
        connection={connection}
        onClose={() => setSettingsOpen(false)}
        onChooseFolder={() => runLibrary(gateway.chooseDownloadFolder())}
        onClearFolder={() => runLibrary(gateway.clearDownloadFolder())}
        onAskEveryTime={(ask) => runLibrary(gateway.setAskEveryTime(ask))}
        onRememberAccount={(remember) => void gateway.setRememberAccount(remember).then(setInfo, report)}
        zoom={zoom.data ?? NO_ZOOM}
        onZoom={setZoom}
        appUpdate={appUpdate.view}
        onCheckAppUpdate={() => runUpdate(gateway.appUpdateCheck())}
        onInstallAppUpdate={() => runUpdate(gateway.appUpdateInstall())}
      />
    </>
  );
}
