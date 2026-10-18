import { Lock, type LucideIcon } from "lucide-react";
import { motion } from "motion/react";
import type { ReactNode } from "react";

export interface TabItem<T extends string> {
  id: T;
  label: string;
  icon: LucideIcon;
  /** A small mark after the label, such as a running download. */
  badge?: { label: string; mark: ReactNode };
  /** Shown with a lock; choosing it asks to sign in. */
  locked?: string;
}

/** A segmented control whose highlight slides to the chosen tab. */
export function Tabs<T extends string>({
  items,
  current,
  label,
  onChange,
  className,
  pillId = "tab-pill",
}: {
  items: TabItem<T>[];
  current: T;
  label: string;
  onChange: (id: T) => void;
  className?: string;
  /** Distinct per control, so two on one screen keep their own highlight. */
  pillId?: string;
}) {
  return (
    <nav className={className ? `tabs ${className}` : "tabs"} aria-label={label}>
      {items.map(({ id, label: text, icon: Icon, badge, locked }) => (
        <button
          key={id}
          type="button"
          className={"tab" + (id === current ? " current" : "") + (locked ? " locked" : "")}
          aria-current={id === current ? "page" : undefined}
          title={locked}
          onClick={() => onChange(id)}
        >
          {id === current && (
            <motion.span className="tab-pill" layoutId={pillId} transition={{ type: "spring", stiffness: 420, damping: 34 }} />
          )}
          <Icon size={16} aria-hidden="true" />
          <span className="tab-label">{text}</span>
          {locked && <Lock className="tab-lock" size={13} aria-label={locked} />}
          {badge && (
            <motion.span className="tab-badge" initial={{ scale: 0 }} animate={{ scale: 1 }} aria-label={badge.label} title={badge.label}>
              {badge.mark}
            </motion.span>
          )}
        </button>
      ))}
    </nav>
  );
}
