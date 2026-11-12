// Runs cargo inside src-tauri with the per-user Rust toolchain on PATH (rustup GNU host + WinLibs MinGW for
// dlltool/as). Never edits the user's PATH. Usage: node scripts/rust.mjs <cargo args…>
import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'

const home = process.env.USERPROFILE ?? ''
const local = process.env.LOCALAPPDATA ?? join(home, 'AppData', 'Local')
const extra = [join(home, '.cargo', 'bin'), join(local, 'Programs', 'winlibs', 'mingw64', 'bin')]
for (const p of extra) if (!existsSync(p)) console.warn(`[rust] missing toolchain folder: ${p}`)
const env = { ...process.env, PATH: [...extra, process.env.PATH ?? process.env.Path ?? ''].join(';') }
delete env.Path
delete env.ELECTRON_RUN_AS_NODE
const r = spawnSync('cargo', process.argv.slice(2), { cwd: resolve(import.meta.dirname, '..', 'src-tauri'), env, stdio: 'inherit', shell: false })
if (r.error) { console.error(`[rust] ${r.error.message}`); process.exit(1) }
process.exit(r.status ?? 1)
