// Starts the built Bridge (release exe). Strips ELECTRON_RUN_AS_NODE that VS Code's terminal exports.
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { resolve } from 'node:path'

const exe = resolve(import.meta.dirname, '..', 'src-tauri', 'target', 'release', 'bridge.exe')
if (!existsSync(exe)) { console.error('Build first: npm run tauri:build'); process.exit(1) }
const env = { ...process.env }
delete env.ELECTRON_RUN_AS_NODE
spawn(exe, process.argv.slice(2), { detached: true, stdio: 'ignore', env }).unref()
