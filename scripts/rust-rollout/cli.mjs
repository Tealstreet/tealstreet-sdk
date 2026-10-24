import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { evaluate, compareArtifacts, validateMatrix } from './model.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const read = (path) => {
  const text = readFileSync(path, 'utf8');
  if (text.length > 2_000_000) throw new Error('Manifest exceeds preparation limit');
  try {
    return JSON.parse(text);
  } catch {
    throw new Error('Malformed JSON manifest');
  }
};

try {
  const [command, ...args] = process.argv.slice(2);
  let result;
  if (command === 'matrix' && args.length === 0) {
    const matrix = validateMatrix(read(`${root}docs/plans/rust-bundle-readiness.json`));
    for (const file of matrix.sourceFiles) {
      const content = execFileSync('git', ['show', `${matrix.sdkRef}:${file.path}`], { cwd: root });
      if (createHash('sha256').update(content).digest('hex') !== file.sha256)
        throw new Error('Pinned source hash mismatch');
    }
    result = { ...matrix, mode: 'dry-run', authority: 'none' };
  } else if (command === 'evaluate' && args.length === 1 && !args[0].startsWith('-')) {
    result = evaluate(read(`${root}docs/plans/rust-bundle-readiness.json`), read(args[0]));
    if (!result.declarationsComplete) process.exitCode = 2;
  } else if (command === 'compare-artifacts' && args.length === 2 && args.every((arg) => !arg.startsWith('-'))) {
    result = compareArtifacts(read(args[0]), read(args[1]));
  } else {
    throw new Error(
      'Usage: cli.mjs matrix | evaluate SYNTHETIC.json | compare-artifacts FROZEN.json CANDIDATE.json; offline preparation only',
    );
  }
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : 'Invalid dry-run input'}\n`);
  process.exitCode = 2;
}
