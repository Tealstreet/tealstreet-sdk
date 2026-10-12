// handleHookFire (HTTP) is the POST /hook/<id> trigger; hooks are created and
// managed from the REPL (hook-repl.ts). Hook fire reuses the #4
// ALS adapter-context machinery so command output streams as cmd:output
// frames on the 'hook-output' topic. We also mirror frames locally so the
// foreground REPL user sees their hook fires too — distinct from cmd:exec
// which is one-way (user typed the command at the REPL prompt, no value
// in echoing it).

import crypto from 'crypto';
import chalk from 'chalk';
import { logger, originalConsoleLog } from '../../logger';
import { runWithAdapterContext, type PrintKind } from '../adapter-context';
import { isRemoteExecEnabled, isListenerEnabled } from '../config';
import { verifyHookToken } from '../hooks-store';
import type { ProcessCommandFn } from './cmd';

export const HOOK_OUTPUT_TOPIC = 'hook-output';

type ServerLike = { publish(topic: string, data: string): number };

export interface HookFireResult {
  status: number;
  body: Record<string, unknown>;
}

const LOCAL_PREFIX_BY_KIND: Record<PrintKind, (text: string) => string> = {
  print: (t) => t,
  success: (t) => chalk.green(t),
  warn: (t) => chalk.yellow(t),
  error: (t) => chalk.red(t),
  info: (t) => chalk.gray(t),
};

export async function handleHookFire(
  server: ServerLike,
  hookId: string,
  providedToken: string,
  processCommand: ProcessCommandFn,
): Promise<HookFireResult> {
  // Defensive: should never hit if listener was disabled (no bind), but
  // covers the case where the gate is flipped between bind and fire.
  if (!isListenerEnabled()) {
    return { status: 503, body: { ok: false, error: 'listener disabled' } };
  }
  const verify = verifyHookToken(hookId, providedToken);
  if (!verify.ok) {
    return verify.reason === 'unknown'
      ? { status: 404, body: { ok: false, error: 'unknown hook' } }
      : { status: 401, body: { ok: false, error: 'invalid token' } };
  }
  if (!isRemoteExecEnabled()) {
    return { status: 423, body: { ok: false, error: 'remoteExec disabled' } };
  }
  const hook = verify.hook;
  const runId = crypto.randomBytes(16).toString('hex');

  // Per-frame sink: WS broadcast + local terminal mirror so the foreground
  // REPL user sees their hook fires too (unlike cmd:exec which is one-way).
  const printSink = (kind: PrintKind, text: string) => {
    try {
      server.publish(HOOK_OUTPUT_TOPIC, JSON.stringify({ t: 'cmd:output', id: runId, kind, text, hookId: hook.id }));
    } catch (err) {
      logger.warn(`hook-output publish failed: ${(err as Error).message}`);
    }
    try {
      originalConsoleLog(`[hook:${hook.name}] ${LOCAL_PREFIX_BY_KIND[kind](text)}`);
    } catch {
      /* ignore local mirror failures */
    }
  };

  // Fire-and-forget the actual command. HTTP returns 200 immediately so
  // webhook callers (tradingview etc.) don't sit on a long REST request.
  // `hook.account` (when set) scopes the run via the ALS accountOverride —
  // `processCommand` resolves the exchange from `exchangeManager.getExchange`
  // instead of the REPL's `session.currentAccountName`.
  void runWithAdapterContext({ printSink, accountOverride: hook.account, source: 'webhook' }, async () => {
    try {
      await processCommand(hook.command);
      server.publish(HOOK_OUTPUT_TOPIC, JSON.stringify({ t: 'cmd:done', id: runId, success: true, hookId: hook.id }));
    } catch (err) {
      server.publish(
        HOOK_OUTPUT_TOPIC,
        JSON.stringify({
          t: 'cmd:done',
          id: runId,
          success: false,
          error: (err as Error).message,
          hookId: hook.id,
        }),
      );
    }
  });

  return {
    status: 200,
    body: { ok: true, runId, command: hook.command, ...(hook.account ? { account: hook.account } : {}) },
  };
}
