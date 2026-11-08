// Generated from safe-cex-types/src/store_model.rs. Do not edit.
import type { MarginType, PositionMode } from '../types';
export type FetchedLeverage = {
    /**
     * @ts-mutable
     */
    buyLeverage: number;
    /**
     * @ts-mutable
     */
    sellLeverage: number;
    /**
     * @ts-mutable
     */
    marginType?: MarginType;
    /**
     * @ts-mutable
     */
    positionMode?: PositionMode;
    /**
     * @ts-mutable
     */
    isFetching?: boolean;
};
