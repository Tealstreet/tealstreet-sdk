// The REPL's hosted-webhook listener: builds the shared cli-core client with
// the CLI's identity, token and command runner. Started from `startRepl` only;
// one-shot commands must never hold this socket open.

import os from 'os';
import {
  WebhookRelayClient,
  type RelayNotice,
  type RelaySocket,
  type RunWebhookBody,
  type WebhookPrintKind,
} from '@tealstreet/cli-core';
import { listAccounts } from '../config';
import { getProxyTokenFromConfig } from '../auth';
import { getCurrentVersion } from '../updater';
import { originalConsoleLog } from '../logger';
import { isListenerEnabled, isRemoteExecEnabled } from './config';
import { getListenerId } from './state';

const mirrorToTerminal = (_kind: WebhookPrintKind, text: string) => {
  try {
    originalConsoleLog(`[webhook] ${text}`);
  } catch {
    /* ignore local mirror failures */
  }
};

let relay: WebhookRelayClient | null = null;
let lastOptions: Parameters<typeof startWebhookRelay>[0] | null = null;

/**
 * Starts the hosted-webhook listener for the REPL. Started even when signed
 * out: the client re-reads the token every minute, so `login` in this REPL
 * brings webhooks up without a restart.
 */
export function startWebhookRelay(opts: {
  runBody: RunWebhookBody;
  notice: RelayNotice;
  createSocket?: (url: string) => RelaySocket;
}): WebhookRelayClient | null {
  if (relay) return relay;
  lastOptions = opts;
  if (!isListenerEnabled() || !isRemoteExecEnabled()) return null;
  relay = new WebhookRelayClient({
    listenerId: getListenerId(),
    kind: 'cli',
    label: `${os.hostname()} CLI`,
    getAccounts: () => listAccounts().map((a) => a.name),
    version: String(getCurrentVersion()),
    getToken: getProxyTokenFromConfig,
    url: process.env.TEALSTREET_NEWS_WS_URL,
    createSocket: opts.createSocket,
    runBody: opts.runBody,
    notice: opts.notice,
    mirror: mirrorToTerminal,
  });
  relay.start();
  return relay;
}

export function stopWebhookRelay(): void {
  relay?.stop();
  relay = null;
}

export function getWebhookRelay(): WebhookRelayClient | null {
  return relay;
}

/** After login/logout: drop the socket and reconnect with the current token, if any. */
export function restartWebhookRelay(): void {
  if (!lastOptions) return;
  const options = lastOptions;
  stopWebhookRelay();
  startWebhookRelay(options);
}
