import { describe, expect, it } from 'vitest';

import { checkTopLevelEffects } from './checkTopLevelEffects';

describe('checkTopLevelEffects', () => {
  // Declarations are legitimate at top level; rejecting them would break real
  // scripts for no security gain.
  it('allows declarations', () => {
    const source = `
import type { CoreApi } from '@tealstreet/script-api-core';
type Config = { size: number };
interface Other { x: number }
enum Mode { A, B }
const CONFIG: Config = { size: 1 };
function helper(n: number) { return fetch('https://x'); }
export const meta = { targets: ['core'], capabilities: { accounts: [], writes: [], idempotent: true, storage: { enabled: false, quotaBytes: 0 } } };
export default async function (args: unknown, api: CoreApi) { void CONFIG; void helper; void api; void Mode; }
`;
    expect(checkTopLevelEffects(source)).toEqual([]);
  });

  // These are what remains reachable at top level once checkRuntimeImports has
  // rejected every non-type import.
  it('flags a top-level call in a const initializer', () => {
    const issues = checkTopLevelEffects(`const x = fetch('https://evil');\nexport default async function () {}`);
    expect(issues).toHaveLength(1);
    expect(issues[0].line).toBe(1);
  });

  it('flags a bare top-level expression statement', () => {
    expect(checkTopLevelEffects(`console.log('side effect');`)).toHaveLength(1);
  });

  it('flags a top-level Bun.spawn', () => {
    expect(checkTopLevelEffects(`Bun.spawn(['rm', '-rf', '~']);`)).toHaveLength(1);
  });

  it('flags top-level await', () => {
    expect(checkTopLevelEffects(`const r = await fetch('https://x');`).length).toBeGreaterThan(0);
  });

  it('flags top-level control flow', () => {
    expect(checkTopLevelEffects(`while (true) { doThing(); }`)).toHaveLength(1);
    expect(checkTopLevelEffects(`if (globalThis.X) { leak(); }`)).toHaveLength(1);
  });

  it('flags new expressions', () => {
    expect(checkTopLevelEffects(`const w = new WebSocket('wss://x');`)).toHaveLength(1);
  });

  // A call inside a helper body only runs when the helper is called.
  it('does not descend into function bodies', () => {
    expect(checkTopLevelEffects(`function f() { return fetch('https://x'); }`)).toEqual([]);
    expect(checkTopLevelEffects(`const f = () => fetch('https://x');`)).toEqual([]);
  });
});
