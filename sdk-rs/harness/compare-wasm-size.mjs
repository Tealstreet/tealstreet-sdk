// Diagnostic only: build a PR merge and its exact master first-parent together.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { assertBuildEnvironment, buildInputFingerprint } from './ci-wasm.mjs';
import { venueFamilies } from '../hosts/wasm/js/venues.mjs';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const command = (root, cmd, args, capture = true) => {
  const result = spawnSync(cmd, args, { cwd: root, encoding: 'utf8', stdio: capture ? 'pipe' : 'inherit' });
  if (result.error || result.status !== 0) throw new Error(`${cmd} ${args.join(' ')} failed: ${result.error ?? result.stderr ?? result.status}`);
  return result.stdout?.trim();
};
export function assertComparisonParents({ merge, parents, expectedMerge, expectedBase, expectedHead }) {
  for (const revision of [merge, ...parents, expectedMerge, expectedBase, expectedHead])
    assert.match(revision ?? '', /^[a-f0-9]{40}$/, 'full immutable revisions required');
  assert.equal(parents.length, 2, 'comparison requires the GitHub PR merge, not a branch head');
  assert.equal(merge, expectedMerge, 'merge changed after selection');
  assert.equal(parents[0], expectedBase, 'base must be the exact master first-parent');
  assert.equal(parents[1], expectedHead, 'PR head changed after selection');
}
export function compareWasmSize({ root = process.cwd(), family, out, env = process.env }) {
  assert(env.GITHUB_ACTIONS === 'true' && /^\d+$/.test(env.GITHUB_RUN_ID ?? ''), 'same CI run required');
  assert(family === 'combined' || venueFamilies.includes(family), 'canonical family required');
  assertBuildEnvironment(env);
  assert(!env.RUSTFLAGS && !env.CARGO_ENCODED_RUSTFLAGS, 'default Rust flags required');
  assert.equal(env.SAFE_CEX_WASM_OPT, 'required');
  assert.equal(env.WASM_OPT_LEVEL || '-Oz', '-Oz');
  const merge = command(root, 'git', ['rev-parse', 'HEAD']);
  const parents = command(root, 'git', ['show', '-s', '--format=%P', 'HEAD']).split(' ');
  assertComparisonParents({ merge, parents, expectedMerge: env.EXPECTED_MERGE_SHA,
    expectedBase: env.EXPECTED_BASE_SHA, expectedHead: env.EXPECTED_HEAD_SHA });
  const base = mkdtempSync(join(tmpdir(), 'wasm-size-base-'));
  command(root, 'git', ['worktree', 'add', '--detach', base, parents[0]]);
  const output = resolve(out);
  mkdirSync(output, { recursive: true });
  const measure = (checkout, label) => {
    const inputsSha256 = buildInputFingerprint(checkout);
    const tools = Object.fromEntries(['rustc', 'wasm-bindgen', 'wasm-opt'].map(tool =>
      [tool, command(join(checkout, 'sdk-rs'), tool, ['--version'])]));
    const text = path => readFileSync(join(checkout, path), 'utf8');
    const rust = /channel\s*=\s*"([\d.]+)"/.exec(text('sdk-rs/rust-toolchain.toml'))?.[1];
    const bindgen = [...text('.github/workflows/rust.yml').matchAll(/WASM_BINDGEN_VERSION:\s*([\d.]+)/g)].map(m => m[1]);
    const binaryen = /^version=(\d+)$/m.exec(text('sdk-rs/harness/install-binaryen.sh'))?.[1];
    assert(rust && tools.rustc.startsWith(`rustc ${rust} `));
    assert(bindgen.length && new Set(bindgen).size === 1 && tools['wasm-bindgen'] === `wasm-bindgen ${bindgen[0]}`);
    assert(binaryen && new RegExp(`^wasm-opt version ${binaryen}(?:\\s|$)`).test(tools['wasm-opt']));
    // Each checkout has its own target directory: no cross-revision workspace fingerprints.
    command(checkout, 'bash', ['sdk-rs/harness/build.sh', ...(family === 'combined' ? [] : ['--venue', family])], false);
    assert.equal(buildInputFingerprint(checkout), inputsSha256, 'build mutated tracked inputs');
    const targets = Object.fromEntries(['web', 'node'].map(target => {
      const bytes = readFileSync(join(checkout, 'sdk-rs/hosts/wasm/pkg', target, 'safe_cex_wasm_bg.wasm'));
      writeFileSync(join(output, `${label}-${target}.wasm`), bytes);
      return [target, { bytes: bytes.length, sha256: sha256(bytes) }];
    }));
    assert.deepEqual(targets.web, targets.node, 'web and node module bytes differ');
    return { sourceHead: command(checkout, 'git', ['rev-parse', 'HEAD']), inputsSha256, tools,
      buildScriptSha256: sha256(text('sdk-rs/harness/build.sh')), targets };
  };
  try {
    const head = measure(root, 'head');
    const baseline = measure(base, 'base');
    assert.deepEqual(head.tools, baseline.tools, 'both builds must use identical pinned tools');
    assert.equal(head.buildScriptSha256, baseline.buildScriptSha256, 'comparison requires identical build flags');
    const report = { schema: 1, runId: env.GITHUB_RUN_ID, family, merge, base: parents[0], head: parents[1],
      buildFlags: { wasmOpt: 'required', wasmOptLevel: '-Oz', optimizationPasses: 2 },
      baseline, current: head, delta: head.targets.web.bytes - baseline.targets.web.bytes };
    writeFileSync(join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify(report, null, 2));
    return report;
  } finally {
    command(root, 'git', ['worktree', 'remove', '--force', base]);
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { family: { type: 'string' }, out: { type: 'string' } } });
  assert(values.out, '--out required');
  compareWasmSize({ family: values.family, out: values.out });
}
