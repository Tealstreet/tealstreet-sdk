import { createHash } from 'node:crypto';
import { appendFileSync, lstatSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { runNxCommand } from './nx-command.mjs';

const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const load = (root, path) => import(pathToFileURL(resolve(root, path)).href);

export async function planWasm(root) {
  const pin = JSON.parse(readFileSync(join(root, 'scripts/rust-beta-wasm.pin.json'), 'utf8'));
  const { betaVenueFamilies: families } = await load(root, 'sdk-rs/hosts/wasm/js/beta-venues.mjs');
  const { betaWasmInputs } = await load(root, 'sdk-rs/harness/beta-wasm-inputs.mjs');
  const { validatePin } = await load(root, 'scripts/fetch-rust-beta-wasm.mjs');
  // A malformed published pin is an error, never a reason to select a fallback.
  const publishedFamilies = Object.keys(pin.families ?? {});
  if (!publishedFamilies.length) throw new Error('Published pin has no beta families');
  validatePin(pin, publishedFamilies, pin.inputs);
  const workflow = readFileSync(join(root, 'mirrors/tealstreet-sdk/.github/workflows/rust.yml'), 'utf8');
  const versions = [...workflow.matchAll(/WASM_BINDGEN_VERSION:\s*([\d.]+)/g)].map((m) => m[1]);
  if (!versions.length || new Set(versions).size !== 1) throw new Error('Missing or conflicting wasm-bindgen pin');
  const toolchain = { wasmBindgen: versions[0], wasmOpt: 'required', profile: 'release', target: 'wasm32-unknown-unknown' };
  const inputs = betaWasmInputs(root, toolchain);
  const mode = inputs.fingerprint === pin.inputs.fingerprint ? 'published' : 'candidate';
  if (mode === 'published') validatePin(pin, families, inputs);
  return { mode, inputs, families, pin, bindgen: versions[0] };
}

function familyHashes(root, families) {
  const hashes = {};
  for (const family of families) {
    if (!/^[a-z][a-z0-9]*$/.test(family)) throw new Error('Invalid beta family name');
    const directory = join(root, 'sdk-rs/hosts/wasm/pkg/web', family);
    if (!lstatSync(directory).isDirectory() || lstatSync(directory).isSymbolicLink()) throw new Error('Invalid family directory');
    const entries = readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name));
    if (!entries.some((entry) => entry.name === 'safe_cex_wasm_bg.wasm') || !entries.some((entry) => entry.name === 'safe_cex_wasm.js')) {
      throw new Error(`Missing built beta family: ${family}`);
    }
    for (const entry of entries) {
      if (!entry.isFile()) throw new Error('Beta assets must be regular files');
      hashes[`${family}/${entry.name}`] = digest(readFileSync(join(directory, entry.name)));
    }
  }
  return hashes;
}

export async function prepareWasm(root, receipt, logDirectory = process.env.RUNNER_TEMP) {
  const plan = await planWasm(root);
  if (plan.mode === 'published') {
    const { installBetaWasm } = await load(root, 'scripts/fetch-rust-beta-wasm.mjs');
    await installBetaWasm({ root, pin: plan.pin, families: plan.families, inputs: plan.inputs, log: console.log });
  } else {
    runNxCommand(root, 'candidate-wasm', process.execPath, ['scripts/build-rust-beta-wasm.mjs'], logDirectory);
  }
  const build = JSON.parse(readFileSync(join(root, 'apps/web/package.json'), 'utf8')).scripts?.build;
  if (typeof build !== 'string' || !build.trim()) throw new Error('NX web build script is missing');
  const record = { mode: plan.mode, inputs: plan.inputs, families: plan.families,
    build, hashes: familyHashes(root, plan.families) };
  writeFileSync(receipt, `${JSON.stringify(record, null, 2)}\n`);
  return record;
}

export async function buildNx(root, receipt, logDirectory = process.env.RUNNER_TEMP) {
  const record = JSON.parse(readFileSync(receipt, 'utf8'));
  const plan = await planWasm(root);
  if (record.mode !== plan.mode || JSON.stringify(record.inputs) !== JSON.stringify(plan.inputs) ||
      JSON.stringify(record.families) !== JSON.stringify(plan.families) ||
      JSON.stringify(record.hashes) !== JSON.stringify(familyHashes(root, plan.families))) {
    throw new Error('Prepared NX candidate inputs/assets changed');
  }
  const build = JSON.parse(readFileSync(join(root, 'apps/web/package.json'), 'utf8')).scripts?.build;
  if (build !== record.build) throw new Error('Prepared NX build script changed');
  // Execute the maintained build body; only the release-only prebuild is separate.
  runNxCommand(join(root, 'apps/web'), 'build', 'corepack', ['yarn', 'exec', '--', 'bash', '-e', '-o', 'pipefail', '-c', build], logDirectory);
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try {
    const [command, directory, receipt] = process.argv.slice(2);
    const root = resolve(directory);
    if (command === 'plan') {
      const { mode, bindgen } = await planWasm(root);
      appendFileSync(process.env.GITHUB_OUTPUT, `mode=${mode}\nbindgen=${bindgen}\n`);
    } else if (command === 'prepare') {
      const { mode } = await prepareWasm(root, receipt);
      console.log(`NX WASM preparation: ${mode}`);
    } else if (command === 'build') {
      await buildNx(root, receipt);
    } else throw new Error('Unknown NX integration command');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
