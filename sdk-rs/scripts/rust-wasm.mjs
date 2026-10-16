import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const sdk = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const root = path.resolve(sdk, '..');
export const policy = JSON.parse(fs.readFileSync(path.join(sdk, 'rust-wasm.json'), 'utf8'));

/**
 * The policy, the helper and the audit all live INSIDE `sdk-rs`, so they travel
 * with the mirror slice and run unchanged in either repo. What differs is which
 * workflow carries the wasm job, so the audit does not name one file: it reads
 * every candidate that exists and holds each one that declares a `wasm:` job to
 * the same three mandatory steps.
 *
 * Fail-closed in both directions. A candidate with a `wasm:` job whose steps are
 * masked, conditioned or renamed fails, and if NO candidate declares the job at
 * all the audit fails too — otherwise deleting the job from the monorepo while
 * the mirror had not picked it up yet would read as green.
 */
const WORKFLOW_CANDIDATES = [
    // Monorepo: the pre-merge tier.
    '.github/workflows/rust.yml',
    // Monorepo: the mirror's workflow, authored here (contributors cannot edit
    // it on the mirror — CODEOWNERS + branch protection).
    'mirrors/tealstreet-sdk/.github/workflows/ci.yml',
    // The same file as seen from inside the mirror repo, where this slice is
    // the whole tree.
    '.github/workflows/ci.yml',
];

export const workflows = WORKFLOW_CANDIDATES.map((rel) => ({ rel, path: path.join(root, rel) }))
    .filter((candidate) => fs.existsSync(candidate.path))
    .map((candidate) => ({ ...candidate, source: fs.readFileSync(candidate.path, 'utf8') }));

/** Kept as a single-source export for the test's negative controls. */
export const workflow = workflows.find((entry) => /^  wasm:$/m.test(entry.source))?.source ?? '';

