import { Download, Globe, Info, Monitor, Moon, Palette, Sun, UserRound, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, type ReactNode } from "react";

import { connectionText } from "../shared/copy";
import { usePresentation } from "../preferences/PresentationProvider";
import { locales, type ThemeChoice } from "../preferences/preferences";
import type { AppInfo, AppUpdateView, ConnectionView, LibraryView } from "../backend/types";
import { Toggle } from "../shared/Toggle";
import { EmailSetting } from "./EmailSetting";
import { PasswordChange } from "./PasswordChange";
import { AppUpdateSection } from "./AppUpdate";
import { LocaleFlag } from "./LocaleFlag";
import { ZOOM_LEVELS } from "./zoom";

interface Props {
  open: boolean;
  info: AppInfo;
  library: LibraryView | null;
  connection: ConnectionView | null;
  onClose: () => void;
  onChooseFolder: () => void;
  onClearFolder: () => void;
  onAskEveryTime: (ask: boolean) => void;
  onRememberAccount: (remember: boolean) => void;
  /** The interface zoom in percent. */
  zoom: number;
  onZoom: (percent: number) => void;
  /** The app's own updates; absent until their state is known. */
  appUpdate: AppUpdateView | null;
  onCheckAppUpdate: () => void;
  onInstallAppUpdate: () => void;
}

// Shown in About; the installers also carry each project's full license.
const NOTICES: readonly { name: string; license: string; owner: string }[] = [
  { name: "cloudflared", license: "Apache-2.0", owner: "Cloudflare, Inc." },
  { name: "Tauri", license: "Apache-2.0 / MIT", owner: "The Tauri Programme" },
  { name: "React", license: "MIT", owner: "Meta Platforms, Inc." },
  { name: "Motion", license: "MIT", owner: "Motion Division Ltd." },
  { name: "Lucide", license: "ISC", owner: "Lucide Contributors" },
  { name: "TanStack Query", license: "MIT", owner: "Tanner Linsley" },
  { name: "RustCrypto (SHA-2)", license: "Apache-2.0 / MIT", owner: "Their contributors" },
  { name: "rustls, ureq", license: "Apache-2.0 / MIT / ISC", owner: "rustls and ureq contributors" },
  { name: "minisign-verify", license: "MIT", owner: "Frank Denis" },
];

const THEMES: readonly { id: ThemeChoice; icon: typeof Sun }[] = [
  { id: "light", icon: Sun },
  { id: "dark", icon: Moon },
  { id: "system", icon: Monitor },
];

export function SettingsPanel(props: Props) {
  const { open, onClose } = props;
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    panel.current?.focus();
    // Only the panel scrolls while it is open, not the page behind it.
    document.documentElement.classList.add("scroll-locked");
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("keydown", close);
      document.documentElement.classList.remove("scroll-locked");
      previous?.focus?.();
    };
  }, [open, onClose]);

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            className="settings-backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
          />
          <motion.div
            ref={panel}
            className="settings-panel"
            role="dialog"
            aria-modal="true"
            aria-labelledby="settings-title"
            tabIndex={-1}
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={{ type: "spring", stiffness: 380, damping: 38 }}
          >
            <SettingsContent {...props} />
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

