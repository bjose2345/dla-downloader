import { CircleAlert, CircleCheck } from "lucide-react";
import { useState, type FormEvent } from "react";

import { gateway } from "../backend/gateway";
import type { AccountProblem } from "../backend/types";
import type { MessageKey } from "../i18n/catalogs";
import { usePresentation } from "../preferences/PresentationProvider";
import { MIN_PASSWORD_LENGTH, passwordFormProblem } from "./PasswordChange";

// Problems with the new password are phrased as in the password form.
const PASSWORD_RULES = new Set(["too_short", "too_long", "invalid", "mismatch", "rejected"]);

export function recoveryProblemKey(code: string): MessageKey {
  return (PASSWORD_RULES.has(code) ? `password.problem.${code}` : `forgot.problem.${code}`) as MessageKey;
}

type Step = "ask" | "reset" | "done";

/**
 * The sign-in screen's "Forgot password?": a code is emailed to the account,
 * then typed here with the new password. Nothing says whether an account
 * exists or has an email address.
 */
export function ForgotPassword({ initialUsername, onClose }: { initialUsername: string; onClose: (username: string) => void }) {
  const { t } = usePresentation();
  const [step, setStep] = useState<Step>("ask");
  const [username, setUsername] = useState(initialUsername);
  const [code, setCode] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");
  const [pending, setPending] = useState(false);
  const [problem, setProblem] = useState<AccountProblem | null>(null);

  const run = async (request: () => Promise<{ changed: boolean; problem: AccountProblem | null }>, after: () => void) => {
    setPending(true);
    setProblem(null);
    try {
      const outcome = await request();
      if (outcome.changed) after();
      else setProblem(outcome.problem ?? { code: "failed" });
    } catch (error) {
      setProblem({ code: "failed", detail: String(error) });
    } finally {
      setPending(false);
    }
  };

  const sendCode = (event?: FormEvent) => {
    event?.preventDefault();
    if (!username.trim()) {
      setProblem({ code: "username" });
      return;
    }
    void run(
      () => gateway.sendResetCode(username.trim()),
      () => setStep("reset"),
    );
  };

  const reset = (event: FormEvent) => {
    event.preventDefault();
    if (!code.trim()) {
      setProblem({ code: "missing" });
      return;
    }
    // The current password is unknown here, so only the new one is checked.
    const typed = passwordFormProblem("-", next, repeat);
    if (typed && typed !== "same") {
      setProblem({ code: typed });
      return;
    }
    void run(
      () => gateway.resetPassword(username.trim(), code.trim(), next),
      () => {
        setNext("");
        setRepeat("");
        setStep("done");
      },
    );
  };

  const problemLine = problem && (
    <p className="recovery-problem" role="alert">
      <CircleAlert size={16} aria-hidden="true" />
      {t(recoveryProblemKey(problem.code), { detail: problem.detail ?? "", min: MIN_PASSWORD_LENGTH })}
    </p>
  );

  return (
    <div className="recovery">
      <h2>{t("forgot.title")}</h2>
      {step === "ask" && (
        <form onSubmit={sendCode} noValidate>
          <p className="recovery-lead">{t("forgot.lead")}</p>
          <div className="welcome-fields">
            <input
              aria-label={t("welcome.username")}
              placeholder={t("welcome.username")}
              autoComplete="off"
              spellCheck={false}
              maxLength={80}
              autoFocus
              disabled={pending}
              value={username}
              onChange={(event) => setUsername(event.target.value)}
            />
          </div>
          {problemLine}
          <button type="submit" className="primary recovery-main" disabled={pending}>
            {pending ? t("forgot.sending") : t("forgot.send")}
          </button>
        </form>
      )}
      {step === "reset" && (
        <form onSubmit={reset} noValidate>
          <p className="recovery-lead">{t("forgot.sent")}</p>
          <div className="welcome-fields">
            <input
              aria-label={t("forgot.code")}
              placeholder={t("forgot.code")}
              autoComplete="one-time-code"
              spellCheck={false}
              maxLength={100}
              autoFocus
              disabled={pending}
              value={code}
              onChange={(event) => setCode(event.target.value)}
            />
            <input
              type="password"
              aria-label={t("password.new")}
              placeholder={t("password.new")}
              autoComplete="new-password"
              maxLength={200}
              disabled={pending}
              value={next}
              onChange={(event) => setNext(event.target.value)}
            />
            <input
              type="password"
              aria-label={t("password.repeat")}
              placeholder={t("password.repeat")}
              autoComplete="new-password"
              maxLength={200}
              disabled={pending}
              value={repeat}
              onChange={(event) => setRepeat(event.target.value)}
            />
          </div>
          <p className="recovery-hint">{t("password.rule", { min: MIN_PASSWORD_LENGTH })}</p>
          {problemLine}
          <button type="submit" className="primary recovery-main" disabled={pending}>
            {pending ? t("forgot.resetting") : t("forgot.reset")}
          </button>
          <button type="button" className="text-button compact" disabled={pending} onClick={() => sendCode()}>
            {t("forgot.resend")}
          </button>
        </form>
      )}
      {step === "done" && (
        <>
          <p className="recovery-done" role="status">
            <CircleCheck size={16} aria-hidden="true" />
            {t("forgot.done")}
          </p>
          <button type="button" className="primary recovery-main" onClick={() => onClose(username.trim())}>
            {t("forgot.signIn")}
          </button>
        </>
      )}
      {step !== "done" && (
        <>
          <p className="recovery-note">{t("forgot.noEmail")}</p>
          <button type="button" className="text-button compact" onClick={() => onClose(username.trim())}>
            {t("forgot.back")}
          </button>
        </>
      )}
    </div>
  );
}
