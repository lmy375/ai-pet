/** Shared helpers for the two name-keyed global pools (models, MCP servers). */

/** Rename one key while keeping the map's order, so chips don't jump around. */
export function renameKey<T>(map: Record<string, T>, from: string, to: string): Record<string, T> {
  return Object.fromEntries(Object.entries(map).map(([k, v]) => [k === from ? to : k, v]));
}

/** `base`, or `base 2` / `base 3` … — the first name not already taken. */
export function uniqueName(base: string, taken: string[]): string {
  if (!taken.includes(base)) return base;
  let i = 2;
  while (taken.includes(`${base} ${i}`)) i += 1;
  return `${base} ${i}`;
}
