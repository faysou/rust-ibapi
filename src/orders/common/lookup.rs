//! Finding one order among this client's open orders and the completed
//! orders, for `find_order`, `find_order_by_perm_id` and `wait_for_fill`.

use std::time::{Duration, Instant};

use crate::client::ids::OrderId;
use crate::orders::{OrderData, OrderLookup, Orders};
use crate::Error;

/// How often a lookup reads completed orders again while neither reply lists
/// the order. A finished order leaves open orders before it reaches completed
/// orders: live, a rejected order was in neither for 19-204 ms after its
/// `Inactive` status. At this interval the record is read about 100 ms after
/// it appears at most, and a lookup sends at most ten requests a second, well
/// under TWS's limit of 50 messages a second.
pub(crate) const LOOKUP_RECHECK_INTERVAL: Duration = Duration::from_millis(100);

/// The order a lookup is for.
#[derive(Clone, Copy, Debug)]
pub(crate) enum OrderKey {
    /// An API order id and the client that placed it: ids are unique only per client.
    Id { client_id: i32, order_id: OrderId },
    /// TWS's permanent id, unique across clients.
    PermId(i64),
}

impl OrderKey {
    /// The key for `perm_id`, checked where it enters the crate. TWS assigns
    /// positive perm ids; a record without one carries `0`, which a key of
    /// `0` or below would match.
    pub(crate) fn perm_id(perm_id: i64) -> Result<Self, Error> {
        if perm_id <= 0 {
            return Err(Error::InvalidArgument(format!("perm id must be positive, got {perm_id}")));
        }
        Ok(OrderKey::PermId(perm_id))
    }

    /// [`OrderLookup::Working`] when an open-orders reply lists the order with
    /// a status that is not terminal. A finished order can still be listed for
    /// a few milliseconds with its final status (live: `Inactive`), which is
    /// not working. Open orders list only this client's orders, so an id
    /// matches without its client.
    pub(crate) fn working(&self, rows: Vec<Orders>) -> Option<OrderLookup> {
        let (mut order, mut status, mut last) = (None, None, None);
        for row in rows {
            match row {
                Orders::OrderData(data) if self.matches_open(data.order.order_id, data.order.perm_id) => {
                    last = Some(data.order_state.status.clone());
                    order = Some(data);
                }
                Orders::OrderStatus(row) if self.matches_open(row.order_id, row.perm_id) => {
                    last = Some(row.status.clone());
                    status = Some(row);
                }
                _ => {}
            }
        }
        last.filter(|kind| !kind.is_terminal()).map(|_| OrderLookup::Working { order, status })
    }

    /// [`OrderLookup::Finished`] when a completed-orders reply lists the
    /// order; the last match wins. An id matches only a record of its client.
    pub(crate) fn finished(&self, rows: Vec<Orders>) -> Option<OrderLookup> {
        rows.into_iter().rev().find_map(|row| match row {
            Orders::OrderData(order) if self.matches_completed(&order) => {
                let status = order.completed_order_status();
                Some(OrderLookup::Finished { order, status })
            }
            _ => None,
        })
    }

    fn matches_open(&self, order_id: i32, perm_id: i64) -> bool {
        match *self {
            OrderKey::Id { order_id: id, .. } => order_id == id.value(),
            OrderKey::PermId(id) => perm_id == id,
        }
    }

    fn matches_completed(&self, data: &OrderData) -> bool {
        match *self {
            OrderKey::Id { client_id, order_id } => data.order.client_id == client_id && data.order.order_id == order_id.value(),
            OrderKey::PermId(id) => data.order.perm_id == id,
        }
    }
}

/// How long to wait at `now` before reading completed orders again, or `None`
/// once `deadline` has passed. The last wait ends at the deadline, so a
/// bound shorter than [`LOOKUP_RECHECK_INTERVAL`] still gets a second read.
/// No deadline (a bound too large to represent) re-checks for good.
pub(crate) fn recheck_delay(deadline: Option<Instant>, now: Instant) -> Option<Duration> {
    match deadline {
        Some(deadline) if now >= deadline => None,
        Some(deadline) => Some(LOOKUP_RECHECK_INTERVAL.min(deadline - now)),
        None => Some(LOOKUP_RECHECK_INTERVAL),
    }
}

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod tests;
