# DELEGATED memory

`delegated_memory::DelegatedMemory::prepare(venue, enabled)` requires the
caller's resolved per-venue DELEGATED policy flag. Memory preparation is the
same for every venue and adds no separate venue allowlist. Call it before
receiving or opening key material. It does not enable DELEGATED admission or
change account construction. The handover and account-build changes consume
this helper separately.

Preparation disables core dumps for the process (`RLIMIT_CORE=0`, and
`PR_SET_DUMPABLE=0` on Linux). Failure refuses preparation. The settings remain
for the process lifetime. Ordinary startup and disabled DELEGATED declarations
do not call the process controls. Only Bybit is currently opted in by the
production declaration.

`zeroed(len)` creates a candidate buffer that an opener can fill directly.
`copy(Vec<u8>)` consumes an existing buffer and zeroizes that input on every exit.
`CandidateCopy` and `ResidentCopy` name the same owned byte-buffer type. Move a
verified candidate into the account, or use `try_clone()` when an independent
resident copy is needed. Byte access is explicit; formatting is redacted, and
the type provides no serializer or implicit clone.

Each buffer owns separate anonymous pages. It attempts `mlock` before receiving
bytes; unavailable locking emits one fixed, non-fatal warning per process.
Drop zeroizes the entire allocation before unlocking and unmapping it, on both
normal and error paths. No file or persistence API is involved. The existing
local signing provider and its secret types remain unchanged.

## Container runtime

The existing image runs as UID/GID `65532:65532`, listens on an unprivileged
port, and needs no writable runtime paths. Its binary, entrypoint, healthcheck,
CA bundle, and shutdown behavior are unchanged. A read-only root filesystem is
a runtime setting, rather than a Dockerfile instruction. Operators can retain
the existing environment and port settings and add:

```text
--read-only --cap-drop=ALL --security-opt=no-new-privileges --ulimit core=0:0
```

No `IPC_LOCK` capability is required: locking uses the runtime's memory-lock
allowance and remains non-fatal when that allowance is exhausted or unavailable.
No deployment, image publishing, or host configuration is performed here.
