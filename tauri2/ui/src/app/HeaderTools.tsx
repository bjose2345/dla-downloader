import { Settings } from "lucide-react";
import { motion } from "motion/react";

import { usePresentation } from "../preferences/PresentationProvider";
import { ThemeToggle } from "./ThemeToggle";

/**
 * The settings gear next to the light/dark switch, on every screen. A dot
 * on it says an app update is ready to install.
 */
export function HeaderTools({ onOpenSettings, updateReady = false }: { onOpenSettings: () => void; updateReady?: boolean }) {
  const { t } = usePresentation();
  const label = updateReady ? t("settings.openUpdateReady") : t("settings.open");
  return (
    <div className="header-buttons">
      {/* Only the gear turns: rotating the round button itself left it half
          a pixel off once the animation ended. An even size centres it on
          whole pixels. */}
      <motion.button
        type="button"
        className="icon-button gear-button"
        aria-label={label}
        title={label}
        onClick={onOpenSettings}
        initial="rest"
        animate="rest"
        whileHover="hover"
        whileTap="tap"
      >
        <motion.span
          className="gear-icon"
          aria-hidden="true"
          variants={{ rest: { rotate: 0, scale: 1 }, hover: { rotate: 45, scale: 1 }, tap: { rotate: 45, scale: 0.9 } }}
          transition={{ type: "spring", stiffness: 260, damping: 18 }}
        >
          <Settings size={20} />
        </motion.span>
        {updateReady && <span className="gear-dot" aria-hidden="true" />}
      </motion.button>
      <ThemeToggle />
    </div>
  );
}
