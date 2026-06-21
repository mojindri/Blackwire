use std::net::{IpAddr, SocketAddr};

use anyhow::{Context as _, Result};

pub(crate) fn listen_socket_addr(listen: IpAddr, port: u16) -> SocketAddr {
    SocketAddr::new(listen, port)
}

pub(crate) fn socket_addr_from_address_port(
    address: &str,
    port: u64,
    context: &str,
) -> Result<SocketAddr> {
    let ip: IpAddr = address
        .parse()
        .with_context(|| format!("{context}: address '{address}' is not an IP literal"))?;
    let port =
        u16::try_from(port).with_context(|| format!("{context}: port {port} is out of range"))?;
    Ok(SocketAddr::new(ip, port))
}
