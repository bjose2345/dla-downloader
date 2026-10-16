import { useId } from "react";

/** The DLA Downloader mark, the same drawing as src-tauri/icons/app-icon.svg. */
export function LogoMark({ size = 40 }: { size?: number }) {
  const gradient = `logo-${useId().replaceAll(":", "")}`;
  return (
    <svg className="logo-mark" width={size} height={size} viewBox="64 64 896 896" aria-hidden="true">
      <defs>
        <linearGradient id={gradient} x1="0.15" y1="0.05" x2="0.9" y2="1">
          <stop offset="0" stopColor="#14b8a6" />
          <stop offset="1" stopColor="#3d7eff" />
        </linearGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="232" fill={`url(#${gradient})`} />
      <g fill="none" stroke="#fff" strokeLinecap="round" strokeLinejoin="round" strokeWidth="88">
        <path d="M512 268V620M368 476L512 620L656 476" />
        <path d="M300 744H724" strokeOpacity="0.72" />
      </g>
    </svg>
  );
}

export function Logo({ size = 40 }: { size?: number }) {
  return (
    <span className="logo">
      <LogoMark size={size} />
      <span className="logo-word">
        DLA <strong>Downloader</strong>
      </span>
    </span>
  );
}
