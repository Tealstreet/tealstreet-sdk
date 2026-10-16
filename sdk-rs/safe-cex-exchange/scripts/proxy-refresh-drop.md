# Proxy refresh outer timer cancellation

The outer `ProxyTokenClient::schedule_refresh` timer has a pre-spawn
`RefreshTimerCompletion`, independent of the token fetch's `FlightCompletion`.
Dropping an unpolled or sleeping TASK releases its timer slot and host handle.
Cleanup advances only the matching generation: an inline-completed task cannot
install its returned stale handle, and an old task cannot clear a replacement.
Host handle destruction runs after releasing the state borrow.

`proxy_token::refresh_drop_tests` drops TASK futures rather than a waiting
caller, checks exact handle destruction, restartability and Weak state release,
and exercises inline completion and obsolete-generation destruction.

Three compiling mutations were observed failing at exit 101, then restored:

- Moving the guard inside async fails
  `refresh_timer_task_drop_releases_slot_unpolled_and_pending` at
  `discarded refresh TASK must release timer slot`.
- Disabling the generation comparison fails
  `old_refresh_task_drop_cannot_clear_replacement_timer` because the replacement
  timer disappears.
- Removing generation invalidation fails
  `inline_refresh_completion_does_not_install_stale_handle` at its named stale
  handle assertion.

This is native cancellation ownership, not a correction to TypeScript timer
arithmetic or refresh timing. The original source lifecycle remains the authority.
