import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';

const script = resolve('.github/actions/fetch-rust-history/fetch-history.py');
const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const historyJobs = ['scope', 'selection', 'harness', 'bans', 'gate', 'bitget-paired', 'cpu-plan', 'cpu-certify'];
const body = (text, name) => {
  const value = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z0-9-]*:\\n|$(?![\\s\\S]))`, 'm').exec(text);
  assert(value, name); return value[1];
};
function checkWorkflow(text) {
  assert.doesNotMatch(text, /fetch-depth: 0/);
  for (const name of historyJobs) {
    const job = body(text, name);
    assert.match(job, /uses: actions\/checkout@v7\n        with:\n          ref: \$\{\{ github.sha \}\}\n          fetch-depth: 1\n          fetch-tags: false\n      - uses: \.\/\.github\/actions\/fetch-rust-history/);
    assert.doesNotMatch(job, /continue-on-error: true/);
  }
}
const git = (cwd, ...args) => execFileSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
function fixture(t) {
  const root = mkdtempSync(resolve(tmpdir(), 'ci-history-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const origin = resolve(root, 'origin'); mkdirSync(origin);
  git(origin, 'init', '-q', '-b', 'master'); git(origin, 'config', 'user.name', 'Fixture'); git(origin, 'config', 'user.email', 'fixture@example.invalid');
  writeFileSync(resolve(origin, 'inventory.json'), '{"allowed":1}'); git(origin, 'add', '.'); git(origin, 'commit', '-qm', 'base');
  const base = git(origin, 'rev-parse', 'HEAD'); git(origin, 'tag', 'reachable-tag', base);
  git(origin, 'switch', '-qc', 'feature'); writeFileSync(resolve(origin, 'own'), 'own'); git(origin, 'add', '.'); git(origin, 'commit', '-qm', 'feature');
  const head = git(origin, 'rev-parse', 'HEAD');
  git(origin, 'switch', '-q', 'master'); writeFileSync(resolve(origin, 'incoming'), 'incoming'); git(origin, 'add', '.'); git(origin, 'commit', '-qm', 'incoming');
  const master = git(origin, 'rev-parse', 'HEAD');
  git(origin, 'switch', '-qc', 'pr-merge'); git(origin, 'merge', '--no-ff', '-qm', 'PR composition', 'feature');
  const checkout = git(origin, 'rev-parse', 'HEAD'), tree = git(origin, 'rev-parse', 'HEAD^{tree}');
  git(origin, 'switch', '-qc', 'unrelated', base); writeFileSync(resolve(origin, 'unrelated'), 'not reachable from the tested source'); git(origin, 'add', '.'); git(origin, 'commit', '-qm', 'unrelated');
  const unrelated = git(origin, 'rev-parse', 'HEAD'); git(origin, 'tag', 'unrelated-tag'); git(origin, 'switch', '-q', 'master');
  const client = (name, sha) => {
    const directory = resolve(root, name); mkdirSync(directory);
    git(directory, 'init', '-q', '-b', 'master'); git(directory, 'remote', 'add', 'origin', pathToFileURL(origin).href);
    git(directory, 'fetch', '--no-tags', '--depth=1', 'origin', sha); git(directory, 'checkout', '-q', '--detach', sha);
    return directory;
  };
  const eventPath = resolve(root, 'event.json');
  const pull = { base: { sha: base, ref: 'master' }, head: { sha: head } };
  const run = (directory, sha = checkout, eventName = 'pull_request', event = { pull_request: pull }) => {
    writeFileSync(eventPath, JSON.stringify(event));
    return spawnSync('python3', [script], { cwd: directory, encoding: 'utf8',
      env: { ...process.env, GITHUB_SHA: sha, GITHUB_EVENT_NAME: eventName, GITHUB_EVENT_PATH: eventPath } });
  };
  return { root, origin, base, head, master, checkout, tree, unrelated, client, run, pull };
}

test('all Rust ancestry jobs use exact-SHA checkout plus targeted history without blanket ref fetches', () => {
  checkWorkflow(workflow);
  for (const mutate of [text => text.replace('fetch-depth: 1', 'fetch-depth: 0'),
    text => text.replace('ref: ${{ github.sha }}', 'ref: master'),
    text => text.replace('      - uses: ./.github/actions/fetch-rust-history\n', ''),
    text => text.replace('fetch-tags: false', 'fetch-tags: true')]) assert.throws(() => checkWorkflow(mutate(workflow)));
});

test('a targeted fetch preserves the PR composition, complete parents and immutable bases without unrelated refs or tags', t => {
  const x = fixture(t), client = x.client('pr-client', x.checkout);
  assert.equal(git(client, 'show', '-s', '--format=%P', 'HEAD'), '', 'working shallow control hides parents');
  assert.throws(() => git(client, 'diff', `${x.base}...${x.head}`), 'scope lacks its immutable commits before the fetch');
  const result = x.run(client); assert.equal(result.status, 0, result.stderr);
  assert.equal(git(client, 'rev-parse', 'HEAD'), x.checkout);
  assert.equal(git(client, 'rev-parse', 'HEAD^{tree}'), x.tree);
  assert.equal(git(client, 'rev-parse', '--is-shallow-repository'), 'false');
  assert.deepEqual(git(client, 'show', '-s', '--format=%P', 'HEAD').split(' '), [x.master, x.head]);
  assert.equal(git(client, 'merge-tree', '--write-tree', x.master, x.head), x.tree);
  assert.equal(git(client, 'diff', '--name-only', `${x.base}...${x.head}`), 'own');
  for (const ref of ['HEAD', 'HEAD^1', 'HEAD^2', x.base, 'refs/remotes/origin/master'])
    assert.equal(git(client, 'show', `${ref}:inventory.json`), '{"allowed":1}');
  assert.deepEqual(git(client, 'for-each-ref', '--format=%(refname)', 'refs/remotes', 'refs/tags').split('\n'), ['refs/remotes/origin/master']);
  assert.throws(() => git(client, 'cat-file', '-e', `${x.unrelated}^{commit}`), 'unrelated branch object is not downloaded');
  assert.equal(x.run(client).status, 0, 'an already complete checkout can fetch again without --unshallow');
});

test('master push history keeps the tested commit when the landing branch advances', t => {
  const x = fixture(t), client = x.client('push-client', x.master);
  writeFileSync(resolve(x.origin, 'later'), 'later'); git(x.origin, 'add', '.'); git(x.origin, 'commit', '-qm', 'later master');
  const tip = git(x.origin, 'rev-parse', 'HEAD');
  const result = x.run(client, x.master, 'push'); assert.equal(result.status, 0, result.stderr);
  assert.equal(git(client, 'rev-parse', 'HEAD'), x.master);
  assert.equal(git(client, 'rev-parse', 'refs/remotes/origin/master'), tip);
  assert.equal(git(client, 'rev-parse', 'HEAD^'), x.base);
  assert.equal(git(client, 'tag', '--list'), '');
});

test('a different base ref is fetched explicitly and malformed or missing immutable source fails closed', t => {
  const x = fixture(t); git(x.origin, 'branch', 'release-base', x.master);
  const client = x.client('base-client', x.checkout);
  const event = { pull_request: { ...x.pull, base: { ...x.pull.base, ref: 'release-base' } } };
  assert.equal(x.run(client, x.checkout, 'pull_request', event).status, 0);
  assert.equal(git(client, 'rev-parse', 'refs/remotes/origin/release-base'), x.master);
  for (const bad of [null, 'bad', 'f'.repeat(40)]) {
    const invalid = structuredClone(event); invalid.pull_request.base.sha = bad;
    assert.notEqual(x.run(client, x.checkout, 'pull_request', invalid).status, 0);
  }
  const invalid = structuredClone(event); invalid.pull_request.base.ref = 'master:refs/heads/other';
  assert.notEqual(x.run(client, x.checkout, 'pull_request', invalid).status, 0);
  assert.notEqual(x.run(client, x.base).status, 0, 'a different checkout never qualifies');
  assert.equal(git(client, 'rev-parse', 'HEAD'), x.checkout);
});
