// Commands a webhook may not run. A hosted webhook body is text from whoever
// holds the webhook URL, so it gets trading verbs and nothing that changes the
// terminal itself: credentials, other access paths, safety limits, persistent
// vars/aliases/triggers a later typed command would silently pick up, or the
// file system. Shared by the standalone CLI and the web app's webhook listener.

import { commandRegistry } from '../commands/CommandRegistry';
import { ensureCommandsRegistered } from '../commands/CommandExecutor';
import { parseDuration } from '../preprocess/sleep';

// Canonical names. Registered commands are resolved through the registry first,
// so an alias (`sim`, `use`, `acc`, `sym`) is refused with its command. The
// standalone-only REPL verbs are not in the registry and are listed directly.
const REFUSED_FROM_WEBHOOK = new Set([
  // standalone REPL verbs
  'login',
  'logout',
  'auth',
  'whoami',
  'update',
  'exit',
  'quit',
  'clients',
  'hook',
  'hooks',
  'webhook',
  'logs',
  'log',
  'clearlogs',
  'audit',
  'footer',
  'script',
  'scripts',
  'watch',
  'wa',
  'wp',
  'wo',
  'wm',
  'export',
  'import',
  // registered commands (canonical names)
  'accounts',
  'account',
  'symbol',
  'alias',
  'unalias',
  'set',
  'sets',
  'unset',
  'track',
  'untrack',
  'when',
  'fatfinger',
  'unfatfinger',
  'budget',
  'whitelist',
  'unwhitelist',
  'simulation',
  'history',
  // `wait` blocks the CLI's command queue for up to ten minutes per step.
  'wait',
]);

/** The refusal message for a webhook-run step, or null when it may run. */
export function webhookRefusal(command: string): string | null {
  // `script:<name>` RUNS a loaded script and is allowed; bare `script ...`
  // manages them and is not. Splitting on whitespace keeps the two apart.
  const trimmed = command.trim();
  // The web worker's router re-splits a step on every `;` (quotes included)
  // and treats a leading `@` as a command, so either would let a refused verb
  // ride inside an allowed one. Neither belongs in a preprocessed step.
  if (trimmed.includes(';') || trimmed.startsWith('@')) {
    return 'A webhook step may not contain ";" or start with "@".';
  }
  const verb = trimmed.split(/\s+/, 1)[0]?.toLowerCase() ?? '';
  if (!verb) return null;
  ensureCommandsRegistered();
  const canonical = commandRegistry.find(verb)?.name.toLowerCase() ?? verb;
  if (!REFUSED_FROM_WEBHOOK.has(canonical) && !REFUSED_FROM_WEBHOOK.has(verb)) return null;
  return `"${verb}" cannot be run from a webhook.`;
}

const MAX_WEBHOOK_STEPS = 50;
const MAX_WEBHOOK_SLEEP_MS = 60_000;

/**
 * Refuses a webhook BODY before it is preprocessed. Nested `repeat` multiplies
 * (1000 x 1000 x 1000 steps) and the expansion is built in memory before any
 * step can be checked, so a body may carry at most one.
 */
export function webhookBodyRefusal(body: string): string | null {
  const repeats = body.match(/\brepeat\b/gi)?.length ?? 0;
  return repeats > 1 ? 'A webhook may use "repeat" at most once.' : null;
}

/**
 * Refuses a preprocessed webhook CHAIN that would hold the listener for long:
 * the CLI runs webhooks on the same queue as typed input, so a body of sleeps
 * would freeze the user's own commands behind it.
 */
export function webhookChainRefusal(steps: ReadonlyArray<{ command: string; block?: boolean }>): string | null {
  if (steps.length > MAX_WEBHOOK_STEPS) return `A webhook may run at most ${MAX_WEBHOOK_STEPS} steps.`;
  let sleptMs = 0;
  for (const step of steps) {
    // Case-insensitive and regardless of `block`: the preprocessor only flags
    // a lowercase `sleep`, but the command registry runs `SLEEP` all the same.
    const sleep = step.command.trim().match(/^sleep\s+(.+)$/i);
    if (!sleep) continue;
    try {
      sleptMs += parseDuration(sleep[1]);
    } catch {
      return `"${step.command}" is not a valid sleep.`;
    }
  }
  return sleptMs > MAX_WEBHOOK_SLEEP_MS ? 'A webhook may sleep at most 60 seconds in total.' : null;
}
