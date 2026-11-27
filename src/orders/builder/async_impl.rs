use super::order_builder::{AttachedOrdersBuilder, BracketOrderBuilder, ClientBound, OrderBuilder};
use super::types::{AttachedOrderIds, BracketOrderIds};
use crate::client::r#async::Client;
use crate::errors::Error;
use crate::orders::{Action, LinkedOrdersError, OrderId, OrderType};

impl OrderBuilder<ClientBound<'_, Client>, Action, OrderType> {
    /// Submit the order asynchronously
    /// Returns the order ID assigned to the submitted order
    ///
    /// # Errors
    /// [`Error::Validation`] if the builder's inputs fail validation, before anything is sent;
    /// [`Error::InvalidArgument`] for a required input the typestate can't see (see
    /// [`OrderBuilder::build`]); [`Error::OrderIdInRequestRange`] if the client's next order id
    /// has reached 1,500,000,000 (reserved for request ids).
    ///
    /// # Examples
    ///
    /// `submit` needs a side and an order type. Without the order type:
    ///
    /// ```compile_fail,E0599
    /// # async fn run(client: &ibapi::Client) {
    /// # let contract = ibapi::contracts::Contract::stock("AAPL").build();
    /// let order_id = client.order(&contract).buy(100).submit();
    /// # }
    /// ```
    ///
    /// Without the side:
    ///
    /// ```compile_fail,E0599
    /// # async fn run(client: &ibapi::Client) {
    /// # let contract = ibapi::contracts::Contract::stock("AAPL").build();
    /// let order_id = client.order(&contract).market().submit();
    /// # }
    /// ```
    pub async fn submit(self) -> Result<OrderId, Error> {
        let ClientBound { client, contract } = self.target;
        let order_id = client.next_order_id();
        let order = self.build()?;
        client.submit_order(order_id, contract, &order).await?;
        Ok(OrderId::new(order_id))
    }

    /// Analyze the order for margin and commission (what-if): TWS evaluates it without placing it.
    ///
    /// Returns the order state that carries the margins and commission. TWS answers some orders
    /// (live: US stock limit orders) with a warning-only frame first; the returned state keeps
    /// that frame's [`warning_text`](crate::orders::OrderState::warning_text) (live: 2026-11-24,
    /// server 223; `docs/wire-facts/orders.md#what-if-answer-frames`). For an order built
    /// elsewhere, use [`Client::analyze_order`].
    ///
    /// # Errors
    /// [`Error::Validation`] if the builder's inputs fail validation, before anything is sent;
    /// [`Error::InvalidArgument`] for a required input the typestate cannot see (see
    /// [`OrderBuilder::build`]).
    /// [`Error::OrderIdInRequestRange`] if the client's next order id has reached 1,500,000,000
    /// (reserved for request ids); the error TWS returns if it rejects the what-if order,
    /// including the 201 for an order the account's margin cannot carry;
    /// [`Error::UnexpectedEndOfStream`] if the reply ends without the order's state;
    /// [`Error::Timeout`] if the state has not come within the client's one-shot timeout (30 s
    /// unless set), as for [`Client::analyze_order`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::contracts::Contract;
    /// use ibapi::Client;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("AAPL").build();
    ///     let state = client.order(&contract).buy(100).limit(150.0).analyze().await.expect("analyze failed");
    ///     println!(
    ///         "initial margin {:?} -> {:?}, commission {:?} {}",
    ///         state.initial_margin_before, state.initial_margin_after, state.commission_and_fees, state.commission_and_fees_currency
    ///     );
    /// }
    /// ```
    pub async fn analyze(self) -> Result<crate::orders::OrderState, Error> {
        let ClientBound { client, contract } = self.target;
        let order = self.build()?;
        client.analyze_order(client.next_order_id(), contract, &order).await
    }
}

