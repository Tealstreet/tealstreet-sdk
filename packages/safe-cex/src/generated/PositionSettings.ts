// Generated from safe-cex-types/src/store_model.rs. Do not edit.
import type { MarginType, PositionMode, PositionSide } from '../types';
/**
 * Last reported per-position settings, separate from current exposure.
 */
export type PositionSettings = {
    readonly id: string;
    readonly symbol: string;
    readonly side: PositionSide;
    readonly leverage: number;
    readonly marginType?: MarginType;
    readonly positionMode?: PositionMode;
};
