/**
 * One-shot (non-REPL) torture harness.
 *
 * Nobody had really exercised `tealstreet <command>` outside the REPL, and a
 * user on 0.10.5 found it never exits. This runs the whole command surface
 * one-shot against REAL exchange accounts in an ISOLATED $HOME, with a hard
 * timeout per command, and reports anything that hangs, crashes, or leaks a
 * stack trace.
 *
 *   yarn workspace @tealstreet/cli torture
 *   TORTURE_EXCHANGES=bybit,bitget TORTURE_TIMEOUT_MS=30000 yarn workspace @tealstreet/cli torture
 *
 * Credentials come from the shared sheet via scripts/keys; they are written
 * into the isolated config and never printed.
 */
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';

import { readRows } from '../../../scripts/keys/lib/sheet';

const TIMEOUT_MS = Number(process.env.TORTURE_TIMEOUT_MS ?? 30_000);
const EXCHANGES = (process.env.TORTURE_EXCHANGES ?? 'bybit').split(',').filter(Boolean);
const ENTRY = new URL('../src/index.ts', import.meta.url).pathname;

type Outcome = {
  command: string;
  exitCode: number | null;
  ms: number;
  timedOut: boolean;
  stackTrace: boolean;
  tail: string;
};

const run = (args: string[], home: string): Promise<Outcome> =>
  new Promise((resolve) => {
    const started = Date.now();
    const child = spawn('bun', [ENTRY, ...args], {
      env: { ...process.env, HOME: home, NO_COLOR: '1' },
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let output = '';
    child.stdout.on('data', (d) => (output += d));
    child.stderr.on('data', (d) => (output += d));

    const killer = setTimeout(() => {
      child.kill('SIGKILL');
      resolve({
        command: args.join(' '),
        exitCode: null,
        ms: Date.now() - started,
        timedOut: true,
        stackTrace: /\n\s+at\s/.test(output),
        tail: output.trim().split('\n').slice(-3).join(' | '),
      });
    }, TIMEOUT_MS);

    child.on('exit', (code) => {
      clearTimeout(killer);
      resolve({
        command: args.join(' '),
        exitCode: code,
        ms: Date.now() - started,
        timedOut: false,
        stackTrace: /\n\s+at\s/.test(output),
        tail: output.trim().split('\n').slice(-3).join(' | '),
      });
    });
  });

/**
 * Copy ONLY the `auth` block from the operator's real config into the isolated
 * one. Several venues force the Tealstreet tunnel, which refuses an
 * unauthenticated client, so without this the harness can only ever exercise
 * failure paths. Accounts are never copied - the isolated config gets its own
 * from the sheet. The token is read and written, never printed.
 */
function seedAuth(home: string): boolean {
  const realConfig = join(process.env.TORTURE_REAL_HOME ?? homedir(), '.tealstreet', 'config.json');
  if (!existsSync(realConfig)) return false;
  const auth = (JSON.parse(readFileSync(realConfig, 'utf8')) as { auth?: unknown }).auth;
  if (!auth) return false;

  const target = join(home, '.tealstreet', 'config.json');
  mkdirSync(join(home, '.tealstreet'), { recursive: true });
  const existing = existsSync(target) ? JSON.parse(readFileSync(target, 'utf8')) : { accounts: [] };
  writeFileSync(target, JSON.stringify({ ...existing, auth }, null, 2), { mode: 0o600 });
  return true;
}

async function seed(home: string): Promise<string[]> {
  const rows = await readRows();
  const added: string[] = [];
  for (const want of EXCHANGES) {
    const row = rows.find(
      (r: { exchange?: string; status?: string; apiKey?: string; secret?: string; passphrase?: string }) =>
        r.exchange?.trim().toLowerCase() === want.toLowerCase() &&
        String(r.status ?? '').trim().toLowerCase() === 'active' &&
        r.apiKey &&
        r.secret,
    );
    if (!row) {
      console.log(`  no active sheet row for ${want} - skipping`);
      continue;
    }
    const name = `t${want}`;
    const args = [ENTRY, 'account', 'add', '-n', name, '-e', want.toLowerCase(), '-k', row.apiKey!, '-s', row.secret!];
    if (row.passphrase) args.push('-p', row.passphrase);
    const res = spawnSync('bun', args, { env: { ...process.env, HOME: home }, encoding: 'utf8' });
    if (res.status === 0) {
      added.push(name);
      console.log(`  seeded ${want} as ${name}`);
    } else {
      console.log(`  FAILED to seed ${want} (exit ${res.status})`);
    }
  }
  return added;
}

// Read-only first, then the ones that touch an exchange. No order placement
// here: this harness is about process lifecycle, not fills.
const COMMANDS: string[][] = [
  // No exchange connection.
  ['--version'],
  ['--help'],
  ['account', 'list'],
  ['auth'],
  ['tasks'],
  // `logs` is deliberately `tail -f` and never returns; not a hang.
  // Promoted verbs — these connect.
  ['balance'],
  // Everything else in the REPL registry reaches one-shot through `exec`.
  ['exec', 'positions'],
  ['exec', 'orders'],
  ['exec', 'spot'],
  ['exec', 'monitor'],
  ['exec', 'symbol BTC/USDT:USDT'],
  ['exec', 'alias'],
  ['exec', 'whitelist'],
  ['exec', 'simulation'],
  ['exec', 'fatfinger'],
];

async function main(): Promise<void> {
  const home = mkdtempSync(join(tmpdir(), 'tealstreet-torture-'));
  console.log(`isolated HOME: ${home}`);
  console.log(`timeout: ${TIMEOUT_MS}ms  exchanges: ${EXCHANGES.join(', ')}`);

  console.log(seedAuth(home) ? 'copied the auth token into the isolated config' : 'NO auth token found - tunnel venues will refuse');
  console.log('seeding accounts...');
  const accounts = await seed(home);
  if (accounts.length === 0) console.log('  WARNING: no accounts seeded; exchange commands will exit early');

  const results: Outcome[] = [];
  for (const command of COMMANDS) {
    const outcome = await run(command, home);
    results.push(outcome);
    const verdict = outcome.timedOut ? 'HANG' : outcome.stackTrace ? 'TRACE' : outcome.exitCode === 0 ? 'ok' : `exit ${outcome.exitCode}`;
    console.log(`${verdict.padEnd(8)} ${String(outcome.ms).padStart(6)}ms  ${outcome.command}`);
    if (outcome.timedOut || outcome.stackTrace || outcome.exitCode !== 0) console.log(`         ${outcome.tail}`);
  }

  const hangs = results.filter((r) => r.timedOut);
  const traces = results.filter((r) => r.stackTrace && !r.timedOut);
  console.log(`\n${results.length} commands, ${hangs.length} hung, ${traces.length} leaked a stack trace`);
  if (hangs.length > 0 || traces.length > 0) process.exit(1);
}

await main();
