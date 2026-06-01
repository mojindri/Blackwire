use std::time::Duration;

use crate::meta::{CloseReason, Protocol, Transport};

pub fn describe_metrics() {
    metrics::describe_gauge!(
        "blackwire_connections_active",
        metrics::Unit::Count,
        "Currently active managed connections"
    );
    metrics::describe_counter!(
        "blackwire_connections_closed_total",
        metrics::Unit::Count,
        "Managed connections closed by reason"
    );
    metrics::describe_histogram!(
        "blackwire_connections_lifetime_seconds",
        metrics::Unit::Seconds,
        "Managed connection lifetime in seconds"
    );
    metrics::describe_counter!(
        "blackwire_connections_bytes_total",
        metrics::Unit::Bytes,
        "Managed connection bytes by direction, protocol, and transport"
    );
}

pub fn record_open() {
    metrics::gauge!("blackwire_connections_active").increment(1.0);
}

pub fn record_close(reason: CloseReason, lifetime: Duration) {
    metrics::gauge!("blackwire_connections_active").decrement(1.0);
    metrics::counter!(
        "blackwire_connections_closed_total",
        "reason" => reason.as_str()
    )
    .increment(1);
    metrics::histogram!("blackwire_connections_lifetime_seconds").record(lifetime.as_secs_f64());
}

pub fn record_bytes(protocol: Protocol, transport: Transport, up: u64, down: u64) {
    metrics::counter!(
        "blackwire_connections_bytes_total",
        "direction" => "up",
        "protocol" => protocol.as_str(),
        "transport" => transport.as_str()
    )
    .increment(up);
    metrics::counter!(
        "blackwire_connections_bytes_total",
        "direction" => "down",
        "protocol" => protocol.as_str(),
        "transport" => transport.as_str()
    )
    .increment(down);
}
