

### Legacy nonce-domain handoff: engine-only prerequisite

| Boundary | Custody / activation gate |
| --- | --- |
| Other processes or WASM memories | The in-memory store covers one process/module only. A complete owned-generation coordinator remains required across memories; local import is no cross-process rollout proof. |
| External issuers | Other applications sharing a credential remain outside the owned census. No proof of remote non-execution is required. A definitive native invalid-nonce refusal grants no write replay. |
| Raw old entry calls | Historically untracked source or target work refuses takeover. A new empty owner table cannot attest old prepared/provider/transport work. Raw calls into a fenced/retired namespace refuse, including stale signer instances. |
| No host rollout | This PR changes no venue table, LocalSigner implementation, native/WASM host forwarding or worker lifecycle. Real same-store provider proof and original source AND target owner joins remain separately reviewed activation prerequisites. |
