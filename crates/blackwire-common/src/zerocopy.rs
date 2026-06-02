//! Linux MSG_ZEROCOPY helpers for raw TCP bulk writes.
//!
//! This module intentionally stays below protocol/transport code. Callers must
//! opt in only after they already know they own a plain `tokio::net::TcpStream`.

/// Default payload floor before MSG_ZEROCOPY is worth attempting.
pub const DEFAULT_ZEROCOPY_MIN_BYTES: usize = 16 * 1024;

/// Result details for a single write operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZeroCopyWriteReport {
    pub bytes: usize,
    pub used_zerocopy: bool,
    pub fallback_used: bool,
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{ZeroCopyWriteReport, DEFAULT_ZEROCOPY_MIN_BYTES};
    use std::io;
    use std::mem;
    use std::os::fd::AsRawFd;

    use tokio::io::{AsyncWriteExt, Interest};
    use tokio::net::TcpStream;

    const CONTROL_BYTES: usize = 256;

    pub fn enable_tcp_zerocopy(stream: &TcpStream) -> io::Result<bool> {
        let value: libc::c_int = 1;
        let rc = unsafe {
            libc::setsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ZEROCOPY,
                &value as *const _ as *const libc::c_void,
                mem::size_of_val(&value) as libc::socklen_t,
            )
        };
        if rc == 0 {
            Ok(true)
        } else {
            let err = io::Error::last_os_error();
            if is_unsupported(&err) {
                Ok(false)
            } else {
                Err(err)
            }
        }
    }

    pub async fn write_all_maybe_zerocopy(
        stream: &mut TcpStream,
        buf: &[u8],
        zerocopy_enabled: bool,
        min_bytes: usize,
    ) -> io::Result<ZeroCopyWriteReport> {
        let min_bytes = min_bytes.max(1);
        if !zerocopy_enabled || buf.len() < min_bytes {
            stream.write_all(buf).await?;
            return Ok(ZeroCopyWriteReport {
                bytes: buf.len(),
                used_zerocopy: false,
                fallback_used: false,
            });
        }

        let fd = stream.as_raw_fd();
        let mut written = 0usize;
        let mut fallback_used = false;

        while written < buf.len() {
            stream.writable().await?;
            let chunk = &buf[written..];
            match stream.try_io(Interest::WRITABLE, || send_zerocopy(fd, chunk)) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "MSG_ZEROCOPY send returned zero",
                    ));
                }
                Ok(n) => {
                    written += n;
                    drain_zerocopy_error_queue(fd);
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                Err(err) if is_unsupported(&err) => {
                    fallback_used = true;
                    stream.write_all(&buf[written..]).await?;
                    return Ok(ZeroCopyWriteReport {
                        bytes: buf.len(),
                        used_zerocopy: written > 0,
                        fallback_used,
                    });
                }
                Err(err) if is_pressure_error(&err) => {
                    fallback_used = true;
                    stream.write_all(&buf[written..]).await?;
                    return Ok(ZeroCopyWriteReport {
                        bytes: buf.len(),
                        used_zerocopy: written > 0,
                        fallback_used,
                    });
                }
                Err(err) => return Err(err),
            }
        }

        drain_zerocopy_error_queue(fd);
        Ok(ZeroCopyWriteReport {
            bytes: buf.len(),
            used_zerocopy: true,
            fallback_used,
        })
    }

    fn send_zerocopy(fd: libc::c_int, buf: &[u8]) -> io::Result<usize> {
        let rc = unsafe {
            libc::send(
                fd,
                buf.as_ptr() as *const libc::c_void,
                buf.len(),
                libc::MSG_ZEROCOPY | libc::MSG_NOSIGNAL,
            )
        };
        if rc >= 0 {
            Ok(rc as usize)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn drain_zerocopy_error_queue(fd: libc::c_int) {
        loop {
            let mut byte = [0u8; 1];
            let mut iov = libc::iovec {
                iov_base: byte.as_mut_ptr() as *mut libc::c_void,
                iov_len: byte.len(),
            };
            let mut control = [0u8; CONTROL_BYTES];
            let mut msg: libc::msghdr = unsafe { mem::zeroed() };
            msg.msg_iov = &mut iov;
            msg.msg_iovlen = 1;
            msg.msg_control = control.as_mut_ptr() as *mut libc::c_void;
            msg.msg_controllen = control.len();

            let rc =
                unsafe { libc::recvmsg(fd, &mut msg, libc::MSG_ERRQUEUE | libc::MSG_DONTWAIT) };
            if rc < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    break;
                }
                break;
            }
        }
    }

    fn is_unsupported(err: &io::Error) -> bool {
        matches!(
            err.raw_os_error(),
            Some(libc::EINVAL | libc::ENOPROTOOPT | libc::EOPNOTSUPP)
        )
    }

    fn is_pressure_error(err: &io::Error) -> bool {
        matches!(err.raw_os_error(), Some(libc::ENOBUFS | libc::ENOMEM))
    }

    pub fn normalized_min_bytes(min_bytes: usize) -> usize {
        if min_bytes == 0 {
            DEFAULT_ZEROCOPY_MIN_BYTES
        } else {
            min_bytes
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::ZeroCopyWriteReport;
    use std::io;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;

    pub fn enable_tcp_zerocopy(_stream: &TcpStream) -> io::Result<bool> {
        Ok(false)
    }

    pub async fn write_all_maybe_zerocopy(
        stream: &mut TcpStream,
        buf: &[u8],
        _zerocopy_enabled: bool,
        _min_bytes: usize,
    ) -> io::Result<ZeroCopyWriteReport> {
        stream.write_all(buf).await?;
        Ok(ZeroCopyWriteReport {
            bytes: buf.len(),
            used_zerocopy: false,
            fallback_used: false,
        })
    }

    pub fn normalized_min_bytes(min_bytes: usize) -> usize {
        min_bytes.max(1)
    }
}

pub use imp::{enable_tcp_zerocopy, normalized_min_bytes, write_all_maybe_zerocopy};
