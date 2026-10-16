import { ChevronDown, LogOut, Settings } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useRef, useState } from "react";

import { usePresentation } from "../preferences/PresentationProvider";
import { useDismiss } from "../shared/useDismiss";

interface Props {
  account: string;
  selfTest: boolean;
  onSignOut: () => void;
  onOpenSettings: () => void;
}

/** A running download pauses at sign-out and resumes at the next sign-in. */
export function AccountMenu({ account, selfTest, onSignOut, onOpenSettings }: Props) {
  const { t } = usePresentation();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);

  useDismiss(open, root, close);

  const initial = account.replace(/^dla-/i, "").charAt(0).toUpperCase() || "D";
  return (
    <div className="account-menu" ref={root}>
      <button type="button" className="account-button" aria-expanded={open} aria-haspopup="menu" onClick={() => setOpen((value) => !value)}>
        <span className="avatar small" aria-hidden="true">
          {initial}
        </span>
        <span className="account-button-name">{account}</span>
        <ChevronDown size={15} aria-hidden="true" />
      </button>
      <AnimatePresence>
        {open && (
          <motion.div
            className="account-popover"
            role="menu"
            initial={{ opacity: 0, y: -6, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -6, scale: 0.98 }}
            transition={{ duration: 0.14 }}
          >
            <div className="account-popover-head">
              <span className="avatar" aria-hidden="true">
                {initial}
              </span>
              <div>
                <span className="muted">{t("account.signedInAs")}</span>
                <strong>{account}</strong>
                <span className="muted">{t(selfTest ? "account.folderSelfTest" : "account.folderPrivate")}</span>
              </div>
            </div>
            <button
              type="button"
              role="menuitem"
              className="menu-item"
              onClick={() => {
                setOpen(false);
                onOpenSettings();
              }}
            >
              <Settings size={16} aria-hidden="true" />
              {t("settings.open")}
            </button>
            <button
              type="button"
              role="menuitem"
              className="menu-item"
              onClick={() => {
                setOpen(false);
                onSignOut();
              }}
            >
              <LogOut size={16} aria-hidden="true" />
              {t("account.signOut")}
            </button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
