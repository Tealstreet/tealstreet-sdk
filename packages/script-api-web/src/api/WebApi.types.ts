/**
 * `WebApi` — the web-runtime extension tier. Mounted by `apps/web` only and
 * unreachable from the standalone CLI. See SPEC §2.3.
 *
 * Two deliberate deviations from the SPEC §2.3 sketch, both load-bearing:
 *
 * 1. **`react` and `components` are declared but EMPTY, and currently carry
 *    no autocomplete.** Read this before assuming otherwise.
 *
 *    Typing them here as `typeof import('react')` /
 *    `ComponentType<ButtonProps>` would pull `@types/react` into
 *    `scripts/generate-monaco-types.ts`'s dts-bundle-generator output, which
 *    emits a triple-slash reference directive for the types library. The
 *    bundler wraps that output in `declare module
 *    '@tealstreet/script-api-web' { … }`, and such a directive inside a
 *    module block is invalid TypeScript — it would corrupt the seeded
 *    `~/.tealstreet/scripts/types.d.ts` for every user. Enumerating the
 *    component map is also impossible from here: the real components live in
 *    `apps/web`, and importing them would invert the dependency direction
 *    (see `docs/custom-scripts/CONVERGENCE-AUDIT-v1.md`).
 *
 *    **A host-side `declare module` augmentation does NOT fix this.**
 *    `WEB_TYPES_DTS` is generated from this package's `src/index.ts` alone,
 *    so an augmentation living in `apps/web` never reaches the bundle: a
 *    script author gets `components: {}` and
 *    "Property 'Button' does not exist". Two things must land together
 *    before these fields are useful, both belonging to the web-editor epic:
 *
 *      - the bundle generator must be taught to concatenate a host-supplied
 *        augmentation slice, and
 *      - that augmentation must target the *declaration*, not `index.ts`'s
 *        re-exported alias — merging into an alias symbol is not supported,
 *        and this repo has no precedent for it (every existing
 *        `declare module` here is an ambient shim for an untyped module).
 *
 *    Until then these exist for shape fidelity with SPEC §2.3 and to keep
 *    `WebApi` stable, nothing more. Do not document them to users as working.
 *
 * 2. **`ui.*` is synchronous and returns numbers.** SPEC §2.3 sketched
 *    `Promise<number | undefined>`, but the real resolvers
 *    (`UITokenResolvers` in `@tealstreet/cli-core`, implemented at
 *    `apps/web/src/components/cli/uiTokenResolvers.ts`) are sync and return
 *    *token strings* like `"$100"`. Making this async would be pretending.
 *    `WebAdapter` is where the string → number conversion happens.
 */

/** Empty on purpose and NOT yet augmentable — see the note above. */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface WebReact {}

/** Empty on purpose and NOT yet augmentable — see the note above. */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface WebComponents {}

/** `$quick<n>` button slots, n in 1..7. */
export type QuickSizeSlot = 1 | 2 | 3 | 4 | 5 | 6 | 7;

/**
 * UI-token resolvers. Each returns `undefined` when no value is available
 * right now (no chart click yet, empty order form, unset quick button) —
 * callers must treat that as "missing", not as zero.
 */
export interface WebUi {
  /** Last chart-click price, in the symbol's quote currency. */
  getClick(): number | undefined;
  /**
   * Size from the linked order form, as **USD notional** — not contracts.
   * Resolves the first non-zero of buy/sell market then buy/sell limit size
   * and converts via the current ticker price.
   */
  getOrderSize(): number | undefined;
  /**
   * Quick-size button value as **USD notional**. Returns `undefined` for a
   * button whose unit is not `$` / `k` / `m` (e.g. a percent-sized button),
   * since those have no notional meaning without a position.
   */
  getQuickSize(slot: QuickSizeSlot): number | undefined;
}

/** What a DOM hotkey can be bound to. */
export type DomTarget = HTMLElement | Document | Window;

export interface WebDomHotkey {
  /**
   * Register `cb` on `target` and return an unsubscribe function.
   *
   * Callers MUST invoke the returned function. A script that registers a DOM
   * listener and is then re-run — the editor re-transpiles on save — would
   * otherwise stack a second live listener on the same target, so one
   * keypress fires the handler N times after N edits. The kernel cannot
   * clean these up on the script's behalf; this is the only mechanism.
   */
  register(target: DomTarget, cb: () => void): DomHotkeyUnsubscribe;
}

/** Returned by {@link WebDomHotkey.register}; call it to detach. */
export type DomHotkeyUnsubscribe = () => void;

export interface WebApi {
  readonly react: WebReact;
  readonly components: WebComponents;
  readonly ui: WebUi;
  readonly domHotkey: WebDomHotkey;
}
