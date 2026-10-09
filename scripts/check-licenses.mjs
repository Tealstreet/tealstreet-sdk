import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Every workspace declares a licence, and every vendored thirdparty/* tree has
// a THIRD_PARTY_NOTICES.md entry, so a new package cannot ship unlicensed.
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'));

const expandWorkspace = (pattern) => {
  if (!pattern.endsWith('/*')) return [pattern];
  const parent = pattern.slice(0, -2);
  const parentPath = join(repoRoot, parent);
  if (!existsSync(parentPath)) return [];
  return readdirSync(parentPath, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => `${parent}/${entry.name}`);
};

const workspaces = readJson(join(repoRoot, 'package.json'))
  .workspaces.flatMap(expandWorkspace)
  .filter((dir) => existsSync(join(repoRoot, dir, 'package.json')));

// A workspace with no open-source grant may ship only with a stated reason.
const UNLICENSED_ALLOWED = {
  'thirdparty/grvt-js-client': 'accepted: GRVT publishes it openly on npm as @grvt/client',
};

const notices = readFileSync(join(repoRoot, 'THIRD_PARTY_NOTICES.md'), 'utf8');
const noticeHeadings = new Set(
  notices
    .split('\n')
    .filter((line) => line.startsWith('## `'))
    .map((line) => line.slice(4, line.lastIndexOf('`'))),
);
const failures = [];

if (readJson(join(repoRoot, 'package.json')).license !== 'MIT') {
  failures.push('root package.json must declare "license": "MIT"');
}

for (const dir of workspaces) {
  const { license } = readJson(join(repoRoot, dir, 'package.json'));
  if (typeof license !== 'string' || license.trim() === '') {
    failures.push(`${dir}/package.json has no "license" field`);
  } else if (license === 'UNLICENSED' && !UNLICENSED_ALLOWED[dir]) {
    failures.push(`${dir} is UNLICENSED and not on the allowlist in scripts/check-licenses.mjs`);
  }
  if (dir.startsWith('thirdparty/') && !noticeHeadings.has(dir)) {
    failures.push(`${dir} has no "## \`${dir}\`" section in THIRD_PARTY_NOTICES.md`);
  }
}

if (failures.length > 0) {
  console.error(`License check failed:\n${failures.map((line) => `  - ${line}`).join('\n')}`);
  process.exit(1);
}
console.log(`License check passed for ${workspaces.length} workspaces.`);
