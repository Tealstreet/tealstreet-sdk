const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const { transform } = require('../generate.cjs');
const root = path.resolve(__dirname, '../../..');

function outputs() {
  const generated = JSON.parse(
    execFileSync(
      'cargo',
      [
        'run',
        '-j2',
        '--locked',
        '--quiet',
        '--manifest-path',
        path.join(root, 'sdk-rs/Cargo.toml'),
        '-p',
        'safe-cex-types',
        '--example',
        'generate_market_data',
      ],
      { cwd: root, encoding: 'utf8' },
    ),
  );
  return generated.map(([name, raw]) => {
    assert.equal(
      raw,
      fs.readFileSync(path.join(__dirname, '../bindings/raw', `${name}.ts`), 'utf8'),
      `${name}: raw Rust binding drift`,
    );
    // Order owns the enum module; reuse its public runtime identity through the facade.
    const source = transform(raw, {}, `${name}.ts`).source.replace('from "./OrderSide"', 'from "../types"');
    return [path.join(root, 'packages/safe-cex/src/generated', `${name}.ts`), source];
  });
}

test('public market data modules are generated from their canonical Rust module', () => {
  for (const [filename, source] of outputs()) {
    assert.equal(
      fs.readFileSync(filename, 'utf8'),
      source,
      `${filename}: regenerate with corepack yarn types:generate`,
    );
    assert(!source.includes('@ts-mutable'));
  }
});
