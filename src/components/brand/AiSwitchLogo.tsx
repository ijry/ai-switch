import { useId, type SVGProps } from "react";

export function AiSwitchLogo(props: SVGProps<SVGSVGElement>) {
  const gradientId = `ai-switch-logo-bg-${useId().replace(/:/g, "")}`;

  return (
    <svg
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 64 64"
      xmlns="http://www.w3.org/2000/svg"
      {...props}
    >
      <defs>
        <linearGradient id={gradientId} x1="10" x2="56" y1="6" y2="60">
          <stop offset="0" stopColor="#292524" />
          <stop offset="1" stopColor="#0C0A09" />
        </linearGradient>
      </defs>
      <rect width="64" height="64" rx="19" fill={`url(#${gradientId})`} />
      <path
        d="M15 19 C27 19 28 32 39 32 C44 32 47 28 50 24"
        fill="none"
        stroke="#10B981"
        strokeLinecap="round"
        strokeWidth="5"
      />
      <path
        d="M15 45 C27 45 28 32 39 32 C44 32 47 36 50 40"
        fill="none"
        stroke="#F59E0B"
        strokeLinecap="round"
        strokeWidth="5"
      />
      <circle cx="15" cy="19" r="4.5" fill="#A7F3D0" />
      <circle cx="15" cy="45" r="4.5" fill="#FCD34D" />
      <circle cx="50" cy="24" r="4.5" fill="#FCD34D" />
      <circle cx="50" cy="40" r="4.5" fill="#A7F3D0" />
      <circle cx="39" cy="32" r="5.5" fill="#FAFAF9" />
      <circle cx="39" cy="32" r="2.4" fill="#1C1917" />
    </svg>
  );
}
