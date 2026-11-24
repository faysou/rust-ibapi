use std::sync::Arc;
use std::time::{Duration, Instant};

use super::common::fill::{lookup_wait, FillTracker};
use super::common::lookup::{recheck_delay, OrderKey};
use super::common::what_if::WhatIfReply;
use super::common::{decoders, encoders, linked, verify};
use super::{
    CancelOrder, ClientBound, ExecutionFilter, Executions, ExerciseOptionsBuilder, OrderBuilder, OrderCancel, OrderLookup, OrderOutcome, OrderState,
    OrderStatus, OrderUpdate, Orders, PlaceOrder,
};
use crate::client::blocking::Subscription;
use crate::common::request_helpers::{self, expect_proto, ONE_SHOT_TIMEOUT};
use crate::contracts::Contract;
use crate::messages::OutgoingMessages;
use crate::orders::OrderId;
use crate::subscriptions::SubscriptionItem;
use crate::{client::sync::Client, Error};

impl Client {
    /// Start building an order for the given contract
    ///
    /// This is the primary API for creating orders, providing a fluent interface
    /// that guides you through the order creation process.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    ///
    /// let order_id = client.order(&contract)
    ///     .buy(100)
    ///     .limit(50.0)
    ///     .submit().expect("order submission failed");
    /// ```
    pub fn order<'a>(&'a self, contract: &'a Contract) -> OrderBuilder<ClientBound<'a, Self>> {
        OrderBuilder::new(self, contract)
    }

