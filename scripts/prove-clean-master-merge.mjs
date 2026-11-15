#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

const DEFAULT_REPO = 'Tealstreet/tealstreet-sdk';
const REPOSITORY = /^[A-Za-z0-9][A-Za-z0-9-]*\/[A-Za-z0-9_][A-Za-z0-9._-]*$/;
const SHA = /^[a-f0-9]{40}$/;
const usage = 'node scripts/prove-clean-master-merge.mjs <PR-number> <CLEARed-full-SHA> [--repo OWNER/REPO] [--post]';

export const execute = (program, args, options) =>
    spawnSync(program, args, {
        ...options,
        maxBuffer: 64 * 1024 * 1024,
        env: { ...process.env, GIT_NO_REPLACE_OBJECTS: '1', GIT_TERMINAL_PROMPT: '0', LC_ALL: 'C' },
    });
const requireThat = (condition, reason) => {
    if (!condition) throw new Error(reason);
};
const checked = (run, program, args, options) => {
    const result = run(program, args, options);
    requireThat(
        !result.error && result.status === 0,
        `${program} ${args[0]} failed (exit ${result.status}): ${result.error?.message ?? result.stderr?.toString().trim()}`,
    );
    return result.stdout;
};
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');

// Text blob IDs and hunk offsets include master's unrelated edits. Preserve
// every other byte, including context, hunk counts, modes, paths and binary patches.
export function prOnlyDiff(bytes) {
    return Buffer.from(
        bytes
            .toString('latin1')
            .split(/(?=^diff --git )/m)
            .map((file) => {
                const binary = /^(GIT binary patch|Binary files )/m.test(file);
                return file
                    .split('\n')
                    .map((line) => {
                        if (!binary) line = line.replace(/^index [a-f0-9]+\.\.[a-f0-9]+/, 'index _.._');
                        return line.replace(/^(@@ -)\d+((?:,\d+)? \+)\d+((?:,\d+)? @@)/, '$1_$2_$3');
                    })
                    .join('\n');
            })
            .join(''),
        'latin1',
    );
}

/** Pure local proof: refs are pinned once; no checkout, install or worktree edits. */
export function proveMerge({ cwd, cleared, head, master = 'origin/master', run = execute }) {
    const bytes = (...args) => checked(run, 'git', args, { cwd });
    const git = (...args) =>
        bytes(...args)
            .toString()
            .trim();
    requireThat(SHA.test(cleared) && SHA.test(head), 'head and CLEARed head must be full lowercase 40-character SHAs');
    requireThat(git('rev-parse', '--is-shallow-repository') === 'false', 'shallow history cannot prove ancestry');
    const masterSha = git('rev-parse', '--verify', `${master}^{commit}`);
    git('cat-file', '-e', `${cleared}^{commit}`);
    const parents = git('show', '-s', '--format=%P', head).split(' ');
    requireThat(parents.length === 2, 'new head must have exactly two parents');
    requireThat(parents[0] === cleared, 'first parent is not the CLEARed head (additional commits or wrong baseline)');
    requireThat(
        run('git', ['merge-base', '--is-ancestor', parents[1], masterSha], { cwd }).status === 0,
        'second parent is not provably on origin/master',
    );

    const merged = run('git', ['merge-tree', '--write-tree', ...parents], { cwd });
    requireThat(
        !merged.error && merged.status === 0,
        `merge-tree is not clean (exit ${merged.status}): ${merged.stdout?.toString().trim()} ${merged.stderr?.toString().trim()}`,
    );
    const computedTree = merged.stdout.toString().trim();
    const tree = git('rev-parse', `${head}^{tree}`);
    requireThat(
        SHA.test(computedTree) && computedTree === tree,
        `merge-tree tree ${JSON.stringify(computedTree)} differs from head tree ${tree}`,
    );

    const base = (commit) => {
        const bases = git('merge-base', '--all', masterSha, commit).split('\n');
        requireThat(bases.length === 1 && SHA.test(bases[0]), 'ambiguous or missing merge base');
        return bases[0];
    };
    const oldBase = base(cleared);
    const newBase = base(head);
    requireThat(newBase === parents[1], 'new merge base is not the merged master parent');
    const diff = (from, to) =>
        bytes(
            '-c',
            'core.quotePath=true',
            'diff',
            '--binary',
            '--full-index',
            '--no-ext-diff',
            '--no-textconv',
            '--no-color',
            '--no-renames',
            '--no-relative',
            '--ignore-submodules=none',
            '--src-prefix=a/',
            '--dst-prefix=b/',
            '--diff-algorithm=myers',
            '--no-indent-heuristic',
            '--unified=3',
            '--inter-hunk-context=0',
            from,
            to,
            '--',
        );
    const before = prOnlyDiff(diff(oldBase, cleared));
    const after = prOnlyDiff(diff(newBase, head));
    requireThat(before.equals(after), `PR-only binary diff differs: before=${digest(before)} after=${digest(after)}`);
    return {
        head,
        cleared,
        master: masterSha,
        parents,
        tree,
        oldBase,
        newBase,
        diffSha256: digest(before),
        diffBytes: before.length,
    };
}

