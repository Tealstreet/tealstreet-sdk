/**
 * Cross-exchange symbol normalization. Re-export of safe-cex's
 * symbolConverter (the cross-trade copier and the script-api both share
 * this single source of truth). See SPEC §2.1 api.utils.
 */
export { convertSymbol } from '@tealstreet/safe-cex/src/utils/symbolConverter';
