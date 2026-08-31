// Builds the `consortium` CLI and stages it as a Tauri resource.
//
// The GUI alone is useless to a new user: the agents talk by shelling out to the
// CLI, so the download has to carry it. Runs from `beforeBuildCommand`, so a
// plain `npm run build` always produces a bundle with both binaries in it.

import { execFileSync } from 'node:child_process';
import { mkdirSync, copyFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const manifest = join(root, 'src-tauri', 'Cargo.toml');
const exe = process.platform === 'win32' ? 'consortium.exe' : 'consortium';

execFileSync('cargo', ['build', '--release', '--manifest-path', manifest, '--bin', 'consortium'],
             { stdio: 'inherit' });

const built = join(root, 'src-tauri', 'target', 'release', exe);
if (!existsSync(built)) throw new Error(`CLI not found after build: ${built}`);

const dest = join(root, 'src-tauri', 'resources');
mkdirSync(dest, { recursive: true });
copyFileSync(built, join(dest, exe));
console.log(`staged ${exe} for bundling`);