    /// Requests all *current* open orders in associated accounts at the current moment.
    /// Open orders are returned once; this function does not initiate a subscription.
    ///
    /// The reply lists other clients' orders too. TWS then raises this
    /// client's next valid order id above the highest order id listed, and
    /// refuses a lower one with error 103 ("Duplicate order id"), without
    /// sending the new id. The client raises its own order-id counter past
    /// each other client's order as the reply arrives, so
    /// [`next_order_id`](Self::next_order_id) and the order builders allocate
    /// above it. Read the reply to its end before placing orders: an order
    /// placed while it is still arriving can get an id TWS refuses. Use
    /// [`open_orders`](Self::open_orders) when this client's orders are enough.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let subscription = client.all_open_orders().expect("request failed");
    /// for order_data in subscription.iter_data() {
    ///     match order_data {
    ///         Ok(order_data) => println!("{order_data:?}"),
    ///         Err(e) => eprintln!("error: {e}"),
    ///     }
    /// }
    /// ```
    pub fn all_open_orders(&self) -> Result<Subscription<Orders>, Error> {
        let request = encoders::encode_all_open_orders()?;
        let subscription = self.send_shared_request(OutgoingMessages::RequestAllOpenOrders, request)?;

        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Requests status updates about future orders placed from TWS. Can only be used with client ID 0.
    ///
    /// # Arguments
    /// * `auto_bind` - if set to true, the newly created orders will be assigned an API order ID and implicitly associated with this client. If set to false, future orders will not be.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 0).expect("connection failed");
    ///
    /// let subscription = client.auto_open_orders(false).expect("request failed");
    /// for order_data in subscription.iter_data() {
    ///     match order_data {
    ///         Ok(order_data) => println!("{order_data:?}"),
    ///         Err(e) => eprintln!("error: {e}"),
    ///     }
    /// }
    /// ```
    pub fn auto_open_orders(&self, auto_bind: bool) -> Result<Subscription<Orders>, Error> {
        let request = encoders::encode_auto_open_orders(auto_bind)?;
        let subscription = self.send_shared_request(OutgoingMessages::RequestAutoOpenOrders, request)?;

        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Cancels an active [`crate::orders::Order`] placed by the same API client ID.
    ///
    /// The confirmation (TWS code 202) arrives as a non-terminal
    /// [`SubscriptionItem::Notice`](crate::subscriptions::SubscriptionItem);
    /// the subscription stays open until dropped, so break once cancellation
    /// is observed.
    ///
    /// The order's [`place_order`](Self::place_order) subscription, if one is
    /// open, keeps receiving the order's events. Both subscriptions yield the
    /// order's statuses, `Cancelled` included, and the 202 notice; fills and
    /// commission reports reach only the `place_order` subscription, since
    /// [`CancelOrder`] has no variant for them. A refused cancel (10147 for an
    /// unknown order, 10148 for a finished one) ends only this subscription;
    /// the `place_order` subscription receives the error as a notice.
    ///
    /// Sends an immediate cancel with default [`OrderCancel`] fields (no manual cancel
    /// time, operator ID or manual order indicator). To set any of them, use
    /// [`cancel_order_with`](Self::cancel_order_with).
    ///
    /// # Arguments
    /// * `order_id` - ID of the [`crate::orders::Order`] to cancel.
    ///
    /// # Errors
    /// [`Error::OrderIdInRequestRange`] if `order_id` is at or above 1,500,000,000 (reserved for request ids).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::subscriptions::SubscriptionItem;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let order_id = 15;
    /// let subscription = client.cancel_order(order_id).expect("request failed");
    /// for item in &subscription {
    ///     match item {
    ///         Ok(SubscriptionItem::Data(status)) => println!("status: {status:?}"),
    ///         Ok(SubscriptionItem::Notice(n)) if n.is_cancellation() => {
    ///             println!("cancelled: {n}");
    ///             break;
    ///         }
    ///         Ok(SubscriptionItem::Notice(n)) => println!("notice: {n}"),
    ///         Err(e) => { eprintln!("cancel err: {e:?}"); break; }
    ///     }
    /// }
    /// ```
    pub fn cancel_order(&self, order_id: impl Into<OrderId>) -> Result<Subscription<CancelOrder>, Error> {
        self.cancel_order_with(order_id, &OrderCancel::default())
    }

    /// Cancels an open [`crate::orders::Order`], sending the attributes in `order_cancel`.
    ///
    /// Use this over [`cancel_order`](Self::cancel_order) to send a manual cancel time
    /// ([`OrderCancel::manual_order_cancel_time`]), an operator ID
    /// ([`OrderCancel::ext_operator`]) or [`OrderCancel::manual_order_indicator`] with the
    /// cancel, e.g. for CFTC Tag 50 / CME Rule 576 tagging. Responses are as for
    /// [`cancel_order`](Self::cancel_order).
    ///
    /// # Errors
    /// - [`Error::OrderIdInRequestRange`] if `order_id` is at or above 1,500,000,000 (reserved for request ids).
    /// - [`Error::InvalidArgument`], with nothing sent, for an [`OrderCancel::ext_operator`] TWS would leave unanswered.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::orders::OrderCancel;
    /// use ibapi::subscriptions::SubscriptionItem;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let order_cancel = OrderCancel {
    ///     ext_operator: "TRADER7".to_owned(),
    ///     manual_order_indicator: Some(1),
    ///     ..OrderCancel::default()
    /// };
    /// let subscription = client.cancel_order_with(15, &order_cancel).expect("request failed");
    /// for item in &subscription {
    ///     match item {
    ///         Ok(SubscriptionItem::Notice(n)) if n.is_cancellation() => {
    ///             println!("cancelled: {n}");
    ///             break;
    ///         }
    ///         Ok(item) => println!("{item:?}"),
    ///         Err(e) => { eprintln!("cancel err: {e:?}"); break; }
    ///     }
    /// }
    /// ```
    pub fn cancel_order_with(&self, order_id: impl Into<OrderId>, order_cancel: &OrderCancel) -> Result<Subscription<CancelOrder>, Error> {
        let order_id = order_id.into().checked()?;
        verify::verify_order_cancel(order_cancel)?;
        let request = encoders::encode_cancel_order(order_id.value(), order_cancel)?;
        let subscription = self.send_order(order_id, request)?;

        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Requests completed [`crate::orders::Order`]s.
    ///
    /// A completed order's `total_quantity` is the quantity left unfilled, not the size
    /// ordered: `0` for a filled order. The size ordered is `total_quantity + filled_quantity`.
    /// [`OrderData::completed_order_status`](crate::orders::OrderData::completed_order_status)
    /// gives the record's status with `filled` and `remaining` set this way.
    /// `OrderData::order_id` is the order's id, the same as `order.order_id`.
    ///
    /// # Arguments
    /// * `api_only` - request only orders placed by the API.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let subscription = client.completed_orders(false).expect("request failed");
    /// for order_data in subscription.iter_data() {
    ///     match order_data {
    ///         Ok(order_data) => println!("{order_data:?}"),
    ///         Err(e) => eprintln!("error: {e}"),
    ///     }
    /// }
    /// ```
    pub fn completed_orders(&self, api_only: bool) -> Result<Subscription<Orders>, Error> {
        let request = encoders::encode_completed_orders(api_only)?;
        let subscription = self.send_shared_request(OutgoingMessages::RequestCompletedOrders, request)?;

        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Requests executions matching the filter.
    ///
    /// Covers the current day (since midnight) by default; set
    /// [`ExecutionFilter::last_n_days`] or [`ExecutionFilter::specific_dates`]
    /// to reach earlier days.
    /// Along with the [`crate::orders::ExecutionData`], the [`crate::orders::CommissionAndFeesReport`] will also be returned.
    /// To join each commission to its execution, read the reply with
    /// `collect_to_end_within` and pass it to [`FillMatcher::pair_executions`](crate::orders::FillMatcher::pair_executions).
    ///
    /// The reply reaches only this subscription. An open
    /// [`place_order`](Self::place_order) subscription or
    /// [`order_update_stream`](Self::order_update_stream) neither receives it
    /// nor takes rows from it. A reply that TWS sends between one of this
    /// client's fills and the fill's commission report lists the fill without
    /// the report; the live report follows on the order's streams, and is
    /// copied here too, so a reply can carry a fill's report twice. Key
    /// commissions by `execution_id`.
    /// When requesting executions, a filter can be specified to receive only a subset of them
    ///
    /// # Arguments
    /// * `filter` - filter criteria used to determine which execution reports are returned
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::orders::{ExecutionFilter, ExecutionFilterSide};
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let filter = ExecutionFilter {
    ///    side: Some(ExecutionFilterSide::Buy),
    ///    ..ExecutionFilter::default()
    /// };
    ///
    /// let subscription = client.executions(filter).expect("request failed");
    /// for execution_data in subscription.iter_data() {
    ///     match execution_data {
    ///         Ok(execution_data) => println!("{execution_data:?}"),
    ///         Err(e) => eprintln!("error: {e}"),
    ///     }
    /// }
    /// ```
    pub fn executions(&self, filter: ExecutionFilter) -> Result<Subscription<Executions>, Error> {
        let request_id = self.mint_request_id();

        let request = encoders::encode_executions(request_id.raw(), &filter)?;
        let subscription = self.send_request(request_id, request)?;

        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Cancels all open [`crate::orders::Order`]s.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// client.global_cancel().expect("request failed");
    /// ```
    pub fn global_cancel(&self) -> Result<(), Error> {
        self.global_cancel_with(&OrderCancel::default())
    }

    /// Cancels all open [`crate::orders::Order`]s, sending the attributes in `order_cancel`.
    ///
    /// Use this over [`global_cancel`](Self::global_cancel) to send an operator ID
    /// ([`OrderCancel::ext_operator`]) or [`OrderCancel::manual_order_indicator`] with the cancel.
    ///
    /// # Errors
    /// [`Error::InvalidArgument`], with nothing sent, for an [`OrderCancel::ext_operator`] TWS would leave unanswered.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::orders::OrderCancel;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let order_cancel = OrderCancel {
    ///     ext_operator: "TRADER7".to_owned(),
    ///     ..OrderCancel::default()
    /// };
    /// client.global_cancel_with(&order_cancel).expect("request failed");
    /// ```
    pub fn global_cancel_with(&self, order_cancel: &OrderCancel) -> Result<(), Error> {
        verify::verify_order_cancel(order_cancel)?;
        let message = encoders::encode_global_cancel(order_cancel)?;
        self.send_message(message)?;

        Ok(())
    }

    /// Gets the next valid order ID from the TWS server.
    ///
    /// Unlike [Self::next_order_id], this function requests the next valid order ID from the TWS server.
    /// This can be for ensuring that order IDs are unique across multiple clients.
    ///
    /// The returned id is reserved: the client's order-ID generator moves past
    /// it, so [Self::next_order_id] and the order builders never hand it out.
    /// The generator only moves up, never below order IDs already allocated
    /// locally, including IDs whose order has not yet reached the server.
    ///
    /// Use this method when coordinating order IDs across multiple client instances or when you need to synchronize with the server's order ID sequence at the start of a session.
    ///
    /// # Errors
    /// - [`Error::OrderIdInRequestRange`] if TWS returns an id at or above 1,500,000,000 (reserved for request ids).
    /// - [`Error::Timeout`] if TWS sends no answer within 30 seconds, or within [`one_shot_timeout`](crate::client::blocking::ClientBuilder::one_shot_timeout) when set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    ///
    /// // Connect to the TWS server at the given address with client ID.
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// // Request the next valid order ID from the server.
    /// let next_valid_order_id = client.next_valid_order_id().expect("request failed");
    /// println!("next_valid_order_id: {next_valid_order_id}");
    /// ```
    pub fn next_valid_order_id(&self) -> Result<i32, Error> {
        let next_order_id = request_helpers::blocking::one_shot_shared(
            self,
            OutgoingMessages::RequestIds,
            encoders::encode_next_valid_order_id,
            expect_proto(decoders::decode_next_valid_id_proto),
        )?;

        let next_order_id = OrderId::from(next_order_id).checked()?;
        // Reserve the id handed to the caller, as `next_order_id` does: raising only to it
        // left it free, so the next client-allocated id repeated it.
        self.id_manager().raise_order_id(next_order_id.next());
        Ok(next_order_id.value())
    }

    /// Requests all open orders places by this specific API client (identified by the API client id).
    /// For client ID 0, this will bind previous manual TWS orders.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let subscription = client.open_orders().expect("request failed");
    /// for order_data in subscription.iter_data() {
    ///     match order_data {
    ///         Ok(order_data) => println!("{order_data:?}"),
    ///         Err(e) => eprintln!("error: {e}"),
    ///     }
    /// }
    /// ```
    pub fn open_orders(&self) -> Result<Subscription<Orders>, Error> {
        let request = encoders::encode_open_orders()?;
        let subscription = self.send_shared_request(OutgoingMessages::RequestOpenOrders, request)?;

        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Places or modifies an [`crate::orders::Order`].
    ///
    /// Submits an [`crate::orders::Order`] using [Client] for the given [Contract].
    /// Upon successful submission, the client will start receiving events related to the order's activity via the subscription, including order status updates and execution reports.
    ///
    /// The subscription keeps receiving the order's events after
    /// [`cancel_order`](Self::cancel_order) or a modify (another `place_order`
    /// with the same ID): each opens its own subscription beside this one, and
    /// every subscription on the order ID receives each event once. After a
    /// cancel, this subscription yields the `Cancelled` status and the 202
    /// notice too, as well as fills and commission reports that arrive after
    /// the cancel was sent.
    ///
    /// To modify a working order, call `place_order` again with its order ID, its contract and
    /// the full order with the changed fields: clone the order you placed, or take
    /// [`order`](crate::orders::OrderData::order) and [`contract`](crate::orders::OrderData::contract)
    /// from [`open_orders`](Self::open_orders). TWS keeps the order's perm ID and answers an
    /// accepted modify with the order's open order and status. Only the client that placed the
    /// order can modify it: TWS reads the same ID from another client as a new order. TWS
    /// (live: 2026-11-24, server 223; `docs/wire-facts/orders.md#modifying-an-order`):
    ///
    /// - accepts a new limit price, quantity or time in force;
    /// - refuses another action or contract with 105, and another order type with 329;
    /// - refuses a zero quantity with 321, so send every field, not only the changed ones;
    /// - refuses the ID of a filled, cancelled or rejected order with 104, and of an order it
    ///   has not acknowledged yet with 103;
    /// - sends nothing for a modify of a held order (`transmit` false); the modify that sets
    ///   `transmit` releases it with the new terms.
    ///
    /// A refused modify leaves the order working. Its error ends only the modify's
    /// subscription; this subscription and the order's other subscriptions receive the error
    /// as a notice and keep receiving the order's events.
    ///
    /// # Arguments
    /// * `order_id` - ID for [`crate::orders::Order`]. Get next valid ID using [Client::next_order_id].
    ///   The client's order-ID generator moves past this ID and every ID the order carries
    ///   (preset attached orders, parent), so IDs it allocates later don't reuse them. If you
    ///   number orders from your own range, client-allocated IDs (the order builder's `submit`,
    ///   `exercise_options`) then continue from your highest ID, inside that range.
    /// * `contract` - [Contract] to submit order for.
    /// * `order` - [`crate::orders::Order`] to submit.
    ///
    /// # Errors
    /// - [`Error::OrderIdInRequestRange`] if `order_id`, or a `parent_id` or preset attached-order id on `order`, is at or above 1,500,000,000 (reserved for request ids).
    /// - [`Error::InvalidArgument`], with nothing sent, for an [`ext_operator`](crate::orders::Order::ext_operator) TWS would reject.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use ibapi::orders::PlaceOrder;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let contract = Contract::stock("MSFT").build();
    /// let order = client.order(&contract)
    ///     .buy(100)
    ///     .market()
    ///     .build_order()
    ///     .expect("failed to build order");
    /// let order_id = client.next_order_id();
    ///
    /// let events = client.place_order(order_id, &contract, &order).expect("request failed");
    ///
    /// for event in events.iter_data() {
    ///     match event? {
    ///         PlaceOrder::OrderStatus(order_status) => {
    ///             println!("order status: {order_status:?}")
    ///         }
    ///         PlaceOrder::OpenOrder(open_order) => println!("open order: {open_order:?}"),
    ///         PlaceOrder::ExecutionData(execution) => println!("execution: {execution:?}"),
    ///         PlaceOrder::CommissionAndFeesReport(report) => println!("commission report: {report:?}"),
    ///    }
    /// }
    /// # Ok::<(), ibapi::Error>(())
    /// ```
    pub fn place_order(&self, order_id: impl Into<OrderId>, contract: &Contract, order: &super::Order) -> Result<Subscription<PlaceOrder>, Error> {
        let checked_id = verify::verify_and_reserve_order_ids(self.id_manager(), self.server_version(), order_id.into(), order)?;

        let request = encoders::encode_place_order(checked_id.value(), contract, order)?;
        let subscription = self.send_order(checked_id, request)?;

        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Analyzes `order` for margin and commission (what-if): TWS evaluates it without placing it.
    ///
    /// For an order built elsewhere; [`OrderBuilder::analyze`]
    /// calls this. Sends a copy of `order` with [`what_if`](crate::orders::Order::what_if) set
    /// under the client's next order id, so `order` itself is not changed and needs neither.
    ///
    /// Returns the order state that carries the margins and commission. TWS answers some orders
    /// (live: US stock limit orders) with a warning-only frame first; the returned state keeps
    /// that frame's [`warning_text`](crate::orders::OrderState::warning_text) (live: 2026-11-24,
    /// server 223; `docs/wire-facts/orders.md#what-if-answer-frames`).
    ///
    /// The answer's frames also reach [`order_update_stream`](Client::order_update_stream), as
    /// every order frame does, with `order.what_if` set; a stream consumer that analyzes through
    /// this call can skip them.
    ///
    /// # Errors
    /// - [`Error::OrderIdInRequestRange`] if the client's next order id, or a `parent_id` or preset attached-order id on `order`, is at or above 1,500,000,000 (reserved for request ids).
    /// - [`Error::InvalidArgument`], with nothing sent, for an [`ext_operator`](crate::orders::Order::ext_operator) TWS would reject.
    /// - The error TWS returns if it rejects the what-if order, including the 201 for an order the account's margin cannot carry.
    /// - [`Error::UnexpectedEndOfStream`] if the reply ends without the order's state.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use ibapi::orders::{order_builder, Action};
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    /// let order = order_builder::limit_order(Action::Buy, 100.0, 150.0);
    ///
    /// let state = client.analyze_order(&contract, &order).expect("analyze failed");
    /// println!(
    ///     "initial margin {:?} -> {:?}, commission {:?} {}",
    ///     state.initial_margin_before, state.initial_margin_after, state.commission_and_fees, state.commission_and_fees_currency
    /// );
    /// ```
    pub fn analyze_order(&self, contract: &Contract, order: &super::Order) -> Result<OrderState, Error> {
        let order_id = self.next_order_id();
        let order = super::Order {
            what_if: true,
            ..order.clone()
        };

        let responses = self.place_order(order_id, contract, &order)?;

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

    /// Submits or modifies an [`crate::orders::Order`] without returning a subscription.
    ///
    /// This is a fire-and-forget method that submits an [`crate::orders::Order`] for the given [Contract]
    /// but does not return a subscription for order updates. To receive order status updates,
    /// fills, and commission reports, use the [`order_update_stream`](Client::order_update_stream) method
    /// or use [`place_order`](Client::place_order) instead which returns a subscription.
    ///
    /// # Arguments
    /// * `order_id` - ID for [`crate::orders::Order`]. Get next valid ID using [Client::next_order_id].
    ///   The client's order-ID generator moves past this ID and every ID the order carries
    ///   (preset attached orders, parent), so IDs it allocates later don't reuse them. If you
    ///   number orders from your own range, client-allocated IDs (the order builder's `submit`,
    ///   `exercise_options`) then continue from your highest ID, inside that range.
    /// * `contract` - [Contract] to submit order for.
    /// * `order` - [`crate::orders::Order`] to submit.
    ///
    /// # Returns
    /// * `Ok(())` if the order was successfully sent
    /// * `Err(Error)` if validation failed or sending failed
    ///
    /// # Errors
    /// - [`Error::OrderIdInRequestRange`] if `order_id`, or a `parent_id` or preset attached-order id on `order`, is at or above 1,500,000,000 (reserved for request ids).
    /// - [`Error::InvalidArgument`], with nothing sent, for an [`ext_operator`](crate::orders::Order::ext_operator) TWS would reject.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    ///
    /// # fn main() -> Result<(), ibapi::Error> {
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100)?;
    ///
    /// let contract = Contract::stock("MSFT").build();
    /// let order = client.order(&contract)
    ///     .buy(100)
    ///     .market()
    ///     .build_order()?;
    /// let order_id = client.next_order_id();
    ///
    /// // Submit order without waiting for confirmation
    /// client.submit_order(order_id, &contract, &order)?;
    ///
    /// // Monitor all order updates via the order update stream
    /// // This will receive updates for ALL orders, not just this one
    /// use ibapi::orders::OrderUpdate;
    /// for event in client.order_update_stream()?.iter_data() {
    ///     match event? {
    ///         OrderUpdate::OrderStatus(status) => println!("Order Status: {status:?}"),
    ///         OrderUpdate::ExecutionData(exec) => println!("Execution: {exec:?}"),
    ///         OrderUpdate::CommissionAndFeesReport(report) => println!("Commission: {report:?}"),
    ///         OrderUpdate::OrderBound(binding) => println!("Order binding: {binding:?}"),
    ///         _ => {}
    ///     }
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn submit_order(&self, order_id: impl Into<OrderId>, contract: &Contract, order: &super::Order) -> Result<(), Error> {
        let checked_id = verify::verify_and_reserve_order_ids(self.id_manager(), self.server_version(), order_id.into(), order)?;

        let request = encoders::encode_place_order(checked_id.value(), contract, order)?;
        self.send_message(request)?;

        Ok(())
    }

    /// Submit multiple OCA (One-Cancels-All) orders
    ///
    /// When one order in the group is filled, all others are automatically cancelled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use ibapi::orders::OcaType;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// let contract1 = Contract::stock("AAPL").build();
    /// let contract2 = Contract::stock("MSFT").build();
    ///
    /// let order1 = client.order(&contract1)
    ///     .buy(100)
    ///     .limit(50.0)
    ///     .oca_group("MyOCA", OcaType::CancelWithBlock)
    ///     .build_order().expect("order build failed");
    ///     
    /// let order2 = client.order(&contract2)
    ///     .buy(100)
    ///     .limit(45.0)
    ///     .oca_group("MyOCA", OcaType::CancelWithBlock)
    ///     .build_order().expect("order build failed");
    ///
    /// let order_ids = client.submit_oca_orders(
    ///     vec![(contract1, order1), (contract2, order2)]
    /// ).expect("OCA submission failed");
    /// ```
    pub fn submit_oca_orders(&self, orders: Vec<(Contract, crate::orders::Order)>) -> Result<Vec<OrderId>, Error> {
        let mut order_ids = Vec::new();

        for (contract, mut order) in orders.into_iter() {
            let order_id = self.next_order_id();
            order.order_id = order_id;
            order_ids.push(OrderId::new(order_id));
            self.submit_order(order_id, &contract, &order)?;
        }

        Ok(order_ids)
    }

    /// Submits orders that TWS must release together, such as a parent with its children:
    /// only the last order transmits.
    ///
    /// Each order is sent under its own [`order_id`](crate::orders::Order::order_id), which the
    /// caller sets, e.g. from [`next_order_id`](Self::next_order_id); a child names its parent
    /// in [`parent_id`](crate::orders::Order::parent_id). The `transmit` flag of each order is
    /// overwritten: `false` on all but the last, `true` on the last. TWS holds the earlier
    /// orders until the last one arrives, then releases them all.
    ///
    /// Every order is checked before anything is sent. When a send fails part-way, the orders
    /// already sent are held at TWS, listed nowhere, and stay held until cancelled, so this
    /// cancels each of them before returning the error.
    ///
    /// # Errors
    /// [`LinkedOrdersError`](crate::orders::LinkedOrdersError), whose
    /// [`error`](crate::orders::LinkedOrdersError::error) is:
    /// - [`Error::OrderIdInRequestRange`] or [`Error::InvalidArgument`], with nothing sent, for
    ///   an order [`submit_order`](Self::submit_order) would reject;
    /// - the send error, such as [`Error::ConnectionReset`], with the held orders sent before it
    ///   and the outcome of cancelling each of them.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use ibapi::orders::builder::BracketPrices;
    /// use ibapi::orders::{order_builder, Action};
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    ///
    /// // Ids parent, parent + 1 and parent + 2; children carry `parent_id`.
    /// let prices = BracketPrices { entry: 220.0, take_profit: 230.0, stop_loss: 210.0 };
    /// let orders = order_builder::bracket_order(client.next_order_id(), Action::Buy, 100.0, prices).expect("invalid bracket");
    /// let orders = orders.into_iter().map(|order| (contract.clone(), order)).collect();
    ///
    /// match client.submit_linked_orders(orders) {
    ///     Ok(()) => println!("bracket transmitted"),
    ///     Err(e) => {
    ///         println!("{e}: sent {:?}, cancelled {:?}", e.sent, e.cancelled);
    ///         for (order_id, error) in &e.cancel_failures {
    ///             println!("order {order_id} may still be held: {error}");
    ///         }
    ///     }
    /// }
    /// ```
    // A failed multi-order submit is rare and dwarfed by its I/O; boxing would only cost callers a deref.
    #[allow(clippy::result_large_err)]
    pub fn submit_linked_orders(&self, orders: Vec<(Contract, crate::orders::Order)>) -> Result<(), crate::orders::LinkedOrdersError> {
        let requests =
            linked::encode_linked_orders(self.id_manager(), self.server_version(), orders).map_err(crate::orders::LinkedOrdersError::unsent)?;

        let mut sent = Vec::with_capacity(requests.len());
        for (order_id, request) in requests {
            if let Err(error) = self.send_message(request) {
                let mut failure = crate::orders::LinkedOrdersError::after_sends(error, sent);
                for order_id in failure.sent.clone().into_iter().rev() {
                    failure.record_cancel(order_id, self.cancel_order(order_id));
                }
                return Err(failure);
            }
            sent.push(order_id);
        }

        Ok(())
    }

    /// Creates a subscription stream for receiving real-time order updates.
    ///
    /// This method establishes a stream that receives all order-related events including:
    /// - Order status updates (e.g., submitted, filled, cancelled)
    /// - Open order information
    /// - Execution data for trades
    /// - Commission reports
    /// - Order-related messages and notices
    ///
    /// The stream will receive updates for all orders placed through this client connection,
    /// including both new orders submitted after creating the stream and existing orders.
    ///
    /// # Returns
    ///
    /// Returns a `Subscription<OrderUpdate>` that yields `OrderUpdate` enum variants containing:
    /// - `OrderStatus`: Current status of an order (filled amount, average price, etc.)
    /// - `OpenOrder`: Complete order details including contract and order parameters
    /// - `ExecutionData`: Details about individual trade executions
    /// - `CommissionAndFeesReport`: Commission information for executed trades
    ///
    /// Order-bound TWS errors and warnings (e.g. rejections, code 399 order
    /// messages) arrive as [`SubscriptionItem::Notice`](crate::subscriptions::SubscriptionItem),
    /// not as `OrderUpdate` variants. They surface via `iter()` / `next()` as
    /// below; `iter_data()` drops them (logging at `warn!` level), so match on
    /// notices explicitly when monitoring fire-and-forget orders for rejection.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created, typically due to
    /// connection issues or internal errors.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::orders::OrderUpdate;
    /// use ibapi::subscriptions::SubscriptionItem;
    /// use ibapi::TRANSPORT_RECONNECT_CODE;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// // Create order update stream
    /// let updates = client.order_update_stream().expect("failed to create stream");
    ///
    /// // `iter()` surfaces both data and notices; `iter_data()` would drop notices.
    /// for item in updates.iter() {
    ///     match item? {
    ///         SubscriptionItem::Data(OrderUpdate::OrderStatus(status)) => {
    ///             println!("Order {} status: {} - filled: {}/{}",
    ///                 status.order_id, status.status, status.filled, status.remaining);
    ///         }
    ///         SubscriptionItem::Data(update) => println!("update: {update:?}"),
    ///         SubscriptionItem::Notice(notice) if notice.code == TRANSPORT_RECONNECT_CODE => {
    ///             println!("reconnected: rebuild order state from snapshots");
    ///         }
    ///         SubscriptionItem::Notice(notice) if notice.is_error() => {
    ///             eprintln!("order {:?} rejected: {}", notice.request_id, notice.message);
    ///         }
    ///         SubscriptionItem::Notice(notice) => println!("notice: {}", notice.message),
    ///     }
    /// }
    /// # Ok::<(), ibapi::Error>(())
    /// ```
    ///
    /// # Note
    ///
    /// This stream provides updates for all orders, not just a specific order.
    /// To track a specific order, filter the updates by order ID.
    ///
    /// To pair each [`CommissionAndFeesReport`](crate::orders::CommissionAndFeesReport) with the
    /// [`ExecutionData`](crate::orders::ExecutionData) it belongs to, feed both to a
    /// [`FillMatcher`](crate::orders::FillMatcher): they share `execution_id`, and the
    /// matcher joins them in either arrival order.
    ///
    /// The stream carries what TWS reports as it happens. A reply to
    /// [`executions`](Self::executions), with its commission reports, goes only
    /// to the request that asked and never appears here.
    ///
    /// # Gaps and recovery
    ///
    /// TWS does not replay order events, so two situations leave a gap the
    /// stream cannot fill on its own:
    ///
    /// - **Reconnect.** The stream survives the client's automatic reconnects:
    ///   the same subscription keeps delivering once the connection returns,
    ///   but updates TWS emitted during the outage are lost. Once the new
    ///   session is live, the stream yields one
    ///   [`SubscriptionItem::Notice`]
    ///   with code [`TRANSPORT_RECONNECT_CODE`](crate::TRANSPORT_RECONNECT_CODE)
    ///   to mark the gap. It classifies as an error, so match its code before
    ///   treating error notices as rejections. Watch [`Self::notice_stream`] for the
    ///   connectivity notices (codes 1100 connectivity lost, 1101 restored
    ///   with data lost, 1102 restored with data maintained, 1300 socket reset).
    /// - **Error.** An `Err` item ends the stream; every later call returns
    ///   `None`. A frame that fails to decode is a bug in TWS or this crate,
    ///   so it is surfaced rather than skipped.
    ///
    /// Client shutdown (disconnect, drop, or a reconnect that gives up) also
    /// ends the stream; after it, `order_update_stream` returns
    /// [`Error::Shutdown`].
    ///
    /// After an error, drop the ended subscription and call
    /// `order_update_stream` again; while the old one is held and not
    /// cancelled, a second call returns [`Error::AlreadySubscribed`].
    /// After a reconnect, the existing subscription is still live. In both
    /// cases, rebuild state from snapshots:
    ///
    /// 1. [`Self::open_orders`]: this client's open orders and their statuses.
    ///    Not [`Self::all_open_orders`]: it also lists other clients' orders,
    ///    and TWS then raises this client's next valid order id past the
    ///    highest order id listed, which `open_orders` does not.
    /// 2. [`Self::completed_orders`]`(false)`: orders that filled or were
    ///    cancelled during the gap.
    /// 3. [`Self::executions`] with a default [`ExecutionFilter`]: fills and
    ///    their commissions. The default covers the current day only; set
    ///    `last_n_days` when the gap may span midnight, as the daily gateway
    ///    reset does.
    ///
    /// `completed_orders` and `executions` cover every client's orders, while
    /// this stream reports only this client's. Keep the entries whose `order.client_id` /
    /// `execution.client_id` matches, or state will hold orders the stream
    /// never updates.
    ///
    /// Take the snapshots once the stream is live again (on the reconnect
    /// notice, or once the new subscription exists), so no event falls
    /// between the two. An event can then arrive both ways; apply updates
    /// idempotently, keyed by `perm_id` for orders and `execution_id` for fills.
    pub fn order_update_stream(&self) -> Result<Subscription<OrderUpdate>, Error> {
        let subscription = self.create_order_update_subscription()?;
        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Streams one order's status updates.
    ///
    /// Starts with the order's latest status, if this client has seen one
    /// (the latest statuses of the 10,000 most recent orders are kept), then
    /// yields each new [`OrderStatus`] as TWS sends it. A reconnect keeps only
    /// statuses of orders that ended: TWS does not replay what happened while
    /// disconnected, so a working order's stream starts empty until its next
    /// status. If TWS refuses the
    /// order with an error before any status (e.g. 110, a price off the
    /// minimum tick), the stream yields that error as [`Error::Notice`] and
    /// ends; a stream opened later starts with it. Otherwise the stream does
    /// not end on its own when the order does; stop reading, or see
    /// [`wait_for_fill`](Self::wait_for_fill) for the common case.
    ///
    /// Takes nothing from [`order_update_stream`](Self::order_update_stream) or
    /// a [`place_order`](Self::place_order) subscription: all may be open at
    /// once, as may several streams on one order. Nothing is sent to TWS.
    ///
    /// # Errors
    ///
    /// [`Error::OrderIdInRequestRange`] if `order_id` is at or above 1,500,000,000 (reserved for request ids).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    /// let order_id = client.order(&contract).buy(100).limit(150.0).submit().expect("order submission failed");
    ///
    /// let statuses = client.order_status_stream(order_id).expect("stream failed");
    /// for status in statuses.iter_data() {
    ///     let status = status.expect("stream error");
    ///     println!("{}: {}/{} filled", status.status, status.filled, status.filled + status.remaining);
    ///     if status.status.is_terminal() {
    ///         break;
    ///     }
    /// }
    /// ```
    pub fn order_status_stream(&self, order_id: impl Into<OrderId>) -> Result<Subscription<OrderStatus>, Error> {
        let subscription = self.message_bus.create_order_status_subscription(order_id.into().checked()?)?;
        Ok(Subscription::new(Arc::clone(&self.message_bus), subscription, self.decoder_context()))
    }

    /// Waits until an order fills or ends, or `timeout` elapses.
    ///
    /// Returns [`OrderOutcome::Filled`] when the order filled completely,
    /// [`OrderOutcome::Ended`] when it was cancelled or became inactive first,
    /// [`OrderOutcome::Rejected`] with TWS's error when TWS refused the order
    /// without a status (e.g. 110, a price off the minimum tick), and
    /// [`OrderOutcome::TimedOut`] with the last status seen when time runs
    /// out. A timeout leaves the order working; cancel it yourself if needed.
    ///
    /// Built on [`order_status_stream`](Self::order_status_stream), so an
    /// order that already finished before the call is reported at once, as
    /// long as this client saw its status since connecting. When it has seen
    /// none, the wait first looks the order up: an order this client's open
    /// orders ([`open_orders`](Self::open_orders)) list is still working, and
    /// the wait goes on; otherwise one found among
    /// [`completed_orders`](Self::completed_orders) ends the wait at once,
    /// with a status built from the completed order, whose fill prices are
    /// `None` (see [`executions`](Self::executions) for them). Each lookup
    /// reads its replies to the end, which can add a round trip to `timeout`.
    /// With client id 0, the open-orders request also binds manual TWS orders
    /// to this client. An order placed by another client, or one in neither
    /// reply, waits out the timeout.
    ///
    /// # Errors
    ///
    /// [`Error::OrderIdInRequestRange`] if `order_id` is at or above 1,500,000,000
    /// (reserved for request ids), [`Error::ConnectionReset`] if the connection is
    /// down or drops during the wait, [`Error::Shutdown`] if the client shuts down.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Duration;
    ///
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::Contract;
    /// use ibapi::orders::OrderOutcome;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::stock("AAPL").build();
    ///
    /// let order_id = client.order(&contract).buy(100).market().submit().expect("order submission failed");
    ///
    /// match client.wait_for_fill(order_id, Duration::from_secs(30)).expect("wait failed") {
    ///     OrderOutcome::Filled(status) => println!("filled {} @ {:?}", status.filled, status.average_fill_price),
    ///     OrderOutcome::Ended(status) => println!("ended {:?} after {} filled", status.status, status.filled),
    ///     OrderOutcome::TimedOut(status) => println!("still working: {status:?}"),
    ///     OrderOutcome::Rejected(notice) => println!("rejected: {notice}"),
    /// }
    /// ```
    pub fn wait_for_fill(&self, order_id: impl Into<OrderId>, timeout: Duration) -> Result<OrderOutcome, Error> {
        let order_id = order_id.into();
        let statuses = self.order_status_stream(order_id)?;
        let mut tracker = FillTracker::default();
        // `None` for a timeout too large to represent: wait without one.
        let deadline = Instant::now().checked_add(timeout);
        // Nothing queued: this client has no status for the order (e.g.
        // placed on an earlier connection). Look it up.
        let mut first = statuses.try_next();
        if first.is_none() {
            if let Some(status) = self.find_finished(order_id, deadline)? {
                if let Some(outcome) = tracker.observe_completed(status) {
                    return Ok(outcome);
                }
            }
        }
        loop {
            let item = match (first.take(), deadline) {
                (Some(item), _) => Some(item),
                (None, Some(deadline)) => statuses.next_timeout(deadline.saturating_duration_since(Instant::now())),
                (None, None) => statuses.next(),
            };
            match item {
                Some(Ok(SubscriptionItem::Data(status))) => {
                    if let Some(outcome) = tracker.observe(status) {
                        return Ok(outcome);
                    }
                }
                Some(Ok(SubscriptionItem::Notice(_))) => {}
                Some(Err(error)) => return FillTracker::error(error),
                // `next_timeout` gives `None` at the deadline; before it, the
                // stream ended.
                None if deadline.is_some_and(|deadline| Instant::now() >= deadline) => return Ok(tracker.timed_out()),
                None => return Err(Error::UnexpectedEndOfStream),
            }
        }
    }

    /// Look up an order this client has no status for: its status from
    /// completed orders, or `None` when this client's open orders list it
    /// working (their statuses reach the order's status streams like any
    /// other) or neither reply does.
    fn find_finished(&self, order_id: OrderId, deadline: Option<Instant>) -> Result<Option<OrderStatus>, Error> {
        let key = OrderKey::Id {
            client_id: self.client_id(),
            order_id,
        };
        match self.look_up_order(key, Duration::ZERO, || deadline.map(lookup_wait)) {
            Ok(OrderLookup::Finished { status, .. }) => Ok(Some(status)),
            Ok(_) => Ok(None),
            // A reply cut short (deadline, closed channel, TWS error) proves
            // nothing: keep waiting.
            Err(Error::Timeout | Error::UnexpectedEndOfStream | Error::Notice(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Looks up one of this client's orders by its API order id: still
    /// working, finished, or not known to TWS.
    ///
    /// Reads this client's [`open_orders`](Self::open_orders): an order they
    /// list with a status that is not [terminal](crate::orders::OrderStatusKind::is_terminal)
    /// is [`OrderLookup::Working`]. Otherwise reads
    /// [`completed_orders`](Self::completed_orders), which list every client's
    /// orders: a record of this client with this id is
    /// [`OrderLookup::Finished`]. Ids are unique only per client, so another
    /// client's order with the same id never matches. TWS can list a
    /// completed order with client id and order id `0`; find such an order
    /// with [`find_order_by_perm_id`](Self::find_order_by_perm_id).
    ///
    /// A finished order leaves open orders before it reaches completed
    /// orders. Live, an order rejected with error 201 was gone from open
    /// orders within 50 ms of its `Inactive` status, and first listed among
    /// completed orders 19 to 204 ms after it. When neither reply lists the
    /// order, completed orders are read again every 100 ms until `within` has
    /// passed since the call, then the answer is [`OrderLookup::NotFound`].
    /// With `within` zero each reply is read once. Right after an order's
    /// last status, pass at least a few hundred milliseconds.
    ///
    /// Each reply is read to its end marker, within 30 seconds or the
    /// [`one_shot_timeout`](crate::client::blocking::ClientBuilder::one_shot_timeout)
    /// when set; a reply cut short is an error, never `NotFound`. With client
    /// id 0, the open-orders request also binds manual TWS orders to this client.
    ///
    /// # Errors
    ///
    /// [`Error::OrderIdInRequestRange`] if `order_id` is at or above 1,500,000,000
    /// (reserved for request ids), [`Error::Timeout`] if a reply does not end
    /// in time, [`Error::ConnectionReset`] if the connection is down or drops
    /// during the lookup, [`Error::UnexpectedEndOfStream`] if a reply ends
    /// without its end marker.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Duration;
    ///
    /// use ibapi::client::blocking::Client;
    /// use ibapi::orders::{OrderLookup, OrderStatusKind};
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// match client.find_order(42, Duration::from_secs(1)).expect("lookup failed") {
    ///     OrderLookup::Working { status, .. } => println!("working: {status:?}"),
    ///     OrderLookup::Finished { status, .. } if status.status == OrderStatusKind::Filled => println!("filled {}", status.filled),
    ///     OrderLookup::Finished { status, .. } => println!("{} after {} filled", status.status, status.filled),
    ///     OrderLookup::NotFound => println!("unknown to TWS"),
    /// }
    /// ```
    pub fn find_order(&self, order_id: impl Into<OrderId>, within: Duration) -> Result<OrderLookup, Error> {
        let key = OrderKey::Id {
            client_id: self.client_id(),
            order_id: order_id.into().checked()?,
        };
        let reply_wait = self.one_shot_timeout().unwrap_or(ONE_SHOT_TIMEOUT);
        self.look_up_order(key, within, || Some(reply_wait))
    }

    /// Looks up an order of any client by its TWS permanent id (`perm_id`):
    /// still working, finished, or not known to TWS.
    ///
    /// [`find_order`](Self::find_order) with the order matched by `perm_id`,
    /// on open orders and completed orders alike. Completed orders list every
    /// client's orders, so a finished order of another client is found. Open
    /// orders are this client's only: another client's working order is
    /// `NotFound`. [`all_open_orders`](Self::all_open_orders) lists those, but
    /// it raises the next valid order id at TWS without telling this client,
    /// so the next order this client places can fail with error 103
    /// (duplicate order id); this lookup does not use it.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidArgument`] if `perm_id` is `0` or below (TWS assigns
    /// positive perm ids; a record without one carries `0`), with nothing
    /// sent; [`Error::Timeout`] if a reply does not end in time,
    /// [`Error::ConnectionReset`] if the connection is down or drops during
    /// the lookup, [`Error::UnexpectedEndOfStream`] if a reply ends without
    /// its end marker.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::time::Duration;
    ///
    /// use ibapi::client::blocking::Client;
    /// use ibapi::orders::OrderLookup;
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    ///
    /// match client.find_order_by_perm_id(1290490951, Duration::ZERO).expect("lookup failed") {
    ///     OrderLookup::Working { order, .. } => println!("working: {order:?}"),
    ///     OrderLookup::Finished { order, status } => println!("{} ({})", status.status, order.order_state.completed_status),
    ///     OrderLookup::NotFound => println!("not listed"),
    /// }
    /// ```
    pub fn find_order_by_perm_id(&self, perm_id: i64, within: Duration) -> Result<OrderLookup, Error> {
        let key = OrderKey::perm_id(perm_id)?;
        let reply_wait = self.one_shot_timeout().unwrap_or(ONE_SHOT_TIMEOUT);
        self.look_up_order(key, within, || Some(reply_wait))
    }

    /// Reads open orders once, then completed orders until they list the
    /// order or `within` has passed since the call. `reply_wait` bounds each
    /// read; `None` reads without a bound.
    fn look_up_order(&self, key: OrderKey, within: Duration, reply_wait: impl Fn() -> Option<Duration>) -> Result<OrderLookup, Error> {
        let read = |subscription: Subscription<Orders>| match reply_wait() {
            Some(wait) => subscription.collect_to_end_within(wait),
            None => subscription.collect_to_end(),
        };
        let deadline = Instant::now().checked_add(within);
        if let Some(working) = key.working(read(self.open_orders()?)?) {
            return Ok(working);
        }
        loop {
            if let Some(finished) = key.finished(read(self.completed_orders(false)?)?) {
                return Ok(finished);
            }
            match recheck_delay(deadline, Instant::now()) {
                Some(delay) => std::thread::sleep(delay),
                None => return Ok(OrderLookup::NotFound),
            }
        }
    }

    /// Exercise or lapse an option position.
    ///
    /// Terminal: [`ExerciseOptionsBuilder::submit`]. Pick [`exercise`](ExerciseOptionsBuilder::exercise)
    /// or [`lapse`](ExerciseOptionsBuilder::lapse); [`account`](ExerciseOptionsBuilder::account),
    /// [`override_natural_action`](ExerciseOptionsBuilder::override_natural_action) and
    /// [`manual_order_time`](ExerciseOptionsBuilder::manual_order_time) are optional.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ibapi::client::blocking::Client;
    /// use ibapi::contracts::{Contract, ExpirationDate, OptionRight};
    ///
    /// let client = Client::connect("127.0.0.1:4002", 100).expect("connection failed");
    /// let contract = Contract::option("AAPL", ExpirationDate::new(2025, 12, 19).expect("valid date"), 150.0, OptionRight::Call);
    /// let subscription = client
    ///     .exercise_options(&contract)
    ///     .exercise(1)
    ///     .override_natural_action()
    ///     .submit()
    ///     .expect("exercise_options failed");
    /// ```
    pub fn exercise_options<'a>(&'a self, contract: &'a Contract) -> ExerciseOptionsBuilder<'a, Self> {
        ExerciseOptionsBuilder::new(self, contract)
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
