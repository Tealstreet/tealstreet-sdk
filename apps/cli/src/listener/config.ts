// Centralizes reads for the three listener kill switches in
// ~/.tealstreet/config.json. All default to true if unset. Read on every
// call (no memoization) so a future `set listener.<flag>` REPL command
// flips behavior without a restart. config.json is small; fs cost is
// negligible compared to the cost of users hitting stale state.
//
// Only `isListenerEnabled` and `isRemoteExecEnabled` are used as of this
// commit; `isAccountSyncEnabled` lands here for commit #5.

import { loadConfig } from '../config';

export function isListenerEnabled(): boolean {
  return loadConfig().listener?.enabled ?? true;
}

export function isAccountSyncEnabled(): boolean {
  return loadConfig().listener?.accountSync ?? true;
}

export function isRemoteExecEnabled(): boolean {
  return loadConfig().listener?.remoteExec ?? true;
}
