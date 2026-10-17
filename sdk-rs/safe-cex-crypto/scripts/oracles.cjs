// All seven scheme families must execute their independent live oracle in CI.
const assert = require('node:assert/strict');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const root = path.resolve(__dirname,'../../..');
const fixtures = path.join(__dirname,'../tests/fixtures');
const commands = [
  ['generate.cjs'],
  ['lighter/go-wasm.cjs'],
  ['lighter/go-wasm.cjs','--edges'],
  ['lighter/go-wasm.cjs','--noncanonical'],
  ['lighter/go-wasm.cjs','--security'],
  ['lighter/go-wasm.cjs','--lengths'],
  ['lighter/go-wasm.cjs','--two-noncanonical'],
  ['../../scripts/check-independent-lighter.cjs'],
  ['ethereum/generate.cjs'],
  ['ethereum/primitives.cjs'],
  ['ethereum/icap-maximum.cjs'],
  ['ethereum/typed-validation.cjs'],
  ['ethereum/aster-preimages.cjs'],
  ['../../scripts/check-ethereum-isolation.cjs'],
  ['msgpack/generate.cjs'],
  ['ed25519/node.cjs'],
  ['starknet/sdk.cjs'],
  ['starknet/typed-data.cjs'],
  ['starknet/domain-field-identities.cjs'],
  ['starknet/typed-boundaries.cjs'], ['starknet/typed-shapes.cjs'], ['starknet/typed-compositions.cjs'], ['starknet/typed-closed-kinds.cjs'], ['starknet/typed-exempt-controls.cjs'], ['starknet/typed-composite-shapes.cjs'],
  ['zklink/npm-wasm.cjs'],
];
for(const [script,...args] of commands) {
  const result=spawnSync(process.execPath,[path.join(fixtures,script),...args],{cwd:root,stdio:'inherit',env:process.env});
  assert.equal(result.status,0,`live oracle failed: ${script} ${args.join(' ')}`);
}
const native=spawnSync(process.execPath,[path.join(__dirname,'zklink-native.cjs')],{cwd:root,stdio:'inherit',env:process.env});
assert.equal(native.status,0,'native SDK boundary oracle failed');
const drift=spawnSync('git',['diff','--exit-code','--','sdk-rs/safe-cex-crypto/tests/fixtures/*.json'],{cwd:root,stdio:'inherit'});
assert.equal(drift.status,0,'crypto oracle fixture regeneration drift');

const identity=spawnSync('cargo',['test','--manifest-path','sdk-rs/Cargo.toml','--locked','-p','safe-cex-crypto','--test','stark_enum_cardinality','--test','field_identities','--test','independent_lighter_field_identity','--test','live_poseidon_types','--jobs','1','--','--include-ignored'],{cwd:root,stdio:'inherit',env:process.env});
assert.equal(identity.status,0,'native cryptographic identity supplements failed');
