/** Immediate fake route for canned replies; never a production transport fallback. */
export const prepareRecordedHttp = function (request) {
  const now = () => this.now();
  const send = this.request.bind(this);
  const original = new URL(request.url);
  const expiresAt = now() + (request.timeoutMs ?? 20_000);
  let cancelled = false;
  let bound = false;
  return {
    expiresAt,
    cancel: () => {
      cancelled = true;
    },
    bind(packet) {
      if (
        bound ||
        cancelled ||
        packet.request !== request.request ||
        packet.method !== request.method ||
        new URL(packet.url).origin !== original.origin ||
        packet.route !== request.route
      )
        throw new Error('Recorded HTTP preparation mismatch');
      bound = true;
      const snapshot = { ...packet, headers: { ...packet.headers } };
      let sent = false;
      const ready = () =>
        !sent && !cancelled && !request.signal?.aborted && !packet.signal?.aborted && now() < expiresAt;
      return {
        ready,
        cancel: () => {
          cancelled = true;
        },
        dispatch() {
          if (!ready()) throw new Error('Recorded HTTP resource unavailable');
          sent = true;
          return send(snapshot);
        },
      };
    },
  };
};