impl BracketOrderBuilder<'_, Client, OrderType, f64, f64> {
    /// Submits bracket orders asynchronously and returns their three order IDs.
    ///
    /// Allocates one ID per order before sending the bracket through
    /// [`Client::submit_linked_orders`]. The parent and take-profit are sent with `transmit`
    /// off, and the stop-loss releases the group.
    ///
    /// A failed write may have reached TWS. The error lists IDs whose sends completed,
    /// so it can be empty even if the first frame reached TWS. Concurrent allocations can
    /// leave gaps between the IDs. To recover from an ambiguous send error or an interrupted
    /// submission, assign and retain each ID before calling [`Client::submit_linked_orders`].
    ///
    /// # Cancellation
    /// Dropping the future, including on timeout, can leave orders held at TWS or transmit the
    /// group without returning the allocated IDs. See [`Client::submit_linked_orders`] for
    /// reconciliation before retrying.
    ///
    /// # Errors
    /// [`LinkedOrdersError`] whose [`error`](LinkedOrdersError::error) is
    /// [`Error::Validation`] if the builder's inputs fail validation,
    /// [`Error::InvalidArgument`] for a required input the typestate cannot see, or
    /// [`Error::OrderIdInRequestRange`] if the client's next order id has reached 1,500,000,000
    /// (reserved for request ids), both before anything is sent. On a send failure, the error
    /// records completed sends and the results of cancel attempts.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn run(client: &ibapi::Client) -> Result<(), ibapi::Error> {
    /// use ibapi::contracts::Contract;
    ///
    /// let contract = Contract::stock("AAPL").build();
    /// let ids = client
    ///     .order(&contract)
    ///     .buy(100)
    ///     .bracket()
    ///     .entry_limit(150.0)
    ///     .take_profit(160.0)
    ///     .stop_loss(145.0)
    ///     .submit_all()
    ///     .await?;
    /// println!("parent {} take-profit {} stop-loss {}", ids.parent, ids.take_profit, ids.stop_loss);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// `submit_all` needs the entry, the take-profit and the stop-loss. Without the entry:
    ///
    /// ```compile_fail,E0599
    /// # async fn run(client: &ibapi::Client) {
    /// # let contract = ibapi::contracts::Contract::stock("AAPL").build();
    /// let ids = client.order(&contract).buy(100).bracket().take_profit(160.0).stop_loss(145.0).submit_all();
    /// # }
    /// ```
    ///
    /// Without the take-profit:
    ///
    /// ```compile_fail,E0599
    /// # async fn run(client: &ibapi::Client) {
    /// # let contract = ibapi::contracts::Contract::stock("AAPL").build();
    /// let ids = client.order(&contract).buy(100).bracket().entry_limit(150.0).stop_loss(145.0).submit_all();
    /// # }
    /// ```
    ///
    /// Without the stop-loss:
    ///
    /// ```compile_fail,E0599
    /// # async fn run(client: &ibapi::Client) {
    /// # let contract = ibapi::contracts::Contract::stock("AAPL").build();
    /// let ids = client.order(&contract).buy(100).bracket().entry_market().take_profit(160.0).submit_all();
    /// # }
    /// ```
    pub async fn submit_all(self) -> Result<BracketOrderIds, LinkedOrdersError> {
        let ClientBound { client, contract } = self.parent_builder.target;
        let orders = self.build().map_err(LinkedOrdersError::unsent)?;

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
        client.submit_linked_orders(orders).await?;

        Ok(BracketOrderIds::new(parent_id, tp_id, sl_id))
    }
}

impl AttachedOrdersBuilder<'_, Client> {
    /// Submit the order with its preset children asynchronously.
    ///
    /// Allocates the parent id, then one id per requested child, and sends a single
    /// place-order request. Fire-and-forget: whether TWS attached the children shows up only
    /// on the order update stream — see [`Order::preset_stop_loss_order_id`](crate::orders::Order::preset_stop_loss_order_id).
    ///
    /// # Errors
    /// [`Error::Validation`] if the builder's inputs fail validation, before anything is sent;
    /// [`Error::InvalidArgument`] for a required input the typestate can't see (see
    /// [`OrderBuilder::build`]); [`Error::OrderIdInRequestRange`] if the client's next order id
    /// has reached 1,500,000,000 (reserved for request ids).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::contracts::Contract;
    /// use ibapi::Client;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let client = Client::connect("127.0.0.1:4002", 100).await.expect("connection failed");
    ///     let contract = Contract::stock("AAPL").build();
    ///     let ids = client
    ///         .order(&contract)
    ///         .buy(100)
    ///         .limit(150.0)
    ///         .preset_stop_loss()
    ///         .preset_profit_taker()
    ///         .submit()
    ///         .await
    ///         .expect("submit failed");
    ///     println!("parent {} stop-loss {:?} profit-taker {:?}", ids.parent, ids.stop_loss, ids.profit_taker);
    /// }
    /// ```
    pub async fn submit(self) -> Result<AttachedOrderIds, Error> {
        let ClientBound { client, contract } = self.parent_builder.target;
        let (order, ids) = self.build_with_ids(|| client.next_order_id())?;
        client.submit_order(order.order_id, contract, &order).await?;
        Ok(ids)
    }
}
