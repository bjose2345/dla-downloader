import { Mail } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";

import { gateway } from "../backend/gateway";
import type { AccountProblem } from "../backend/types";
import type { MessageKey } from "../i18n/catalogs";
import { usePresentation } from "../preferences/PresentationProvider";

/** What is wrong with the typed email form before anything is sent, or null. */
export function emailFormProblem(email: string, current: string): string | null {
  const address = email.trim();
  if (!address || !current) return "missing";
  const at = address.indexOf("@");
  if (at < 1 || at !== address.lastIndexOf("@") || at === address.length - 1 || /\s/.test(address)) return "invalid_email";
  return null;
}

/**
 * Settings › Account: the address SFTPGo sends a reset code to when the
 * password is forgotten. Changing it asks for the current password.
 */
export function EmailSetting({ signedIn, open }: { signedIn: boolean; open: boolean }) {
  const { t } = usePresentation();
  const [email, setEmail] = useState<string | null>(null);
  const [loadProblem, setLoadProblem] = useState<AccountProblem | null>(null);
  const [editing, setEditing] = useState(false);
  const [typed, setTyped] = useState("");
  const [current, setCurrent] = useState("");
  const [saving, setSaving] = useState(false);
  const [problem, setProblem] = useState<AccountProblem | null>(null);
  const [saved, setSaved] = useState(false);

  // Read again each time Settings opens, so a change made elsewhere shows.
  useEffect(() => {
    if (!signedIn || !open) return;
    let live = true;
    gateway
      .accountEmail()
      .then((outcome) => {
        if (!live) return;
        setEmail(outcome.changed ? (outcome.email ?? "") : null);
        setLoadProblem(outcome.problem);
      })
      .catch((error: unknown) => live && setLoadProblem({ code: "failed", detail: String(error) }));
    return () => {
      live = false;
    };
  }, [signedIn, open]);

  const close = () => {
    setTyped("");
    setCurrent("");
    setProblem(null);
    setEditing(false);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const wrong = emailFormProblem(typed, current);
    if (wrong) {
      setProblem({ code: wrong });
      return;
    }
    setSaving(true);
    setProblem(null);
    try {
      const outcome = await gateway.setAccountEmail(current, typed.trim());
      if (outcome.changed) {
        setEmail(outcome.email ?? typed.trim());
        close();
        setSaved(true);
      } else {
        setProblem(outcome.problem ?? { code: "failed" });
      }
    } catch (error) {
      setProblem({ code: "failed", detail: String(error) });
    } finally {
      setSaving(false);
    }
  };

  let description = t("settings.emailDescription");
  if (!signedIn) description = t("password.signInFirst");
  else if (saved) description = t("email.saved");
  else if (loadProblem) description = t(`email.problem.${loadProblem.code}` as MessageKey, { detail: loadProblem.detail ?? "" });
  else if (email === "") description = t("settings.emailNone");

  return (
    <>
      <div className="toggle-row action-row">
        <span>
          <span className="toggle-label">{t("settings.email")}</span>
          {signedIn && email && <span className="email-address">{email}</span>}
          <span className={"toggle-description" + (saved && signedIn ? " password-done" : "")}>{description}</span>
        </span>
        {!editing && (
          <button
            type="button"
            className="secondary compact"
            disabled={!signedIn || email === null}
            onClick={() => {
              setSaved(false);
              setTyped(email ?? "");
              setEditing(true);
            }}
          >
            <Mail size={15} aria-hidden="true" />
            {email ? t("email.change") : t("email.add")}
          </button>
        )}
      </div>
      {editing && signedIn && (
        <form className="password-form" onSubmit={submit} noValidate>
          <label>
            <span className="small-label">{t("email.address")}</span>
            <input type="email" autoComplete="email" value={typed} onChange={(event) => setTyped(event.target.value)} />
          </label>
          <label>
            <span className="small-label">{t("password.current")}</span>
            <input type="password" autoComplete="current-password" value={current} onChange={(event) => setCurrent(event.target.value)} />
          </label>
          {problem && (
            <p className="password-problem" role="alert">
              {t(`email.problem.${problem.code}` as MessageKey, { detail: problem.detail ?? "" })}
            </p>
          )}
          <div className="password-actions">
            <button type="button" className="text-button compact" onClick={close} disabled={saving}>
              {t("password.cancel")}
            </button>
            <button type="submit" className="primary compact" disabled={saving}>
              {saving ? t("email.saving") : t("email.save")}
            </button>
          </div>
        </form>
      )}
    </>
  );
}
