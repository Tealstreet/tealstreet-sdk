import type {
  CoinbaseStarbaseAuthResult,
  CoinbaseStarbaseHeartbeatSignal,
  CoinbaseStarbaseJsonRpcClient,
  CoinbaseStarbaseTestResult,
} from './coinbase.starbase';
import type { CoinbaseStarbaseCancelOnDisconnectScope } from './coinbase.starbase-session';

import {
  CoinbaseStarbaseAuthSession,
  CoinbaseStarbaseHeartbeatController,
  CoinbaseStarbaseProtocolError,
} from './coinbase.starbase';
import {
  buildCoinbaseStarbaseDisableCancelOnDisconnect,
  buildCoinbaseStarbaseEnableCancelOnDisconnect,
  buildCoinbaseStarbaseGetCancelOnDisconnect,
  decodeCoinbaseStarbaseCancelOnDisconnectMutationResult,
  decodeCoinbaseStarbaseGetCancelOnDisconnectResult,
} from './coinbase.starbase-session';

/**
 * Inert lifecycle controller for a future Coinbase Starbase WebSocket.
 *
 * This module deliberately owns neither a socket nor a wire-frame classifier.
 * Coinbase publishes heartbeat names and channel payloads, but not the unified
 * socket's outer inbound notification envelope. A future transport must pass
 * already-classified heartbeat signals and perform subscription replay through
 * the injected callback.
 */

export type CoinbaseStarbaseWebsocketSessionState =
  'idle' | 'activating' | 'ready' | 'disconnected' | 'rate-limit-backoff' | 'failed';

export type CoinbaseStarbaseWebsocketDisconnectReason = 'heartbeat-timeout' | 'rate-limit' | 'transport';

export type CoinbaseStarbaseCancelOnDisconnectPolicy = {
  enabled: boolean;
  scope?: CoinbaseStarbaseCancelOnDisconnectScope;
};

type CoinbaseStarbaseWebsocketSessionControllerOptions = {
  authSession: CoinbaseStarbaseAuthSession;
  cancelOnDisconnect?: CoinbaseStarbaseCancelOnDisconnectPolicy;
  client: CoinbaseStarbaseJsonRpcClient;
  heartbeat: CoinbaseStarbaseHeartbeatController;
  replaySubscriptions: () => Promise<void>;
};

export class CoinbaseStarbaseWebsocketSessionController {
  private activation?: Promise<void>;
  private activationGeneration = 0;
  private readonly authSession: CoinbaseStarbaseAuthSession;
  private readonly cancelOnDisconnect?: CoinbaseStarbaseCancelOnDisconnectPolicy;
  private readonly client: CoinbaseStarbaseJsonRpcClient;
  private readonly heartbeat: CoinbaseStarbaseHeartbeatController;
  private readonly replaySubscriptions: () => Promise<void>;

  state: CoinbaseStarbaseWebsocketSessionState = 'idle';

  constructor(options: CoinbaseStarbaseWebsocketSessionControllerOptions) {
    if (options.client.kind !== 'websocket') {
      throw new CoinbaseStarbaseProtocolError('Starbase WebSocket session requires a WebSocket JSON-RPC client');
    }

    this.authSession = options.authSession;
    this.cancelOnDisconnect = options.cancelOnDisconnect;
    this.client = options.client;
    this.heartbeat = options.heartbeat;
    this.replaySubscriptions = options.replaySubscriptions;
  }

  activate(): Promise<void> {
    if (this.state === 'ready') return Promise.resolve();
    if (this.activation) return this.activation;
    if (this.state === 'rate-limit-backoff') {
      return Promise.reject(
        new CoinbaseStarbaseProtocolError('Starbase rate-limit backoff must be acknowledged before reconnect'),
      );
    }

    const generation = ++this.activationGeneration;
    this.state = 'activating';
    const activation = this.activateGeneration(generation);
    this.activation = activation;
    const clearActivation = () => {
      if (this.activation === activation) this.activation = undefined;
    };
    void activation.then(clearActivation, clearActivation);
    return activation;
  }

  async maintainAuthentication(): Promise<CoinbaseStarbaseAuthResult> {
    this.assertReady('refresh authentication');
    return this.authSession.authenticate();
  }

  async handleClassifiedHeartbeat(
    signal: CoinbaseStarbaseHeartbeatSignal,
  ): Promise<CoinbaseStarbaseTestResult | undefined> {
    this.assertReady('handle a heartbeat');
    return this.heartbeat.handleSignal(signal);
  }

  recordDisconnect(reason: CoinbaseStarbaseWebsocketDisconnectReason): void {
    this.activationGeneration += 1;
    this.activation = undefined;
    this.state = reason === 'rate-limit' ? 'rate-limit-backoff' : 'disconnected';
  }

  acknowledgeRateLimitBackoff(): void {
    if (this.state !== 'rate-limit-backoff') {
      throw new CoinbaseStarbaseProtocolError('Starbase session is not waiting for rate-limit backoff');
    }
    this.state = 'disconnected';
  }

  private async activateGeneration(generation: number): Promise<void> {
    try {
      // A WebSocket authentication is connection-local. Never reuse the auth
      // session's cached result when bringing up a new socket.
      await this.authSession.authenticate(true);
      this.assertCurrentActivation(generation);

      await this.heartbeat.enable();
      this.assertCurrentActivation(generation);

      if (this.cancelOnDisconnect) {
        await this.applyCancelOnDisconnect(this.cancelOnDisconnect);
        this.assertCurrentActivation(generation);
      }

      await this.replaySubscriptions();
      this.assertCurrentActivation(generation);
      this.state = 'ready';
    } catch (error) {
      if (generation === this.activationGeneration) this.state = 'failed';
      throw error;
    }
  }

  private async applyCancelOnDisconnect(policy: CoinbaseStarbaseCancelOnDisconnectPolicy): Promise<void> {
    const mutation = policy.enabled
      ? buildCoinbaseStarbaseEnableCancelOnDisconnect(policy.scope)
      : buildCoinbaseStarbaseDisableCancelOnDisconnect(policy.scope);
    const mutationResult = await this.client.request(mutation.method, mutation.params);
    decodeCoinbaseStarbaseCancelOnDisconnectMutationResult(mutationResult);

    const readback = buildCoinbaseStarbaseGetCancelOnDisconnect(policy.scope);
    const current = decodeCoinbaseStarbaseGetCancelOnDisconnectResult(
      await this.client.request(readback.method, readback.params),
    );
    const expectedScope = policy.scope ?? 'connection';
    if (current.enabled !== policy.enabled || current.scope !== expectedScope) {
      throw new CoinbaseStarbaseProtocolError('Starbase cancel-on-disconnect readback did not match policy');
    }
  }

  private assertCurrentActivation(generation: number): void {
    if (generation !== this.activationGeneration || this.state !== 'activating') {
      throw new CoinbaseStarbaseProtocolError('Starbase WebSocket activation was superseded by a disconnect');
    }
  }

  private assertReady(action: string): void {
    if (this.state !== 'ready') {
      throw new CoinbaseStarbaseProtocolError(`Starbase WebSocket session must be ready to ${action}`);
    }
  }
}
