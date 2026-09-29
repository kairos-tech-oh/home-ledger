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
