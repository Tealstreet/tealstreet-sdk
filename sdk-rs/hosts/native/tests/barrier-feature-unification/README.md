# Native barrier feature unification proof

This test package deliberately has its own workspace. The engine dependency
always enables `http-mutation-barriers`, while the native dependency enables
only ordinary `http-plans`. The enclosing workspace's all-features test run
would otherwise unify the native host capability and erase the negative case.

Run both modes from `sdk-rs`:

```sh
cargo test --locked --manifest-path hosts/native/tests/barrier-feature-unification/Cargo.toml
cargo test --locked --manifest-path hosts/native/tests/barrier-feature-unification/Cargo.toml --features host-barriers
```

The same public Host::run fixture admits a genuine two-leg Mutation barrier.
Without host support it requires one typed NotSent and zero bind/readiness/send
calls. With support it requires all bind/readiness checks before two sends,
two replies and one successful terminal. It uses no host-private fields or
fixed event-count drain. The inline provider is lifecycle evidence, not a
family cryptographic or signer-custody readiness proof.
