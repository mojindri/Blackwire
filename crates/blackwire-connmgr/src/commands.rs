use crate::meta::CloseReason;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseSelector {
    Id(u64),
    User(String),
    Inbound(String),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionCommandResult {
    pub matched: usize,
}
