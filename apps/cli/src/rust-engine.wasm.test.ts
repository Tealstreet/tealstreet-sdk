import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { expect, it } from 'vitest';

it('runs the embedded Rust engine from a standalone CLI executable outside the repository', () => {
  const directory = mkdtempSync(join(tmpdir(), 'cli-rust-loader-'));
  const executable = join(directory, 'rust-probe');
  try {
    execFileSync('bun', ['build', 'scripts/rust-engine-probe.ts', '--compile', '--outfile', executable], {
      cwd: new URL('..', import.meta.url),
      timeout: 90000,
      stdio: 'pipe',
    });
    const output = execFileSync(executable, [], {
      cwd: directory,
      timeout: 30000,
      encoding: 'utf8',
      env: { ...process.env, NODE_PATH: '' },
    });
    expect(output).toContain('rust-smoke: native Binance Spot market loaded');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}, 120000);
