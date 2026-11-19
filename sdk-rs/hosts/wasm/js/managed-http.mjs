import { failureErrorName, requestFailureFields, transportFailureMessage } from './request-failure.mjs';
import { bindGroup } from './managed-http-group.mjs';

/** Closed transport preparation errors; arbitrary provider text never crosses the wire. */
export class HttpPreparationError extends Error {
  constructor(reason, cause) {
    const admission = reason === 'timedOut' && cause === 'info_admission';
    super(admission ? 'HTTP admission deadline expired'
      : reason === 'timedOut' ? 'Relay or network request timed out' : 'HTTP transport preparation failed');
    if (admission) this.cause = 'info_admission';
    this.code = `HTTP_PREPARATION_${reason.replace(/[A-Z]/g, (letter) => `_${letter}`).toUpperCase()}`;
  }
}
function preparationReason(error, fallback) {
  switch (error?.code) {
    case 'HTTP_PREPARATION_UNSUPPORTED': return 'unsupported';
    case 'HTTP_PREPARATION_TIMED_OUT':
      // Admission is explicit; phase/readiness alone cannot prove its cause.
      try {
        if (Object.getOwnPropertyDescriptor(error, 'cause')?.value === 'info_admission') return 'infoAdmissionTimedOut';
      } catch { /* Foreign descriptor traps disclose no cause. */ }
      return 'timedOut';
    case 'HTTP_PREPARATION_STALE': return 'stale';
    case 'HTTP_PREPARATION_INVALID': return 'invalid';
    case 'HTTP_PREPARATION_NETWORK': return 'network';
    default: return fallback;
  }
}

