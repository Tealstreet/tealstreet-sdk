import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

for (const command of ['build', 'run']) {
  test(
    `the native session harness invokes cargo ${command} inside its pinned workspace`,
    { skip: process.platform === 'win32' },
    (t) => {
      const directory = mkdtempSync(join(tmpdir(), 'native-fixture-cwd-'));
      t.after(() => rmSync(directory, { recursive: true, force: true }));
      const record = join(directory, 'cargo-call.json');
      // Intercept actual Cargo calls without Rust compilation or sockets in these probes.
      writeFileSync(
        join(directory, 'cargo'),
        `#!${process.execPath}
if (process.argv[2] !== ${JSON.stringify(command)}) process.exit(0);
require('node:fs').writeFileSync(${JSON.stringify(record)}, JSON.stringify({ cwd: process.cwd(), args: process.argv.slice(2) }));
console.log('cargo-probe-refused');
process.exit(86);
`,
        { mode: 0o755 },
      );
      const root = fileURLToPath(new URL('../../', import.meta.url));
      const workspace = fileURLToPath(new URL('../', import.meta.url));
      const entry = fileURLToPath(new URL('./server-adapter.integration.mts', import.meta.url));
      const environment = { ...process.env, PATH: `${directory}${delimiter}${process.env.PATH ?? ''}` };
      delete environment.NODE_TEST_CONTEXT;
      const probe = spawnSync(process.execPath, ['--import', 'tsx', '--test', entry], {
        cwd: root,
        env: environment,
        encoding: 'utf8',
        timeout: 20_000,
      });
      assert.equal(probe.error, undefined);
      assert.equal(probe.status, 1, 'the deliberate Cargo refusal must fail the integration harness');
      const invocation = JSON.parse(readFileSync(record, 'utf8'));
      assert.equal(realpathSync(invocation.cwd), realpathSync(workspace));
      assert.deepEqual(invocation.args, [
        command,
        '--quiet',
        '--locked',
        '-p',
        'safe-cex-native',
        '--example',
        'session-adapter-fixture',
      ]);
    },
  );
}
