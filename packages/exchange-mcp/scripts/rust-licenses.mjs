import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';

const textsIn = (directory) =>
  readdirSync(directory)
    .filter((name) => /^(licen[cs]e|copying|notice)([.\-_]|$)/i.test(name))
    .map((name) => path.join(directory, name))
    .filter((file) => statSync(file).isFile())
    .map((file) => readFileSync(file, 'utf8').trim())
    .join('\n\n');

/** Retain notices from the existing pinned engine; this adds no Rust dependencies. */
export const rustLicenseEntries = (repoRoot, fallbackText) => {
  // Metadata may need locked workspace/dev crate manifests absent from the WASM
  // build cache. Let Cargo fetch them; the notice closure below still excludes dev edges.
  const metadata = JSON.parse(
    execFileSync(
      'cargo',
      ['metadata', '--locked', '--format-version', '1', '--filter-platform', 'wasm32-unknown-unknown'],
      { cwd: path.join(repoRoot, 'sdk-rs'), encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 },
    ),
  );
  const packages = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const root = metadata.packages.find((pkg) => pkg.name === 'safe-cex-wasm');
  if (!root) throw new Error('safe-cex-wasm is absent from locked Cargo metadata');
  const queue = [root.id];
  const seen = new Set();
  while (queue.length) {
    const id = queue.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dependency of nodes.get(id).deps)
      if (dependency.dep_kinds.some(({ kind }) => kind !== 'dev')) queue.push(dependency.pkg);
  }
  const entries = [];
  for (const id of seen) {
    const pkg = packages.get(id);
    if (!pkg.source) continue;
    const directory = path.dirname(pkg.manifest_path);
    const file = pkg.license_file && path.resolve(directory, pkg.license_file);
    const license = pkg.license?.replaceAll('/', ' OR ');
    const text =
      (file && existsSync(file) ? readFileSync(file, 'utf8').trim() : '') ||
      textsIn(directory) ||
      fallbackText({
        name: pkg.name,
        license: license ?? '',
        author: pkg.authors.join(', ') || undefined,
      });
    if (!license || !text) throw new Error(`Missing Rust licence text: ${pkg.name}@${pkg.version}`);
    const source = pkg.source.startsWith('registry+')
      ? `https://crates.io/crates/${pkg.name}/${pkg.version}`
      : pkg.source.replace(/^git\+/, '');
    entries.push({ name: `Rust: ${pkg.name}`, version: pkg.version, license, text: `${text}\n\nSource: ${source}` });
  }
  // This is adapted source inside our own workspace rather than a Cargo crate.
  entries.push({
    name: 'Rust: zkLink native primitives',
    version: '3152525e2615ecbbdb56fd6728ce265ad2dde485',
    license: 'Apache-2.0',
    text:
      `${readFileSync(path.join(repoRoot, 'sdk-rs/crypto/LICENSE-zklink'), 'utf8').trim()}\n\n` +
      'Copyright 2023-present, zk.link. Modified by Tealstreet Ltd: deterministic signing and contract serialization.\n' +
      'Source: https://github.com/zkLinkProtocol/zklink_sdk/tree/3152525e2615ecbbdb56fd6728ce265ad2dde485',
  });
  return entries;
};
