# Lighter SDK API error construction

The shipped `createLighterSDKClient` installs `LighterApiClient`, whose SDK
failure paths construct `LighterApiError` through the production Realm global.
The previous binding allocated a plain Error-labelled object. It bypassed the
installed Error constructor, exposed an enumerable message, lacked inherited
Error toString, and never executed the optional captureStackTrace hook.

The binding now uses the installed Error superclass with the original message
and new.target. Its prototype inherits Error.prototype. SDK fields initialize in
source order before name/code/details/requestId/statusCode assignments. A truthy
capture property is read again and called with Error as receiver and the original
SDK constructor as the second argument. A weak constructor reference avoids a
self-retaining closure while preserving identity if the global name changes.
The metadata DTO remains the source of name and default code.

Two actual NativeRuntime/production-Realm regressions assemble the real reqwest
HTTP host and invoke the installed SDK error constructor. On master, both compile
and fail: message enumerable=true, and capture call count=0 instead of 1.
Test-only RED commit: d04a0970d8. The repaired binding passes both; the existing
SDK binding group passes 18 tests. The source oracle loads the maintained TS
constructor with TypeScript and Node, observing four constructors plus throwing
and non-callable capture cases. Native controls retain unpaired UTF16, omitted
message versus inherited empty message, field order, details alias, negative
zero, callback receiver/constructor identity, stack mutation, original thrown
identity, and the exact non-callable TypeError. The source oracle runs as part
of the native descriptor regression.

A source scan of every venue crate found no other Error-labelled plain-object
constructor or custom Error constructor binding requiring this cutover. Raw
transport Axios metadata constructors are a different pre-interceptor boundary.

After normally merging master c1ad856ceb, restoring its constructor body still
compiles and fails both named regressions. Exact restoration of the repair passes
all 18 SDK binding tests, including both NativeRuntime constructor witnesses.

The shared Error implementation does not generate an engine call stack or
provide a stock captureStackTrace. The repair preserves supplied diagnostics and
executes a supplied capture hook; it does not invent a V8 stack. This shared OPEN
residual is recorded under shared runtime engine-generated Error stacks in
`.aimux/plans/audits/known-residuals.md`. The constructor manifest remains
divergent for that residual rather than claiming complete engine equivalence.
