# Four unused State forwarders

The authored Rust workspace has no calls or function references to these four
ExchangeState methods: detect_balance_issue, is_post_only_rejection,
get_response_header and observe_request_weight. Their only Rust occurrences were
the declarations and direct forwarding bodies. The constructor's native receiver
record installs name/options/clients, not these members. The separate inverse
Scope methods with camel-case names do not use these State methods.

The live request-weight interceptor in ExchangeState::install_guards already
calls request_weight::{observe_request_weight,get_response_header} directly.
Context::capture_post_only already calls order_logic::is_post_only_rejection_error
directly. The deprecated balance predicate remains in its canonical free helper.
Those helpers and their bindings are unchanged; no replacement implementation
or new exemption is introduced. The four source manifest rows retain their
status and now name only the existing canonical helpers, with explicit rename
notes and corrected binding descriptions.

The actual default compiler inventory has the same 229 Binance owners before
and after. State has 20 APIs before and 16 after: exactly the four named methods
are absent. Every other API, raw-input role and generic signature is identical.
Full discovery still includes 197 shared Deserialize owners. State remains
unclassified pending its complete live-method receipt cohort; this cleanup does
not turn that expected failure into a passing classification.

Validation uses the existing native_dependencies/terminal HTTP fixtures:

- private_classes::production_native_request_weight_callbacks_observe_success_and_rejection:
  five actual signed-client success/rejection outcomes versus maintained TS/Axios
  guards, including current header access and reconciliation pause state.
- private_classes::private_order_aliases::native_batch_capabilities_classify_rejections_and_log_close_conflicts:
  real batch response keeps accepted IDs, suppresses post-only refusals and emits
  the ordinary row error; the actual order/Store conflict path remains active.
- accepted_predicate_roles: existing deprecated-false and post-only code/message
  precedence assertions over the canonical helpers.

This is removal of uncalled forwarding APIs, not a behavior change to the live
bindings or an unrestricted JavaScript equivalence claim. Existing typed-output,
ASCII header-name and JSON/error-carrier boundaries remain unchanged.
