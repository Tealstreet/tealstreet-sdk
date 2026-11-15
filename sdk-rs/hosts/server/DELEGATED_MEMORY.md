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
do not call the process controls. Shared hosted-venue eligibility remains the
account admission gate; this memory layer adds no per-venue opt-in.

`zeroed(len)` creates a candidate buffer that an opener can fill directly.
`copy(Vec<u8>)` consumes an existing buffer and zeroizes that input on every exit.
`CandidateCopy` and `ResidentCopy` name the same owned byte-buffer type. Move a
verified candidate into the account, or use `try_clone()` when an independent
resident copy is needed. Byte access is explicit; formatting is redacted, and
the byte-buffer type provides no serializer or implicit clone. This opening
buffer is distinct from the canonical signing provider resident described below.

Each buffer owns separate anonymous pages. It attempts `mlock` before receiving
bytes; unavailable locking emits one fixed, non-fatal warning per process.
Drop zeroizes the entire allocation before unlocking and unmapping it, on both
normal and error paths. No file or persistence API is involved. The existing
LOCAL and COSIGNED constructors remain unchanged.

## Canonical resident provider

Native `prepare_protected_local_signer_for` uses the same canonical venue setup
as ordinary local construction. It consumes the uniquely owned provider and
protects all installed key slots, including secondary identities and Coinbase
P-256/Ed25519 credentials. No raw key export API is added. Each protected
allocation uses independent anonymous pages excluded from core dumps, attempts
`mlock`, and zeroizes its entire allocation before release. Locking remains
best-effort and shares the fixed once-per-process warning above; allocation or
dump-protection failures refuse setup rather than falling back to heap storage.
The current allocation backend is Linux; other native platforms refuse protected
setup as unsupported.

`ProtectedProvider` is the separate erasure authority. DELEGATED `ResidentCopy`
clones share that authority; ordinary engine provider handles cannot retain it.
Explicit `erase()` or the last authority drop destroys the installed provider
before returning, even with engine handles outstanding. All such handles,
including previously created but unpolled signing futures, then refuse. Erasure
serializes with synchronous signing, and poisoned state is recovered only to
erase, never to resume. A signature already returned before erasure cannot be
recalled. Pending admission probes retain their resident owner through queued
execution and cleanup. This API does not itself add a retention deadline.

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
