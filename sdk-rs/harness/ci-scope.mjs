import { appendFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Must equal rust.yml's push `paths:` list (affected-ci-workflow.test.mjs pins it).
// Pull requests run unfiltered so the required check always reports; this list
// decides whether that run selects any Rust coverage.
export const RUST_SCOPE_PATTERNS = Object.freeze([
  'sdk-rs/**',
  'package.json',
  'yarn.lock',
  'tsconfig*.json',
  'thirdparty/**',
  '.cargo/**',
  'rust-toolchain*',
  'packages/**',
  'apps/cli/**',
  '.github/workflows/rust.yml',
  '.github/actions/prepare-yarn/**',
  '.github/actions/prefetch-cargo/**',
  '.github/actions/install-yarn/**',
  '.github/actions/install-binaryen/**',
]);

const patternRegex = pattern => new RegExp(`^${pattern
  .split('**').map(part => part.split('*').map(text => text.replace(/[.+?^${}()|[\]\\]/g, '\\$&')).join('[^/]*'))
  .join('.*')}$`);
const SCOPE = RUST_SCOPE_PATTERNS.map(patternRegex);

export const inRustScope = paths => paths.some(path => SCOPE.some(pattern => pattern.test(path)));

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let relevant = true;
  if (process.env.GITHUB_EVENT_NAME === 'pull_request') {
    const { BASE_SHA: base, HEAD_SHA: head } = process.env;
    if (!/^[a-f0-9]{40}$/.test(base ?? '') || !/^[a-f0-9]{40}$/.test(head ?? '')) throw new Error('Pull request base and head SHAs required');
    const paths = execFileSync('git', ['diff', '--name-only', `${base}...${head}`], { encoding: 'utf8' }).split('\n').filter(Boolean);
    relevant = inRustScope(paths);
    console.log(JSON.stringify({ rustScope: { relevant, changed: paths.length } }));
  }
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `relevant=${relevant}\n`);
}
