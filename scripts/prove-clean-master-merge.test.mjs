import assert from 'node:assert/strict';
import { chmodSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { execute, main, proveMerge, prOnlyDiff } from './prove-clean-master-merge.mjs';

test('canonical PR diff preserves non-UTF8 bytes, context, modes and binary blob IDs', () => {
    const patch = (ids, starts, mode = '100644') =>
        Buffer.concat([
            Buffer.from(
                `diff --git a/x b/x\nindex ${ids} ${mode}\n--- a/x\n+++ b/x\n@@ -${starts[0]},2 +${starts[1]},2 @@ anchor\n context\n-old\n+`,
            ),
            Buffer.from([0xff, 0xfe, 10]),
        ]);
    const a = patch('aaaa..bbbb', [10, 10]);
    const b = patch('cccc..dddd', [20, 30]);
    assert.deepEqual(prOnlyDiff(a), prOnlyDiff(b));
    assert.notDeepEqual(prOnlyDiff(a), prOnlyDiff(patch('cccc..dddd', [20, 30], '100755')));
    assert.notDeepEqual(
        prOnlyDiff(a),
        prOnlyDiff(Buffer.from(b.toString('latin1').replace(' context', ' changed'), 'latin1')),
    );
    const differentBytes = Buffer.from(b);
    differentBytes[differentBytes.length - 2] = 0xfd;
    assert.notDeepEqual(prOnlyDiff(a), prOnlyDiff(differentBytes));
    const binary = (ids) => Buffer.from(`diff --git a/x b/x\nindex ${ids} 100644\nGIT binary patch\nliteral 3\nabc\n`);
    assert.notDeepEqual(prOnlyDiff(binary('aaaa..bbbb')), prOnlyDiff(binary('cccc..dddd')));
});

function fixture(
    t,
    {
        conflict = false,
        overlap = false,
        context = false,
        binary = false,
        adopted = false,
        rename = false,
        mode = false,
    } = {},
) {
    const cwd = mkdtempSync(join(tmpdir(), 'merge-proof-test-'));
    t.after(() => rmSync(cwd, { recursive: true, force: true }));
    const git = (...args) => {
        const result = execute('git', args, { cwd });
        assert.equal(result.status, 0, result.stderr?.toString());
        return result.stdout.toString().trim();
    };
    const put = (name, text) => writeFileSync(join(cwd, name), text);
    const commit = (message) => {
        git('add', '-A');
        git('commit', '-m', message);
        return git('rev-parse', 'HEAD');
    };
    git('init', '-b', 'master', '--object-format=sha1');
    git('config', 'user.name', 'Merge proof test');
    git('config', 'user.email', 'merge-proof@example.invalid');
    git('config', 'commit.gpgsign', 'false');
    mkdirSync(join(cwd, 'empty-hooks'));
    git('config', 'core.hooksPath', join(cwd, 'empty-hooks'));
    const middle = Array.from({ length: 20 }, (_, i) => `line-${i}\n`).join('');
    const original = overlap || context ? `start\n${middle}end\n` : binary ? Buffer.from([0, 1, 2]) : 'original\n';
    const feature = overlap || context ? `PR\n${middle}end\n` : binary ? Buffer.from([0, 3, 4]) : 'PR\n';
    put('feature.txt', original);
    const base = commit('base');
    git('switch', '-c', 'feature');
    if (rename) git('mv', 'feature.txt', 'renamed feature.txt');
    else if (mode) {
        chmodSync(join(cwd, 'feature.txt'), 0o755);
        git('update-index', '--chmod=+x', 'feature.txt');
    } else put('feature.txt', feature);
    if (mode) git('commit', '-m', 'feature mode');
    else commit('feature');
    const cleared = git('rev-parse', 'HEAD');
    git('switch', 'master');
    if (mode && adopted) {
        chmodSync(join(cwd, 'feature.txt'), 0o755);
        git('update-index', '--chmod=+x', 'feature.txt');
        git('commit', '-m', 'master mode');
    } else {
        if (conflict) put('feature.txt', 'conflicting master\n');
        else if (overlap) put('feature.txt', `start\n${middle}MASTER\n`);
        else if (context) put('feature.txt', `start\n${middle.replace('line-2', 'MASTER')}end\n`);
        else if (adopted) put('feature.txt', feature);
        else put('master-only.txt', 'master\n');
        commit('master');
    }
    const master = git('rev-parse', 'HEAD');
    git('update-ref', 'refs/remotes/origin/master', master);
    git('switch', 'feature');
    const merge = execute('git', ['merge', '--no-ff', master, '-m', 'merge master'], { cwd });
    if (conflict) {
        assert.equal(merge.status, 1);
        put('feature.txt', 'resolved\n');
        commit('manual resolution');
    } else assert.equal(merge.status, 0, merge.stderr?.toString());
    const head = git('rev-parse', 'HEAD');
    return { cwd, git, put, commit, base, cleared, master, head };
}
const proof = (f, overrides = {}) => proveMerge({ cwd: f.cwd, cleared: f.cleared, head: f.head, ...overrides });

for (const [name, options] of [
    ['text', {}],
    ['binary', { binary: true }],
    ['rename', { rename: true }],
    ['mode', { mode: true }],
    ['unrelated master edits in the same text file', { overlap: true }],
]) {
    test(`proves a clean master-only merge with unchanged ${name} patch`, (t) => {
        const f = fixture(t, options);
        const result = proof(f);
        assert.equal(result.tree, f.git('rev-parse', `${f.head}^{tree}`));
        assert.deepEqual(result.parents, [f.cleared, f.master]);
        assert.equal(result.oldBase, f.base);
        assert.equal(result.newBase, f.master);
        assert.match(result.diffSha256, /^[a-f0-9]{64}$/);
        assert.ok(result.diffBytes > 0);
    });
}

test('does not read or alter dirty working files, HEAD or index; disables external diff', (t) => {
    const f = fixture(t);
    f.git('config', 'diff.external', 'must-not-be-executed');
    f.put('feature.txt', 'uncommitted work\n');
    f.put('untracked.txt', 'keep me\n');
    const before = f.git('status', '--porcelain');
    proof(f);
    assert.equal(f.git('status', '--porcelain'), before);
    assert.equal(f.git('rev-parse', 'HEAD'), f.head);
    assert.equal(readFileSync(join(f.cwd, 'feature.txt'), 'utf8'), 'uncommitted work\n');
});

test('rejects an ordinary commit or a different first parent', (t) => {
    const f = fixture(t);
    assert.throws(() => proof(f, { head: f.cleared }), /exactly two parents/);
    assert.throws(() => proof(f, { cleared: f.base }), /first parent/);
    f.put('later.txt', 'additional fix');
    const later = f.commit('extra fix');
    assert.throws(() => proof(f, { head: later }), /exactly two parents/);
});

test('rejects reversed parents, octopus merges and a side-branch second parent', (t) => {
    const f = fixture(t);
    const tree = f.git('rev-parse', `${f.head}^{tree}`);
    const reversed = f.git('commit-tree', tree, '-p', f.master, '-p', f.cleared, '-m', 'reversed');
    assert.throws(() => proof(f, { head: reversed }), /first parent/);
    const third = f.git('commit-tree', tree, '-p', f.base, '-m', 'third');
    const octopus = f.git('commit-tree', tree, '-p', f.cleared, '-p', f.master, '-p', third, '-m', 'octopus');
    assert.throws(() => proof(f, { head: octopus }), /exactly two parents/);
    f.git('update-ref', 'refs/remotes/origin/master', f.base);
    assert.throws(() => proof(f), /second parent/);
});

test('rejects a hand-edited merge even when both parents are correct', (t) => {
    const f = fixture(t);
    f.put('smuggled.txt', 'extra behavior');
    f.commit('prepare a different tree');
    const forged = f.git(
        'commit-tree',
        f.git('rev-parse', 'HEAD^{tree}'),
        '-p',
        f.cleared,
        '-p',
        f.master,
        '-m',
        'edited merge',
    );
    assert.throws(() => proof(f, { head: forged }), /merge-tree tree .* differs/);
});

test('rejects conflict resolutions, checking merge-tree exit status and stdout', (t) => {
    const f = fixture(t, { conflict: true });
    assert.throws(() => proof(f), /merge-tree is not clean \(exit 1\).*CONFLICT/s);
});

for (const [name, options] of [
    ['changed text context', { context: true }],
    ['absorbed text change', { adopted: true }],
    ['absorbed binary change', { binary: true, adopted: true }],
    ['absorbed mode change', { mode: true, adopted: true }],
]) {
    test(`fails closed when the complete PR-only patch differs: ${name}`, (t) => {
        const f = fixture(t, options);
        assert.throws(() => proof(f), /PR-only binary diff differs/);
    });
}

test('rejects ambiguous criss-cross merge bases', (t) => {
    const f = fixture(t);
    const tree = f.git('rev-parse', `${f.base}^{tree}`);
    const a = f.git('commit-tree', tree, '-p', f.base, '-m', 'a');
    const b = f.git('commit-tree', tree, '-p', f.base, '-m', 'b');
    const aa = f.git('commit-tree', tree, '-p', a, '-p', b, '-m', 'aa');
    const bb = f.git('commit-tree', tree, '-p', b, '-p', a, '-m', 'bb');
    const head = f.git('commit-tree', tree, '-p', aa, '-p', bb, '-m', 'head');
    f.git('update-ref', 'refs/remotes/origin/master', bb);
    assert.throws(() => proof(f, { cleared: aa, head }), /ambiguous or missing merge base/);
});

test('rejects shallow history and invalid SHAs', (t) => {
    const f = fixture(t);
    assert.throws(() => proof(f, { cleared: '--bad' }), /full lowercase/);
    assert.throws(() => proof(f, { cleared: '0'.repeat(40) }), /cat-file failed/);
    const shallow = join(f.cwd, '.git', 'shallow');
    writeFileSync(shallow, `${f.base}\n`);
    assert.throws(() => proof(f), /shallow history/);
});

function cli(t, options = {}) {
    const f = fixture(t);
    const messages = [];
    const posted = [];
    let reads = 0;
    let fetches = 0;
    const run = (program, args, opts) => {
        const success = (s) => ({ status: 0, stdout: Buffer.from(s), stderr: Buffer.alloc(0) });
        if (program === 'git' && args[0] === 'remote')
            return success(options.origin ?? 'https://github.com/Tealstreet/tealstreet-sdk.git');
        if (program === 'git' && args[0] === 'fetch') {
            fetches++;
            if (options.fetchDenied)
                return { status: 128, stdout: Buffer.alloc(0), stderr: Buffer.from('fetch unavailable') };
            return success('');
        }
        if (program === 'gh') {
            if (options.denied)
                return { status: 1, stdout: Buffer.alloc(0), stderr: Buffer.from('HTTP 403: unavailable') };
            if (args[0] === 'pr') {
                assert.equal(args[args.indexOf('--repo') + 1], `github.com/${options.repo ?? 'Tealstreet/tealstreet-sdk'}`);
                reads++;
                return success(
                    JSON.stringify({
                        number: 42,
                        state: options.state ?? 'OPEN',
                        baseRefName: options.base ?? 'master',
                        headRefOid: options.moved && reads > 1 ? f.cleared : f.head,
                        comments: posted.map((body) => ({ body })),
                    }),
                );
            }
            assert.deepEqual(args, [
                'api',
                '--hostname',
                'github.com',
                '--method',
                'POST',
                `repos/${options.repo ?? 'Tealstreet/tealstreet-sdk'}/issues/42/comments`,
                '--input',
                '-',
            ]);
            if (options.postDenied)
                return { status: 1, stdout: Buffer.alloc(0), stderr: Buffer.from('HTTP 403: posting denied') };
            posted.push(JSON.parse(opts.input).body);
            return success('{}');
        }
        assert.equal(program, 'git');
        return execute(program, args, opts);
    };
    return {
        ...f,
        messages,
        posted,
        fetches: () => fetches,
        invoke: (args = ['42', f.cleared]) => main(args, { cwd: f.cwd, run, log: (s) => messages.push(s) }),
    };
}

test('CLI fetches via git, prints exact proof, and posts an LF comment once on request', (t) => {
    const c = cli(t);
    assert.equal(c.invoke(), 0);
    assert.equal(c.posted.length, 0);
    assert.match(c.messages[0], /^PASS [a-f0-9]{40}\nparents:/);
    assert.equal(c.invoke(['42', c.cleared, '--post']), 0);
    assert.equal(c.posted.length, 1);
    assert.equal(c.posted[0].split('\n')[0], `MERGE-PROOF PASS ${c.head} (CLEARs at ${c.cleared} carry)`);
    assert.ok(!c.posted[0].includes('\r'));
    assert.ok(c.posted[0].includes(`merge-tree: ${c.git('rev-parse', 'HEAD^{tree}')} == ${c.head}^{tree}`));
    assert.equal(c.invoke(['42', c.cleared, '--post']), 0);
    assert.equal(c.posted.length, 1);
    assert.equal(c.fetches(), 3);
});

for (const [name, options, reason] of [
    ['head moves', { moved: true }, /head moved/],
    ['fetch fails', { fetchDenied: true }, /fetch unavailable/],
    ['posting fails', { postDenied: true }, /posting denied/],
    ['GitHub 403', { denied: true }, /HTTP 403/],
    ['closed PR', { state: 'MERGED' }, /PR must be open/],
    ['different base', { base: 'prod' }, /PR must be open/],
    ['wrong origin', { origin: 'https://example.invalid/other/repo.git' }, /origin must be/],
]) {
    test(`CLI fails without posting when ${name}`, (t) => {
        const c = cli(t, options);
        assert.equal(c.invoke(['42', c.cleared, '--post']), 1);
        assert.match(c.messages[0], /^FAIL:/);
        assert.match(c.messages[0], reason);
        assert.equal(c.posted.length, 0);
        assert.equal(
            c.messages.some((message) => message.startsWith('PASS')),
            false,
        );
    });
}

test('CLI rejects malformed arguments without invoking git or gh', () => {
    const run = () => assert.fail('unexpected command');
    for (const args of [[], ['--bad', 'a'.repeat(40)], ['42', 'short'], ['42', 'a'.repeat(40), '--bogus']]) {
        assert.equal(main(args, { run, log: () => {} }), 1);
    }
});

for (const origin of ['git@github.com:Tealstreet/tealstreet-sdk.git', 'ssh://git@github.com/Tealstreet/tealstreet-sdk']) {
    test(`CLI proves and posts for the SDK SSH origin: ${origin}`, (t) => {
        const c = cli(t, { origin });
        assert.equal(c.invoke(['42', c.cleared, '--post']), 0);
        assert.equal(c.posted.length, 1);
        assert.ok(c.posted[0].includes('repository: github.com/Tealstreet/tealstreet-sdk'));
    });
}

test('CLI routes explicit repository reads and posting to the same verified origin', (t) => {
    const c = cli(t, { repo: 'Example/another-repo', origin: 'https://github.com/Example/another-repo.git' });
    assert.equal(c.invoke(['42', c.cleared, '--repo', 'Example/another-repo', '--post']), 0);
    assert.equal(c.posted.length, 1);
    assert.ok(c.posted[0].includes('repository: github.com/Example/another-repo'));
    assert.equal(c.invoke(['42', c.cleared, '--post', '--repo', 'Example/another-repo']), 0);
    assert.equal(c.posted.length, 1);
});

test('CLI rejects a repository override that does not match origin before any GitHub request', (t) => {
    const c = cli(t);
    assert.equal(c.invoke(['42', c.cleared, '--repo', 'Example/another-repo', '--post']), 1);
    assert.match(c.messages[0], /origin must be/);
    assert.equal(c.fetches(), 0);
    assert.equal(c.posted.length, 0);
});

test('CLI rejects missing, repeated or malformed repository options before external commands', () => {
    const run = () => assert.fail('unexpected command');
    const sha = 'a'.repeat(40);
    for (const flags of [
        ['--repo'], ['--repo', '--post'], ['--repo', '../repo'], ['--repo', 'a/b/c'],
        ['--repo', 'https://github.com/a/b'], ['--repo', 'a/b?query'],
        ['--repo', 'a/b', '--repo', 'a/b'], ['--post', '--post'],
    ]) assert.equal(main(['42', sha, ...flags], { run, log: () => {} }), 1);
});
