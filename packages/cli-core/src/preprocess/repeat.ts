/** Hard cap on the iteration count for a single repeat block. */
export const MAX_REPEAT_COUNT = 1000;
/** Hard cap on nested depth — prevents accidental fork-bomb-y patterns. */
export const MAX_REPEAT_DEPTH = 8;

const REPEAT_RE = /\brepeat\s+(\d+)\s*\(/;

/**
 * Expand `repeat <count> ( ... )` blocks into `<inner>; <inner>; ...`
 * (count copies). Handles nesting and recurses into the body before
 * expanding the outer block, so `repeat 2 (repeat 3 (a))` produces six
 * copies of `a`.
 */
export function expandRepeats(input: string, depth = 0): string {
  if (depth > MAX_REPEAT_DEPTH) {
    throw new Error(`repeat: blocks nested deeper than ${MAX_REPEAT_DEPTH}`);
  }

  let out = '';
  let i = 0;

  while (i < input.length) {
    const remaining = input.slice(i);
    const m = remaining.match(REPEAT_RE);
    if (!m || typeof m.index !== 'number') {
      out += remaining;
      break;
    }

    out += remaining.slice(0, m.index);

    const count = parseInt(m[1], 10);
    if (count <= 0) {
      throw new Error(`repeat: count must be positive, got ${count}`);
    }
    if (count > MAX_REPEAT_COUNT) {
      throw new Error(`repeat: count ${count} exceeds max of ${MAX_REPEAT_COUNT}`);
    }

    // Position of the `(` in the original input
    const openIdx = i + m.index + m[0].length - 1;
    let parenDepth = 1;
    let closeIdx = -1;
    let inQuotes: '"' | "'" | null = null;
    for (let j = openIdx + 1; j < input.length; j++) {
      const c = input[j];
      if (inQuotes) {
        if (c === inQuotes && input[j - 1] !== '\\') inQuotes = null;
        continue;
      }
      if (c === '"' || c === "'") {
        inQuotes = c;
        continue;
      }
      if (c === '(') parenDepth++;
      else if (c === ')') {
        parenDepth--;
        if (parenDepth === 0) {
          closeIdx = j;
          break;
        }
      }
    }
    if (closeIdx === -1) {
      throw new Error('repeat: unmatched ( in block');
    }

    const inner = input.slice(openIdx + 1, closeIdx).trim();
    if (!inner) {
      throw new Error('repeat: empty block');
    }
    const expandedInner = expandRepeats(inner, depth + 1);
    const expanded = Array(count).fill(expandedInner).join('; ');
    out += expanded;
    i = closeIdx + 1;
  }

  return out;
}
