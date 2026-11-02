// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

// The filled phase symbols the Mac draws with SF Symbols
// (checkmark.circle.fill, hand.raised.fill), which outline icon sets lack.

import { useLocale } from "../i18n";

import { type CSSProperties, useId } from "react";
import { CLAUDE_TINT } from "./format";
import type { Provider } from "./types";

export interface GlyphProps {
  size?: number;
  strokeWidth?: number;
  style?: CSSProperties;
  className?: string;
  /** Agent marks breathe like the Mac's glyph while the session works; the rest ignore it. */
  working?: boolean;
}

/** A filled circle in the current color with a dark tick cut into it. */
export function CheckCircleFill({ size = 14, style, className }: GlyphProps) {
  useLocale();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <circle cx="12" cy="12" r="11" fill="currentColor" />
      <path
        d="M7 12.4l3.3 3.3L17.2 8.6"
        fill="none"
        stroke="#000"
        strokeWidth="2.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** A raised open hand, filled. */
export function HandFill({ size = 14, style, className }: GlyphProps) {
  useLocale();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <g fill="currentColor">
        <rect x="6.6" y="4.2" width="2.9" height="10.5" rx="1.45" />
        <rect x="9.9" y="2" width="2.9" height="12" rx="1.45" />
        <rect x="13.2" y="2.8" width="2.9" height="11.5" rx="1.45" />
        <rect x="16.5" y="5" width="2.9" height="10" rx="1.45" />
        <path d="M6.6 11.5h12.8v3.6c0 4.2-2.9 7.4-6.6 7.4-2.3 0-3.9-1-5.3-2.9L3.6 14.6c-.7-.9-.5-2.1.4-2.7.8-.6 1.9-.4 2.6.4V11.5z" />
      </g>
    </svg>
  );
}

/** The four-pointed sparkle, filled: a session at work beside its count. */
export function SparkleFill({ size = 14, style, className }: GlyphProps) {
  useLocale();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <path
        d="M12 1.5c.6 4.9 2.2 8.4 10.5 10.5-8.3 2.1-9.9 5.6-10.5 10.5C11.4 17.6 9.8 14.1 1.5 12 9.8 9.9 11.4 6.4 12 1.5z"
        fill="currentColor"
      />
    </svg>
  );
}

/** A gauge with its needle at the end, filled (gauge.with.dots.needle.100percent):
 * a session the plan's limit stopped. */
export function GaugeFill({ size = 14, style, className }: GlyphProps) {
  useLocale();
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" style={style} className={className} aria-hidden>
      <circle cx="12" cy="12" r="11" fill="currentColor" />
      <g fill="#000">
        <circle cx="6.1" cy="14.8" r="1.15" />
        <circle cx="6.6" cy="9.4" r="1.15" />
        <circle cx="9.9" cy="6.3" r="1.15" />
        <circle cx="14.3" cy="6.3" r="1.15" />
      </g>
      <path d="M12 13.6l6.4-1.9" stroke="#000" strokeWidth="2.4" strokeLinecap="round" />
      <circle cx="12" cy="13.6" r="2.1" fill="#000" />
    </svg>
  );
}

// The makers' marks, as the Mac takes them from the installed apps. Paths
// from @lobehub/icons-static-svg (MIT); the marks belong to Anthropic and
// OpenAI and only name whose session it is.
const CLAUDE_PATH =
  "M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z";
const CODEX_PATH =
  "M8.086.457a6.105 6.105 0 013.046-.415c1.333.153 2.521.72 3.564 1.7a.117.117 0 00.107.029c1.408-.346 2.762-.224 4.061.366l.063.03.154.076c1.357.703 2.33 1.77 2.918 3.198.278.679.418 1.388.421 2.126a5.655 5.655 0 01-.18 1.631.167.167 0 00.04.155 5.982 5.982 0 011.578 2.891c.385 1.901-.01 3.615-1.183 5.14l-.182.22a6.063 6.063 0 01-2.934 1.851.162.162 0 00-.108.102c-.255.736-.511 1.364-.987 1.992-1.199 1.582-2.962 2.462-4.948 2.451-1.583-.008-2.986-.587-4.21-1.736a.145.145 0 00-.14-.032c-.518.167-1.04.191-1.604.185a5.924 5.924 0 01-2.595-.622 6.058 6.058 0 01-2.146-1.781c-.203-.269-.404-.522-.551-.821a7.74 7.74 0 01-.495-1.283 6.11 6.11 0 01-.017-3.064.166.166 0 00.008-.074.115.115 0 00-.037-.064 5.958 5.958 0 01-1.38-2.202 5.196 5.196 0 01-.333-1.589 6.915 6.915 0 01.188-2.132c.45-1.484 1.309-2.648 2.577-3.493.282-.188.55-.334.802-.438.286-.12.573-.22.861-.304a.129.129 0 00.087-.087A6.016 6.016 0 015.635 2.31C6.315 1.464 7.132.846 8.086.457zm-.804 7.85a.848.848 0 00-1.473.842l1.694 2.965-1.688 2.848a.849.849 0 001.46.864l1.94-3.272a.849.849 0 00.007-.854l-1.94-3.393zm5.446 6.24a.849.849 0 000 1.695h4.848a.849.849 0 000-1.696h-4.848z";

