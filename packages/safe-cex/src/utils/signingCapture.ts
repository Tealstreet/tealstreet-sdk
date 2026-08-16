/**
 * Marker for a request that is dispatched only to CAPTURE its signed config,
 * never to reach the network.
 *
 * `BaseExchange.signHttpRequest` runs a real `xhr.request` with a per-request
 * adapter that resolves `{ status: 200 }` without sending anything. Adapters sit
 * INSIDE axios dispatch, so every interceptor still fires — which means anything
 * counting responses sees a successful REST call that never happened. In hosted
 * mode this is once per signature.
 *
 * The flag is explicit rather than inferred from the presence of a per-request
 * adapter: a phantom count is invisible once written, so the thing that
 * suppresses it should be greppable from both sides.
 *
 * Deliberately a plain property on the axios config, not a header — it must
 * never be serialized onto the wire.
 */
export const SIGNING_CAPTURE_FLAG = '__tealstreetSigningCapture';

export const isSigningCaptureRequest = (config: unknown): boolean =>
  Boolean((config as Record<string, unknown> | null | undefined)?.[SIGNING_CAPTURE_FLAG]);
