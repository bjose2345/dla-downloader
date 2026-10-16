import { connectionText } from "../shared/copy";
import { gateway } from "../backend/gateway";
import { usePresentation } from "../preferences/PresentationProvider";
import type { AppInfo, ConnectionView, LibraryView, SessionView } from "../backend/types";
import { AccountMenu } from "./AccountMenu";
import { HeaderTools } from "./HeaderTools";
import { LibraryPage } from "../library/LibraryPage";
import { Logo } from "../shared/Logo";

interface Props {
  info: AppInfo;
  account: string;
  library: LibraryView | null;
  connection: ConnectionView | null;
  failure: string | null;
  run: (command: Promise<SessionView>) => void;
  runLibrary: (command: Promise<LibraryView>) => void;
  onOpenSettings: () => void;
  updateReady: boolean;
}

/** The signed-in app: the Download page under a header with the account. */
export function AppShell({ info, account, library, connection, failure, run, runLibrary, onOpenSettings, updateReady }: Props) {
  const { t } = usePresentation();
  const route = connection?.archive ?? null;
  const link = connectionText(t, route);

  return (
    <div className="shell">
      <header className="header">
        <div className="brand">
          <Logo size={34} />
        </div>
        <div className="header-tools">
          <span className={"connection " + link.tone} title={route && "detail" in route ? `${link.text}: ${route.detail}` : link.text}>
            <span className="dot" />
            <span>{link.text}</span>
          </span>
          <AccountMenu
            account={account}
            selfTest={info.selfTest}
            onSignOut={() => run(gateway.signOut())}
            onOpenSettings={onOpenSettings}
          />
          <HeaderTools onOpenSettings={onOpenSettings} updateReady={updateReady} />
        </div>
      </header>

      <main className="wide">
        {failure && <div className="notice">{t("app.unexpected", { detail: failure })}</div>}
        {route?.state === "unavailable" && (
          <div className="notice">
            {t("connection.unavailable")}: {route.detail}
          </div>
        )}
        <section className="page">
          <div className="page-heading">
            <span className="eyebrow">{t("library.eyebrow")}</span>
            <h1>{t("library.title")}</h1>
          </div>
          {library && <LibraryPage view={library} run={runLibrary} />}
        </section>
      </main>
    </div>
  );
}
