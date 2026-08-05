import { useId } from "react";

export type AgentIconPlatform =
  | "codex"
  | "claude"
  | "grok"
  | "gemini"
  | "opencode"
  | "openclaw"
  | "hermes";

type AgentIconProps = {
  platform: AgentIconPlatform;
  className?: string;
};

type AgentSvgProps = {
  size?: string | number;
};

const baseAgentSvgStyle = { flex: "none", lineHeight: 1 } as const;

function useSvgGradientId(prefix: string) {
  return `${prefix}-${useId().replace(/:/g, "")}`;
}

function CodexAgentSvg({ size = "100%" }: AgentSvgProps) {
  const gradientId = useSvgGradientId("codex-agent");
  return (
    <svg height={size} style={baseAgentSvgStyle} viewBox="2 2 20 20" width={size} xmlns="http://www.w3.org/2000/svg">
      <path
        d="M9.064 3.344a4.578 4.578 0 012.285-.312c1 .115 1.891.54 2.673 1.275.01.01.024.017.037.021a.09.09 0 00.043 0 4.55 4.55 0 013.046.275l.047.022.116.057a4.581 4.581 0 012.188 2.399c.209.51.313 1.041.315 1.595a4.24 4.24 0 01-.134 1.223.123.123 0 00.03.115c.594.607.988 1.33 1.183 2.17.289 1.425-.007 2.71-.887 3.854l-.136.166a4.548 4.548 0 01-2.201 1.388.123.123 0 00-.081.076c-.191.551-.383 1.023-.74 1.494-.9 1.187-2.222 1.846-3.711 1.838-1.187-.006-2.239-.44-3.157-1.302a.107.107 0 00-.105-.024c-.388.125-.78.143-1.204.138a4.441 4.441 0 01-1.945-.466 4.544 4.544 0 01-1.61-1.335c-.152-.202-.303-.392-.414-.617a5.81 5.81 0 01-.37-.961 4.582 4.582 0 01-.014-2.298.124.124 0 00.006-.056.085.085 0 00-.027-.048 4.467 4.467 0 01-1.034-1.651 3.896 3.896 0 01-.251-1.192 5.189 5.189 0 01.141-1.6c.337-1.112.982-1.985 1.933-2.618.212-.141.413-.251.601-.33.215-.089.43-.164.646-.227a.098.098 0 00.065-.066 4.51 4.51 0 01.829-1.615 4.535 4.535 0 011.837-1.388zm3.482 10.565a.637.637 0 000 1.272h3.636a.637.637 0 100-1.272h-3.636zM8.462 9.23a.637.637 0 00-1.106.631l1.272 2.224-1.266 2.136a.636.636 0 101.095.649l1.454-2.455a.636.636 0 00.005-.64L8.462 9.23z"
        fill={`url(#${gradientId})`}
      />
      <defs>
        <linearGradient gradientUnits="userSpaceOnUse" id={gradientId} x1="12" x2="12" y1="3" y2="21">
          <stop stopColor="#B1A7FF" />
          <stop offset=".5" stopColor="#7A9DFF" />
          <stop offset="1" stopColor="#3941FF" />
        </linearGradient>
      </defs>
    </svg>
  );
}

function ClaudeAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg height={size} style={baseAgentSvgStyle} viewBox="0 0 24 24" width={size} xmlns="http://www.w3.org/2000/svg">
      <path
        clipRule="evenodd"
        d="M20.998 10.949H24v3.102h-3v3.028h-1.487V20H18v-2.921h-1.487V20H15v-2.921H9V20H7.488v-2.921H6V20H4.487v-2.921H3V14.05H0V10.95H3V5h17.998v5.949zM6 10.949h1.488V8.102H6v2.847zm10.51 0H18V8.102h-1.49v2.847z"
        fill="#D97757"
        fillRule="evenodd"
      />
    </svg>
  );
}

function GrokAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg height={size} style={baseAgentSvgStyle} viewBox="0 0 24 24" width={size} xmlns="http://www.w3.org/2000/svg">
      <rect fill="#0B0F14" height="24" rx="5" width="24" />
      <path d="M6.2 7.4h2.1l2.1 5.2 2.1-5.2h2.1l-3.4 8.1H9.6L6.2 7.4zm10.1 0H18v8.1h-1.7V7.4z" fill="#F8FAFC" />
      <circle cx="18.4" cy="16.8" fill="#22D3EE" r="1.1" />
    </svg>
  );
}

function GeminiAgentSvg({ size = "100%" }: AgentSvgProps) {
  const gradientId = useSvgGradientId("gemini-agent");
  return (
    <svg height={size} style={baseAgentSvgStyle} viewBox="0 0 24 24" width={size} xmlns="http://www.w3.org/2000/svg">
      <path d="M0 4.391A4.391 4.391 0 014.391 0h15.217A4.391 4.391 0 0124 4.391v15.217A4.391 4.391 0 0119.608 24H4.391A4.391 4.391 0 010 19.608V4.391z" fill={`url(#${gradientId})`} />
      <path
        clipRule="evenodd"
        d="M19.74 1.444a2.816 2.816 0 012.816 2.816v15.48a2.816 2.816 0 01-2.816 2.816H4.26a2.816 2.816 0 01-2.816-2.816V4.26A2.816 2.816 0 014.26 1.444h15.48zM7.236 8.564l7.752 3.728-7.752 3.727v2.802l9.557-4.596v-3.866L7.236 5.763v2.801z"
        fill="#1E1E2E"
        fillRule="evenodd"
      />
      <defs>
        <linearGradient gradientUnits="userSpaceOnUse" id={gradientId} x1="24" x2="0" y1="6.587" y2="16.494">
          <stop stopColor="#EE4D5D" />
          <stop offset=".328" stopColor="#B381DD" />
          <stop offset=".476" stopColor="#207CFE" />
        </linearGradient>
      </defs>
    </svg>
  );
}

function OpenCodeAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg fill="currentColor" fillRule="evenodd" height={size} style={baseAgentSvgStyle} viewBox="0 0 24 24" width={size} xmlns="http://www.w3.org/2000/svg">
      <path d="M16 6H8v12h8V6zm4 16H4V2h16v20z" />
    </svg>
  );
}

function OpenClawAgentSvg({ size = "100%" }: AgentSvgProps) {
  const shellId = useSvgGradientId("openclaw-agent-shell");
  const leftId = useSvgGradientId("openclaw-agent-left");
  const rightId = useSvgGradientId("openclaw-agent-right");
  return (
    <svg height={size} style={baseAgentSvgStyle} viewBox="0 0 24 24" width={size} xmlns="http://www.w3.org/2000/svg">
      <path d="M12 2.568c-6.33 0-9.495 5.275-9.495 9.495 0 4.22 3.165 8.44 6.33 9.494v2.11h2.11v-2.11s1.055.422 2.11 0v2.11h2.11v-2.11c3.165-1.055 6.33-5.274 6.33-9.494S18.33 2.568 12 2.568z" fill={`url(#${shellId})`} />
      <path d="M3.56 9.953C.396 8.898-.66 11.008.396 13.118c1.055 2.11 3.164 1.055 4.22-1.055.632-1.477 0-2.11-1.056-2.11z" fill={`url(#${leftId})`} />
      <path d="M20.44 9.953c3.164-1.055 4.22 1.055 3.164 3.165-1.055 2.11-3.164 1.055-4.22-1.055-.632-1.477 0-2.11 1.056-2.11z" fill={`url(#${rightId})`} />
      <path d="M5.507 1.875c.476-.285 1.036-.233 1.615.037.577.27 1.223.774 1.937 1.488a.316.316 0 01-.447.447c-.693-.693-1.279-1.138-1.757-1.361-.475-.222-.795-.205-1.022-.069a.317.317 0 01-.326-.542zM16.877 1.913c.58-.27 1.14-.323 1.616-.038a.317.317 0 01-.326.542c-.227-.136-.547-.153-1.022.069-.478.223-1.064.668-1.756 1.361a.316.316 0 11-.448-.447c.714-.714 1.36-1.218 1.936-1.487z" fill="#FF4D4D" />
      <path d="M8.835 9.109a1.266 1.266 0 100-2.532 1.266 1.266 0 000 2.532zM15.165 9.109a1.266 1.266 0 100-2.532 1.266 1.266 0 000 2.532z" fill="#050810" />
      <path d="M9.046 8.16a.527.527 0 100-1.056.527.527 0 100 1.055zM15.376 8.16a.527.527 0 100-1.055.527.527 0 100 1.054z" fill="#00E5CC" />
      <defs>
        <linearGradient gradientUnits="userSpaceOnUse" id={shellId} x1="-.659" x2="27.023" y1=".458" y2="22.855">
          <stop stopColor="#FF4D4D" />
          <stop offset="1" stopColor="#991B1B" />
        </linearGradient>
        <linearGradient gradientUnits="userSpaceOnUse" id={leftId} x1="0" x2="4.311" y1="9.672" y2="14.949">
          <stop stopColor="#FF4D4D" />
          <stop offset="1" stopColor="#991B1B" />
        </linearGradient>
        <linearGradient gradientUnits="userSpaceOnUse" id={rightId} x1="19.385" x2="24.399" y1="9.953" y2="14.462">
          <stop stopColor="#FF4D4D" />
          <stop offset="1" stopColor="#991B1B" />
        </linearGradient>
      </defs>
    </svg>
  );
}

function HermesAgentSvg({ size = "100%" }: AgentSvgProps) {
  return (
    <svg fill="none" height={size} style={baseAgentSvgStyle} viewBox="0 0 24 24" width={size} xmlns="http://www.w3.org/2000/svg">
      <rect height="20" rx="6" stroke="currentColor" strokeWidth="2" width="20" x="2" y="2" />
      <path d="M7 16V8M17 8v8M8 12h8M5 7.2l2.4-2.1M19 7.2l-2.4-2.1" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" />
    </svg>
  );
}

export function AgentIcon({ platform, className }: AgentIconProps) {
  const iconClassName = ["inline-flex shrink-0", className].filter(Boolean).join(" ");
  const icon =
    platform === "codex" ? <CodexAgentSvg /> :
    platform === "claude" ? <ClaudeAgentSvg /> :
    platform === "grok" ? <GrokAgentSvg /> :
    platform === "gemini" ? <GeminiAgentSvg /> :
    platform === "opencode" ? <OpenCodeAgentSvg /> :
    platform === "openclaw" ? <OpenClawAgentSvg /> : <HermesAgentSvg />;

  return <span aria-hidden="true" className={iconClassName}>{icon}</span>;
}
