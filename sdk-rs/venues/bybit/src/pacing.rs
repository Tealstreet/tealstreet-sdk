//! Account-wide physical release budgets; no venue-owned timers or retry authority.
use safe_cex_engine::request_context::{Pacing, PacingSet};

/// Restore the legacy transport's maxRPS=5 for single writes.
pub(crate) const WRITE: Pacing = Pacing {
    group: 2,
    minimum_gap_ms: 200.0,
};
/// One order's share of the 1200ms ten-order batch floor. Bybit counts the
/// orders inside a batch, so a batch's floor scales with its order count.
const BATCH_ORDER_GAP_MS: f64 = 120.0;
/// The batch budget different commands share; each set below raises its floor.
pub(crate) const BATCH: Pacing = Pacing {
    group: 3,
    minimum_gap_ms: BATCH_ORDER_GAP_MS,
};
/// Orders per batch request the floor can charge (Bybit's non-VIP batch limit).
const BATCH_ORDERS: usize = 10;

macro_rules! batch_set {
    ($orders:literal) => {
        PacingSet {
            group: 10 + $orders,
            constraints: &[
                WRITE,
                Pacing {
                    group: BATCH.group,
                    minimum_gap_ms: BATCH_ORDER_GAP_MS * $orders as f64,
                },
            ],
        }
    };
}
/// A plan whose largest batch carries `n` orders uses `BATCH_SETS[n - 1]`.
pub(crate) const BATCH_SETS: [PacingSet; BATCH_ORDERS] = [
    batch_set!(1),
    batch_set!(2),
    batch_set!(3),
    batch_set!(4),
    batch_set!(5),
    batch_set!(6),
    batch_set!(7),
    batch_set!(8),
    batch_set!(9),
    batch_set!(10),
];

const fn batch_write(index: usize) -> Pacing {
    Pacing {
        group: BATCH_SETS[index].group,
        minimum_gap_ms: 0.0,
    }
}
pub(crate) const ADMITTED: [Pacing; 1 + BATCH_ORDERS] = [
    WRITE,
    batch_write(0),
    batch_write(1),
    batch_write(2),
    batch_write(3),
    batch_write(4),
    batch_write(5),
    batch_write(6),
    batch_write(7),
    batch_write(8),
    batch_write(9),
];

/// The pacing for a plan whose largest batch request carries `orders` orders;
/// an unknown or larger count is charged the whole ten-order floor.
pub(crate) fn batch_write_for(orders: Option<usize>) -> Pacing {
    let orders = orders.map_or(BATCH_ORDERS, |n| n.clamp(1, BATCH_ORDERS));
    ADMITTED[orders]
}
