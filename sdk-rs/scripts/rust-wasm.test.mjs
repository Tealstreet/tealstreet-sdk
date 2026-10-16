import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import { metadata, policy, workflow, workflows, validateCoverage, validateWorkflow, validateWorkflows, buildArguments, clippyArguments } from './rust-wasm.mjs';

const workspace = metadata();

test('wasm job builds every deliberately classified browser crate', () => {
    validateCoverage(workspace, policy);
    validateWorkflow(workflow);
    const args = buildArguments(policy);
    const selected = args.flatMap((arg, index) => (arg === '-p' ? [args[index + 1]] : []));
    assert.deepEqual(selected.toSorted(), policy.wasm.map((entry) => entry.package).toSorted());
    assert.ok(args[args.indexOf('--features') + 1].split(',').includes('safe-cex-ffi/wasm'));
});

test('removing crypto from the wasm list fails closed', () => {
    const broken = structuredClone(policy);
    broken.wasm = broken.wasm.filter((entry) => entry.package !== 'safe-cex-crypto');
    assert.throws(() => validateCoverage(workspace, broken), /unclassified workspace crates: safe-cex-crypto/);
});

test('a new workspace crate requires deliberate classification', () => {
    const broken = structuredClone(workspace);
    const id = 'new-unclassified-crate';
    broken.workspace_members.push(id);
    broken.packages.push({ id, name: id, features: {} });
    assert.throws(() => validateCoverage(broken, policy), /unclassified workspace crates: new-unclassified-crate/);
});

test('FFI cannot silently build with its wasm bindings disabled', () => {
    const broken = structuredClone(policy);
    broken.wasm.find((entry) => entry.package === 'safe-cex-ffi').features = [];
    assert.throws(() => validateCoverage(workspace, broken), /FFI must build its wasm binding feature/);
});

test('the workflow cannot revert to an incomplete hand-picked build', () => {
    const broken = workflow.replace(
        'run: node scripts/rust-wasm.mjs --build',
        'run: cargo build --locked --target wasm32-unknown-unknown -p safe-cex-types',
    );
    assert.throws(() => validateWorkflow(broken), /Build wasm workspace must invoke the full-coverage helper/);
});

test('a commented helper cannot disguise a skipped build', () => {
    const broken = workflow.replace(
        'run: node scripts/rust-wasm.mjs --build',
        'run: true\n        #        run: node scripts/rust-wasm.mjs --build',
    );
    assert.throws(() => validateWorkflow(broken), /Build wasm workspace must invoke the full-coverage helper/);
});

test('duplicate run declarations are rejected', () => {
    const broken = workflow.replace(
        'run: node scripts/rust-wasm.mjs --build',
        'run: node scripts/rust-wasm.mjs --build\n        run: true',
    );
    assert.throws(() => validateWorkflow(broken), /Build wasm workspace must have exactly one run command/);
});


test('wasm clippy selects every classified browser crate with locked all-target coverage', () => {
    const args = clippyArguments(policy);
    assert.equal(args[0], 'clippy', 'wasm lint must execute clippy rather than build');
    assert.ok(args.includes('--locked'));
    assert.ok(args.includes('--all-targets'));
    assert.equal(args[args.indexOf('--target') + 1], 'wasm32-unknown-unknown');
    assert.equal(args[args.indexOf('--jobs') + 1], '2');
    const selected = args.flatMap((arg, index) => arg === '-p' ? [args[index + 1]] : []);
    assert.deepEqual(selected.toSorted(), policy.wasm.map(entry => entry.package).toSorted());
    assert.ok(args[args.indexOf('--features') + 1].split(',').includes('safe-cex-ffi/wasm'));
});

for (const replacement of ['', '        if: false\n', '        continue-on-error: true\n', '        run: true\n']) {
    test(`wasm clippy cannot disappear or bypass: ${JSON.stringify(replacement)}`, () => {
        const broken = replacement === ''
            ? workflow.replace(/      - name: Lint wasm workspace\n        run: [^\n]+\n/, '')
            : workflow.replace('      - name: Lint wasm workspace\n', '      - name: Lint wasm workspace\n' + replacement);
        assert.throws(() => validateWorkflow(broken), /Lint wasm workspace/);
    });
}


for (const cargoStatus of [0, 17]) {
    test(`CLI executes classified wasm clippy and preserves failure ${cargoStatus}`, t => {
        const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'wasm-clippy-fixture-'));
        t.after(() => fs.rmSync(directory, {recursive:true, force:true}));
        const snapshot = path.join(directory, 'workspace.json');
        const calls = path.join(directory, 'calls.jsonl');
        fs.writeFileSync(snapshot, JSON.stringify(workspace));
        const cargo = path.join(directory, 'cargo');
        fs.writeFileSync(cargo, `#!/usr/bin/env node
const fs=require('node:fs');
const args=process.argv.slice(2);
fs.appendFileSync(${JSON.stringify(calls)},JSON.stringify(args)+'\\n');
if(args[0]==='metadata')process.stdout.write(fs.readFileSync(${JSON.stringify(snapshot)}));
else process.exit(${cargoStatus});
`);
        fs.chmodSync(cargo, 0o755);
        const result = spawnSync(process.execPath, [fileURLToPath(new URL('./rust-wasm.mjs', import.meta.url)), '--clippy'], {
            env: {...process.env, PATH:directory+path.delimiter+process.env.PATH}, encoding:'utf8',
        });
        const commands = fs.readFileSync(calls, 'utf8').trim().split('\n').map(line=>JSON.parse(line));
        assert.equal(commands.length,2,'CLI must query metadata and actually execute lint');
        assert.equal(commands[1][0],'clippy','CLI must reach clippy, not a successful build');
        assert.deepEqual(commands[1], clippyArguments(policy),'CLI must execute the full classified clippy command');
        assert.equal(result.status, cargoStatus===0?0:1, result.stderr);
        if(cargoStatus!==0)assert.match(result.stderr,/wasm workspace clippy failed/);
    });
}


/**
 * The helper and the policy live inside the slice, so the same audit runs in the
 * monorepo and on the public mirror. Which workflow carries the wasm job is
 * therefore not fixed, and these three cases pin the only answers that are safe:
 * at least one carries it, every carrier is held to the full step shape, and a
 * carrier's failure names the file so the error is actionable in either repo.
 */
test('some workflow in this checkout declares the wasm job', () => {
    const carrying = validateWorkflows(workflows);
    assert.ok(carrying.length > 0);
});

test('a checkout where no workflow declares the wasm job fails closed', () => {
    const stripped = workflows.map((entry) => ({ ...entry, source: entry.source.replace(/^  wasm:$/m, '  retired:') }));
    assert.throws(() => validateWorkflows(stripped), /no workflow declares a wasm job/);
});

test('a broken carrier is reported with its file name', () => {
    const broken = [{ rel: 'mirrors/tealstreet-sdk/.github/workflows/ci.yml', source: workflow.replace('      - name: Lint wasm workspace\n', '      - name: Lint wasm workspace\n        continue-on-error: true\n') }];
    assert.throws(() => validateWorkflows(broken), /mirrors\/tealstreet-sdk\/\.github\/workflows\/ci\.yml: Lint wasm workspace/);
});
