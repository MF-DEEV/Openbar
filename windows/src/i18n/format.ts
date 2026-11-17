// SPDX-License-Identifier: GPL-3.0-or-later
/** Only explicit tf calls format strings; filename-pattern help stays literal. */
export function formatSwift(format: string, ...args: (string | number)[]): string {
  let sequential = 0;
  return format.replace(
    /%(?:(\d+)\$)?(@|l{0,2}[diu]|(?:\.\d+)?f|%)/g,
    (token, position: string | undefined, type: string) => {
      if (type === "%") return "%";
      const value = args[position ? Number(position) - 1 : sequential++];
      if (value === undefined) return token;
      if (type === "@") return String(value);
      const n = Number(value);
      if (!Number.isFinite(n)) return String(value);
      if (type.endsWith("f")) return n.toFixed(type.startsWith(".") ? Math.min(20, Number(type.slice(1, -1))) : 6);
      return String(type.endsWith("u") ? Math.trunc(n) >>> 0 : Math.trunc(n));
    },
  );
}
