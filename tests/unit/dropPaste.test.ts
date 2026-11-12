import { describe, expect, it, vi } from 'vitest'
import { extractDropInputs, type DropData } from '../../src/renderer/dropPaste'

const file = (name: string) => ({ name }) as unknown as File

/** Minimal DataTransfer: files + a format → data table. `live` is cleared after the call to mimic the browser. */
function fakeDt(files: File[], data: Record<string, string>): DropData & { clear(): void } {
  let store: Record<string, string> = { ...data }
  let list: File[] = files
  return {
    get files() { return list },
    getData: (f: string) => store[f] ?? '',
    clear() { store = {}; list = [] },
  }
}

describe('extractDropInputs', () => {
  it('files: returns the real paths from pathsForFiles', async () => {
    const a = file('a.txt'), b = file('b.txt')
    const paths = vi.fn(async (fs: File[]) => fs.map((f) => `C:/Docs/${f.name}`))
    const dt = fakeDt([a, b], { 'text/uri-list': 'https://ignored.example/', 'text/plain': 'ignored' })
    expect(await extractDropInputs(dt, paths)).toEqual(['C:/Docs/a.txt', 'C:/Docs/b.txt'])
    expect(paths).toHaveBeenCalledWith([a, b])
  })

  it('reads the DataTransfer before awaiting (it is emptied after the drop event)', async () => {
    const dt = fakeDt([file('a')], { 'text/uri-list': 'https://example.com/' })
    const p = extractDropInputs(dt, async () => [])
    dt.clear() // the drop handler has returned
    expect(await p).toEqual(['https://example.com/'])
  })

  it('empty path result falls back to uri-list, skipping # comments and blanks', async () => {
    const dt = fakeDt([file('x')], {
      'text/uri-list': '# dragged from Edge\r\nhttps://a.example/\r\n\r\n  https://b.example/  \n#c',
      'text/plain': 'https://a.example/',
    })
    expect(await extractDropInputs(dt, async () => [])).toEqual(['https://a.example/', 'https://b.example/'])
  })

  it('pathsForFiles failing also falls back', async () => {
    const dt = fakeDt([file('x')], { 'text/uri-list': 'https://a.example/' })
    expect(await extractDropInputs(dt, async () => { throw new Error('no host') })).toEqual(['https://a.example/'])
  })

  it('then trimmed text/plain', async () => {
    const dt = fakeDt([file('x')], { 'text/uri-list': '# only a comment', 'text/plain': '  C:/Tools/thing.exe \n' })
    expect(await extractDropInputs(dt, async () => [''])).toEqual(['C:/Tools/thing.exe'])
  })

  it('no files: does not call pathsForFiles', async () => {
    const paths = vi.fn(async () => ['nope'])
    const dt = fakeDt([], { 'text/plain': 'kanban' })
    expect(await extractDropInputs(dt, paths)).toEqual(['kanban'])
    expect(paths).not.toHaveBeenCalled()
  })

  it('nothing usable → []', async () => {
    expect(await extractDropInputs(fakeDt([], { 'text/plain': '   ' }), async () => [])).toEqual([])
    expect(await extractDropInputs({ files: null, getData: () => '' }, async () => [])).toEqual([])
  })
})
