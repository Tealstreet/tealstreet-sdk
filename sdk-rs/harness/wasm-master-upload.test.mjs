import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const step = workflow.split('      - name: ').find((part) => part.startsWith('Retain sealed same-run family build\n'));
assert.ok(step, 'the sealed family artifact upload must exist');
const expression = /^        if: \$\{\{ (.+) \}\}$/m.exec(step)?.[1] ?? 'success()';

const cases = [
  ['master over-cap push cannot feed the caps bot', 'refs/heads/master', 'push', false, true, false, false],
  ['master over-cap dispatch cannot feed the caps bot', 'refs/heads/master', 'workflow_dispatch', false, true, false, false],
  ['successful master family remains measurable', 'refs/heads/master', 'push', true, true, false, true],
  ['PR over-cap evidence remains available', 'refs/pull/123/merge', 'pull_request', false, true, false, true],
  ['branch dispatch over-cap evidence remains available', 'refs/heads/fix/example', 'workflow_dispatch', false, true, false, true],
  ['successful branch family remains available', 'refs/heads/fix/example', 'workflow_dispatch', true, true, false, true],
  ['a cancelled master does not upload', 'refs/heads/master', 'push', false, true, true, false],
  ['a cancelled PR does not upload', 'refs/pull/123/merge', 'pull_request', false, true, true, false],
  ['an unsealed master cannot upload', 'refs/heads/master', 'push', false, false, false, false],
  ['an unsealed PR cannot upload', 'refs/pull/123/merge', 'pull_request', false, false, false, false],
];

for (const [name, ref, event, success, sealed, cancelled, expected] of cases) {
  test(name, () => {
    // Evaluate the committed upload predicate with GitHub's relevant status facts.
    const upload = runInNewContext(expression, {
      github: { ref, event_name: event },
      steps: { family_seal: { outcome: sealed ? 'success' : 'failure' } },
      cancelled: () => cancelled,
      success: () => success,
    }, { timeout: 1000 });
    assert.equal(upload, expected, `${ref}: success=${success}, sealed=${sealed}, cancelled=${cancelled}`);
  });
}
