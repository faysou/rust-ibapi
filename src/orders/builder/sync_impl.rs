use super::order_builder::{AttachedOrdersBuilder, BracketOrderBuilder, ClientBound, OrderBuilder};
use super::types::{AttachedOrderIds, BracketOrderIds};
use crate::client::sync::Client;
use crate::errors::Error;
use crate::orders::common::what_if::WhatIfReply;
use crate::orders::PlaceOrder;
use crate::orders::{LinkedOrdersError, OrderId};
impl OrderBuilder<ClientBound<'_, Client>> {
    /// Submit the order synchronously
    /// Returns the order ID assigned to the submitted order
    ///
    /// # Errors
    /// [`Error::Validation`] if the builder's inputs fail validation, before anything is sent;
    /// [`Error::OrderIdInRequestRange`] if the client's next order id has reached 1,500,000,000
    /// (reserved for request ids).
    pub fn submit(self) -> Result<OrderId, Error> {
        let ClientBound { client, contract } = self.target;
        let order_id = client.next_order_id();
        let order = self.build()?;
        client.submit_order(order_id, contract, &order)?;
        Ok(OrderId::new(order_id))
    }

    /// Analyze the order for margin and commission (what-if): TWS evaluates it without placing it.
    ///
    /// Returns the order state that carries the margins and commission. TWS answers some orders
    /// (live: US stock limit orders) with a warning-only frame first; the returned state keeps
    /// that frame's [`warning_text`](crate::orders::OrderState::warning_text) (live: 2026-11-24,
    /// server 223; `docs/wire-facts/orders.md#what-if-answer-frames`).
    ///
    /// # Errors
    /// [`Error::Validation`] if the builder's inputs fail validation, before anything is sent;
    /// [`Error::OrderIdInRequestRange`] if the client's next order id has reached 1,500,000,000
    /// (reserved for request ids); the error TWS returns if it rejects the what-if order,
    /// including the 201 for an order the account's margin cannot carry;
    /// [`Error::UnexpectedEndOfStream`] if the reply ends without the order's state.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    /// let state = client.order(&contract).buy(100).limit(150.0).analyze().expect("analyze failed");
    /// println!(
    ///     "initial margin {:?} -> {:?}, commission {:?} {}",
    ///     state.initial_margin_before, state.initial_margin_after, state.commission_and_fees, state.commission_and_fees_currency
    /// );
    /// ```
    pub fn analyze(mut self) -> Result<crate::orders::OrderState, Error> {
        self.what_if = true;
        let ClientBound { client, contract } = self.target;
        let order_id = client.next_order_id();
        let order = self.build()?;

        let responses = client.place_order(order_id, contract, &order)?;

        // `?` propagates a rejected what-if order (#735), including the 201 that follows the
        // margins of an order the account cannot carry.
        let mut reply = WhatIfReply::default();
        for response in responses.iter_data() {
            if let PlaceOrder::OpenOrder(order_data) = response? {
                if order_data.order_id == order_id {
                    if let Some(state) = reply.observe(order_data.order_state) {
                        return Ok(state);
                    }
                }
            }
        }

        Err(Error::UnexpectedEndOfStream)
    }
}

impl BracketOrderBuilder<'_, Client> {
    /// Submit bracket orders synchronously
    /// Returns BracketOrderIds containing all three order IDs
    ///
    /// Reserves three consecutive order ids, then sends the parent and take-profit held and the
    /// stop-loss with `transmit` set, which releases all three, through
    /// [`Client::submit_linked_orders`]. When a send fails, the held orders already sent are
    /// cancelled before the error returns.
    ///
    /// # Errors
    /// [`LinkedOrdersError`] whose [`error`](LinkedOrdersError::error) is
    /// [`Error::Validation`] if the builder's inputs fail validation, or
    /// [`Error::OrderIdInRequestRange`] if the client's next order id has reached 1,500,000,000
    /// (reserved for request ids), both before anything is sent; or the send error, with the
    /// orders sent and cancelled.
    // A failed multi-order submit is rare and dwarfed by its I/O; boxing would only cost callers a deref.
    #[allow(clippy::result_large_err)]
    pub fn submit_all(self) -> Result<BracketOrderIds, LinkedOrdersError> {
        let ClientBound { client, contract } = self.parent_builder.target;
        let orders = self.build().map_err(|e| LinkedOrdersError::unsent(e.into()))?;

        // Reserve all order IDs upfront to prevent collisions
        let parent_id = client.next_order_id();
        let tp_id = client.next_order_id();
        let sl_id = client.next_order_id();
        let reserved_ids = [parent_id, tp_id, sl_id];

        let orders = orders
            .into_iter()
            .zip(reserved_ids)
            .map(|(mut order, order_id)| {
                order.order_id = order_id;
                if order_id != parent_id {
                    order.parent_id = parent_id;
                }
                (contract.clone(), order)
            })
            .collect();
        client.submit_linked_orders(orders)?;

        Ok(BracketOrderIds::new(parent_id, tp_id, sl_id))
    }
}

impl AttachedOrdersBuilder<'_, Client> {
    /// Submit the order with its preset children synchronously.
    ///
    /// Allocates the parent id, then one id per requested child, and sends a single
    /// place-order request. Fire-and-forget: whether TWS attached the children shows up only
    /// on the order update stream — see [`Order::preset_stop_loss_order_id`](crate::orders::Order::preset_stop_loss_order_id).
    ///
    /// # Errors
    /// [`Error::Validation`] if the builder's inputs fail validation, before anything is sent;
    /// [`Error::OrderIdInRequestRange`] if the client's next order id has reached 1,500,000,000
    /// (reserved for request ids).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    /// let ids = client
    ///     .order(&contract)
    ///     .buy(100)
    ///     .limit(150.0)
    ///     .preset_stop_loss()
    ///     .preset_profit_taker()
    ///     .submit()
    ///     .expect("submit failed");
    /// println!("parent {} stop-loss {:?} profit-taker {:?}", ids.parent, ids.stop_loss, ids.profit_taker);
    /// ```
    pub fn submit(self) -> Result<AttachedOrderIds, Error> {
        let ClientBound { client, contract } = self.parent_builder.target;
        let (order, ids) = self.build_with_ids(|| client.next_order_id())?;
        client.submit_order(order.order_id, contract, &order)?;
        Ok(ids)
    }
}