// Numeric ABI follows the engine's repr(u8) RequestClass. Strings and formatting
// stay in the host; labels grant no release, credential or retry authority.
// #3106 admission ABI: keep these codes independent of deadline classes.
const readClasses = Object.freeze(['normal', 'critical', 'bulk', 'reconciliation']);
const requestClasses = Object.freeze(['discovery', 'orderRead', 'orderWrite', 'history', 'read']);
const preparationStages = new Set(['route_resolve', 'proxy_token', 'tunnel', 'transport_prepare', 'catalogue_wait', 'budget_wait', 'prepared', 'sent']);
const routeKinds = new Set(['direct', 'proxy', 'tunnel']);
const duration = (value) => Number.isFinite(value) ? Math.min(3_600_000, Math.max(0, Math.floor(value))) : 0;

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

  assertCurrent(resource) {
    const check = resource.assertCurrent;
    if (!check) return;
    try {
      if (this.host.commandAdmissions.get(resource.command) !== check) throw new Error('Retired command');
      if (check() !== undefined) throw new Error('Invalid owner check');
    } catch {
      // Owner retirement is terminal, not a stale-route reprepare permission.
      throw new HttpPreparationError('invalid');
    }
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

  notePreparation(token, resource, stage, route) {
    if (!this.live(token, resource) || resource.phase === 'released' || !preparationStages.has(stage)) return;
    try {
      const now = this.host.transport.now();
      const elapsed = duration(now - resource.stageAt);
      if (resource.preparationStage === 'budget_wait') resource.budgetWaitMs = duration(resource.budgetWaitMs + elapsed);
      else if (!['prepared', 'sent'].includes(resource.preparationStage)) resource.prepareMs = duration(resource.prepareMs + elapsed);
      resource.stageAt = now;
      resource.preparationStage = stage;
      if (routeKinds.has(route) && (resource.routeKind === undefined || stage === 'route_resolve')) resource.routeKind = route;
    } catch { /* Observation cannot change request settlement. */ }
  }

  preparationFacts(resource) {
    const elapsed = duration(this.host.transport.now() - resource.stageAt);
    return { preparation_stage: resource.preparationStage, error_name: resource.errorName ?? 'other',
      route_kind: resource.routeKind ?? 'unknown',
      budget_wait_ms: duration(resource.budgetWaitMs + (resource.preparationStage === 'budget_wait' ? elapsed : 0)),
      prepare_ms: duration(resource.prepareMs + (!['budget_wait', 'prepared', 'sent'].includes(resource.preparationStage) ? elapsed : 0)) };
  }

  timeoutFacts(resource) {
    return { requestClass: resource.requestClass, budgetMs: resource.budgetMs,
      elapsedMs: Math.max(0, Math.floor(this.host.transport.now() - resource.startedAt)) };
  }

  timeoutMessage(resource) {
    const facts = this.timeoutFacts(resource);
    return `class=${facts.requestClass} budget=${facts.budgetMs}ms elapsed=${facts.elapsedMs}ms`;
  }

  failure(resource, kind, cause) {
    if (resource.failureReported || this.host.disposed) return;
    resource.failureReported = true;
    try {
      this.host.handlers.onEvent?.({ level: 'debug', code: 'http.request_failed',
        message: cause === 'info_admission' ? 'HTTP admission deadline expired'
          : kind === 'timeout' ? `HTTP request timed out (${this.timeoutMessage(resource)})` : 'HTTP request failed',
        fields: { ...resource.diagnostic, ...this.preparationFacts(resource), kind, phase: resource.phase, ...(cause ? { cause } : {}), ...(kind === 'timeout' ? this.timeoutFacts(resource) : {}) } });
    } catch { /* Diagnostic consumers cannot change request settlement or cleanup. */ }
  }

  fail(token, resource, code, error) {
    if (!this.live(token, resource)) return;
    resource.errorName = failureErrorName(error);
    if (!this.live(token, resource)) return;
    const admission = code === 'infoAdmissionTimedOut';
    this.failure(resource, admission || code === 'timedOut' ? 'timeout' : code, admission ? 'info_admission' : undefined);
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
      resource.diagnostic = requestFailureFields(action.packet);
      resource.bound = resource.prepared.bind({ ...action.packet, readClass: resource.readClass, headers: Object.fromEntries(action.packet.headers), signal: resource.controller.signal });
      if (!resource.bound || typeof resource.bound.ready !== 'function' || typeof resource.bound.dispatch !== 'function') throw new Error('Invalid bound HTTP resource');
      resource.phase = 'bound';
    } catch (error) { this.fail(token, resource, preparationReason(error, 'invalid'), error); return; }
    this.claim(token, resource, action.permit, action.packet.request);
  }

  async prepare(action) {
    if (this.groupHandoff) throw new Error('HTTP group handoff cannot be interrupted');
    const { host } = this;
    const token = action.preparation;
    if (this.resources.has(token) || host.disposed
      || !host.invoke(host.engine.httpPreparationPending, token)) return;
    const resource = { readClass: 'normal', controller: new AbortController(), phase: 'preparing',
      diagnostic: requestFailureFields(action.packet), request: action.packet.request,
      startedAt: host.transport.now(), stageAt: host.transport.now(), preparationStage: 'route_resolve',
      prepareMs: 0, budgetWaitMs: 0, budgetMs: action.packet.timeoutMs,
      requestClass: action.packet.method.toUpperCase() === 'GET' ? 'read' : 'orderWrite' };
    this.resources.set(token, resource);
    if (typeof host.transport.prepareHttp !== 'function') { this.fail(token, resource, 'unsupported'); return; }
    try {
      resource.command = host.commandAdmissions?.size && typeof host.engine.httpCommandOwner === 'function'
        ? host.invoke(host.engine.httpCommandOwner, action.packet.request) : undefined;
      if (resource.command !== undefined && (!Number.isSafeInteger(resource.command) || resource.command < 0)) {
        throw new HttpPreparationError('invalid');
      }
      resource.assertCurrent = host.commandAdmissions?.get(resource.command);
      this.assertCurrent(resource);
      const nativeReadClass = typeof host.engine.httpReadClass === 'function'
        ? host.invoke(host.engine.httpReadClass, action.packet.request) : 0;
      const readClass = Number.isInteger(nativeReadClass) ? readClasses[nativeReadClass] ?? 'normal' : 'normal';
      resource.readClass = readClass;
      const classCode = action.requestClass;
      const requestClass = Number.isInteger(classCode) ? requestClasses[classCode] : undefined;
      if (requestClass) resource.requestClass = requestClass;
      const prepared = await host.transport.prepareHttp({ ...action.packet, readClass, headers: Object.fromEntries(action.packet.headers), intent: action.intent, onPreparationStage: (stage, route) => this.notePreparation(token, resource, stage, route), ...(requestClass ? { requestClass } : {}), signal: resource.controller.signal });
      if (!this.live(token, resource) || !host.invoke(host.engine.httpPreparationPending, token)) {
        prepared?.cancel?.();
        if (this.live(token, resource)) this.cancel(token);
        return;
      }
      resource.prepared = prepared;
      this.assertCurrent(resource);
      if (!prepared || typeof prepared.bind !== 'function') { this.fail(token, resource, 'invalid'); return; }
      if (this.next === 0xffff_ffff_ffff_ffffn) { this.fail(token, resource, 'invalid'); return; }
      this.notePreparation(token, resource, 'prepared');
      resource.phase = 'prepared';
      const id = String(++this.next);
      host.runFor(host.stageCauses.get(`prepare:${token}`), host.invoke(host.engine.httpPrepared, token, id, prepared.expiresAt, host.transport.now()));
    } catch (error) { this.fail(token, resource, preparationReason(error, 'network'), error); }
  }

  claim(token, resource, permit, request, admitted = false) {
    if (!this.live(token, resource)) return;
    if (!admitted && resource.bound.admitDispatch) {
      try {
        void Promise.resolve(resource.bound.admitDispatch(() => this.claim(token, resource, permit, request, true)))
          .catch((error) => this.fail(token, resource, preparationReason(error, 'invalid'), error));
      } catch (error) { this.fail(token, resource, preparationReason(error, 'invalid'), error); }
      return;
    }
    const { host } = this;
    let response;
    try {
      if (!resource.bound.ready()) { this.fail(token, resource, 'timedOut'); return; }
      const now = host.transport.now();
      this.assertCurrent(resource);
      const result = JSON.parse(host.invoke(host.engine.httpClaim, permit, now));
      if (result.status === 'wait' && Number.isFinite(result.until)) {
        this.notePreparation(token, resource, 'budget_wait');
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
      this.notePreparation(token, resource, 'sent');
      resource.phase = 'released';
      this.assertCurrent(resource);
      response = resource.bound.dispatch(); // Synchronous transport API handoff.
      host.stage(host.stageCauses.get(`http:${request}`), 'sent');
    } catch (error) {
      if (resource.phase === 'released' && this.live(token, resource)) {
        const message = transportFailureMessage(error);
        if (!this.live(token, resource)) return;
        resource.errorName = failureErrorName(error);
        if (!this.live(token, resource)) return;
        this.failure(resource, 'network');
        this.post(() => host.run(host.invoke(host.engine.httpFailed, request, message, host.transport.now())));
        this.cancel(token);
      } else this.fail(token, resource, preparationReason(error, 'invalid'), error);
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
      if (this.live(token, resource)) await host.receive(request, value, () => {
        this.assertCurrent(resource);
        return this.live(token, resource);
      });
    } catch (error) {
      if (this.live(token, resource)) {
        const message = transportFailureMessage(error);
        if (!this.live(token, resource)) return;
        resource.errorName = failureErrorName(error);
        if (!this.live(token, resource)) return;
        this.failure(resource, 'network');
        this.post(() => host.run(host.invoke(host.engine.httpFailed, request, message, host.transport.now())));
      }
    } finally {
      if (this.live(token, resource)) this.cancel(token);
    }
  }
}

/** Fetch has no asynchronous route setup. Bind validates/builds before claim. */
export function prepareFetch(request, fetcher = globalThis.fetch) {
  try { request.onPreparationStage?.('transport_prepare', 'direct'); }
  catch { /* Observation cannot prevent direct preparation. */ }
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
