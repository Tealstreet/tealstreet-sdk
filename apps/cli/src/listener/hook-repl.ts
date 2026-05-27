// REPL command handlers for hooks. Pattern mirrors clients.ts.

import type { StandalonePlatformAdapter } from '../platform-adapter';
import { loadHooks, createHook, rotateHookSecret, revokeHook, revokeAllHooks, findHook } from './hooks-store';

const LISTENER_BASE_URL = 'http://127.0.0.1:53219';

function printSecretBanner(adapter: StandalonePlatformAdapter, hookId: string, secret: string): void {
  adapter.print([`  POST ${LISTENER_BASE_URL}/hook/${hookId}`, `  X-Hook-Token: ${secret}`]);
}

export function cmdHookList(adapter: StandalonePlatformAdapter): void {
  const hooks = loadHooks();
  if (hooks.length === 0) {
    adapter.printInfo('No hooks.');
    return;
  }
  adapter.printInfo(`Hooks (${hooks.length}):`);
  for (const h of hooks) {
    const acct = h.account ? `[${h.account}]` : '       ';
    const cmd = h.command.length > 60 ? h.command.slice(0, 57) + '...' : h.command;
    adapter.print([`  ${h.id.slice(0, 8)}…  ${h.name.padEnd(20)} ${acct}  ${cmd}`]);
  }
}

// Parses `<name> "<cmd>" [--account <name>]`.
// Quoting is intentionally permissive: " or ' supported, no escapes.
// Anything that doesn't fit the shape returns null and the caller prints help.
function parseCreateArgs(rest: string): { name: string; command: string; account?: string } | null {
  const m = rest.match(/^(\S+)\s+(["'])([^"']+)\2(?:\s+--account\s+(\S+))?\s*$/);
  if (!m) return null;
  const [, name, , command, account] = m;
  return { name, command, account };
}

export function cmdHookCreate(adapter: StandalonePlatformAdapter, rest: string): void {
  const parsed = parseCreateArgs(rest);
  if (!parsed) {
    adapter.printError('Usage: hook create <name> "<command>" [--account <name>]');
    return;
  }
  try {
    const { hook, secret } = createHook(parsed);
    adapter.printSuccess(`Created hook '${hook.name}' (${hook.id.slice(0, 8)}…). Fire with:`);
    printSecretBanner(adapter, hook.id, secret);
    adapter.printInfo(
      'Save the secret now — it will not be shown again. Use `hook show <prefix>` to reveal later, or `hook rotate <prefix>` to mint a new one.',
    );
  } catch (err) {
    adapter.printError(`Failed to create hook: ${(err as Error).message}`);
  }
}

export function cmdHookRevoke(adapter: StandalonePlatformAdapter, target: string | undefined): void {
  if (!target) {
    adapter.printError('Usage: hook revoke <id|prefix|--all>');
    return;
  }
  if (target === '--all') {
    const n = revokeAllHooks();
    adapter.printSuccess(`Revoked ${n} hook(s).`);
    return;
  }
  if (target.length < 8) {
    adapter.printError('Prefix must be at least 8 characters (or pass the full id).');
    return;
  }
  const { removed, matches } = revokeHook(target);
  if (removed.length === 1) {
    adapter.printSuccess(`Revoked ${removed[0].id}.`);
    return;
  }
  if (matches && matches.length > 1) {
    adapter.printError(`Ambiguous prefix matches ${matches.length} hooks:`);
    for (const m of matches) {
      adapter.print([`  ${m.id}  ${m.name}`]);
    }
    return;
  }
  adapter.printError(`No hook matches "${target}".`);
}

export function cmdHookRotate(adapter: StandalonePlatformAdapter, target: string | undefined): void {
  if (!target || target.length < 8) {
    adapter.printError('Usage: hook rotate <id|prefix>  (prefix ≥ 8 chars)');
    return;
  }
  const result = rotateHookSecret(target);
  if ('error' in result) {
    if (result.matches && result.matches.length > 1) {
      adapter.printError(`Ambiguous prefix matches ${result.matches.length} hooks:`);
      for (const m of result.matches) adapter.print([`  ${m.id}  ${m.name}`]);
      return;
    }
    adapter.printError(`No hook matches "${target}".`);
    return;
  }
  adapter.printSuccess(
    `Rotated secret for hook '${result.hook.name}' (${result.hook.id.slice(0, 8)}…). New fire credentials:`,
  );
  printSecretBanner(adapter, result.hook.id, result.secret);
}

export function cmdHookShow(adapter: StandalonePlatformAdapter, target: string | undefined): void {
  if (!target || target.length < 8) {
    adapter.printError('Usage: hook show <id|prefix>  (prefix ≥ 8 chars)');
    return;
  }
  const result = findHook(target);
  if ('error' in result) {
    if (result.matches && result.matches.length > 1) {
      adapter.printError(`Ambiguous prefix matches ${result.matches.length} hooks:`);
      for (const m of result.matches) adapter.print([`  ${m.id}  ${m.name}`]);
      return;
    }
    adapter.printError(`No hook matches "${target}".`);
    return;
  }
  const h = result.hook;
  adapter.printSuccess(`Hook '${h.name}' (${h.id})`);
  adapter.print([
    `  command: ${h.command}`,
    ...(h.account ? [`  account: ${h.account}`] : []),
    `  created: ${new Date(h.createdAt).toISOString()}`,
  ]);
  printSecretBanner(adapter, h.id, h.secret);
}
