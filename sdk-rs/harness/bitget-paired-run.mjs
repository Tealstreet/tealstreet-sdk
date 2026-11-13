import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

// Tip alone took 20–22 minutes in runs 43273706928 / 43281701341.
// Allow both tip and anchor, plus their independent builds, while leaving ten
// minutes of the job budget for setup, process teardown and evidence upload.
export function runBitgetPaired({ out, timeoutMs = 80 * 60_000, killGraceMs = 5000,
  command = process.execPath, args = [fileURLToPath(new URL('./bitget-paired-ci.mjs', import.meta.url)), '--out', out] }) {
  return new Promise((finish, reject) => {
    const child = spawn(command, args, { stdio: 'inherit', detached: true });
    let timedOut = false, closed = false, killed = false, status, interrupted, killTimer;
    const killGroup = signal => {
      try { process.kill(-child.pid, signal); }
      catch (error) { if (error.code !== 'ESRCH') throw error; }
    };
    const complete = () => {
      if (!closed || (timedOut && !killed)) return;
      clearTimeout(deadline);
      process.removeListener('SIGTERM', terminate);
      process.removeListener('SIGINT', interrupt);
      if (timedOut) {
        mkdirSync(out, { recursive: true });
        writeFileSync(resolve(out, 'steady-assessment.json'), JSON.stringify({
          valid: false, certified: false, certificate: null, disposition: 'TIMED-OUT',
          errors: [`Bitget paired collection exceeded ${timeoutMs} ms; no complete certificate; no retry`],
        }, null, 2) + '\n');
      }
      finish(timedOut ? 124 : interrupted ?? status);
    };
    const stop = signal => {
      interrupted = signal === 'SIGTERM' ? 143 : 130;
      console.error(`::error::Bitget paired collector interrupted by ${signal}; NOT CERTIFIED`);
      killGroup('SIGKILL');
    };
    const terminate = () => stop('SIGTERM'), interrupt = () => stop('SIGINT');
    process.once('SIGTERM', terminate);
    process.once('SIGINT', interrupt);
    const deadline = setTimeout(() => {
      timedOut = true;
      console.error(`::error title=Bitget paired CPU deadline exceeded::Collection exceeded ${timeoutMs} ms; NOT CERTIFIED. Stopping collector and descendants; no retry.`);
      killGroup('SIGTERM');
      // The leader may exit on TERM while a build/replay descendant ignores it.
      killTimer = setTimeout(() => { killGroup('SIGKILL'); killed = true; complete(); }, killGraceMs);
    }, timeoutMs);
    child.once('error', error => {
      clearTimeout(deadline); clearTimeout(killTimer);
      process.removeListener('SIGTERM', terminate);
      process.removeListener('SIGINT', interrupt);
      reject(error);
    });
    child.once('close', (code, signal) => {
      closed = true; status = code ?? 1;
      if (signal && !timedOut) console.error(`::error::Bitget paired collector terminated by ${signal}; NOT CERTIFIED`);
      complete();
    });
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { out: { type: 'string' } }, strict: true });
  if (!values.out) throw new Error('Bitget paired collection requires --out');
  process.exitCode = await runBitgetPaired({ out: resolve(values.out) });
}
