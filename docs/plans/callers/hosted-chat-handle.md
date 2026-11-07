# Hosted journals: public chat handle beside user tag

NX must add an optional `chatHandle` string claim to the existing RS256
`proxyToken` JWT used to open `/account`. Its value must come exclusively from
`chat_users.username` for that token's authenticated `userId`, for example:

```json
{"userId":"auth0|opaque-user","chatHandle":"Trader-063249","exp":1794000000}
```

Keep the existing token transport (`proxyToken`/`token` WebSocket query),
signature and expiration rules. Do not send the handle as a query parameter or
in `init`, and do not send email, account names, profile display names or other
identity fields. Resolve it in the NX token issuer, not from a client-supplied
handle. Missing chat profiles should omit the claim.

The NX web/native token endpoint is
`apps/web/src/app/api/proxykey/route.ts`; it calls the synchronous signer in
`apps/web/src/app/api/utils/generatePrivateProxyAccessToken.ts`. After the
existing cookie/Bearer authentication succeeds, query only:

```ts
const { data, error } = await supabase
  .from('chat_users')
  .select('username')
  .eq('user_id', session.user.sub)
  .maybeSingle();
```

Use the existing server-only Supabase client. Pass a validated result as an
optional argument to the signer and add it to the payload before `jwt.sign`.
Keep signing synchronous; put the lookup in the async endpoint. Missing rows,
invalid values or lookup errors omit `chatHandle` and still issue the normal
token. Do not create/repair chat profiles in the authentication path. The CLI
issuers `apps/web/src/app/api/cli/login/route.ts` and
`apps/web/src/app/api/cli/token/route.ts` currently sign their own payloads;
apply the same lookup and optional claim there for CLI sessions too. Existing
cached tokens remain tag-only until refreshed; a rename is reflected by a
newly minted token.

The server accepts only 1–32 ASCII letters, digits, `_` or `-`. It rejects the
whole handle, without trimming, escaping into another name or truncating, if
it is empty, oversized, non-string or contains whitespace, controls, Unicode,
`@`, other punctuation or log delimiters. Invalid or absent handles do not prevent a
valid user from connecting: those sessions keep the tag-only label.

At session start the journal maps the unchanged 12-hex `user_tag` to
`chat=<handle>`. The existing native account label appends the same public
handle alongside the tag, and the existing no-return diagnostic retains it.
No new line logs account names. A reused account keeps its build-time engine
label; every reconnect gets a fresh session mapping, including handle changes
and omission. Authorization, resume ownership, account fingerprints and routing
still use the existing user identity, never the handle.

This SDK PR contains the receiver only. NX must update the JWT issuer before
handles appear, and deaws2 owns exchange-server rollout. No schema change,
chat lookup in the exchange server, deploy or roll is performed here.
