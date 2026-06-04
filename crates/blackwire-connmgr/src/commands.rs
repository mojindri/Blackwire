use crate::meta::CloseReason;

/// Selector used to identify which connections to close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseSelector {
    /// Close the connection with this specific ID.
    Id(u64),
    /// Close all connections for the named user.
    User(String),
    /// Close all connections accepted by the named inbound.
    Inbound(String),
    /// Close all connections routed to the named outbound.
    Outbound(String),
}

impl CloseSelector {
    pub(crate) fn reason(&self) -> CloseReason {
        match self {
            CloseSelector::Id(_) => CloseReason::ClosedById,
            CloseSelector::User(_) => CloseReason::ClosedByUser,
            CloseSelector::Inbound(_) => CloseReason::ClosedByInbound,
            CloseSelector::Outbound(_) => CloseReason::ClosedByOutbound,
        }
    }
}

/// Result of a bulk connection close command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionCommandResult {
    /// Number of connections that matched the selector and were cancelled.
    pub matched: usize,
}
