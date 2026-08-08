import type { DomTarget } from '../api/WebApi.types';

/**
 * Runtime guard for {@link DomTarget}.
 *
 * Deliberately NOT a type predicate. A duck-type on `addEventListener` also
 * matches `WebSocket`, `XMLHttpRequest`, `AbortSignal`, `MediaQueryList`,
 * SVG elements and text nodes, so narrowing to `HTMLElement | Document |
 * Window` on that basis would be a lie the compiler then trusts. Callers that
 * need a real narrowing should use `instanceof` against their own realm.
 */
export const canAddEventListener = (value: unknown): boolean =>
  typeof value === 'object' &&
  value !== null &&
  typeof (value as { addEventListener?: unknown }).addEventListener === 'function';

/** Whether `value` is a DOM target in *this* realm. Cross-realm (iframe)
 *  objects fail this check even when they are valid targets there. */
export const isDomTarget = (value: unknown): value is DomTarget => {
  if (!canAddEventListener(value)) return false;
  if (typeof window !== 'undefined' && (value === window || value === window.document)) return true;
  return typeof Node !== 'undefined' && value instanceof Node && value.nodeType === Node.ELEMENT_NODE;
};
