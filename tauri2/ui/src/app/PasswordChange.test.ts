import { describe, expect, it } from "vitest";

import { MIN_PASSWORD_LENGTH, passwordFormProblem } from "./PasswordChange";

describe("passwordFormProblem", () => {
  it("checks the typed passwords before anything is sent", () => {
    expect(passwordFormProblem("", "new-password", "new-password")).toBe("missing");
    expect(passwordFormProblem("old", "short", "short")).toBe("too_short");
    expect(passwordFormProblem("old", "new-password", "new-passwort")).toBe("mismatch");
    expect(passwordFormProblem("same-password", "same-password", "same-password")).toBe("same");
    expect(passwordFormProblem("old", "new-password", "new-password")).toBeNull();
  });

  it("counts characters, not bytes", () => {
    expect(MIN_PASSWORD_LENGTH).toBe(8);
    expect(passwordFormProblem("old", "日本語の7文字", "日本語の7文字")).toBe("too_short");
    expect(passwordFormProblem("old", "日本語の8文字だ", "日本語の8文字だ")).toBeNull();
  });
});
