import { bindGroup } from './managed-http-group.mjs';

/** Closed transport preparation errors; arbitrary provider text never crosses the wire. */
export class HttpPreparationError extends Error {
  constructor(reason) {
    super(reason === 'timedOut' ? 'HTTP transport request timed out' : 'HTTP transport preparation failed');
    this.code = `HTTP_PREPARATION_${reason.replace(/[A-Z]/g, (letter) => `_${letter}`).toUpperCase()}`;
  }
}
function preparationReason(error, fallback) {
  switch (error?.code) {
    case 'HTTP_PREPARATION_UNSUPPORTED': return 'unsupported';
    case 'HTTP_PREPARATION_TIMED_OUT': return 'timedOut';
    case 'HTTP_PREPARATION_STALE': return 'stale';
    case 'HTTP_PREPARATION_INVALID': return 'invalid';
    case 'HTTP_PREPARATION_NETWORK': return 'network';
    default: return fallback;
  }
}

// Host-owned route resources. Engine tokens remain strings through every hop.
// No asynchronous work is allowed between claim and bound.dispatch().
export class ManagedHttp {
  constructor(host) {
    this.host = host;
    this.resources = new Map();
    this.next = 0n;
  }

  live(token, resource) {
    return !this.host.disposed && this.resources.get(token) === resource;
  }

  cancel(token) {
    if (this.groupHandoff) throw new Error('HTTP group handoff cannot be interrupted');
    const resource = this.resources.get(token);
    if (!resource) return;
    this.resources.delete(token);
    this.host.stageCauses.delete(`prepare:${token}`);
    resource.controller.abort();
    if (resource.wake !== undefined) this.host.transport.clearTimeout(resource.wake);
    try { resource.bound?.cancel?.(); } catch { /* Keep releasing the preparation. */ }
    try { resource.prepared?.cancel?.(); } catch { /* No resource cleanup can suppress engine settlement. */ }
  }

  dispose() {
    if (this.groupHandoff) throw new Error('HTTP group handoff cannot be interrupted');
    for (const token of [...this.resources.keys()]) {
      try { this.cancel(token); } catch { /* Continue releasing independent resources. */ }
    }
  }

  fail(token, resource, code) {
    if (!this.live(token, resource)) return;
    this.cancel(token);
    const { host } = this;
    host.run(host.invoke(host.engine.httpPreparationFailed, token, code, host.transport.now()));
  }

  execute(action) {
    if (this.groupHandoff) throw new Error('HTTP group handoff cannot be interrupted');
    if (action.phase === 'bindGroup') { bindGroup(this, action, preparationReason); return; }
    const token = action.preparation;
    if (typeof token !== 'string') throw new Error('Invalid HTTP preparation token');
    if (action.phase === 'cancel') {
      if (action.signing !== undefined) {
        const pending = this.host.primitiveSignings.get(action.signing);
        if (pending) {
          this.host.primitiveSignings.delete(action.signing);
          this.host.transport.clearTimeout(pending.timer);
          pending.controller.abort();
        }
      }
      this.cancel(token); return;
    }
    if (action.phase === 'prepare') { this.host.ownStage(`prepare:${token}`); void this.prepare(action); return; }
    if (action.phase !== 'bind') throw new Error('Invalid managed HTTP phase');
    const resource = this.resources.get(token);
    if (!resource || resource.phase !== 'prepared') return;
    const { host } = this;
    const cause = host.stageCauses.get(`prepare:${token}`) ?? host.stageCause;
    if (cause !== undefined && host.commands.get(cause)?.onStage) host.stageCauses.set(`http:${action.packet.request}`, cause);
    try {
      if (typeof action.permit !== 'string') throw new Error('Invalid HTTP permit');
      resource.bound = resource.prepared.bind({ ...action.packet, headers: Object.fromEntries(action.packet.headers), signal: resource.controller.signal });
      if (!resource.bound || typeof resource.bound.ready !== 'function' || typeof resource.bound.dispatch !== 'function') throw new Error('Invalid bound HTTP resource');
      resource.phase = 'bound';
    } catch (error) { this.fail(token, resource, preparationReason(error, 'invalid')); return; }
    this.claim(token, resource, action.permit, action.packet.request);
  }

  async prepare(action) {
    if (this.groupHandoff) throw new Error('HTTP group handoff cannot be interrupted');
    const { host } = this;
    const token = action.preparation;
    if (this.resources.has(token) || host.disposed
      || !host.invoke(host.engine.httpPreparationPending, token)) return;
    const resource = { controller: new AbortController(), phase: 'preparing' };
    this.resources.set(token, resource);
    if (typeof host.transport.prepareHttp !== 'function') { this.fail(token, resource, 'unsupported'); return; }
    try {
      const prepared = await host.transport.prepareHttp({ ...action.packet, headers: Object.fromEntries(action.packet.headers), intent: action.intent, signal: resource.controller.signal });
      if (!this.live(token, resource) || !host.invoke(host.engine.httpPreparationPending, token)) {
        prepared?.cancel?.();
        if (this.live(token, resource)) this.cancel(token);
        return;
      }
      resource.prepared = prepared;
      if (!prepared || typeof prepared.bind !== 'function') { this.fail(token, resource, 'invalid'); return; }
      if (this.next === 0xffff_ffff_ffff_ffffn) { this.fail(token, resource, 'invalid'); return; }
      resource.phase = 'prepared';
      const id = String(++this.next);
      host.runFor(host.stageCauses.get(`prepare:${token}`), host.invoke(host.engine.httpPrepared, token, id, prepared.expiresAt, host.transport.now()));
    } catch (error) { this.fail(token, resource, preparationReason(error, 'network')); }
  }

