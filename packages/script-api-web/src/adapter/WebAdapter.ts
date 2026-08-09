import type { QuickSizeSlot, WebComponents, WebDomHotkey, WebReact } from '../api/WebApi.types';

/**
 * `WebAdapter` — what `apps/web` implements so the kernel can build a
 * `WebApi`. The `CoreAdapter` analogue (see
 * `@tealstreet/script-api-core`'s `AdapterContext`), and it lives here for
 * the same reason: the *contract* is portable, the *implementation* is not.
 *
 * The implementation cannot live in this package. It needs the jotai store,
 * `exchangeAccountController`, and `apps/web` atoms — importing those would
 * invert the dependency direction. See
 * `docs/custom-scripts/CONVERGENCE-AUDIT-v1.md` §B3.
 *
 * Note the return types: the host's underlying resolvers
 * (`UITokenResolvers` from `@tealstreet/cli-core`) hand back **token
 * strings** such as `"$100"` or `"50000"`, because the CLI's substitute
 * layer requires complete tokens rather than bare numbers. This adapter is
 * the seam where those become numbers for script authors.
 */
export interface WebAdapter {
  /** Last chart-click price. Host parses the numeric token string. */
  getClick(): number | undefined;
  /** Order-form size as USD notional. Host strips the `$` token prefix. */
  getOrderSize(): number | undefined;
  /** Quick-button size as USD notional, or `undefined` for non-`$` units. */
  getQuickSize(slot: QuickSizeSlot): number | undefined;

  readonly domHotkey: WebDomHotkey;
  readonly react: WebReact;
  readonly components: WebComponents;
}
