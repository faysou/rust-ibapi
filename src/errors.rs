//! Error types for the IBAPI library.
//!
//! This module defines all error types that can occur during API operations,
//! including I/O errors, parsing errors, and TWS-specific protocol errors.

use std::{num::ParseIntError, string::FromUtf8Error};
use thiserror::Error;

use crate::accounts::types::AccountId;
use crate::market_data::historical::{HistoricalParseError, WhatToShow};
use crate::messages::{Notice, ResponseMessage};
use crate::orders::builder::ValidationError;

/// The main error type for IBAPI operations.
///
/// This enum is marked `#[non_exhaustive]` to allow adding new error variants
/// in future versions without breaking compatibility.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    // External error types
    /// I/O error from network operations.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Failed to parse an integer from string.
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),

    /// Invalid UTF-8 sequence in response data.
    #[error(transparent)]
    FromUtf8(#[from] FromUtf8Error),

    /// Failed to parse time/date string.
    #[error(transparent)]
    ParseTime(#[from] time::error::Parse),

    /// Mutex was poisoned by a panic in another thread.
    #[error("{0}")]
    Poison(String),

    // IBAPI-specific errors
    /// Feature or method not yet implemented.
    #[error("not implemented")]
    NotImplemented,

    /// Failed to parse a protocol message.
    /// Contains: (field_index, field_value, error_description)
    #[error("parse error: {0} - {1} - {2}")]
    Parse(usize, String, String),

    /// Server version requirement not met.
    /// Contains: (required_version, actual_version, feature_name)
    #[error("server version {0} required, got {1}: {2}")]
    ServerVersion(i32, i32, String),

    /// Generic error with custom message.
    #[error("error occurred: {0}")]
    Simple(String),

    /// Invalid argument provided to API method.
    #[error("InvalidArgument: {0}")]
    InvalidArgument(String),

    /// An order builder rejected its inputs before anything was sent (e.g.
    /// `OrderBuilder::submit`), or `?` was applied to a [`ValidationError`].
    /// Carries the `ValidationError`, so callers match on its variants instead
    /// of parsing a message.
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),

    /// The connection to TWS/Gateway is gone for good: the automatic
    /// reconnect used up its attempts, or reading from the socket failed in a
    /// way reconnecting cannot fix. Terminal - every later request, and
    /// `Client::wait_connected`, fails with it. Streams open when the
    /// connection dropped already ended with [`Error::ConnectionReset`].
    #[error("ConnectionFailed")]
    ConnectionFailed,

    /// TWS/Gateway accepted the TCP connection but closed before completing
    /// the handshake — typically a host allow-list mismatch on the gateway.
    /// Payload carries the underlying diagnostic.
    #[error("connection rejected: {0}")]
    ConnectionRejected(String),

    /// TWS refused the session because another connection holds this client
    /// id (TWS error 326). Connect with a different client id, or close the
    /// other connection.
    ///
    /// Returned by `Client::connect`, which doesn't retry. The automatic
    /// reconnect does retry it: after a dropped connection TWS can hold the
    /// old session until it notices the socket is dead, and frees the id
    /// then. If every attempt meets it, the client fails with
    /// [`Error::ConnectionFailed`]; the error log names the cause.
    #[error("client id {client_id} is already in use by another connection")]
    ClientIdInUse {
        /// The refused client id.
        client_id: i32,
    },

    /// IB Gateway sent a timezone name that could not be mapped to an IANA zone.
    #[error("unrecognized IB Gateway timezone {0:?}; register a mapping with `ibapi::register_timezone_alias({0:?}, \"<IANA-name>\")` before connecting, or set `IBAPI_TIMEZONE_ALIASES={0}=<IANA-name>` in the environment. See https://github.com/wboayue/rust-ibapi/blob/main/docs/troubleshooting.md#gateway-time-zone. To request it as a built-in, file an issue at https://github.com/wboayue/rust-ibapi/issues")]
    UnsupportedTimeZone(String),

    /// Connection was reset by TWS/Gateway.
    #[error("ConnectionReset")]
    ConnectionReset,

    /// Operation was cancelled by user or system.
    #[error("Cancelled")]
    Cancelled,

    /// Client is shutting down.
    #[error("Shutdown")]
    Shutdown,

    /// Reached end of data stream.
    #[error("EndOfStream")]
    EndOfStream,

    /// Received unexpected message type. The string carries the `Debug` repr
    /// of the offending wire envelope for diagnostic logging; the structured
    /// payload is no longer exposed (rust-ibapi 3.x retired
    /// `ResponseMessage` from the public surface).
    #[error("UnexpectedResponse: {0}")]
    UnexpectedResponse(String),

    /// A message arrived in the wrong wire format for the reader handling it —
    /// text framing at a proto-only decoder, or proto framing at a text-field
    /// accessor. The string carries the `Debug` repr of the offending envelope.
    ///
    /// Deliberately distinct from [`Error::UnexpectedResponse`], which means
    /// "not my message type" and is *skipped* on shared channels. A framing
    /// mismatch is not skippable: the message was addressed to this reader and
    /// could not be read. At `server_versions::PROTOBUF_REST_MESSAGES_3` this is
    /// unreachable in production, so receiving it means the gateway broke
    /// protocol.
    #[error("UnexpectedWireFormat: {0}")]
    UnexpectedWireFormat(String),

    /// Stream ended unexpectedly.
    #[error("UnexpectedEndOfStream")]
    UnexpectedEndOfStream,

    /// TWS answered a historical-ticks request with ticks of another kind. It
    /// does this when the contract has no ticks of the requested kind: a
    /// `TRADES` request (`historical_ticks(..).last()`) for a forex pair, a
    /// spot metal or an index CFD is answered with midpoint ticks. Terminal;
    /// TWS sends nothing more for the request. Request `received` instead.
    ///
    /// A window with no ticks of the requested kind is not this error: TWS
    /// answers it with an empty batch of the requested kind.
    #[error("historical ticks requested as {requested}, but TWS answered with {received} ticks")]
    UnexpectedTickKind {
        /// The kind the request asked for.
        requested: WhatToShow,
        /// The kind TWS sent.
        received: WhatToShow,
    },

    /// The deadline passed before the request ended. Says nothing about the
    /// request at TWS, which may still be running; see
    /// `Subscription::cancel_and_drain`.
    ///
    /// Also returned by one-shot requests answered without a request id
    /// (`server_time`, `managed_accounts`, `request_fa`, ...) when TWS sends
    /// no answer within 30 seconds.
    #[error("timed out before the request ended")]
    Timeout,

    /// A subscription with a buffer limit had `limit` unread items when
    /// another arrived. Terminal: every item queued before it was delivered,
    /// and later frames for the request are discarded. Not a TWS error.
    #[error("subscription buffer limit exceeded ({limit} unread items)")]
    BufferLimitExceeded {
        /// The configured maximum of unread items.
        limit: usize,
    },

    /// A frame arrived whose length prefix cannot describe a TWS message:
    /// shorter than the 4-byte message id, or larger than the 16 MiB ceiling
    /// (`0x00FFFFFF`) that the official client enforces as
    /// `Constants.MaxMsgSize`.
    ///
    /// The length prefix is positional — there is no delimiter or magic value
    /// to re-anchor on — so a single bad prefix desynchronizes every subsequent
    /// read on that socket. Left unchecked, a garbage length is either a
    /// multi-gigabyte allocation or a read that silently consumes (and
    /// destroys) every real message until it is satisfied. Both end in
    /// permanently mis-framed messages that still decode without error, so this
    /// is deliberately raised as a hard framing fault rather than skipped.
    ///
    /// Treated as [`is_connection_lost`](Self::is_connection_lost): reconnecting
    /// is the only way to re-anchor the stream.
    #[error("InvalidFrame: {0}")]
    InvalidFrame(String),

    /// An IB notice frame (TWS error/warning/system message) received in
    /// response to a request. Carries the full typed [`Notice`] — code,
    /// message, optional timestamp, and advanced-order-reject JSON.
    ///
    /// Use [`Notice::category`] / [`Notice::is_order_rejection`] /
    /// [`Notice::is_warning`] to classify without string-parsing. Distinct
    /// from [`Error::ConnectionRejected`] (handshake-time refusal) and the
    /// transport variants ([`Error::Io`], [`Error::ConnectionReset`]).
    #[error("{0}")]
    Notice(Notice),

    /// Attempted to create a duplicate subscription.
    #[error("AlreadySubscribed")]
    AlreadySubscribed,

    /// `account_updates` was called for a different account while one is
    /// live. TWS keeps one account-updates subscription per connection; use
    /// `account_updates_multi` to stream several accounts at once.
    #[error("account updates already streaming {active}; cancel it before requesting {requested}")]
    AccountUpdatesInUse {
        /// The account currently streaming.
        active: AccountId,
        /// The account that was refused.
        requested: AccountId,
    },

    /// An order id at or above 1,500,000,000, the range reserved for request
    /// ids. The client routes frames by id range, so an order there would be
    /// taken for a request. Returned for an id you pass, one
    /// `next_valid_order_id` receives from TWS, or the client's next order id
    /// after a reconnect moved it there.
    #[error("order id {order_id} is at or above {floor}, which is reserved for request ids", floor = crate::client::ids::REQUEST_ID_FLOOR)]
    OrderIdInRequestRange {
        /// The refused order id.
        order_id: i32,
    },

    /// Wraps errors parsing historical data parameters.
    #[error("HistoricalParseError: {0}")]
    HistoricalParseError(#[from] HistoricalParseError),

    /// Failed to decode a protobuf message. The decode detail is in
    /// `Display`; [`source`](std::error::Error::source) is `None`.
    #[error("protobuf decode error: {0}")]
    ProtobufDecode(ProtobufDecodeError),
}

/// A protobuf message from TWS failed to decode. Payload of
/// [`Error::ProtobufDecode`].
///
/// Opaque: the `Display` text is its only content, and the `Debug` format is
/// not a stable contract. The underlying decoder is an implementation detail,
/// so its error type is not part of this crate's API.
#[derive(Debug, Clone)]
pub struct ProtobufDecodeError(prost::DecodeError);

impl std::fmt::Display for ProtobufDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ProtobufDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl From<crate::transport::routing::DecodedError> for Error {
    /// Project a dispatcher-decoded error payload to [`Error::Notice`].
    /// Mirrors the [`From<ResponseMessage>`] projection but skips the
    /// wire-message re-parse since the dispatcher already extracted the
    /// fields, and moves the message string instead of cloning.
    fn from(payload: crate::transport::routing::DecodedError) -> Error {
        Error::Notice(Notice::from(payload))
    }
}

impl Error {
    /// Build an [`Error::ProtobufDecode`]. Crate-private so the prost error
    /// type stays out of the public API; decoders reach it through
    /// `ResponseMessage::proto` (via `crate::proto::decoders::DecodeProto`).
    pub(crate) fn protobuf_decode(err: prost::DecodeError) -> Error {
        Error::ProtobufDecode(ProtobufDecodeError(err))
    }

    /// Build an [`Error::UnexpectedResponse`] from an internal `ResponseMessage`.
    /// Captures the `Debug` repr in the variant's `String` payload — the
    /// structured envelope is no longer exposed publicly. Crate-private; the
    /// variant's pattern `Error::UnexpectedResponse(_)` remains matchable by
    /// downstream code.
    pub(crate) fn unexpected_response(message: &ResponseMessage) -> Error {
        Error::UnexpectedResponse(format!("{message:?}"))
    }

    /// Build an [`Error::UnexpectedWireFormat`] from an internal `ResponseMessage`.
    /// Same capture as [`Error::unexpected_response`], different variant.
    pub(crate) fn unexpected_wire_format(message: &ResponseMessage) -> Error {
        Error::UnexpectedWireFormat(format!("{message:?}"))
    }

    /// Build an [`Error::Parse`] when the failing input came from a text-protocol
    /// wire field whose index is not load-bearing (e.g. inside a helper that has
    /// lost the index, or in a proto codepath). Encapsulates the placeholder `0`
    /// so the variant tuple stays the same shape across call sites while
    /// readers don't have to remember the convention.
    pub(crate) fn parse_field(value: impl Into<String>, reason: impl Into<String>) -> Error {
        Error::Parse(0, value.into(), reason.into())
    }

    /// Same as [`Error::parse_field`], but named for proto-decoded inputs where
    /// the first arg is a logical field/identifier rather than a wire-field
    /// string value. Variant shape is identical; the name disambiguates intent
    /// at the call site.
    pub(crate) fn parse_proto(field: impl Into<String>, reason: impl Into<String>) -> Error {
        Error::Parse(0, field.into(), reason.into())
    }

    /// Build an [`Error::Parse`] for cursor EOF: the message ran out of fields
    /// while the caller was trying to read field index `i`. `label` names the
    /// expected type ("int", "string", "datetime", ...) so the resulting
    /// `parse error: i -  - expected <label> and found end of message`
    /// pinpoints both location and intent.
    pub(crate) fn eof_at(i: usize, label: &str) -> Error {
        Error::Parse(i, String::new(), format!("expected {label} and found end of message"))
    }

    /// Returns `true` if this error means the TWS/Gateway stream is unusable in
    /// place and the client should reconnect, rather than retry the in-flight
    /// request.
    ///
    /// Matches [`Error::ConnectionReset`] and connection-kind [`Error::Io`] errors
    /// (broken pipe, unexpected EOF, connection reset/abort) — recoverable losses
    /// where re-establishing the connection is the right response — plus
    /// [`Error::InvalidFrame`], where the socket is still open but the framing has
    /// desynchronized and only a fresh connection can re-anchor it.
    ///
    /// Returns `false` for failures reconnecting cannot recover, so a read loop can
    /// branch on them separately to stop retrying: intentional teardown
    /// ([`Error::Shutdown`]), handshake refusal ([`Error::ConnectionRejected`],
    /// [`Error::ClientIdInUse`]), and
    /// a session the client gave up on ([`Error::ConnectionFailed`]: reconnection
    /// exhausted, or a read error reconnecting cannot fix).
    ///
    /// The client reconnects on its own, and streams open when the connection
    /// dropped end with [`Error::ConnectionReset`] as soon as it does, before the
    /// reconnect starts. A request sent while it is still reconnecting is refused
    /// with `ConnectionReset` too; once it gives up, requests fail with
    /// `ConnectionFailed` instead. So on `true`, call `Client::wait_connected`
    /// before resubscribing: it returns once the session is back, or with
    /// `ConnectionFailed` / `Shutdown` if it never will be.
    ///
    /// # Examples
    ///
    /// In a subscription read loop, branch on this predicate to decide whether to
    /// wait for the reconnect and resubscribe or surface a request-level failure:
    ///
    /// ```
    /// use ibapi::Error;
    ///
    /// fn on_stream_error(err: Error) -> Result<(), Error> {
    ///     if err.is_connection_lost() {
    ///         // wait_connected, resubscribe, then keep going
    ///         Ok(())
    ///     } else {
    ///         // a request-level failure (or terminal disconnect) — surface it
    ///         Err(err)
    ///     }
    /// }
    ///
    /// assert!(on_stream_error(Error::ConnectionReset).is_ok());
    /// assert!(on_stream_error(Error::ConnectionFailed).is_err()); // reconnect gave up
    /// assert!(on_stream_error(Error::Shutdown).is_err());
    /// ```
    pub fn is_connection_lost(&self) -> bool {
        use std::io::ErrorKind;
        match self {
            Error::Io(io_err) => matches!(
                io_err.kind(),
                ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe
            ),
            Error::ConnectionReset | Error::InvalidFrame(_) => true,
            _ => false,
        }
    }

    /// Returns `true` if this error was raised before the failing call wrote anything to TWS, so
    /// the request it would have sent certainly did not go out. Returns `false` when the request
    /// may have reached TWS: an order call must then ask TWS (open orders, executions, the order
    /// update stream) what became of it.
    ///
    /// Written for the calls that place, modify, cancel or exercise orders: `place_order`,
    /// `submit_order`, `cancel_order`, `cancel_order_with`, `global_cancel`, `global_cancel_with`,
    /// the order builder's `submit` and `analyze`, and `exercise_options`. Each runs its checks,
    /// then encodes, then writes the request once; only the write can fail after bytes left.
    ///
    /// `true` for the errors only those checks raise:
    ///
    /// | Variant | Raised by |
    /// |---|---|
    /// | [`Validation`](Error::Validation) | an order builder rejecting its inputs |
    /// | [`InvalidArgument`](Error::InvalidArgument) | argument checks, such as an `ext_operator` TWS would reject |
    /// | [`OrderIdInRequestRange`](Error::OrderIdInRequestRange) | an order id at or above the request-id range |
    /// | [`ServerVersion`](Error::ServerVersion) | an order field the connected server does not support |
    /// | [`AlreadySubscribed`](Error::AlreadySubscribed) | a second order update stream, which writes nothing |
    /// | [`AccountUpdatesInUse`](Error::AccountUpdatesInUse) | the account-updates slot check, before the request is written |
    ///
    /// `false` for every other variant, including those raised both before and after a write:
    ///
    /// - [`ConnectionReset`](Error::ConnectionReset) refuses a request while the client
    ///   reconnects, but is also what a write cut off by a dropped connection returns, when part
    ///   of the frame may have gone out.
    /// - [`ConnectionFailed`](Error::ConnectionFailed), [`Shutdown`](Error::Shutdown) and
    ///   [`Timeout`](Error::Timeout) refuse a request up front, but a closed socket fails a
    ///   pending write with `Shutdown`, and a one-shot request retried after a reset returns
    ///   them once its first attempt was written.
    /// - [`Io`](Error::Io) is a failed socket write.
    /// - [`Poison`](Error::Poison) comes from a lock, including locks taken after the handshake
    ///   was written.
    /// - [`Notice`](Error::Notice) and the decode, parse and stream variants report what TWS
    ///   sent back, so the request was sent.
    ///
    /// A call that sends several requests reports only on the one that failed: the bracket
    /// builder's `submit_all` and `submit_oca_orders` send their orders one at a time, so a
    /// `true` error on a later order leaves the earlier ones sent. Likewise
    /// `next_valid_order_id` returns `OrderIdInRequestRange` for an id TWS sent back, after its
    /// id request was written; that request places nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use ibapi::Error;
    ///
    /// fn report(err: &Error) -> &'static str {
    ///     if err.is_unsent() {
    ///         "not sent"
    ///     } else {
    ///         "unknown: query the order"
    ///     }
    /// }
    ///
    /// assert_eq!(report(&Error::InvalidArgument("bad ext_operator".into())), "not sent");
    /// assert_eq!(report(&Error::ConnectionReset), "unknown: query the order");
    /// ```
    pub fn is_unsent(&self) -> bool {
        match self {
            Error::Validation(_)
            | Error::InvalidArgument(_)
            | Error::OrderIdInRequestRange { .. }
            | Error::ServerVersion(..)
            | Error::AlreadySubscribed
            | Error::AccountUpdatesInUse { .. } => true,
            Error::Io(_)
            | Error::ParseInt(_)
            | Error::FromUtf8(_)
            | Error::ParseTime(_)
            | Error::Poison(_)
            | Error::NotImplemented
            | Error::Parse(..)
            | Error::Simple(_)
            | Error::ConnectionFailed
            | Error::ConnectionRejected(_)
            | Error::ClientIdInUse { .. }
            | Error::UnsupportedTimeZone(_)
            | Error::ConnectionReset
            | Error::Cancelled
            | Error::Shutdown
            | Error::EndOfStream
            | Error::UnexpectedResponse(_)
            | Error::UnexpectedWireFormat(_)
            | Error::UnexpectedEndOfStream
            | Error::Timeout
            | Error::BufferLimitExceeded { .. }
            | Error::InvalidFrame(_)
            | Error::Notice(_)
            | Error::HistoricalParseError(_)
            | Error::ProtobufDecode(_) => false,
        }
    }

    /// Returns `true` if a read timed out before any byte of the next frame
    /// arrived, which both dispatchers treat as "nothing yet" rather than a
    /// failure — the socket is fine and the next poll continues. A timeout
    /// mid-frame never surfaces: the frame reader waits it out. Distinct from
    /// [`Self::is_connection_lost`], which means the socket is gone.
    pub(crate) fn is_read_timeout(&self) -> bool {
        use std::io::ErrorKind;
        matches!(self, Error::Io(io_err) if matches!(io_err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut))
    }
}

// Manual Clone because `std::io::Error` doesn't implement it.
impl Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Error::Io(e) => Error::Io(std::io::Error::new(e.kind(), e.to_string())),
            Error::ParseInt(e) => Error::ParseInt(e.clone()),
            Error::FromUtf8(e) => Error::FromUtf8(e.clone()),
            Error::ParseTime(e) => Error::ParseTime(*e),
            Error::Poison(s) => Error::Poison(s.clone()),
            Error::NotImplemented => Error::NotImplemented,
            Error::Parse(i, v, m) => Error::Parse(*i, v.clone(), m.clone()),
            Error::ServerVersion(a, b, s) => Error::ServerVersion(*a, *b, s.clone()),
            Error::Simple(s) => Error::Simple(s.clone()),
            Error::InvalidArgument(s) => Error::InvalidArgument(s.clone()),
            Error::Validation(e) => Error::Validation(e.clone()),
            Error::ConnectionFailed => Error::ConnectionFailed,
            Error::ConnectionRejected(s) => Error::ConnectionRejected(s.clone()),
            Error::ClientIdInUse { client_id } => Error::ClientIdInUse { client_id: *client_id },
            Error::UnsupportedTimeZone(s) => Error::UnsupportedTimeZone(s.clone()),
            Error::ConnectionReset => Error::ConnectionReset,
            Error::Cancelled => Error::Cancelled,
            Error::Shutdown => Error::Shutdown,
            Error::EndOfStream => Error::EndOfStream,
            Error::UnexpectedResponse(m) => Error::UnexpectedResponse(m.clone()),
            Error::UnexpectedWireFormat(m) => Error::UnexpectedWireFormat(m.clone()),
            Error::UnexpectedEndOfStream => Error::UnexpectedEndOfStream,
            Error::UnexpectedTickKind { requested, received } => Error::UnexpectedTickKind {
                requested: *requested,
                received: *received,
            },
            Error::Timeout => Error::Timeout,
            Error::BufferLimitExceeded { limit } => Error::BufferLimitExceeded { limit: *limit },
            Error::InvalidFrame(m) => Error::InvalidFrame(m.clone()),
            Error::Notice(n) => Error::Notice(n.clone()),
            Error::AlreadySubscribed => Error::AlreadySubscribed,
            Error::AccountUpdatesInUse { active, requested } => Error::AccountUpdatesInUse {
                active: active.clone(),
                requested: requested.clone(),
            },
            Error::OrderIdInRequestRange { order_id } => Error::OrderIdInRequestRange { order_id: *order_id },
            Error::HistoricalParseError(e) => Error::HistoricalParseError(e.clone()),
            Error::ProtobufDecode(e) => Error::ProtobufDecode(e.clone()),
        }
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(err: std::sync::PoisonError<T>) -> Error {
        Error::Poison(format!("Mutex poison error: {err}"))
    }
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
