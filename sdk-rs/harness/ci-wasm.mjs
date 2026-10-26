// Reuse a CI-built engine only when its fixed build-input tree matches HEAD.
// This accelerates private replay iteration; full PR-head CI still gates merge.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const INPUTS = Object.freeze([
  'sdk-rs/engine', 'sdk-rs/crypto', 'sdk-rs/registry', 'sdk-rs/venues',
  'sdk-rs/hosts/wasm', 'sdk-rs/Cargo.toml', 'sdk-rs/Cargo.lock',
  'sdk-rs/rust-toolchain.toml', 'sdk-rs/.cargo', 'sdk-rs/harness/build.sh',
  'sdk-rs/harness/install-binaryen.sh', 'sdk-rs/harness/ci-wasm.mjs',
  '.github/workflows/rust.yml', 'package.json', '.cargo', 'rust-toolchain', 'rust-toolchain.toml',
  'sdk-rs/rust-toolchain',
  'packages/safe-cex/src/exchanges/rust/construction-rules.json',
]);
const FILES = Object.freeze(['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'package.json']);
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const git = (root, ...args) => {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`Cannot inspect build-input tree: git ${args[0]}`);
  return result.stdout;
};
export const buildInputFingerprint = (root = process.cwd()) => {
  if (git(root, 'status', '--porcelain', '--untracked-files=all', '--', ...INPUTS).trim())
    throw new Error('Build inputs have dirty or untracked files; refusing CI wasm reuse');
  return sha256(git(root, 'ls-tree', '-r', '-z', 'HEAD', '--', ...INPUTS));
};
const flags = (env) => ({
  rustFlagsSha256: sha256(env.RUSTFLAGS ?? ''),
  encodedRustFlagsSha256: sha256(env.CARGO_ENCODED_RUSTFLAGS ?? ''),
  wasmOptLevel: env.WASM_OPT_LEVEL || '-Oz',
  wasmOpt: env.SAFE_CEX_WASM_OPT ?? '',
});
const expectedFlags = flags({ SAFE_CEX_WASM_OPT: 'required' });
const assertFlags = (value) => {
  if (JSON.stringify(value) !== JSON.stringify(expectedFlags))
    throw new Error('CI wasm build flags differ from the pinned replay build contract');
};
export const assertBuildEnvironment = (env) => {
  const overrides = Object.keys(env).filter((name) => env[name] &&
    /^(CARGO_(PROFILE_|TARGET_.*_(RUSTFLAGS|LINKER|RUNNER)$|BUILD_(RUSTFLAGS|RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|TARGET)$)|RUSTUP_TOOLCHAIN$|RUSTC($|_)|RUSTDOCFLAGS$)/.test(name));
  if (overrides.length) throw new Error(`Unsupported CI wasm build overrides: ${overrides.join(', ')}`);
};
const assertTools = (root, tools) => {
  const text = (path) => readFileSync(resolve(root, path), 'utf8');
  const rust = /channel\s*=\s*"([\d.]+)"/.exec(text('sdk-rs/rust-toolchain.toml'))?.[1];
  const bindgen = [...text('.github/workflows/rust.yml').matchAll(/WASM_BINDGEN_VERSION:\s*([\d.]+)/g)].map((m) => m[1]);
  const binaryen = /^version=(\d+)$/m.exec(text('sdk-rs/harness/install-binaryen.sh'))?.[1];
  if (!rust || !binaryen || !bindgen.length || new Set(bindgen).size !== 1 ||
      !tools?.rustc?.startsWith(`rustc ${rust} `) ||
      tools?.['wasm-bindgen'] !== `wasm-bindgen ${bindgen[0]}` ||
      !tools?.['wasm-opt']?.match(new RegExp(`^wasm-opt version ${binaryen}(?:\\s|$)`)))
    throw new Error('CI wasm tool versions differ from the pinned build contract');
};
const toolVersion = (root, tool, env) => {
  const result = spawnSync(tool, ['--version'], { cwd: resolve(root, 'sdk-rs'), encoding: 'utf8', env });
  if (result.status !== 0 || !result.stdout.trim()) throw new Error(`Cannot identify ${tool}`);
  return result.stdout.trim();
};
export const archiveCiWasm = ({ root = process.cwd(), venue, destination, env = process.env }) => {
  if (env.GITHUB_ACTIONS !== 'true' || !/^\d+$/.test(env.GITHUB_RUN_ID ?? ''))
    throw new Error('CI wasm archival requires a GitHub Actions run');
  assertBuildEnvironment(env);
  const buildFlags = flags(env);
  assertFlags(buildFlags);
  const metadata = {
    schema: 1,
    sourceHead: git(root, 'rev-parse', 'HEAD').trim(),
    runId: env.GITHUB_RUN_ID,
    inputsSha256: buildInputFingerprint(root),
    family: venueFamily(venue),
    venue,
    target: 'nodejs',
    profile: 'release',
    buildFlags,
    tools: Object.fromEntries(['rustc', 'wasm-bindgen', 'wasm-opt'].map((tool) => [tool, toolVersion(root, tool, env)])),
    hashes: {},
  };
  assertTools(root, metadata.tools);
  const source = resolve(root, 'sdk-rs/hosts/wasm/pkg/node');
  // Read every required file before writing anything: partial packages fail.
  const bytes = Object.fromEntries(FILES.map((name) => [name, readFileSync(resolve(source, name))]));
  mkdirSync(destination, { recursive: true });
  for (const name of FILES) {
    metadata.hashes[name] = sha256(bytes[name]);
    writeFileSync(resolve(destination, name), bytes[name]);
  }
  writeFileSync(resolve(destination, 'build.json'), `${JSON.stringify(metadata, null, 2)}\n`);
  return metadata;
};
const validatePackage = ({ root, directory, venue, fingerprint, sourceHead, runId }) => {
  if (typeof directory !== 'string' || !directory.trim())
    throw new Error('CI wasm directory must be nonempty; refusing reuse (no fallback)');
  const metadata = JSON.parse(readFileSync(resolve(directory, 'build.json'), 'utf8'));
  if (metadata.schema !== 1 || metadata.target !== 'nodejs' || metadata.profile !== 'release' ||
      !/^[a-f0-9]{40}$/.test(metadata.sourceHead ?? '') || !/^\d+$/.test(metadata.runId ?? '') ||
      metadata.family !== venueFamily(venue) || metadata.family !== venueFamily(metadata.venue))
    throw new Error('Invalid CI wasm build contract or venue family');
  if ((sourceHead !== undefined && metadata.sourceHead !== sourceHead) ||
      (runId !== undefined && metadata.runId !== runId)) throw new Error('CI wasm source/run differs from paired selection');
  assertFlags(metadata.buildFlags);
  assertTools(root, metadata.tools);
  if (metadata.inputsSha256 !== fingerprint())
    throw new Error('CI wasm git-tree fingerprint differs from HEAD; refusing reuse (no fallback)');
  for (const name of FILES) {
    if (metadata.hashes?.[name] !== sha256(readFileSync(resolve(directory, name))))
      throw new Error(`CI wasm package hash mismatch: ${name}`);
  }
  return { path: resolve(directory, 'safe_cex_wasm.js'), metadata };
};
export const validateCiWasm = ({ root = process.cwd(), directory, venue }) =>
  validatePackage({ root, directory, venue, fingerprint: () => buildInputFingerprint(root) });

export const validateSameRunCiWasm = ({ root = process.cwd(), directory, venue, env = process.env }) => {
  if (env.GITHUB_ACTIONS !== 'true' || !/^\d+$/.test(env.GITHUB_RUN_ID ?? '')) throw new Error('Same-run CI context required');
  return validatePackage({ root, directory, venue, fingerprint: () => buildInputFingerprint(root),
    sourceHead: git(root, 'rev-parse', 'HEAD').trim(), runId: env.GITHUB_RUN_ID });
};

// Measurement-only source contract; final paired certification separately
// authenticates the selected landing/anchor and every physical observation.
export const validatePairedCiWasm = ({ root = process.cwd(), directory, venue, sourceHead, env = process.env }) => {
  if (venue !== 'bitget' || env.GITHUB_ACTIONS !== 'true' || env.GITHUB_RUN_ATTEMPT !== '1' ||
      !/^\d+$/.test(env.GITHUB_RUN_ID ?? '') || !/^[a-f0-9]{40}$/.test(sourceHead ?? ''))
    throw new Error('Paired module selection requires original CI public Bitget evidence');
  buildInputFingerprint(root);
  git(root, 'merge-base', '--is-ancestor', sourceHead, 'HEAD');
  const inputsSha256 = sha256(git(root, 'ls-tree', '-r', '-z', sourceHead, '--', ...INPUTS));
  return validatePackage({ root, directory, venue, fingerprint: () => inputsSha256, sourceHead, runId: env.GITHUB_RUN_ID });
};
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: {
    venue: { type: 'string' }, archive: { type: 'string' }, verify: { type: 'string' },
  } });
  if (values.verify) {
    if (!values.venue || values.archive) throw new Error('Select --venue and --verify only');
    validateSameRunCiWasm({ directory: resolve(values.verify), venue: values.venue });
    console.log('Verified same-run family archive');
    process.exit(0);
  }
  if (!values.venue || !values.archive) throw new Error('Expected --venue VENUE --archive DIRECTORY');
  const metadata = archiveCiWasm({ venue: values.venue, destination: resolve(values.archive) });
  console.log(`Archived CI wasm ${metadata.family} from ${metadata.sourceHead} (${metadata.inputsSha256})`);
}
