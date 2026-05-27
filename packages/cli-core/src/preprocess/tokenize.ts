export type ChainOp = ';' | '&';

export interface ChainSegment {
  command: string;
  /** Separator that *preceded* this segment. First segment is null. */
  separator: ChainOp | null;
}

/**
 * Split a chain-of-commands string at top-level `;` and `&`.
 *
 * Paren- and quote-aware so neither `repeat 5 (a; b)` nor
 * `alias x "buy 1; sell 1"` gets mis-split. Caller is responsible for
 * expanding `repeat` blocks first if it wants per-iteration steps.
 */
export function splitChain(input: string): ChainSegment[] {
  const segments: ChainSegment[] = [];
  let buf = '';
  let parenDepth = 0;
  let separator: ChainOp | null = null;
  let inQuotes: '"' | "'" | null = null;

  for (let i = 0; i < input.length; i++) {
    const c = input[i];

    if (inQuotes) {
      if (c === inQuotes && input[i - 1] !== '\\') inQuotes = null;
      buf += c;
      continue;
    }

    if (c === '"' || c === "'") {
      inQuotes = c;
      buf += c;
      continue;
    }

    if (c === '(') {
      parenDepth++;
      buf += c;
      continue;
    }
    if (c === ')') {
      if (parenDepth === 0) {
        throw new Error('Unmatched ) in command');
      }
      parenDepth--;
      buf += c;
      continue;
    }

    if (parenDepth === 0 && (c === ';' || c === '&')) {
      const trimmed = buf.trim();
      if (trimmed) segments.push({ command: trimmed, separator });
      buf = '';
      separator = c;
      continue;
    }

    buf += c;
  }

  if (parenDepth !== 0) {
    throw new Error('Unmatched ( in command');
  }
  if (inQuotes) {
    throw new Error(`Unterminated ${inQuotes} string`);
  }

  const trimmed = buf.trim();
  if (trimmed) segments.push({ command: trimmed, separator });

  return segments;
}
