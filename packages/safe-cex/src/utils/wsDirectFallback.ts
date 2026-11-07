/**
 * Direct-first WebSocket host selection with a sticky fallback CHAIN.
 *
 * Shared by the Bybit, OKX and Bitget private socket classes; the Bitget public
 * classes still carry the same counters inline. A connection
 * that produced ZERO messages is a silent failure. Once `maxSilentConnects`
 * CONSECUTIVE connections have been silent, the socket advances to the next
 * candidate (relay through the private fleet, then the athena passthrough —
 * whatever the caller lists) and stays there; if that candidate is silent for
 * another streak it advances again, and sticks at the last one. Any message at
 * all — a pong, a subscribe ack, even an auth error — proves the current path
 * works and resets the streak, so a wrong API key never trips it.
 *
 * Sticky for the socket's lifetime (a reload starts over), and every user on a
 * fallback shares its egress against the venue's per-IP caps — which is why
 * these must stay fallbacks and never become the default path.
 *
 * The host DATA lives in transport/tealstreet/routing/routingTable.ts (`relay` / `fallback` on a WS
 * channel, attached by the resolver only when the channel resolved DIRECT);
 * this owns only the trigger state.
 */
export class WsDirectFallback {
  private connects = 0;
  private messages = 0;
  private silentConnects: number;
  private activeIndex = 0; // 0 = primary, k = candidates[k - 1]
  private preferLast = false;
  private candidateCount = 0;
  private advancePending = false;

  // Some TS sockets count startup itself as their first silent attempt.
  constructor(
    private readonly maxSilentConnects = 2,
    initialSilentConnects = 0,
  ) {
    this.silentConnects = initialSilentConnects;
  }

  /** A host recovery signal chooses the last declared candidate and stays there. */
  preferFallback(): void {
    this.preferLast = true;
  }

  /**
   * Call exactly once per connect attempt. `fallbackUrls` is the chain to walk
   * in order; `undefined` entries are skipped so callers can pass gated
   * candidates without branching.
   */
  resolveUrl(primaryUrl: string, ...fallbackUrls: Array<string | undefined>): string {
    const candidates = fallbackUrls.filter((url): url is string => typeof url === 'string' && url.length > 0);
    this.candidateCount = candidates.length;
    if (this.advancePending && this.activeIndex < candidates.length) {
      this.activeIndex++;
      this.silentConnects = 0;
      this.advancePending = false;
      this.connects++;
      this.messages = 0;
      return candidates[this.activeIndex - 1];
    }
    this.advancePending = false;
    if (this.connects > 0) {
      this.silentConnects = this.messages === 0 ? this.silentConnects + 1 : 0;
    }
    this.connects++;
    this.messages = 0;
    if (this.preferLast) this.activeIndex = candidates.length;
    if (this.silentConnects >= this.maxSilentConnects && this.activeIndex < candidates.length) {
      this.activeIndex++;
      this.silentConnects = 0;
    }
    return this.activeIndex === 0 ? primaryUrl : (candidates[this.activeIndex - 1] ?? primaryUrl);
  }

  /**
   * The venue REFUSED this path's egress (bitget 40018: the key is bound to other
   * IPs) — a reply, so it never reads as silence. Moves to the next candidate on
   * the following `resolveUrl`; false when none is left to try.
   */
  rejectCurrent = (): boolean => {
    if (this.activeIndex >= this.candidateCount) return false;
    this.advancePending = true;
    return true;
  };

  /** Call from the socket's message handler. */
  onMessage = (): void => {
    this.messages++;
  };

  /**
   * Call when THIS connection produced no messages for a reason that is ours
   * rather than the host's — the only case today is a private socket that never
   * authenticated because the browser's remote signer was gone, and so
   * deliberately sent nothing at all.
   *
   * Without this, that silence reads as "this egress path is broken": two such
   * attempts advance the chain and a couple more stick the socket on the athena
   * passthrough for its lifetime, putting every signer-less user on one shared
   * IP against the venue's per-IP connection cap — the exact outcome the class
   * comment says must never happen. The silence is evidence about our signer,
   * not about the route.
   */
  disregardSilence = (): void => {
    this.messages++;
  };
}
