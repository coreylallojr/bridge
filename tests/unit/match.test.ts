import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { rank } from '../../src/shared/match'

interface Item {
  name: string
  keyword?: string
}
const fields = (t: Item) => [t.name, t.keyword ?? '']
const names = (xs: Item[]) => xs.map((x) => x.name)

describe('rank', () => {
  it('empty query returns []', () => {
    expect(rank('', [{ name: 'A' }], fields)).toEqual([])
    expect(rank('   ', [{ name: 'A' }], fields)).toEqual([])
  })

  it('exact keyword or name (case-insensitive) ranks first', () => {
    const items: Item[] = [{ name: 'Calc helper' }, { name: 'Calculator', keyword: 'calc' }, { name: 'CALC' }]
    expect(names(rank('Calc', items, fields))).toEqual(['Calculator', 'CALC', 'Calc helper'])
  })

  it('tiers: exact, prefix, substring, all-words, subsequence', () => {
    const items: Item[] = [
      { name: 'Kanban board' }, // prefix
      { name: 'board xx kanban' }, // substring
      { name: 'my kanban' }, // substring
      { name: 'kanban' }, // exact
      { name: 'kanban stuff' }, // prefix
    ]
    expect(names(rank('kanban', items, fields))).toEqual([
      'kanban',
      'Kanban board',
      'kanban stuff',
      'board xx kanban',
      'my kanban',
    ])
    expect(names(rank('board kanban', items, fields))).toEqual(['Kanban board', 'board xx kanban'])
  })

  it('subsequence: kb matches Kanban board', () => {
    expect(names(rank('kb', [{ name: 'Notepad' }, { name: 'Kanban board' }], fields))).toEqual(['Kanban board'])
  })

  it('ranks by tier in order', () => {
    const items: Item[] = [
      { name: 'kxaxnxbxo' }, // subsequence
      { name: 'bob kanga' }, // all words
      { name: 'zkan boz' }, // substring
      { name: 'kan bo more' }, // prefix
      { name: 'KAN BO' }, // exact
    ]
    expect(names(rank('kan bo', items, fields))).toEqual(['KAN BO', 'kan bo more', 'zkan boz', 'bob kanga', 'kxaxnxbxo'])
  })

  it('ties keep input order', () => {
    const items: Item[] = [{ name: 'Note 2' }, { name: 'Note 1' }, { name: 'Note 3' }]
    expect(names(rank('note', items, fields))).toEqual(['Note 2', 'Note 1', 'Note 3'])
  })

  it('returns nothing for non-matches', () => {
    expect(rank('zzz', [{ name: 'Notepad' }], fields)).toEqual([])
  })

  it('subsequence tier: spaces in the query are ignored', () => {
    expect(names(rank('k bd', [{ name: 'Kanban board' }], fields))).toEqual(['Kanban board'])
  })

  it('agrees with the shared vectors (also run by the Rust matcher)', () => {
    const file = fileURLToPath(new URL('../fixtures/match-vectors.json', import.meta.url))
    const vectors = JSON.parse(readFileSync(file, 'utf8')) as { query: string; items: string[][]; expected: number[] }[]
    expect(vectors.length).toBeGreaterThanOrEqual(12)
    for (const v of vectors) {
      const indexed = v.items.map((f, i) => ({ f, i }))
      expect(rank(v.query, indexed, (x) => x.f).map((x) => x.i), JSON.stringify(v.query)).toEqual(v.expected)
    }
  })
})
