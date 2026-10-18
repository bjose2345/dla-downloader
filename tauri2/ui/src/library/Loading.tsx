import { LoaderCircle } from "lucide-react";

import { usePresentation } from "../preferences/PresentationProvider";

/** "Loading…", or another word for the wait, with a turning circle, for a pane heading. */
export function LoadingLabel({ text }: { text?: string }) {
  const { t } = usePresentation();
  return (
    <span className="loading-label">
      <LoaderCircle size={14} className="spinning" aria-hidden="true" />
      {text ?? t("library.loading")}
    </span>
  );
}

const ROWS = 8;

/** Placeholder rows shaped like a file table while its files load. */
export function TableSkeleton() {
  const { t } = usePresentation();
  return (
    <div className="table-skeleton" role="status" aria-busy="true">
      <span className="sr-only">{t("library.loading")}</span>
      <div className="skeleton-head" aria-hidden="true" />
      {Array.from({ length: ROWS }, (_, index) => (
        // Four widths in turn, so the rows read as a list, not a grid of blocks.
        <div key={index} className={`skeleton-row v${index % 4}`} aria-hidden="true">
          <span className="skeleton box" />
          <span className="skeleton-cell">
            <span className="skeleton s-name" />
            <span className="skeleton thin s-code" />
          </span>
          <span className="skeleton-cell">
            <span className="skeleton s-title" />
            <span className="skeleton thin s-circle" />
          </span>
          <span className="skeleton s-size" />
          <span className="skeleton s-date" />
          <span className="skeleton button" />
        </div>
      ))}
    </div>
  );
}

/** Placeholder years and months while the tree loads. */
export function TreeSkeleton() {
  return (
    <div className="tree-skeleton" aria-hidden="true">
      {Array.from({ length: 6 }, (_, index) => (
        <span key={index} className={"skeleton" + (index === 0 ? " year" : " month")} />
      ))}
    </div>
  );
}
