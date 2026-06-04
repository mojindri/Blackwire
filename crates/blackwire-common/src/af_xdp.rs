//! Experimental AF_XDP capability probe.
//!
//! AF_XDP is a packet-path backend, not a drop-in replacement for the TCP
//! stream relay. This module deliberately exposes only capability discovery so
//! higher layers can gate future privileged packet benchmarks/configs without
//! changing normal proxy behavior.

use std::io;

/// Outcome of probing whether AF_XDP sockets are usable on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfXdpSupport {
    /// Kernel supports AF_XDP and the process has sufficient privilege.
    Available,
    /// Kernel does not support AF_XDP (missing CONFIG_XDP_SOCKETS or old kernel).
    Unsupported,
    /// Kernel supports AF_XDP but the process lacks the required capability.
    PermissionDenied,
}

/// Probe whether AF_XDP sockets are usable on the current host.
#[cfg(target_os = "linux")]
pub fn probe_af_xdp_support() -> io::Result<AfXdpSupport> {
    const AF_XDP: libc::c_int = 44;

    let fd = unsafe {
        libc::socket(
            AF_XDP,
            libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if fd >= 0 {
        unsafe {
            libc::close(fd);
        }
        return Ok(AfXdpSupport::Available);
    }

    let err = io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EAFNOSUPPORT | libc::EPROTONOSUPPORT | libc::EINVAL) => {
            Ok(AfXdpSupport::Unsupported)
        }
        Some(libc::EPERM | libc::EACCES) => Ok(AfXdpSupport::PermissionDenied),
        _ => Err(err),
    }
}

/// Probe whether AF_XDP sockets are usable on the current host.
#[cfg(not(target_os = "linux"))]
pub fn probe_af_xdp_support() -> io::Result<AfXdpSupport> {
    Ok(AfXdpSupport::Unsupported)
}
