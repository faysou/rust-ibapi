use super::{encoders, verify};
use crate::builder::ValidationError;
use crate::client::id_generator::ClientIdManager;
use crate::contracts::Contract;
use crate::orders::{LinkedOrdersError, Order, OrderId};
use crate::Error;

/// The place-order requests for a linked list, in send order, each with its order's id. Sets
/// `transmit` on the last order only, then checks and reserves every order's ids and encodes
/// it, so a list that fails a check fails with nothing sent: an empty list, an order id of 0
/// (the `Order` default, never set) or an id used twice.
pub(crate) fn encode_linked_orders(
    ids: &ClientIdManager,
    server_version: i32,
    orders: Vec<(Contract, Order)>,
) -> Result<Vec<(OrderId, Vec<u8>)>, Error> {
    if orders.is_empty() {
        return Err(Error::InvalidArgument("submit_linked_orders needs at least one order".into()));
    }
    let mut seen = std::collections::HashSet::new();
    for (_, order) in &orders {
        let reason = if order.order_id == 0 {
            "each linked order needs its own order id; 0 is the unset default"
        } else if !seen.insert(order.order_id) {
            "each linked order needs its own order id; this one is used twice"
        } else {
            continue;
        };
        return Err(Error::Validation(ValidationError::InvalidField {
            field: "Order::order_id",
            value: order.order_id.to_string(),
            reason: reason.into(),
        }));
    }
    let last = orders.len().saturating_sub(1);
    orders
        .into_iter()
        .enumerate()
        .map(|(i, (contract, mut order))| {
            order.transmit = i == last;
            let order_id = verify::verify_and_reserve_order_ids(ids, server_version, OrderId::from(order.order_id), &order)?;
            let request = encoders::encode_place_order(order_id.value(), &contract, &order)?;
            Ok((order_id, request))
        })
        .collect()
}

impl LinkedOrdersError {
    /// A failure before anything was sent.
    pub(crate) fn unsent(error: Error) -> Self {
        Self::after_sends(error, Vec::new())
    }

    /// A failure after the writes for `sent` completed, before any cancel.
    pub(crate) fn after_sends(error: Error, sent: Vec<OrderId>) -> Self {
        Self {
            error,
            sent,
            cancelled: Vec::new(),
            cancel_failures: Vec::new(),
        }
    }

    /// Records the outcome of cancelling `order_id`.
    pub(crate) fn record_cancel<T>(&mut self, order_id: OrderId, outcome: Result<T, Error>) {
        match outcome {
            Ok(_) => self.cancelled.push(order_id),
            Err(error) => self.cancel_failures.push((order_id, error)),
        }
    }
}