function SettingsContent({
  open,
  info,
  library,
  connection,
  onClose,
  onChooseFolder,
  onClearFolder,
  onAskEveryTime,
  onRememberAccount,
  zoom,
  onZoom,
  appUpdate,
  onCheckAppUpdate,
  onInstallAppUpdate,
}: Props) {
  const { t, locale, setLocale, theme, setTheme, reduceMotion, setReduceMotion } = usePresentation();
  const settings = library?.settings;
  const link = connectionText(t, connection?.archive ?? null);
  const ready = link.tone === "ready";
  const detail = ready ? undefined : link.text;

  return (
    <>
      <div className="settings-head">
        <h2 id="settings-title">{t("settings.title")}</h2>
        <button type="button" className="icon-button" aria-label={t("common.close")} title={t("common.close")} onClick={onClose}>
          <X size={19} aria-hidden="true" />
        </button>
      </div>
      <div className="settings-body">
        <Section icon={<Globe size={18} />} title={t("settings.language")} description={t("settings.languageDescription")}>
          <div className="locale-grid" role="radiogroup" aria-label={t("settings.language")}>
            {locales.map((option) => (
              <button
                key={option.id}
                type="button"
                role="radio"
                aria-checked={option.id === locale}
                className={"locale-option" + (option.id === locale ? " current" : "")}
                onClick={() => setLocale(option.id)}
                lang={option.id}
              >
                <LocaleFlag locale={option.id} />
                <span>{option.label}</span>
              </button>
            ))}
          </div>
        </Section>

        <Section icon={<Palette size={18} />} title={t("settings.appearance")} description={t("settings.appearanceDescription")}>
          <div className="choice-row" role="radiogroup" aria-label={t("settings.appearance")}>
            {THEMES.map(({ id, icon: Icon }) => (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={theme === id}
                className={"choice" + (theme === id ? " current" : "")}
                onClick={() => setTheme(id)}
              >
                <Icon size={16} aria-hidden="true" />
                {t(id === "light" ? "settings.themeLight" : id === "dark" ? "settings.themeDark" : "settings.themeSystem")}
              </button>
            ))}
          </div>
          <div className="zoom-setting">
            <span className="toggle-label">{t("settings.zoom")}</span>
            <span className="toggle-description">{t("settings.zoomDescription")}</span>
            <div className="choice-row" role="radiogroup" aria-label={t("settings.zoom")}>
              {ZOOM_LEVELS.map((level) => (
                <button
                  key={level}
                  type="button"
                  role="radio"
                  aria-checked={zoom === level}
                  className={"choice" + (zoom === level ? " current" : "")}
                  onClick={() => onZoom(level)}
                >
                  {new Intl.NumberFormat(locale, { style: "percent" }).format(level / 100)}
                </button>
              ))}
            </div>
          </div>
          <Toggle
            label={t("settings.reduceMotion")}
            description={t("settings.reduceMotionDescription")}
            checked={reduceMotion}
            onChange={setReduceMotion}
          />
        </Section>

        <Section icon={<Download size={18} />} title={t("settings.downloads")} description={t("settings.downloadsDescription")}>
          <div className="folder-setting">
            <span className="small-label">{t("settings.downloadFolder")}</span>
            <span className={"folder-path" + (settings?.directory ? "" : " muted")}>
              {settings?.directory ?? t("settings.downloadFolderUnset")}
            </span>
            <div className="folder-actions">
              <button type="button" className="secondary compact" onClick={onChooseFolder}>
                {t("settings.chooseFolder")}
              </button>
              {settings?.directory && (
                <button type="button" className="text-button compact" onClick={onClearFolder}>
                  {t("settings.resetFolder")}
                </button>
              )}
            </div>
          </div>
          <Toggle
            label={t("settings.askEveryTime")}
            description={t("settings.askEveryTimeDescription")}
            checked={settings?.askEveryTime ?? true}
            disabled={!settings?.directory}
            onChange={onAskEveryTime}
          />
        </Section>

        <Section icon={<UserRound size={18} />} title={t("settings.account")} description={t("settings.accountDescription")}>
          <Toggle
            label={t("settings.rememberUsername")}
            description={t("settings.rememberUsernameDescription")}
            checked={info.rememberAccount}
            onChange={onRememberAccount}
          />
          <EmailSetting signedIn={library?.signedIn ?? false} open={open} />
          <PasswordChange signedIn={library?.signedIn ?? false} />
        </Section>

        <Section icon={<Info size={18} />} title={t("settings.about")} description="">
          <dl className="about-list">
            <dt>{t("settings.version")}</dt>
            <dd>DLA Downloader {info.version}</dd>
            <dt>{t("settings.connectionMode")}</dt>
            <dd className={ready ? "tone-ready" : "tone-offline"} title={detail}>
              {ready ? t("settings.connectionReady") : t("settings.connectionNotReady")}
            </dd>
          </dl>
          {appUpdate?.enabled && <AppUpdateSection update={appUpdate} onCheck={onCheckAppUpdate} onInstall={onInstallAppUpdate} />}
          <h4 className="notices-title">{t("settings.notices")}</h4>
          <p className="muted notices-lead">{t("settings.noticesDescription")}</p>
          <ul className="notices">
            {NOTICES.map((notice) => (
              <li key={notice.name}>
                <strong>{notice.name}</strong>
                <span>{notice.license}</span>
                <span className="muted">© {notice.owner}</span>
              </li>
            ))}
          </ul>
        </Section>
      </div>
    </>
  );
}

function Section({ icon, title, description, children }: { icon: ReactNode; title: string; description: string; children: ReactNode }) {
  return (
    <section className="settings-section">
      <div className="settings-section-head">
        <span className="settings-mark" aria-hidden="true">
          {icon}
        </span>
        <div>
          <h3>{title}</h3>
          {description && <p>{description}</p>}
        </div>
      </div>
      {children}
    </section>
  );
}