  claim(token, resource, permit, request) {
    if (!this.live(token, resource)) return;
    const { host } = this;
    let response;
    try {
      if (!resource.bound.ready()) { this.fail(token, resource, 'timedOut'); return; }
      const result = JSON.parse(host.invoke(host.engine.httpClaim, permit, host.transport.now()));
      if (result.status === 'wait' && Number.isFinite(result.until)) {
        resource.wake = host.transport.setTimeout(() => {
          resource.wake = undefined;
          this.claim(token, resource, permit, request);
        }, Math.max(0, result.until - host.transport.now()));
        return;
      }
      if (result.status !== 'released' && result.status !== 'denied') { this.fail(token, resource, 'invalid'); return; }
      if (result.status === 'denied') {
        // Claim may have queued cancel/reprepare/failure, but never drain before
        // a successful handoff. Denial consumes no physical-send timestamp.
        this.cancel(token);
        host.run(host.invoke(host.engine.prepareDispatch, host.transport.now()));
        return;
      }
      resource.phase = 'released';
      response = resource.bound.dispatch(); // Synchronous transport API handoff.
      host.stage(host.stageCauses.get(`http:${request}`), 'sent');
    } catch (error) {
      if (resource.phase === 'released' && this.live(token, resource)) {
        this.post(() => host.run(host.invoke(host.engine.httpFailed, request, 'HTTP transport request failed', host.transport.now())));
        this.cancel(token);
      } else this.fail(token, resource, preparationReason(error, 'invalid'));
      return;
    }
    // Attach rejection handling before a diagnostic listener can throw.
    void this.receive(token, resource, request, response);
    this.post(() => host.run(host.invoke(host.engine.prepareDispatch, host.transport.now())));
  }

  // A post-release observer cannot change send evidence or strand response work.
  post(callback) {
    try { callback(); }
    catch {
      try { this.host.handlers.onLog?.('warning', 'HTTP post-release callback failed'); }
      catch { /* Callback failures never reclassify an already released attempt. */ }
    }
  }

  async receive(token, resource, request, response) {
    const { host } = this;
    try {
      const value = await response;
      if (this.live(token, resource)) await host.receive(request, value, () => this.live(token, resource));
    } catch {
      if (this.live(token, resource)) this.post(() => host.run(host.invoke(host.engine.httpFailed, request, 'HTTP transport request failed', host.transport.now())));
    } finally {
      if (this.live(token, resource)) this.cancel(token);
    }
  }
}

/** Fetch has no asynchronous route setup. Bind validates/builds before claim. */
export function prepareFetch(request, fetcher = globalThis.fetch) {
  if (request.route === 'publicCors') throw new Error('Public CORS request routing is not configured');
  if (request.intent === 'secondary' && request.route !== 'direct') throw new HttpPreparationError('unsupported');
  const original = new URL(request.url);
  const deadline = Date.now() + (request.timeoutMs ?? 20_000);
  let bound = false;
  return {
    bind(packet) {
      const url = new URL(packet.url);
      if (bound || packet.request !== request.request || packet.method !== request.method
        || url.origin !== original.origin || packet.route !== request.route) throw new Error('HTTP preparation mismatch');
      const signal = AbortSignal.any([packet.signal, AbortSignal.timeout(Math.max(1, deadline - Date.now()))].filter(Boolean));
      const init = { method: packet.method, headers: { ...packet.headers }, body: packet.body, signal, redirect: 'error' };
      new Request(url, init); // Validate the exact immutable dispatch packet now.
      bound = true;
      let sent = false;
      return {
        ready: () => !sent && !signal.aborted && Date.now() < deadline,
        dispatch() {
          if (sent || signal.aborted || Date.now() >= deadline) throw new Error('HTTP preparation expired');
          sent = true;
          // Start fetch before constructing the response continuation.
          const response = fetcher(url.href, init);
          return Promise.resolve(response).then((value) => {
            const lines = [];
            value.headers.forEach((v, name) => lines.push(`${name}: ${v}`));
            return { status: value.status, headers: lines.join('\n'), body: responseChunks(value.body) };
          });
        },
      };
    },
  };
}

async function* responseChunks(stream) {
  if (!stream) return;
  const reader = stream.getReader();
  let complete = false;
  try { for (;;) { const { done, value } = await reader.read(); if (done) { complete = true; return; } yield value; } }
  finally { if (!complete) await reader.cancel().catch(() => {}); reader.releaseLock(); }
}
