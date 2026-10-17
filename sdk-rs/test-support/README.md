# Virtual test runtime wake contracts

Deadline-based fixture sleeps register the latest task waker and wake it when
virtual time reaches the deadline. The registry retains weak waiter slots, so
dropping a pending future cancels its wake registration. Runtime pumps deliver
due wakes before polling jobs and honor driver self-wakes within a bounded
scheduling loop.

Shared HTTP release gates retain one weak registration per request and wake every
live waiter on release. Their futures retain the gate state independently; a
cancelled request leaves no wake target. These helpers supply scheduling and
scripted I/O to the production venue bindings; venue algorithms and assertions
remain in the owning crates.

Harnesses include the shared helpers as inline modules because accepted-set tests
can include one harness repeatedly inside distinct fixture scopes. Each inclusion
gets its own helper types without redeclaring the same file module.
