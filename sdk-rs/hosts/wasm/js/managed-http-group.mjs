// Group resources are built and checked before the shared synchronous claim.
export function bindGroup(manager, action, reason) {
  const { host } = manager;
  const members = action.members;
  if (typeof action.permit !== 'string' || !Array.isArray(members) || members.length < 2 || members.length > 256) throw new Error('Invalid HTTP group action');
  if (typeof host.engine.httpGroupPending !== 'function' || typeof host.engine.httpClaimGroup !== 'function') {
    failGroup(manager, members, 'unsupported'); return;
  }
  if (!host.invoke(host.engine.httpGroupPending, action.permit)) return;
  const entries = new Array(members.length);
  const tokens = new Set();
  try {
    for (let i = 0; i < members.length; i++) {
      const member = members[i];
      if (typeof member.preparation !== 'string' || tokens.has(member.preparation)) throw new Error('Invalid HTTP group member');
      tokens.add(member.preparation);
      const resource = manager.resources.get(member.preparation);
      if (!resource || (member.packet ? resource.phase !== 'prepared' : resource.phase !== 'bound')) throw new Error('Missing HTTP group resource');
      entries[i] = { token: member.preparation, resource, request: member.packet?.request ?? resource.groupRequest, response: undefined, failed: false };
    }
    for (let i = 0; i < members.length; i++) {
      const member = members[i];
      const { resource } = entries[i];
      if (member.packet) {
        resource.bound = resource.prepared.bind({ ...member.packet, headers: Object.fromEntries(member.packet.headers), signal: resource.controller.signal });
        resource.phase = 'bound';
        resource.groupRequest = member.packet.request;
      }
      if (!resource.bound || typeof resource.bound.ready !== 'function' || typeof resource.bound.dispatch !== 'function') throw new Error('Invalid bound HTTP group resource');
    }
  } catch (error) { const code = reason(error, 'invalid'); failGroup(manager, members, code === 'stale' ? 'invalid' : code); return; }
  const group = { permit: action.permit, entries, bounds: new Float64Array(entries.length),
    notifications: Array.from({ length: 256 }, () => ({ callback: undefined, value: undefined })),
    notificationCount: 0, notificationFatal: false, notificationError: new Error('HTTP group transport notification cannot be retained') };
  claimGroup(manager, group);
}

function failGroup(manager, members, code) {
  const { host } = manager;
  const member = members.find((member) => typeof member.preparation === 'string');
  if (!member) throw new Error('Invalid HTTP group failure token');
  const resource = manager.resources.get(member.preparation);
  if (resource) manager.fail(member.preparation, resource, code);
  else host.run(host.invoke(host.engine.httpPreparationFailed, member.preparation, code, host.transport.now()));
}

function claimGroup(manager, group) {
  const { host } = manager;
  if (host.disposed || !host.invoke(host.engine.httpGroupPending, group.permit)) return;
  const bounds = group.bounds;
  if (!(bounds instanceof Float64Array) || bounds.length !== group.entries.length
    || !(bounds.buffer instanceof ArrayBuffer) || bounds.buffer.resizable === true
    || bounds.byteLength !== bounds.length * Float64Array.BYTES_PER_ELEMENT) throw new Error('Invalid HTTP group output storage');
  let status = 0;
  let beforeClaim;
  let claimStarted = false;
  let fatal;
  // External invoke/run/dispose/cleanup/cancel refuse while this guard is set.
  group.notificationCount = 0;
  group.notificationFatal = false;
  manager.groupHandoff = group;
  try {
    for (const entry of group.entries) {
      try {
        if (!manager.live(entry.token, entry.resource) || entry.resource.phase !== 'bound' || !entry.resource.bound.ready()) {
          beforeClaim = 'timedOut'; break;
        }
      } catch { beforeClaim = 'invalid'; break; }
    }
    if (!beforeClaim) {
      claimStarted = true;
      // Narrow internal entry; general host.invoke remains closed to reentry.
      status = host.engine.httpClaimGroup(group.permit, host.transport.now(), bounds);
      if (host.engine.failed?.()) throw new Error('Engine failed during group claim');
      if (status === 1) {
        for (let i = 0; i < group.entries.length; i++) {
          const entry = group.entries[i];
          entry.resource.phase = 'released';
          try {
            const rawNow = host.transport.now();
            const sendBefore = bounds[i];
            if (!Number.isFinite(rawNow) || !Number.isFinite(sendBefore) || rawNow >= sendBefore) entry.failed = true;
            else entry.response = entry.resource.bound.dispatch();
          } catch { entry.failed = true; }
        }
      }
    }
  } catch (error) { fatal = error; }
  finally { manager.groupHandoff = false; }
  if (group.notificationFatal) {
    host.failEngine(group.notificationError);
    return;
  }
  if (fatal && claimStarted) { host.failEngine(fatal); return; }
  for (let i = 0; i < group.notificationCount; i++) {
    const slot = group.notifications[i];
    const callback = slot.callback;
    const value = slot.value;
    slot.callback = undefined;
    slot.value = undefined;
    if (!host.disposed) {
      try { callback(value); }
      catch (error) { host.failEngine(error); return; }
    }
  }
  if (host.disposed) return;
  if (fatal) {
    if (claimStarted) { host.failEngine(fatal); return; }
    beforeClaim = 'invalid';
  }
  if (beforeClaim) {
    failGroup(manager, group.entries.map(({ token }) => ({ preparation: token })), beforeClaim); return;
  }
  if (status === 2) {
    const first = group.entries[0].resource;
    const at = bounds[0];
    if (!Number.isFinite(at)) { host.failEngine(new Error('Invalid HTTP group wait')); return; }
    first.wake = host.transport.setTimeout(() => {
      first.wake = undefined;
      claimGroup(manager, group);
    }, Math.max(0, at - host.transport.now()));
    return;
  }
  if (status === 0) {
    // A stale member's siblings keep their exact bound resources.
    manager.post(() => host.run(host.invoke(host.engine.prepareDispatch, host.transport.now())));
    return;
  }
  if (status !== 1) { host.failEngine(new Error('Invalid HTTP group claim status')); return; }
  // All synchronous handoffs precede every engine failure/response callback.
  for (const entry of group.entries) {
    if (entry.failed) {
      manager.post(() => host.run(host.invoke(host.engine.httpFailed, entry.request, 'HTTP transport request failed', host.transport.now())));
      manager.cancel(entry.token);
    } else void manager.receive(entry.token, entry.resource, entry.request, entry.response);
  }
  manager.post(() => host.run(host.invoke(host.engine.prepareDispatch, host.transport.now())));
}
