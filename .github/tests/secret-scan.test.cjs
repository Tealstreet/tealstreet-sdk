const assert = require('node:assert/strict');
const { execFileSync, spawnSync } = require('node:child_process');
const { mkdtempSync, readFileSync, writeFileSync, mkdirSync, existsSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join } = require('node:path');
const test = require('node:test');

test('the workflow scans the intended history and propagates failures', async (t) => {
  const source = readFileSync(join(__dirname, '../workflows/ci.yml'), 'utf8');
  const step = source.split('      - name: Scan new commits\n')[1];
  assert.ok(step, 'missing scan step');
  const run = step.split('        run: |\n')[1];
  assert.ok(run, 'missing scan script');
  const script = run.split('\n').map((line) => {
    assert.ok(!line.trim() || line.startsWith('          '), 'scan step must be the final workflow block');
    return line.slice(10);
  }).join('\n');
  const directory = mkdtempSync(join(tmpdir(), 'secret-scan-test-'));
  try {
    const realGit = execFileSync('/bin/sh', ['-c', 'command -v git'], { encoding: 'utf8' }).trim();
    const git = (...args) => execFileSync(realGit, args, { cwd: directory, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
    const commit = (message) => { git('commit', '--allow-empty', '-m', message); return git('rev-parse', 'HEAD'); };
    git('init', '-b', 'master');
    git('config', 'user.name', 'Secret Scan Test');
    git('config', 'user.email', 'scan-test@example.invalid');
    git('config', 'commit.gpgsign', 'false');
    git('config', 'core.hooksPath', '/dev/null');
    const master = commit('master base');
    git('switch', '-c', 'landing');
    const landing1 = commit('landing one');
    const landing2 = commit('landing two');
    git('switch', '-c', 'old-feature', landing1);
    const old = commit('feature from older landing');
    git('switch', '-c', 'current-feature', landing2);
    const current = commit('feature from current landing');
    git('switch', 'landing');
    const landing3 = commit('landing advancement');
    git('switch', 'master');
    const master2 = commit('master advancement');
    git('switch', '-c', 'master-feature', master);
    const other = commit('master feature');
    git('switch', 'old-feature');
    git('merge', '--no-ff', master2, '-m', 'merge newer master');
    const mixed = git('rev-parse', 'HEAD');
    git('update-ref', 'refs/remotes/origin/master', master2);
    git('update-ref', 'refs/remotes/origin/feat/rust-rewrite', landing2);

    const bin = join(directory, 'bin');
    mkdirSync(bin);
    writeFileSync(join(bin, 'git'), '#!/bin/sh\nif [ "$1" = "$CODEX_FAIL_GIT" ]; then exit 128; fi\nexec "$CODEX_REAL_GIT" "$@"\n', { mode: 0o755 });
    writeFileSync(join(directory, 'gitleaks'), '#!/bin/sh\nprintf "%s\\n" "$@" > "$CODEX_SCAN_ARGS"\nexit "$CODEX_SCAN_EXIT"\n', { mode: 0o755 });
    const scriptPath = join(directory, 'scan.sh');
    writeFileSync(scriptPath, script);
    let index = 0;
    const check = async (name, head, range, options = {}) => t.test(name, () => {
      const argsPath = join(directory, `args-${index++}`);
      const result = spawnSync('bash', ['-e', '-o', 'pipefail', scriptPath], {
        cwd: directory, encoding: 'utf8',
        env: {
          ...process.env, PATH: `${bin}:${process.env.PATH}`,
          EVENT: 'push', PUSH_BEFORE: '0'.repeat(40), PUSH_AFTER: head, PR_BASE: '', PR_HEAD: '',
          CODEX_REAL_GIT: realGit, CODEX_FAIL_GIT: '', CODEX_SCAN_ARGS: argsPath, CODEX_SCAN_EXIT: '0',
          ...options.env,
        },
      });
      assert.equal(result.status, options.status ?? 0, result.stderr);
      if (options.status === 128) {
        assert.equal(existsSync(argsPath), false, 'fatal Git failure must stop before scanning');
      } else {
        assert.deepEqual(readFileSync(argsPath, 'utf8').trimEnd().split('\n'),
          ['git', '--no-banner', '--redact', '--exit-code', '1', `--log-opts=${range}`, '.']);
      }
    });
    await check('new branch on current landing', current, `${landing2}..${current}`);
    git('update-ref', 'refs/remotes/origin/feat/rust-rewrite', landing3);
    await check('new branch on older landing', old, `${landing1}..${old}`);
    await check('landing moved after branch creation', current, `${landing2}..${current}`);
    await check('master branch keeps master fallback', other, `origin/master..${other}`);
    await check('landing branch merged newer master', mixed, `${landing1}..${mixed}`);
    await check('ordinary push keeps before and after', mixed, `${old}..${mixed}`, { env: { PUSH_BEFORE: old } });
    await check('PR keeps explicit base and head', old, `${landing1}..${old}`, { env: { EVENT: 'pull_request', PR_BASE: landing1, PR_HEAD: old } });
    await check('manual run keeps existing fallback', current, `origin/master..${current}`, { env: { EVENT: 'workflow_dispatch' } });
    await check('branch at trusted landing has no unique commits', landing3, `${landing3}..${landing3}`);
    git('update-ref', 'refs/remotes/origin/feat/rust-rewrite', current);
    await check('queued push already incorporated by trusted landing', current, `${current}..${current}`);
    git('switch', '--orphan', 'unrelated-history');
    const orphan = commit('independent history');
    await check('unrelated history keeps master fallback', orphan, `origin/master..${orphan}`);
    git('update-ref', '-d', 'refs/remotes/origin/feat/rust-rewrite');
    await check('missing landing keeps master fallback', current, `origin/master..${current}`);
    git('update-ref', 'refs/remotes/origin/feat/rust-rewrite', landing3);
    for (const operation of ['show-ref', 'merge-base', 'rev-list']) {
      await check(`fatal ${operation} stops scanning`, current, '', { env: { CODEX_FAIL_GIT: operation }, status: 128 });
    }
    await check('invalid head stops before scanning', 'not-a-commit', '', { status: 128 });
    await check('scanner finding fails the job', current, `${landing2}..${current}`, { env: { CODEX_SCAN_EXIT: '1' }, status: 1 });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
