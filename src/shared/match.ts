// Fuzzy ranking for the command palette and `bridge open <word>`. Pure; safe in main and renderer.

const norm = (s: string): string => s.toLowerCase().trim()

function isSubsequence(needle: string, hay: string): boolean {
  let i = 0
  for (let j = 0; j < hay.length && i < needle.length; j++) {
    if (hay[j] === needle[i]) i++
  }
  return i === needle.length
}

/**
 * Rank items against a query. Tiers (best first): exact match on any field, prefix on any field, substring on any
 * field, every query word present across all fields, subsequence on any field. Ties keep input order. Items that
 * match nothing are left out; an empty query returns []. Callers slice the result.
 */
export function rank<T>(query: string, items: T[], fields: (t: T) => string[]): T[] {
  const q = norm(query)
  if (!q) return []
  const words = q.split(/\s+/).filter(Boolean)
  const compact = q.replace(/\s+/g, '')
  const tiers: T[][] = [[], [], [], [], []]

  for (const item of items) {
    const fs = fields(item).map(norm).filter(Boolean)
    if (fs.length === 0) continue
    let tier = -1
    if (fs.some((f) => f === q)) tier = 0
    else if (fs.some((f) => f.startsWith(q))) tier = 1
    else if (fs.some((f) => f.includes(q))) tier = 2
    else {
      const joined = fs.join(' ')
      if (words.every((w) => joined.includes(w))) tier = 3
      else if (fs.some((f) => isSubsequence(compact, f))) tier = 4
    }
    if (tier >= 0) tiers[tier].push(item)
  }
  return tiers.flat()
}
