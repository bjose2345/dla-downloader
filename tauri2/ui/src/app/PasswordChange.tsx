import { ExternalLink, KeyRound } from "lucide-react";
import { useState, type FormEvent } from "react";

import { gateway } from "../backend/gateway";
import type { AccountProblem } from "../backend/types";
import type { MessageKey } from "../i18n/catalogs";
import { usePresentation } from "../preferences/PresentationProvider";

/** The receiver's minimum, checked here too so a short one is not sent. */
export const MIN_PASSWORD_LENGTH = 8;

/** What is wrong with the typed passwords before anything is sent, or null. */
export function passwordFormProblem(current: string, next: string, repeat: string): string | null {
  if (!current || !next || !repeat) return "missing";
  if ([...next].length < MIN_PASSWORD_LENGTH) return "too_short";
  if (next !== repeat) return "mismatch";
  if (next === current) return "same";
  return null;
}

/**
 * Settings › Account: changes the account's password through DLA's server.
 * Uploads, downloads and updates carry on with the new one.
 */
export function PasswordChange({ signedIn }: { signedIn: boolean }) {
  const { t } = usePresentation();
  const [open, setOpen] = useState(false);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<AccountProblem | null>(null);
  const [changed, setChanged] = useState(false);

  const close = () => {
    setCurrent("");
    setNext("");
    setRepeat("");
    setProblem(null);
    setOpen(false);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const typed = passwordFormProblem(current, next, repeat);
    if (typed) {
      setProblem({ code: typed });
      return;
    }
    setSaving(true);
    setProblem(null);
    try {
      const outcome = await gateway.changePassword(current, next);
      if (outcome.changed) {
        close();
        setChanged(true);
      } else {
        setProblem(outcome.problem ?? { code: "failed" });
      }
    } catch (error) {
      setProblem({ code: "failed", detail: String(error) });
    } finally {
      setSaving(false);
    }
  };

  let description = t("settings.changePasswordDescription");
  if (!signedIn) description = t("password.signInFirst");
  else if (changed) description = t("password.changed");

  return (
    <>
      <div className="toggle-row action-row">
        <span>
          <span className="toggle-label">{t("settings.changePassword")}</span>
          <span className={"toggle-description" + (changed && signedIn ? " password-done" : "")}>{description}</span>
        </span>
        {!open && (
          <button
            type="button"
            className="secondary compact"
            disabled={!signedIn}
            onClick={() => {
              setChanged(false);
              setOpen(true);
            }}
          >
            <KeyRound size={15} aria-hidden="true" />
            {t("settings.changePasswordOpen")}
          </button>
        )}
      </div>
      {open && signedIn && (
        <form className="password-form" onSubmit={submit} noValidate>
          <label>
            <span className="small-label">{t("password.current")}</span>
            <input type="password" autoComplete="current-password" value={current} onChange={(event) => setCurrent(event.target.value)} />
          </label>
          <label>
            <span className="small-label">{t("password.new")}</span>
            <input type="password" autoComplete="new-password" value={next} onChange={(event) => setNext(event.target.value)} />
          </label>
          <label>
            <span className="small-label">{t("password.repeat")}</span>
            <input type="password" autoComplete="new-password" value={repeat} onChange={(event) => setRepeat(event.target.value)} />
          </label>
          <p className="muted password-hint">{t("password.rule", { min: MIN_PASSWORD_LENGTH })}</p>
          {problem && (
            <p className="password-problem" role="alert">
              {t(`password.problem.${problem.code}` as MessageKey, { detail: problem.detail ?? "", min: MIN_PASSWORD_LENGTH })}
            </p>
          )}
          {problem?.code === "unavailable" && (
            <button type="button" className="text-button compact" onClick={() => void gateway.openPasswordPage()}>
              <ExternalLink size={15} aria-hidden="true" />
              {t("password.openWebsite")}
            </button>
          )}
          <div className="password-actions">
            <button type="button" className="text-button compact" onClick={close} disabled={saving}>
              {t("password.cancel")}
            </button>
            <button type="submit" className="primary compact" disabled={saving}>
              {saving ? t("password.saving") : t("password.save")}
            </button>
          </div>
        </form>
      )}
    </>
  );
}
