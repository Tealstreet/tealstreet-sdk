// Generated from safe-cex-types/src/store_model.rs. Do not edit.
import type { MarginType, PositionMode } from '../types';
export type FetchedLeverage = {
    buyLeverage: number;
    sellLeverage: number;
    marginType?: MarginType;
    positionMode?: PositionMode;
    isFetching?: boolean;
};
