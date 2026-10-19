import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { crate } from './tooling.mjs';

try {
  execFileSync('bun', ['--version'], { stdio: 'pipe' });
} catch {
  throw new Error('Bun is missing or unusable on PATH. Install Bun from https://bun.sh/docs/installation, then rerun yarn ffi:smoke:node.');
}
execFileSync(process.execPath, [join(crate, 'scripts/build.mjs'), 'node'], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-methods.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-deactivation.test.mjs')], { stdio: 'inherit' });
execFileSync('bun', ['run', join(crate, 'smoke/bun/run.mjs')], { stdio: 'inherit' });
execFileSync('bun', ['run', join(crate, 'smoke/bun/native-exchange.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-placement.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-streams.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-factory-errors.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-bitunix-market-limit.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-bitrue-gzip.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-binance-request-stats.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-binance-nested-socket.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-factory-routing.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-callback-lifetime.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-errors.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-mexc-secondary.test.mjs')], { stdio: 'inherit' });

execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-hyperliquid-timeout.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-store.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-values.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-metadata.test.mjs')], { stdio: 'inherit' });

execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-lifecycle.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-replay.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-delivery-order.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, '../scripts/live-diff/replay-clock.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-signer-availability.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-startup.test.mjs')], { stdio: 'inherit' });

execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-signer-rejections.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-events.test.mjs')], { stdio: 'inherit' });
execFileSync(process.execPath, ['--test', join(crate, 'tests/native-account-reconnect.test.mjs')], { stdio: 'inherit' });

execFileSync(process.execPath, ['--test', join(crate, 'tests/native-public-markets.test.mjs')], { stdio: 'inherit' });
