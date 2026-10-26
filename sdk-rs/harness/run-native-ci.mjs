import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
const { NATIVE_FULL, NATIVE_CRATES, NATIVE_CHECK, NATIVE_COMMAND } = process.env;
if (!['true', 'false'].includes(NATIVE_FULL)) throw new Error('Missing native coverage selection');
const crates = JSON.parse(NATIVE_CRATES);
const known = readdirSync(new URL('../venues/', import.meta.url));
if (!Array.isArray(crates) || crates.some(crate => !known.includes(crate))) throw new Error('Unknown native crate selection');
function run(command, args, options = {}) {
  const child = spawnSync(command, args, { stdio: 'inherit', ...options });
  if (child.error || child.signal || child.status !== 0) throw new Error(`Native check failed: ${command} (${child.status ?? child.signal ?? child.error})`);
}
if (NATIVE_FULL === 'true' || NATIVE_CHECK === 'fmt') {
  if (!NATIVE_COMMAND) throw new Error('Missing native command');
  run('bash', ['-e', '-o', 'pipefail', '-c', NATIVE_COMMAND]);
} else if (['clippy', 'test'].includes(NATIVE_CHECK)) {
  for (const crate of crates) {
    const args = [NATIVE_CHECK, '--locked', '-p', `safe-cex-venue-${crate}`, '--all-targets'];
    run('cargo', [...args, ...(NATIVE_CHECK === 'clippy' ? ['--', '-D', 'warnings'] : [])]);
    run('cargo', [NATIVE_CHECK, '--locked', '-p', 'safe-cex-venues', '--no-default-features',
      '--features', crate, '--lib', ...(NATIVE_CHECK === 'clippy' ? ['--', '-D', 'warnings'] : [])]);
  }
  // Registry integration tests construct every family; the native host already
  // enables those default features, so check both consumers together once.
  if (crates.length) run('cargo', [NATIVE_CHECK, '--locked', '-p', 'safe-cex-venues', '-p', 'safe-cex-native', '--all-targets',
    ...(NATIVE_CHECK === 'clippy' ? ['--', '-D', 'warnings'] : [])]);
} else console.log(`${NATIVE_CHECK}: no shared native change; covered by scoped crate tests`);
