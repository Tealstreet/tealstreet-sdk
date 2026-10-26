// The bundle inlines every dependency, so their licence texts must ship beside it.
// Reads the esbuild metafile, writes dist/THIRD_PARTY_LICENSES.md, and fails on
// any bundled package whose licence is not allowed below.
import { existsSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { rustLicenseEntries } from './rust-licenses.mjs';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const PACKAGE_DIR = path.resolve(SCRIPTS_DIR, '..');
const REPO_ROOT = path.resolve(PACKAGE_DIR, '../..');
const METAFILE = path.join(PACKAGE_DIR, 'dist/metafile-esm.json');
const OUTPUT = path.join(PACKAGE_DIR, 'dist/THIRD_PARTY_LICENSES.md');
// Copybara re-roots the mirror scaffolding, so the file sits at the root on the mirror.
const NOTICES = ['mirrors/tealstreet-sdk/THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_NOTICES.md']
  .map((file) => path.join(REPO_ROOT, file))
  .find((file) => existsSync(file));

const PERMISSIVE = new Set([
  '0BSD',
  'Apache-2.0',
  'BlueOak-1.0.0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'CC0-1.0',
  'ISC',
  'MIT',
  'Unlicense',
  'Zlib',
]);

// File-level copyleft: allowed because the files ship unmodified, and MPL §3.2
// is met by naming where their source is.
const FILE_COPYLEFT = new Set(['MPL-2.0']);

// Same decision as UNLICENSED_ALLOWED in the mirror's scripts/check-licenses.mjs.
const UNLICENSED_ALLOWED = {
  '@tealstreet/grvt-client': 'accepted 2026-10-08: GRVT publishes it openly on npm as @grvt/client',
};

// Full texts for packages that publish a licence field but no licence file.
const CANONICAL_TEXT = {
  'Apache-2.0': () => readFileSync(path.join(SCRIPTS_DIR, 'licenses/Apache-2.0.txt'), 'utf8').trim(),
  MIT: (author) =>
    readFileSync(path.join(SCRIPTS_DIR, 'licenses/MIT.txt'), 'utf8').replace('{{author}}', author).trim(),
  'CC0-1.0': () => readFileSync(path.join(SCRIPTS_DIR, 'licenses/CC0-1.0.txt'), 'utf8').trim(),
};

/** `node_modules/@a/b/x.js` → `node_modules/@a/b`; `thirdparty/x/src/y.ts` → `thirdparty/x`. */
const packageRoot = (input) => {
  const parts = input.split('/');
  const nm = parts.lastIndexOf('node_modules');
  if (nm >= 0) return parts.slice(0, nm + (parts[nm + 1]?.startsWith('@') ? 3 : 2)).join('/');
  if (parts[0] === 'thirdparty' || parts[0] === 'packages') return parts.slice(0, 2).join('/');
  return undefined;
};

/**
 * Evaluates an SPDX expression: OR needs one allowed side, AND needs both,
 * `X WITH exception` is allowed when X is. Unparseable input is refused.
 */
const isAllowed = (expression) => {
  if (typeof expression !== 'string') return false;
  const tokens = expression.match(/\(|\)|[^\s()]+/g) ?? [];
  let i = 0;
  const primary = () => {
    if (tokens[i] === '(') {
      i += 1;
      const value = or();
      if (tokens[i++] !== ')') throw new Error('unbalanced parentheses');
      return value;
    }
    const id = tokens[i++];
    if (!id || ['AND', 'OR', 'WITH', ')'].includes(id)) throw new Error(`unexpected ${id}`);
    if (tokens[i] === 'WITH') i += 2;
    return PERMISSIVE.has(id) || FILE_COPYLEFT.has(id);
  };
  const and = () => {
    let value = primary();
    while (tokens[i] === 'AND') {
      i += 1;
      value = primary() && value;
    }
    return value;
  };
  const or = () => {
    let value = and();
    while (tokens[i] === 'OR') {
      i += 1;
      value = and() || value;
    }
    return value;
  };
  try {
    const value = or();
    return i === tokens.length && value;
  } catch {
    return false;
  }
};

const licenseFiles = (dir) =>
  readdirSync(dir)
    .filter((name) => /^(licen[cs]e|copying|notice)(\.|$)/i.test(name))
    .map((name) => readFileSync(path.join(dir, name), 'utf8').trim())
    .join('\n\n');

/** The `## \`<root>\`` section of THIRD_PARTY_NOTICES.md, or '' when there is none. */
const noticeSection = (root) => {
  if (!NOTICES) throw new Error(`THIRD_PARTY_NOTICES.md not found while reading ${root}`);
  const lines = readFileSync(NOTICES, 'utf8').split('\n');
  const start = lines.indexOf(`## \`${root}\``);
  if (start < 0) return '';
  const end = lines.findIndex((line, i) => i > start && line.startsWith('## '));
  return lines
    .slice(start + 1, end < 0 ? undefined : end)
    .join('\n')
    .trim();
};

const authorOf = ({ author, name }) => (typeof author === 'string' ? author : author?.name) || `the ${name} authors`;

const fallbackText = (manifest) => {
  const ids = new Set(manifest.license.match(/[A-Za-z0-9.-]+/g));
  return Object.entries(CANONICAL_TEXT)
    .filter(([id]) => ids.has(id))
    .map(([, text]) => text(authorOf(manifest)))
    .join('\n\n');
};

const metafile = JSON.parse(readFileSync(METAFILE, 'utf8'));
const roots = new Set();
for (const input of Object.keys(metafile.inputs)) {
  // esbuild writes inputs relative to its working directory, this package.
  const relative = path.relative(REPO_ROOT, path.resolve(PACKAGE_DIR, input)).split(path.sep).join('/');
  const root = packageRoot(relative);
  if (root) roots.add(root);
}

const packages = new Map();
const refused = [];
for (const root of roots) {
  const dir = path.join(REPO_ROOT, root);
  const manifestPath = path.join(dir, 'package.json');
  if (!existsSync(manifestPath)) continue;
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const { name, version, license } = manifest;
  if (!name) continue;

  // Our own workspaces are MIT under this package's LICENSE; list one only
  // when THIRD_PARTY_NOTICES.md records third-party code inside it.
  if (root.startsWith('packages/')) {
    const notice = noticeSection(root);
    if (notice) packages.set(`${name}@${version}`, { name, version, license, text: notice });
    continue;
  }

  const accepted = license === 'UNLICENSED' && UNLICENSED_ALLOWED[name];
  if (!isAllowed(license) && !accepted) refused.push(`${name}@${version}: ${license ?? 'no licence field'}`);

  const key = `${name}@${version}`;
  if (packages.has(key)) continue;
  const text = [
    licenseFiles(dir) || (root.startsWith('thirdparty/') ? noticeSection(root) : '') || fallbackText(manifest),
    [...FILE_COPYLEFT].some((id) => license?.includes(id))
      ? `Bundled unmodified; source: https://www.npmjs.com/package/${name}/v/${version}`
      : '',
    accepted ? `Distributed under an accepted exception: ${accepted}.` : '',
  ]
    .filter(Boolean)
    .join('\n\n');
  if (!text) refused.push(`${name}@${version}: ${license} with no licence text to ship`);
  packages.set(key, { name, version, license, text });
}

if (refused.length) {
  console.error('Bundled packages that cannot ship:\n' + refused.map((line) => `- ${line}`).join('\n'));
  process.exit(1);
}

const rust = rustLicenseEntries(REPO_ROOT, fallbackText);
const sections = [...packages.values(), ...rust]
  .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version))
  .map(({ name, version, license, text }) => `## ${name}@${version} (${license})\n\n${text}`);
writeFileSync(
  OUTPUT,
  `# Third-party licences\n\n\`dist/index.js\` and its owned WASM assets bundle the packages below. Rust entries conservatively include the pinned engine's non-dev Cargo dependency graph, including build tools.\n\n${sections.join('\n\n')}\n`,
);
rmSync(METAFILE);
console.log(`THIRD_PARTY_LICENSES.md: ${packages.size} JS entries, ${rust.length} Rust notices`);
