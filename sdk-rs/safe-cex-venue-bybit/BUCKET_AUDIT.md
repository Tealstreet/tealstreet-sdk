# Bybit divergence buckets

At the October 13 checkpoint, the venue's 520 rows remain 105 ported, 24
divergent and 391 pending; four journal rows remain pending. No relocation-only
row was promoted. The reviewed notes already name observable differences, not
merely new Rust paths. Fifteen notes receive opening buckets; status, rs, kind,
layer and all other parsed TOML fields remain unchanged.

| Rows | Opening bucket | Existing behavioral witness |
|---|---|---|
| Linear/inverse history fallback (2) | CHOSEN | `history-fixtures`: truthy non-string output fields and surrogate-containing fee JSON differ; source substring status bug remains preserved. |
| Public subscribe, private subscribe, diagnostic send (3) | FINDING | `check-store-notification`: no-socket send reaches reconnect/health, commits state then skips later subscribers; source unawaited rejection is not represented by the unit-returning notification contract. |
| Public onMessage (1) | MIXED | `ws-public-fixtures`: inherited dispatch effects, raw numeric and UTF-16 callback boundaries are separate. |
| Linear/inverse/spot ticker handler (3) | MIXED | `ticker-event-fixtures` plus notification witness: native nonfinite writes are retained, inherited key membership and notification error behavior still differ. |
| Inverse/spot parseTickers (2) | CHOSEN | `check-exchange-state`: inherited market lookup returns source rows missing id/symbol; owned Rust market map skips them. |
| Inverse mapPosition (1) | MIXED | `inverse-position-fixtures` and composed fetch witness: function/object side versus whole-record refusal, typed symbols, and deferred source-error priority. |
| bybitInstrumentClass, shared-store target (1) | FINDING | Existing store instrument oracle/tests retain venueLabel and downstream undefined copy. They do not establish equivalence between a callable/prototype object and Rust's single PrototypeProperty marker. No faithful native-value or timing claim is made. |
| reinitOnThrow, newer row (1) | CHOSEN | Constructor oracle plus dropped-request witness: source unawaited rejection sets the flag, cancelled Rust operation does not. |
| Inverse fetchPositionsHelper, newer row (1) | MIXED | 125 live composed cases retain later errors, requests, logging, clocks and reinit before refusing typed output; inherited output restriction remains. |

The thirteen coordinator-listed rows retain four CHOSEN, five MIXED and four
FINDING classifications. The two newer rows add one CHOSEN and one MIXED.
Total edited openings: CHOSEN 5, MIXED 6, FINDING 4. Zero rows became ported;
net venue status delta is 0/0/0.

The three ticker handler rows share exactly one direct Store write in
`ws_public::handle_tickers_event`. Its conversion explicitly binds
`StorePatchError`; it cannot silently compile through a notification error-type
change. The existing notification FINDING remains separate from the native
numeric patch repair. Indirect notifications retain their reachability inventory
in STORE_NOTIFICATION_AUDIT.md.
