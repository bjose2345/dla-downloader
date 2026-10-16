import { Search, X } from "lucide-react";

import { usePresentation } from "../preferences/PresentationProvider";

interface Props {
  value: string;
  onChange: (value: string) => void;
  /** The placeholder, also read out as the field's name. */
  label: string;
}

/** The search box above a list: Escape or the cross clears it. */
export function SearchField({ value, onChange, label }: Props) {
  const { t } = usePresentation();
  return (
    <label className="search">
      <span className="sr-only">{label}</span>
      <Search className="search-glyph" size={18} aria-hidden="true" />
      <input
        type="search"
        value={value}
        placeholder={label}
        spellCheck={false}
        autoComplete="off"
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") onChange("");
        }}
      />
      {value && (
        <button type="button" className="icon-button small search-clear" aria-label={t("search.clear")} onClick={() => onChange("")}>
          <X size={16} aria-hidden="true" />
        </button>
      )}
    </label>
  );
}
