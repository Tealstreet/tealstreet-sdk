// REPL commands for the hosted webhook relay: `webhook [status]`,
// `webhook claim`, `webhook release`. Pattern mirrors hook-repl.ts.

import type { StandalonePlatformAdapter } from '../platform-adapter';
import { isRemoteExecEnabled } from './config';
import { getWebhookRelay } from './webhook-relay';
import { BRAND } from '../brand';

function relayOrExplain(adapter: StandalonePlatformAdapter) {
  const relay = getWebhookRelay();
  if (relay) return relay;
  if (!isRemoteExecEnabled()) {
    adapter.printError(`Webhooks are off: listener.remoteExec is false in ~/${BRAND.configDirName}/config.json.`);
  } else {
    adapter.printError(`Webhooks are off: listener.enabled is false in ~/${BRAND.configDirName}/config.json.`);
  }
  return null;
}

export async function cmdWebhookStatus(adapter: StandalonePlatformAdapter): Promise<void> {
  const relay = relayOrExplain(adapter);
  if (!relay) return;
  if (relay.connection === 'unauthorized') {
    adapter.printWarning('Webhooks are waiting for you to sign in: run `login`.');
    return;
  }
  if (relay.connection !== 'connected') {
    adapter.printWarning(`Webhook relay is ${relay.connection}.`);
    return;
  }
  const state = await relay.requestState();
  if (!state) {
    adapter.printError('Could not read webhook state from the relay.');
    return;
  }
  const holder = state.listeners.find((l) => l.listener_id === state.claimed_listener_id);
  if (!state.claimed_listener_id) {
    adapter.printWarning('No listener receives your webhooks. Run `webhook claim` to take them here.');
  } else if (relay.holdsClaim) {
    adapter.printSuccess('This CLI receives your webhooks.');
  } else {
    adapter.printInfo(`Webhooks go to: ${holder?.label ?? state.claimed_listener_id}`);
  }
  adapter.printInfo(`Alerts wait ${state.hold_seconds}s for an offline listener, then expire unrun.`);
  for (const l of state.listeners) {
    const mark = l.listener_id === state.claimed_listener_id ? '*' : ' ';
    const me = l.listener_id === relay.listenerId ? ' (this CLI)' : '';
    const accounts = l.accounts.length > 0 ? l.accounts.join(', ') : 'no accounts';
    adapter.print([`  ${mark} ${l.label}${me}  [${l.kind}, ${l.online ? 'online' : 'offline'}]  ${accounts}`]);
  }
}

export async function cmdWebhookClaim(adapter: StandalonePlatformAdapter): Promise<void> {
  const relay = relayOrExplain(adapter);
  if (!relay) return;
  const status = await relay.claim();
  if (status === 'ok') adapter.printSuccess('This CLI now receives your webhooks.');
  else adapter.printError(`Could not take the webhooks: ${status}.`);
}

export async function cmdWebhookRelease(adapter: StandalonePlatformAdapter): Promise<void> {
  const relay = relayOrExplain(adapter);
  if (!relay) return;
  const status = await relay.release();
  if (status === 'ok') adapter.printSuccess('Released: no listener receives your webhooks until one is claimed.');
  else if (status === 'not_holder') adapter.printError('This CLI does not hold the webhooks.');
  else adapter.printError(`Could not release the webhooks: ${status}.`);
}
