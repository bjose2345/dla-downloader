import { ArrowRight, CircleAlert } from "lucide-react";
import { AnimatePresence, motion, useAnimate } from "motion/react";
import { useEffect, useRef, useState, type FormEvent } from "react";

import { connectionText, signInText } from "../shared/copy";
import { usePresentation } from "../preferences/PresentationProvider";
import type { AppInfo, ConnectionView, Notice } from "../backend/types";
import { ForgotPassword } from "./ForgotPassword";
import { HeaderTools } from "./HeaderTools";
import { Logo } from "../shared/Logo";
import { Typewriter } from "./Typewriter";

interface Props {
  info: AppInfo;
  connection: ConnectionView | null;
  notice: Notice | null;
  onSignIn: (username: string, password: string) => Promise<unknown>;
  onOpenSettings: () => void;
  updateReady: boolean;
}

// These point at what was typed, so the fields are marked too.
const CREDENTIAL_PROBLEMS = new Set(["sign_in_rejected", "self_test_account_only"]);

export function WelcomeScreen({ info, connection, notice, onSignIn, onOpenSettings, updateReady }: Props) {
  const { t } = usePresentation();
  // Empty unless the user chose "Remember my username" in Settings.
  const [username, setUsername] = useState(info.lastAccount ?? "");
  const [password, setPassword] = useState("");
  const [pending, setPending] = useState(false);
  // "Forgot password?" swaps the form for the recovery steps.
  const [recovering, setRecovering] = useState(false);
  // The last problem stays until the user edits a field.
  const [dismissed, setDismissed] = useState<Notice | null>(null);
  const [fields, animate] = useAnimate();
  const usernameInput = useRef<HTMLInputElement>(null);
  const passwordInput = useRef<HTMLInputElement>(null);

  const problem = notice && notice !== dismissed ? notice : null;
  const invalid = Boolean(problem && CREDENTIAL_PROBLEMS.has(problem.code));
  // Sign-in checks the account with the download service; say so only when it is not ready.
  const link = connectionText(t, connection?.archive ?? null);

  useEffect(() => {
    if (!notice || !fields.current) return;
    void animate(fields.current, { x: [0, -10, 9, -6, 4, 0] }, { duration: 0.42 });
    passwordInput.current?.focus();
  }, [notice, animate, fields]);

  const edit = (update: () => void) => {
    update();
    if (problem) setDismissed(problem);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!username.trim()) return usernameInput.current?.focus();
    if (!password) return passwordInput.current?.focus();
    setPending(true);
    try {
      await onSignIn(username.trim(), password);
    } finally {
      setPassword("");
      setPending(false);
    }
  };

  return (
    <div className="welcome">
      <header className="welcome-top">
        <AnimatePresence>
          {link.tone !== "ready" && (
            <motion.span
              className={"connection " + link.tone}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              title={connection && "detail" in connection.archive ? connection.archive.detail : undefined}
            >
              <span className="dot" />
              <span>{link.text}</span>
            </motion.span>
          )}
        </AnimatePresence>
        <HeaderTools onOpenSettings={onOpenSettings} updateReady={updateReady} />
      </header>

      <motion.main
        className="welcome-main"
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.45, ease: "easeOut" }}
      >
        <Logo size={44} />
        {/* The recovery steps need the room. */}
        {!recovering && <Typewriter />}

        {recovering ? (
          <ForgotPassword
            initialUsername={username.trim()}
            onClose={(name) => {
              if (name) setUsername(name);
              setPassword("");
              setRecovering(false);
            }}
          />
        ) : (
        <form className="welcome-form" onSubmit={submit} noValidate>
          <div className="welcome-fields" ref={fields}>
            <label className="sr-only" htmlFor="username">
              {t("welcome.username")}
            </label>
            <input
              id="username"
              ref={usernameInput}
              autoComplete="off"
              spellCheck={false}
              maxLength={80}
              placeholder={t("welcome.username")}
              aria-invalid={invalid}
              disabled={pending}
              autoFocus={!username}
              value={username}
              onChange={(event) => edit(() => setUsername(event.target.value))}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !password) {
                  event.preventDefault();
                  passwordInput.current?.focus();
                }
              }}
            />
            <div className="password-field">
              <label className="sr-only" htmlFor="password">
                {t("welcome.password")}
              </label>
              <input
                id="password"
                ref={passwordInput}
                type="password"
                autoComplete="off"
                maxLength={200}
                placeholder={t("welcome.password")}
                aria-invalid={invalid}
                aria-describedby={problem ? "sign-in-problem" : undefined}
                disabled={pending}
                autoFocus={Boolean(username)}
                value={password}
                onChange={(event) => edit(() => setPassword(event.target.value))}
              />
              <motion.button
                type="submit"
                className="go"
                aria-label={t("welcome.signIn")}
                title={t("welcome.signIn")}
                disabled={pending || !username.trim() || !password}
                whileTap={{ scale: 0.92 }}
              >
                {pending ? <span className="spinner" aria-hidden="true" /> : <ArrowRight size={18} aria-hidden="true" />}
              </motion.button>
            </div>
          </div>
          <button type="button" className="text-button compact forgot-link" disabled={pending} onClick={() => setRecovering(true)}>
            {t("forgot.link")}
          </button>

          <AnimatePresence>
            {problem && (
              <motion.p
                key={problem.code + (problem.detail ?? "")}
                id="sign-in-problem"
                className="welcome-problem"
                role="alert"
                initial={{ opacity: 0, y: -6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={{ duration: 0.2 }}
              >
                <CircleAlert size={16} aria-hidden="true" />
                {signInText(t, problem)}
              </motion.p>
            )}
          </AnimatePresence>
        </form>
        )}
      </motion.main>

    </div>
  );
}
