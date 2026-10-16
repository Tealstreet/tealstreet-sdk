import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Cargo retains named Rust/TS assertion failures and its exit 101 at this boundary.
const result = spawnSync('cargo', ['test', '--locked', '-p', 'safe-cex-venue-coinbase', '--all-targets', '--', '--include-ignored'], {
  cwd: fileURLToPath(new URL('../..', import.meta.url)), stdio: 'inherit',
});
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const declarationTests = spawnSync(process.execPath, ['--test', fileURLToPath(new URL('./store-notification-declarations.test.mjs', import.meta.url))], {
  cwd: fileURLToPath(new URL('../..', import.meta.url)), stdio: 'inherit',
});
if (declarationTests.error) throw declarationTests.error;
if (declarationTests.status !== 0) process.exit(declarationTests.status ?? 1);
const audit = spawnSync(process.execPath, [fileURLToPath(new URL('./check-store-notification.mjs', import.meta.url))], {
  cwd: fileURLToPath(new URL('../..', import.meta.url)), stdio: 'inherit',
});
if (audit.error) throw audit.error;
process.exit(audit.status ?? 1);