export function metadata() {
    const result = spawnSync('cargo', ['metadata', '--no-deps', '--locked', '--format-version', '1'], {
        cwd: sdk,
        encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.error?.message ?? result.stderr);
    return JSON.parse(result.stdout);
}

export function validateCoverage(workspace, declaration) {
    assert.deepEqual(Object.keys(declaration).sort(), ['nativeOnly', 'wasm']);
    assert.ok(Array.isArray(declaration.wasm) && declaration.wasm.length > 0);
    assert.ok(Array.isArray(declaration.nativeOnly));
    const members = new Map(
        workspace.packages.filter((pkg) => workspace.workspace_members.includes(pkg.id)).map((pkg) => [pkg.name, pkg]),
    );
    assert.equal(members.size, workspace.workspace_members.length, 'workspace members must resolve uniquely');
    const classified = new Set();
    for (const entry of [...declaration.wasm, ...declaration.nativeOnly]) {
        assert.ok(members.has(entry.package), `policy names a non-workspace crate: ${entry.package}`);
        assert.ok(!classified.has(entry.package), `duplicate crate classification: ${entry.package}`);
        classified.add(entry.package);
    }
    const missing = [...members.keys()].filter((name) => !classified.has(name));
    assert.deepEqual(missing, [], `unclassified workspace crates: ${missing.join(', ')}`);
    for (const entry of declaration.nativeOnly) {
        assert.deepEqual(Object.keys(entry).sort(), ['package', 'reason']);
        assert.ok(
            typeof entry.reason === 'string' && entry.reason.trim(),
            `native-only reason required: ${entry.package}`,
        );
    }
    for (const entry of declaration.wasm) {
        assert.deepEqual(Object.keys(entry).sort(), ['features', 'package']);
        assert.ok(Array.isArray(entry.features), `feature list required: ${entry.package}`);
        for (const feature of entry.features) {
            assert.ok(feature.startsWith(`${entry.package}/`), `feature must name its selected crate: ${feature}`);
            assert.ok(
                Object.hasOwn(members.get(entry.package).features, feature.slice(entry.package.length + 1)),
                `unknown feature: ${feature}`,
            );
        }
    }
    const ffi = declaration.wasm.find((entry) => entry.package === 'safe-cex-ffi');
    assert.ok(ffi?.features.includes('safe-cex-ffi/wasm'), 'FFI must build its wasm binding feature');
}

export function validateWorkflow(source) {
    const job = source.match(/^  wasm:\n([\s\S]*?)(?=^  [\w-]+:|$(?![\s\S]))/m)?.[1];
    assert.ok(job, 'wasm job missing');
    assert.ok(!/^    (?:if|continue-on-error):/m.test(job), 'wasm job must be mandatory');
    const steps = [...job.matchAll(/^      - name: ([^\n]+)\n([\s\S]*?)(?=^      - |$(?![\s\S]))/gm)];
    for (const [name, command] of [
        ['Audit wasm workspace coverage', 'node --test scripts/rust-wasm.test.mjs'],
        ['Build wasm workspace', 'node scripts/rust-wasm.mjs --build'],
        ['Lint wasm workspace', 'node scripts/rust-wasm.mjs --clippy'],
    ]) {
        const matches = steps.filter((step) => step[1] === name);
        assert.equal(matches.length, 1, `${name} must run exactly once`);
        const step = matches[0][2];
        assert.ok(
            !/^        (?:if|continue-on-error|working-directory|shell):/m.test(step),
            `${name} must run without bypasses`,
        );
        const runs = [...step.matchAll(/^        run: ([^\n]+)$/gm)];
        assert.equal(runs.length, 1, `${name} must have exactly one run command`);
        assert.equal(runs[0][1], command, `${name} must invoke the full-coverage helper`);
    }
}

/**
 * Every candidate that declares a `wasm:` job must declare it correctly, and at
 * least one candidate must declare it. A candidate without the job is a repo
 * that has handed the tier to the other one, which is allowed; nobody declaring
 * it is the tier running nowhere.
 */
export function validateWorkflows(candidates) {
    assert.ok(candidates.length > 0, 'no workflow file to audit');
    const carrying = candidates.filter((candidate) => /^  wasm:$/m.test(candidate.source));
    assert.ok(
        carrying.length > 0,
        `no workflow declares a wasm job (looked in ${candidates.map((c) => c.rel).join(', ')})`,
    );
    for (const candidate of carrying) {
        try {
            validateWorkflow(candidate.source);
        } catch (error) {
            error.message = `${candidate.rel}: ${error.message}`;
            throw error;
        }
    }
    return carrying.map((candidate) => candidate.rel);
}

export function buildArguments(declaration) {
    const args = ['build', '--locked', '--target', 'wasm32-unknown-unknown', '--jobs', '2'];
    for (const entry of declaration.wasm) args.push('-p', entry.package);
    const features = declaration.wasm.flatMap((entry) => entry.features);
    if (features.length) args.push('--features', features.join(','));
    return args;
}

export function clippyArguments(declaration) {
    const args = buildArguments(declaration);
    args[0] = 'clippy';
    args.splice(1, 0, '--all-targets');
    return args;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    try {
        assert.ok(
            process.argv.length === 2 || (process.argv.length === 3 && ['--build', '--clippy'].includes(process.argv[2])),
            'expected optional --build or --clippy',
        );
        validateCoverage(metadata(), policy);
        validateWorkflows(workflows);
        console.log(
            `Wasm coverage: ${policy.wasm.map((entry) => entry.package).join(', ')}; native-only: ${policy.nativeOnly.map((entry) => entry.package).join(', ')}.`,
        );
        if (process.argv.length === 3) {
            const args = process.argv[2] === '--clippy' ? clippyArguments(policy) : buildArguments(policy);
            const result = spawnSync('cargo', args, {
                cwd: sdk,
                stdio: 'inherit',
            });
            assert.equal(result.status, 0, result.error?.message ?? `wasm workspace ${args[0]} failed`);
        }
    } catch (error) {
        console.error(error.message);
        process.exitCode = 1;
    }
}