export type MarkProps = GlyphProps;

/** Claude's spark, in its orange unless the parent says otherwise. */
export function ClaudeMark({ size = 14, style, className, working }: MarkProps) {
  useLocale();
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      style={{ color: CLAUDE_TINT, ...style }}
      className={`agent-mark${working ? " is-working" : ""}${className ? ` ${className}` : ""}`}
      aria-hidden
    >
      <path d={CLAUDE_PATH} fill="currentColor" fillRule="evenodd" />
    </svg>
  );
}

/** Codex's cloud with its prompt cut out, in the app icon's violet-to-blue. */
export function CodexMark({ size = 14, style, className, working }: MarkProps) {
  useLocale();
  const id = useId();
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      style={style}
      className={`agent-mark${working ? " is-working" : ""}${className ? ` ${className}` : ""}`}
      aria-hidden
    >
      <defs>
        <linearGradient id={id} gradientUnits="userSpaceOnUse" x1="12" x2="12" y1="0" y2="24">
          <stop stopColor="#B1A7FF" />
          <stop offset=".5" stopColor="#7A9DFF" />
          <stop offset="1" stopColor="#3941FF" />
        </linearGradient>
      </defs>
      <path d={CODEX_PATH} fill={`url(#${id})`} fillRule="evenodd" clipRule="evenodd" />
    </svg>
  );
}

const markClass = (working?: boolean, className?: string) =>
  `agent-mark${working ? " is-working" : ""}${className ? ` ${className}` : ""}`;

// The makers' marks of Gemini CLI, GitHub Copilot, OpenCode and Cursor
// (NotchHookAgentMarks.swift), from @lobehub/icons-static-svg (MIT). Gemini
// CLI's keeps its gradient; the others are one color, as their makers show
// them on dark.
export const GEMINI_SQUARE =
  "M0 4.391A4.391 4.391 0 014.391 0h15.217A4.391 4.391 0 0124 4.391v15.217A4.391 4.391 0 0119.608 24H4.391A4.391 4.391 0 010 19.608V4.391z";
export const GEMINI_FACE =
  "M19.74 1.444a2.816 2.816 0 012.816 2.816v15.48a2.816 2.816 0 01-2.816 2.816H4.26a2.816 2.816 0 01-2.816-2.816V4.26A2.816 2.816 0 014.26 1.444h15.48zM7.236 8.564l7.752 3.728-7.752 3.727v2.802l9.557-4.596v-3.866L7.236 5.763v2.801z";
export const COPILOT_PATH =
  "M19.245 5.364c1.322 1.36 1.877 3.216 2.11 5.817.622 0 1.2.135 1.592.654l.73.964c.21.278.323.61.323.955v2.62c0 .339-.173.669-.453.868C20.239 19.602 16.157 21.5 12 21.5c-4.6 0-9.205-2.583-11.547-4.258-.28-.2-.452-.53-.453-.868v-2.62c0-.345.113-.679.321-.956l.73-.963c.392-.517.974-.654 1.593-.654l.029-.297c.25-2.446.81-4.213 2.082-5.52 2.461-2.54 5.71-2.851 7.146-2.864h.198c1.436.013 4.685.323 7.146 2.864zm-7.244 4.328c-.284 0-.613.016-.962.05-.123.447-.305.85-.57 1.108-1.05 1.023-2.316 1.18-2.994 1.18-.638 0-1.306-.13-1.851-.464-.516.165-1.012.403-1.044.996a65.882 65.882 0 00-.063 2.884l-.002.48c-.002.563-.005 1.126-.013 1.69.002.326.204.63.51.765 2.482 1.102 4.83 1.657 6.99 1.657 2.156 0 4.504-.555 6.985-1.657a.854.854 0 00.51-.766c.03-1.682.006-3.372-.076-5.053-.031-.596-.528-.83-1.046-.996-.546.333-1.212.464-1.85.464-.677 0-1.942-.157-2.993-1.18-.266-.258-.447-.661-.57-1.108-.32-.032-.64-.049-.96-.05zm-2.525 4.013c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zm5 0c.539 0 .976.426.976.95v1.753c0 .525-.437.95-.976.95a.964.964 0 01-.976-.95v-1.752c0-.525.437-.951.976-.951zM7.635 5.087c-1.05.102-1.935.438-2.385.906-.975 1.037-.765 3.668-.21 4.224.405.394 1.17.657 1.995.657h.09c.649-.013 1.785-.176 2.73-1.11.435-.41.705-1.433.675-2.47-.03-.834-.27-1.52-.63-1.813-.39-.336-1.275-.482-2.265-.394zm6.465.394c-.36.292-.6.98-.63 1.813-.03 1.037.24 2.06.675 2.47.968.957 2.136 1.104 2.776 1.11h.044c.825 0 1.59-.263 1.995-.657.555-.556.765-3.187-.21-4.224-.45-.468-1.335-.804-2.385-.906-.99-.088-1.875.058-2.265.394zM12 7.615c-.24 0-.525.015-.84.044.03.16.045.336.06.526l-.001.159a2.94 2.94 0 01-.014.25c.225-.022.425-.027.612-.028h.366c.187 0 .387.006.612.028-.015-.146-.015-.277-.015-.409.015-.19.03-.365.06-.526a9.29 9.29 0 00-.84-.044z";
