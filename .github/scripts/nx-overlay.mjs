import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const git = (root, args, input) => execFileSync('git', args, {
  cwd: root, input, maxBuffer: 64 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'],
});
const text = (root, ...args) => git(root, args).toString();
const sha = (value) => {
  if (!/^[a-f0-9]{40}$/.test(value ?? '')) throw new Error('Expected an immutable full Git SHA');
  return value;
};

// Match Copybara's literal path grammar; unfamiliar syntax cannot drop a change.
export function pathSet(config, name) {
  const clean = config.replace(/#[^\n]*/g, '');
  const block = clean.match(new RegExp(`^${name}\\s*=\\s*glob\\(\\s*\\[([^\\]]*)\\]\\s*(?:,\\s*exclude\\s*=\\s*\\[([^\\]]*)\\]\\s*,?\\s*)?\\)`, 'm'));
  if (!block) throw new Error(`Unsupported Copybara path set: ${name}`);
  const list = (source = '') => {
    if (source.replace(/"[^"\\]*"|\s|,/g, '')) throw new Error(`Unsupported path list: ${name}`);
    return [...source.matchAll(/"([^"\\]+)"/g)].map(([, pattern]) => {
      if (pattern === '**') return () => true;
      const root = pattern.replace(/\/\*\*$/, '');
      if (!root || /[?*\[\]{}]/.test(root) || root.split('/').some((p) => !p || p === '.' || p === '..')) {
        throw new Error(`Unsupported Copybara path: ${pattern}`);
      }
      return (file) => file === root || (pattern.endsWith('/**') && file.startsWith(`${root}/`));
    });
  };
  const included = list(block[1]);
  const excluded = list(block[2]);
  if (!included.length) throw new Error(`Empty Copybara path set: ${name}`);
  return (file) => included.some((matches) => matches(file)) && !excluded.some((matches) => matches(file));
}

export function overlaySdk({ sdk, nx, base, head }) {
  sha(base); sha(head);
  if (resolve(sdk) === resolve(nx)) throw new Error('SDK and NX must be separate checkouts');
  if (text(sdk, 'rev-parse', 'HEAD').trim() !== head) throw new Error('SDK checkout moved');
  if (text(nx, 'status', '--porcelain', '--untracked-files=all').trim()) throw new Error('NX checkout is dirty');
  const nxSha = sha(text(nx, 'rev-parse', 'HEAD').trim());
  const ancestor = sha(text(sdk, 'merge-base', base, head).trim());
  const config = readFileSync(resolve(nx, 'mirrors/copy.bara.sky'), 'utf8');
  const destination = pathSet(config, 'TEALSTREET_SDK_FILES');
  const origin = pathSet(config, 'TEALSTREET_SDK_MIRROR_FILES');
  const scaffolding = pathSet(config, 'ROOT_SCAFFOLDING');
  const groups = new Map([['', []], ['mirrors/tealstreet-sdk', []]]);
  const files = text(sdk, 'diff', '--no-renames', '--name-only', '-z', ancestor, head, '--').split('\0').filter(Boolean);
  for (const file of files) {
    if (!origin(file)) throw new Error(`SDK change is excluded from its mirror: ${file}`);
    const prefix = scaffolding(file) ? 'mirrors/tealstreet-sdk' : '';
    const mapped = prefix ? `${prefix}/${file}` : file;
    if (!destination(mapped)) throw new Error(`SDK change has no NX destination: ${file}`);
    let headEntry = '';
    // Only the old/new blobs are needed by git apply's three-way merge.
    for (const ref of [ancestor, head]) {
      const entry = text(sdk, 'ls-tree', '-z', ref, '--', file);
      if (!entry) continue;
      const match = /^(100644|100755) blob ([a-f0-9]{40})\t([^\0]+)\0$/.exec(entry);
      if (!match || match[3] !== file) throw new Error(`SDK changes require regular files: ${file}`);
      if (ref === head) headEntry = `${match[1]} ${match[2]}`;
      const bytes = git(sdk, ['cat-file', 'blob', match[2]]);
      const actual = git(nx, ['hash-object', '-w', '--stdin'], bytes).toString().trim();
      if (actual !== match[2]) throw new Error('Git object format differs between SDK and NX');
    }
    const current = text(nx, 'ls-tree', '-z', nxSha, '--', mapped);
    const target = current ? /^(\d+) blob ([a-f0-9]{40})\t/.exec(current) : null;
    const currentEntry = target ? `${target[1]} ${target[2]}` : '';
    if (current && !target) throw new Error(`NX destination is not a file: ${mapped}`);
    if (currentEntry !== headEntry) groups.get(prefix).push(`:(literal)${file}`);
  }
  for (const [prefix, paths] of groups) {
    if (!paths.length) continue;
    const patch = git(sdk, ['diff', '--binary', '--full-index', '--no-renames', ancestor, head, '--', ...paths]);
    git(nx, ['apply', '--index', '--3way', '--binary', '--whitespace=nowarn', ...(prefix ? [`--directory=${prefix}`] : [])], patch);
  }
  if (text(nx, 'diff', '--cached', '--name-only', '-z')) {
    git(nx, ['-c', 'user.name=SDK integration CI', '-c', 'user.email=sdk-integration@example.invalid', '-c', 'commit.gpgsign=false',
      '-c', 'core.hooksPath=/dev/null', 'commit', '-m', `Ephemeral SDK caller check ${head}`]);
  }
  return { nxSha, sdkBase: ancestor, sdkHead: head, candidateSha: text(nx, 'rev-parse', 'HEAD').trim(), changed: files.length };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  try {
    const [sdk, nx, base, head] = process.argv.slice(2);
    console.log(JSON.stringify(overlaySdk({ sdk, nx, base, head })));
  } catch (error) {
    console.error(error.stderr?.toString() || error.message);
    process.exitCode = 1;
  }
}
