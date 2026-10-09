// Runtime gate for offering the tradetunnel's `rpc.relay` as a fallback for a
// PRIVATE exchange socket (see `RelayWebSocket` in @tealstreet/http-over-websocket
// and `relay` on `ResolvedWs`). The host app pushes it from a per-user feature
// flag through '__workerConfig'; safe-cex never sees the flag name. Off by
// default, so a plain deploy changes nothing for anyone: the relay is only
// added to a socket's candidate chain while this is on.
let wsRelayEnabled = false;

export const setWsRelayEnabled = (enabled: boolean): void => {
  wsRelayEnabled = enabled === true;
};

export const isWsRelayEnabled = (): boolean => wsRelayEnabled;