export const OPENCODE_PATH = "M16 6H8v12h8V6zm4 16H4V2h16v20z";
export const CURSOR_PATH =
  "M22.106 5.68L12.5.135a.998.998 0 00-.998 0L1.893 5.68a.84.84 0 00-.419.726v11.186c0 .3.16.577.42.727l9.607 5.547a.999.999 0 00.998 0l9.608-5.547a.84.84 0 00.42-.727V6.407a.84.84 0 00-.42-.726zm-.603 1.176L12.228 22.92c-.063.108-.228.064-.228-.061V12.34a.59.59 0 00-.295-.51l-9.11-5.26c-.107-.062-.063-.228.062-.228h18.55c.264 0 .428.286.296.514z";
/** How the one-color marks show on the island's black. */
export const MONO_MARK = "#F0F0F0";

/** Gemini CLI's rounded square in Gemini's red, violet and blue, its prompt cut into the dark face. */
export function GeminiMark({ size = 14, style, className, working }: MarkProps) {
  useLocale();
  const id = useId();
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      style={style}
      className={markClass(working, className)}
      aria-hidden
    >
      <defs>
        <linearGradient id={id} gradientUnits="userSpaceOnUse" x1="24" x2="0" y1="6.587" y2="16.494">
          <stop stopColor="#EE4D5D" />
          <stop offset=".328" stopColor="#B381DD" />
          <stop offset=".476" stopColor="#207CFE" />
        </linearGradient>
      </defs>
      <path d={GEMINI_SQUARE} fill={`url(#${id})`} />
      <path d={GEMINI_FACE} fill="#1E1E2E" fillRule="evenodd" clipRule="evenodd" />
    </svg>
  );
}

function monoMark(d: string) {
  return function MonoMark({ size = 14, style, className, working }: MarkProps) {
    useLocale();
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        style={{ color: MONO_MARK, ...style }}
        className={markClass(working, className)}
        aria-hidden
      >
        <path d={d} fill="currentColor" fillRule="evenodd" />
      </svg>
    );
  };
}

/** GitHub Copilot's goggled face. */
export const CopilotMark = monoMark(COPILOT_PATH);
/** OpenCode's hollow block. */
export const OpencodeMark = monoMark(OPENCODE_PATH);
/** Cursor's cube. */
export const CursorMark = monoMark(CURSOR_PATH);

/** How much larger than a symbol each mark draws to look the same size: the
 * spark's thin rays need more of the box than the cloud (AgentProvider.markScale). */
export const MARK_SCALE: Readonly<Record<Provider, number>> = {
  claude: 1.45,
  codex: 1.2,
  gemini: 1.15,
  copilot: 1.15,
  opencode: 1.15,
  cursor: 1.15,
};

const MARKS: Readonly<Record<Provider, (props: MarkProps) => ReturnType<typeof ClaudeMark>>> = {
  claude: ClaudeMark,
  codex: CodexMark,
  gemini: GeminiMark,
  copilot: CopilotMark,
  opencode: OpencodeMark,
  cursor: CursorMark,
};

/** The mark of an agent, to draw as a component. */
export const markOf = (provider: Provider) => MARKS[provider] ?? ClaudeMark;

/** The mark of whoever runs a session. */
export function ProviderMark({ provider, ...props }: MarkProps & { provider: Provider }) {
  useLocale();
  const Mark = markOf(provider);
  return <Mark {...props} />;
}
