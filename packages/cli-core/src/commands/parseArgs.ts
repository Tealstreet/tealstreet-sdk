import type { ParsedArgs } from './Command';

/**
 * Parse command line string into structured arguments
 *
 * Supports:
 * - Positional arguments: command arg1 arg2
 * - Short flags: -f, -v
 * - Long flags: --force, --verbose
 * - Short options: -n value, -k "value with spaces"
 * - Long options: --name value, --key "value with spaces"
 * - Combined short flags: -fv (expands to -f -v)
 * - Quoted values: "value with spaces", 'single quotes'
 */
export function parseArgs(input: string): ParsedArgs {
  const result: ParsedArgs = { _: [] };
  const tokens = tokenize(input);

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];

    if (token.startsWith('--')) {
      // Long option: --name or --name=value or --name value
      const eqIndex = token.indexOf('=');
      if (eqIndex !== -1) {
        // --name=value
        const key = token.slice(2, eqIndex);
        const value = token.slice(eqIndex + 1);
        result[key] = value;
      } else {
        const key = token.slice(2);
        const next = tokens[i + 1];
        if (next && !next.startsWith('-')) {
          // --name value
          result[key] = next;
          i++;
        } else {
          // --flag (boolean)
          result[key] = true;
        }
      }
    } else if (token.startsWith('-') && token.length > 1) {
      // Short option(s): -n, -n value, -abc (multiple flags)
      const chars = token.slice(1);

      if (chars.length === 1) {
        // Single short option
        const next = tokens[i + 1];
        if (next && !next.startsWith('-')) {
          result[chars] = next;
          i++;
        } else {
          result[chars] = true;
        }
      } else {
        // Multiple flags: -abc -> a=true, b=true, c=true
        // Unless last char has a value following
        for (let j = 0; j < chars.length; j++) {
          const char = chars[j];
          if (j === chars.length - 1) {
            // Last char might have a value
            const next = tokens[i + 1];
            if (next && !next.startsWith('-')) {
              result[char] = next;
              i++;
            } else {
              result[char] = true;
            }
          } else {
            result[char] = true;
          }
        }
      }
    } else {
      // Positional argument
      result._.push(token);
    }
  }

  return result;
}

const ORDER_COMMANDS = new Set([
  'buy',
  'sell',
  'close',
  'stop',
  'be',
  'trail',
  'move',
  'scale',
  'bump',
  'chase',
  'twap',
  'swarm',
  'nuke',
  'cancel',
  'leverage',
  'kill',
]);
/** A token parseArgs would drop, or that would make it drop the next one. */
export function firstDashToken(input: string): string | undefined {
  return tokenize(input).find((token) => token.length > 1 && token.startsWith('-'));
}

const BUMP_SIDE_FLAGS = new Set(['--buys', '-b', '--sells', '-s']);

/**
 * parseArgs drops a dash token and the token after it, so a command that can
 * change an order refuses them rather than acting on what is left. Same forms
 * as the Rust exec router.
 */
export function refusedOrderToken(commandName: string, input: string): string | undefined {
  if (!ORDER_COMMANDS.has(commandName)) return undefined;
  const tokens = tokenize(input);
  const refused = tokens.find(
    (token, index) =>
      token.length > 1 &&
      token.startsWith('-') &&
      !(commandName === 'bump' && index === tokens.length - 1 && BUMP_SIDE_FLAGS.has(token)),
  );
  return refused === undefined
    ? undefined
    : `${commandName}: refusing "${refused}": flags and negative values are not supported, nothing was placed`;
}

/**
 * Tokenize input string, respecting quotes
 */
function tokenize(input: string): string[] {
  const tokens: string[] = [];
  let current = '';
  let inQuote: string | null = null;

  for (let i = 0; i < input.length; i++) {
    const char = input[i];

    if (inQuote) {
      if (char === inQuote) {
        // End quote
        inQuote = null;
      } else {
        current += char;
      }
    } else if (char === '"' || char === "'") {
      // Start quote
      inQuote = char;
    } else if (char === ' ' || char === '\t') {
      // Whitespace - end current token
      if (current) {
        tokens.push(current);
        current = '';
      }
    } else {
      current += char;
    }
  }

  // Push final token
  if (current) {
    tokens.push(current);
  }

  return tokens;
}

/**
 * Get a positional argument by index
 */
export function getPositional(args: ParsedArgs, index: number): string | undefined {
  return args._[index];
}

/**
 * Get an option value by one or more names
 */
export function getOption(args: ParsedArgs, ...names: string[]): string | undefined {
  for (const name of names) {
    const value = args[name];
    if (typeof value === 'string') {
      return value;
    }
  }
  return undefined;
}

/**
 * Check if a flag is set
 */
export function hasFlag(args: ParsedArgs, ...names: string[]): boolean {
  return names.some((name) => args[name] === true);
}
