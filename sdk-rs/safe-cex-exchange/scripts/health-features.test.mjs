// Verify actual declarations and resolved graphs; regex remains optional in the core.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const sdk = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
function cargo(args) {
    const result = spawnSync('cargo', args, { cwd: sdk, encoding: 'utf8' });
    assert.equal(result.status, 0, result.error?.message ?? result.stderr);
    return result.stdout;
}
const actual = JSON.parse(cargo(['metadata', '--locked', '--no-deps', '--format-version', '1']));
const facadeSource = fs.readFileSync(path.join(sdk, 'safe-cex/src/lib.rs'), 'utf8');

function declarations(metadata) {
    const core = metadata.packages.find((pkg) => pkg.name === 'safe-cex-exchange');
    const facade = metadata.packages.find((pkg) => pkg.name === 'safe-cex');
    assert.ok(core && facade, 'core and facade must remain workspace packages');
    assert.ok(metadata.workspace_members.includes(core.id) && metadata.workspace_members.includes(facade.id), 'core and facade must remain workspace members');
    const regex = core.dependencies.find((dep) => dep.name === 'regex' && dep.kind === null);
    const exchange = facade.dependencies.find((dep) => dep.name === 'safe-cex-exchange' && dep.kind === null);
    return { core, facade, regex, exchange };
}
export function validatePolicy(metadata, source) {
    const { core, regex, exchange } = declarations(metadata);
    assert.deepEqual(core.features.default, [], 'core default must remain empty');
    assert.ok(regex?.optional === true, 'core regex must remain optional');
    assert.deepEqual(core.features.health, ['dep:regex'], 'health must explicitly opt into optional regex');
    assert.ok(exchange && exchange.optional === false, 'facade must depend on exchange core unconditionally');
    assert.ok(exchange.features.includes('health'), 'facade must enable exchange health by default');
    assert.equal(exchange.uses_default_features, false, 'facade must select health explicitly instead of core defaults');
    const code = source.replace(/\/\/[^\n]*|\/\*[\s\S]*?\*\//g, '').trim();
    assert.match(code, /^pub use safe_cex_exchange::\*;/, 'facade must unconditionally re-export exchange-core contracts');
}
function hasRegex(tree) {
    return tree.split('\n').some((line) => line.trim().split(/\s+/)[0] === 'regex');
}

test('actual feature declarations keep health opt-in for the core and enabled in the facade', () => {
    validatePolicy(actual, facadeSource);
});
test('actual dependency graphs keep the core health regex opt-in and enable health through the facade', () => {
    // Crypto and Store also use regex. Check the core's own optional edge,
    // rather than rejecting those unrelated transitive dependencies.
    const direct = ['-e', 'normal', '--depth', '1', '--prefix', 'none'];
    const core = cargo(['tree', '--locked', '-p', 'safe-cex-exchange', ...direct, '--format', '{p}']);
    const health = cargo(['tree', '--locked', '-p', 'safe-cex-exchange', '--features', 'health', ...direct, '--format', '{p}']);
    const facade = cargo(['tree', '--locked', '-p', 'safe-cex', ...direct, '--format', '{p}\t{f}']);
    assert.equal(hasRegex(core), false, 'default core direct dependencies must exclude health regex');
    assert.equal(hasRegex(health), true, 'explicit core health must include its direct regex dependency');
    const exchange = facade.split('\n').find(line => line.startsWith('safe-cex-exchange '));
    assert.ok(exchange?.split('\t')[1]?.split(',').includes('health'), 'resolved facade must enable core health');
});
for (const [label, replacement] of [
    ['removed', ''],
    ['conditional', '#[cfg(feature = "health")]\npub use safe_cex_exchange::*;'],
    ['nested', 'pub mod hidden { pub use safe_cex_exchange::*; }'],
]) test(`${label} facade core re-export is rejected`, () => {
    const broken = facadeSource.replace('pub use safe_cex_exchange::*;', replacement);
    assert.throws(() => validatePolicy(actual, broken), /facade must unconditionally re-export exchange-core contracts/);
});
test('removing facade health is rejected for its intended reason', () => {
    const broken = structuredClone(actual);
    declarations(broken).exchange.features = [];
    assert.throws(() => validatePolicy(broken, facadeSource), /facade must enable exchange health by default/);
});
test('making health a core default is rejected for its intended reason', () => {
    const broken = structuredClone(actual);
    declarations(broken).core.features.default = ['health'];
    assert.throws(() => validatePolicy(broken, facadeSource), /core default must remain empty/);
});
test('making regex mandatory in the core is rejected for its intended reason', () => {
    const broken = structuredClone(actual);
    declarations(broken).regex.optional = false;
    assert.throws(() => validatePolicy(broken, facadeSource), /core regex must remain optional/);
});
