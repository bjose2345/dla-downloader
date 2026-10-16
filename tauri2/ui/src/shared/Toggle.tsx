import { motion } from "motion/react";

/** A labelled on/off switch with a line of explanation. */
export function Toggle({
  label,
  description,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className={"toggle-row" + (disabled ? " disabled" : "")}>
      <span>
        <span className="toggle-label">{label}</span>
        <span className="toggle-description">{description}</span>
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        className={"switch" + (checked ? " on" : "")}
        disabled={disabled}
        onClick={() => onChange(!checked)}
      >
        <motion.span className="switch-knob" layout transition={{ type: "spring", stiffness: 520, damping: 34 }} />
      </button>
    </label>
  );
}
