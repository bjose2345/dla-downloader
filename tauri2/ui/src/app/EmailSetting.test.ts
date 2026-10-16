import { describe, expect, it } from "vitest";

import { emailFormProblem } from "./EmailSetting";
import { recoveryProblemKey } from "./ForgotPassword";

describe("emailFormProblem", () => {
  it("needs an address and the current password", () => {
    expect(emailFormProblem("", "pw")).toBe("missing");
    expect(emailFormProblem("a@example.org", "")).toBe("missing");
    expect(emailFormProblem(" a@example.org ", "pw")).toBeNull();
  });

  it("catches what is plainly not an address", () => {
    for (const bad of ["no-at-sign", "@example.org", "a@", "two@@example.org", "a b@example.org"]) {
      expect(emailFormProblem(bad, "pw")).toBe("invalid_email");
    }
  });
});

describe("recoveryProblemKey", () => {
  it("phrases password rules as the password form does", () => {
    expect(recoveryProblemKey("too_short")).toBe("password.problem.too_short");
    expect(recoveryProblemKey("mismatch")).toBe("password.problem.mismatch");
    expect(recoveryProblemKey("code")).toBe("forgot.problem.code");
    expect(recoveryProblemKey("too_many")).toBe("forgot.problem.too_many");
  });
});
