// Blocks publishing if any tracked file holds personal/workplace data: real emails, real user paths, Domo IDs,
// plus your own private terms (one regex per line, # for comments) from %APPDATA%/Bridge/public-check-terms.txt.
// That file stays out of the repo, so this check never leaks the very terms it guards.
// Usage: npm run check:public  (exit 1 + list of hits). Scans `git ls-files` only.
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

const SKIP = /\.(png|ico|jpg|jpeg|gif|woff2?|ttf|onnx|exe|dll)$/i
// Placeholder user names the tests use on purpose ("pat o" = the Pat O'Brien quoting test).
const ALLOWED_USERS = new Set(['me', 'someone', 'demo', 'x', 'pat o', 'jane.doe'])
const termsFile = join(process.env.APPDATA ?? '', 'Bridge', 'public-check-terms.txt')
const privateTerms = existsSync(termsFile)
  ? readFileSync(termsFile, 'utf8').split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith('#'))
  : []
const RULES = [
  ...privateTerms.map((t) => ['private term', new RegExp(t, 'i')]),
  ['Domo ID', /domo\.com\/(page|card|datasources?|app-studio)\/\d{5,}/i],
  // Any email except reserved example domains and GitHub no-reply addresses.
  ['email', /[A-Za-z0-9._%+-]+@(?![A-Za-z0-9.-]*example(\.(com|org|net))?\b)(?!users\.noreply\.github\.com\b)[A-Za-z0-9-]+\.[A-Za-z.]{2,}/],
]
const userPath = /[A-Za-z]:[\\/]{1,2}Users[\\/]{1,2}([^\\/"'`\r\n]+)/gi

const files = execFileSync('git', ['ls-files'], { encoding: 'utf8' }).split('\n').filter((f) => f && !SKIP.test(f))
const hits = []
for (const f of files) {
  let text
  try { text = readFileSync(f, 'utf8') } catch { continue }
  text.split(/\r?\n/).forEach((line, i) => {
    for (const [why, re] of RULES) if (re.test(line)) hits.push(`${f}:${i + 1}  ${why}`)
    for (const m of line.matchAll(userPath)) {
      if (!ALLOWED_USERS.has(m[1].toLowerCase())) hits.push(`${f}:${i + 1}  user path (${m[1]})`)
    }
  })
}
if (!privateTerms.length) console.warn(`check:public: no private terms file at ${termsFile} (generic rules only)`)
if (hits.length) {
  console.error(`check:public FAILED: ${hits.length} hit(s)\n` + hits.join('\n'))
  process.exit(1)
}
console.log(`check:public OK: ${files.length} files clean (${privateTerms.length} private terms)`)
