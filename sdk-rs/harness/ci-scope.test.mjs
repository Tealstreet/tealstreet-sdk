import assert from 'node:assert/strict';
import test from 'node:test';
import { inRustScope } from './ci-scope.mjs';

test('Rust paths and shared inputs are in scope', () => {
  for (const path of ['sdk-rs/engine/src/lib.rs', 'package.json', 'yarn.lock', 'tsconfig.base.json',
    '.cargo/config.toml', 'rust-toolchain.toml', 'packages/safe-cex/src/index.ts', 'apps/cli/src/main.ts',
    '.github/workflows/rust.yml', '.github/actions/install-yarn/action.yml'])
    assert.equal(inRustScope([path]), true, path);
});

test('paths outside the filter select no Rust coverage', () => {
  assert.equal(inRustScope([]), false);
  for (const path of ['docs/plans/RUST_REWRITE.md', 'README.md', '.github/workflows/ci.yml', 'apps/documentation/package.json',
    'packages', 'nested/package.json', 'sdk-rsx/lib.rs', '.github/actions/other/action.yml'])
    assert.equal(inRustScope([path]), false, path);
});

test('one in-scope path among others makes the change relevant', () => {
  assert.equal(inRustScope(['docs/a.md', 'sdk-rs/venues/okx/src/lib.rs']), true);
});
