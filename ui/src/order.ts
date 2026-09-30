// Display order for lists of figures. No imports, so the kb can run it.

/** Items split by key, in the given order; keys it does not name follow, as first seen. */
export function sections<T>(
  items: T[],
  keyOf: (item: T) => string,
  order: readonly string[],
): { key: string; items: T[] }[] {
  const seen = new Map<string, T[]>();
  for (const item of items) {
    const key = keyOf(item);
    seen.set(key, [...(seen.get(key) ?? []), item]);
  }
  const keys = [...order.filter((k) => seen.has(k)), ...[...seen.keys()].filter((k) => !order.includes(k))];
  return keys.map((key) => ({ key, items: seen.get(key)! }));
}

/** Largest total first; equal totals by name, so the order never shuffles. */
export function largestFirst<T extends { total: string; name: string }>(items: T[]): T[] {
  return [...items].sort(
    (a, b) => Number(b.total) - Number(a.total) || a.name.localeCompare(b.name),
  );
}

/**
 * The order to show sections in. What was arranged comes first, as arranged;
 * any kind not arranged (a new one, or all of them by default) follows, the
 * sections with the most items first and equal counts in `fallback` order.
 * Only kinds that have items are returned.
 */
export function sectionOrder(
  counts: Record<string, number>,
  arranged: readonly string[],
  fallback: readonly string[],
): string[] {
  const present = Object.keys(counts).filter((k) => counts[k] > 0);
  const first = arranged.filter((k) => present.includes(k));
  const rank = (k: string) => {
    const i = fallback.indexOf(k);
    return i < 0 ? fallback.length : i;
  };
  const rest = present
    .filter((k) => !first.includes(k))
    .sort((a, b) => counts[b] - counts[a] || rank(a) - rank(b) || a.localeCompare(b));
  return [...first, ...rest];
}