export function main(args, { cwd = process.cwd(), run = execute, log = console.log } = {}) {
    try {
        if (args.length === 1 && args[0] === '--help') {
            log(usage);
            return 0;
        }
        const [pr, cleared, ...flags] = args;
        let repo = DEFAULT_REPO;
        let repoSelected = false;
        let post = false;
        for (let index = 0; index < flags.length; index++) {
            const flag = flags[index];
            if (flag === '--post' && !post) post = true;
            else if (flag === '--repo' && !repoSelected && REPOSITORY.test(flags[index + 1] ?? '')) {
                repo = flags[++index];
                repoSelected = true;
            } else throw new Error(usage);
        }
        requireThat(
            args.length >= 2 &&
                /^[1-9][0-9]*$/.test(pr) &&
                Number.isSafeInteger(Number(pr)) &&
                SHA.test(cleared),
            usage,
        );
        const call = (program, argv, input) => checked(run, program, argv, { cwd, input });
        const origin = call('git', ['remote', 'get-url', 'origin']).toString().trim();
        const originRepo = origin.match(
            /^(?:https:\/\/github\.com\/|git@github\.com:|ssh:\/\/git@github\.com\/)([^/]+\/[^/]+?)(?:\.git)?$/i,
        )?.[1];
        requireThat(
            originRepo?.toLowerCase() === repo.toLowerCase(),
            `origin must be the GitHub ${repo} repository`,
        );
        const readPr = () => {
            const data = JSON.parse(
                call('gh', [
                    'pr',
                    'view',
                    pr,
                    '--repo',
                    `github.com/${repo}`,
                    '--json',
                    `number,headRefOid,baseRefName,state${post ? ',comments' : ''}`,
                ]).toString(),
            );
            requireThat(
                data.number === Number(pr) &&
                    data.state === 'OPEN' &&
                    data.baseRefName === 'master' &&
                    SHA.test(data.headRefOid),
                'PR must be open against master with a full head SHA',
            );
            return data;
        };
        const initial = readPr();
        call('git', [
            'fetch',
            '--no-tags',
            'origin',
            'refs/heads/master:refs/remotes/origin/master',
            `refs/pull/${pr}/head`,
        ]);
        const proof = proveMerge({ cwd, cleared, head: initial.headRefOid, run });
        const current = readPr();
        requireThat(current.headRefOid === proof.head, 'PR head moved during proof; rerun at the new head');
        const evidence =
            `parents: ${proof.parents.join(' ')}\nrepository: github.com/${repo}\norigin/master: ${proof.master}\n` +
            `merge-tree: ${proof.tree} == ${proof.head}^{tree}\n` +
            `PR-only diff: ${proof.oldBase}..${cleared} == ${proof.newBase}..${proof.head}\n` +
            `PR-only diff SHA256 (text blob IDs/line offsets excluded): ${proof.diffSha256} (${proof.diffBytes} bytes)`;
        if (post) {
            const body = `MERGE-PROOF PASS ${proof.head} (CLEARs at ${cleared} carry)\n\n${evidence}\n`;
            if (!current.comments?.some((comment) => comment.body === body)) {
                call(
                    'gh',
                    [
                        'api',
                        '--hostname',
                        'github.com',
                        '--method',
                        'POST',
                        `repos/${repo}/issues/${pr}/comments`,
                        '--input',
                        '-',
                    ],
                    JSON.stringify({ body }),
                );
            }
        }
        log(`PASS ${proof.head}\n${evidence}`);
        return 0;
    } catch (error) {
        log(`FAIL: ${error.message}`);
        return 1;
    }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
    process.exitCode = main(process.argv.slice(2));
}
