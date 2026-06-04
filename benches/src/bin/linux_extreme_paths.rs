//! Benchmarks for Linux-specific extreme data-plane code paths (AF_XDP, splice, etc.).

#[cfg(target_os = "linux")]
mod linux {
    use std::env;
    use std::time::{Duration, Instant};

    use anyhow::{Context as _, Result};
    use blackwire_common::splice::{splice_bidirectional_with_backend, SpliceBackendPolicy};
    use blackwire_common::zerocopy::{enable_tcp_zerocopy, write_all_maybe_zerocopy};
    use blackwire_transport::{AfXdpBackend, TunAfXdpConfig};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    const DEFAULT_BYTES: usize = 64 * 1024 * 1024;
    const DEFAULT_ITERS: usize = 3;
    const CHUNK_BYTES: usize = 256 * 1024;
    const TIMEOUT: Duration = Duration::from_secs(30);

    struct Row {
        name: &'static str,
        bytes: usize,
        iterations: usize,
        elapsed: Duration,
        notes: String,
    }

    impl Row {
        fn throughput_mib_s(&self) -> f64 {
            let mib = (self.bytes * self.iterations) as f64 / (1024.0 * 1024.0);
            mib / self.elapsed.as_secs_f64()
        }
    }

    pub async fn main() -> Result<()> {
        let bytes = env_usize("BLACKWIRE_EXTREME_BYTES").unwrap_or(DEFAULT_BYTES);
        let iterations = env_usize("BLACKWIRE_EXTREME_ITERS").unwrap_or(DEFAULT_ITERS);
        let payload = vec![0xA5; CHUNK_BYTES.min(bytes)];

        println!("# Linux Extreme Path Benchmark");
        println!();
        println!("- total bytes per iteration: `{bytes}`");
        println!("- iterations: `{iterations}`");
        println!("- chunk bytes: `{}`", payload.len());
        println!();
        println!("| path | MiB/s | elapsed ms | notes |");
        println!("| --- | ---: | ---: | --- |");

        let copy = bench_write_path("tcp_write_all", bytes, iterations, &payload, false).await?;
        print_row(&copy);

        let zerocopy =
            bench_write_path("tcp_msg_zerocopy", bytes, iterations, &payload, true).await?;
        print_row(&zerocopy);

        let epoll = bench_splice_path(
            "splice_epoll",
            bytes,
            iterations,
            &payload,
            SpliceBackendPolicy::EpollOnly,
        )
        .await?;
        print_row(&epoll);

        match bench_splice_path(
            "splice_io_uring",
            bytes,
            iterations,
            &payload,
            SpliceBackendPolicy::RequireIoUring,
        )
        .await
        {
            Ok(row) => print_row(&row),
            Err(err) => println!(
                "| splice_io_uring | n/a | n/a | unavailable: {} |",
                escape_md(&err.to_string())
            ),
        }

        match env::var("BLACKWIRE_AF_XDP_IFACE") {
            Ok(interface) if !interface.is_empty() => match bench_af_xdp_open(&interface) {
                Ok(row) => print_row(&row),
                Err(err) => println!(
                    "| af_xdp_open | n/a | n/a | unavailable on `{}`: {} |",
                    escape_md(&interface),
                    escape_md(&err.to_string())
                ),
            },
            _ => println!(
                "| af_xdp_open | n/a | n/a | skipped: set `BLACKWIRE_AF_XDP_IFACE` for privileged AF_XDP bring-up |"
            ),
        }

        Ok(())
    }

    async fn bench_write_path(
        name: &'static str,
        bytes: usize,
        iterations: usize,
        payload: &[u8],
        zerocopy: bool,
    ) -> Result<Row> {
        let mut used_zerocopy = 0usize;
        let mut fallback = 0usize;
        let start = Instant::now();
        for _ in 0..iterations {
            let (mut writer, reader) = tcp_pair().await?;
            let reader = tokio::spawn(async move { drain_tcp(reader, bytes).await });
            let enabled = if zerocopy {
                enable_tcp_zerocopy(&writer).context("enable TCP zerocopy")?
            } else {
                false
            };
            let mut remaining = bytes;
            while remaining > 0 {
                let n = remaining.min(payload.len());
                if zerocopy {
                    let report =
                        write_all_maybe_zerocopy(&mut writer, &payload[..n], enabled, 1).await?;
                    if report.used_zerocopy {
                        used_zerocopy += 1;
                    }
                    if report.fallback_used {
                        fallback += 1;
                    }
                } else {
                    writer.write_all(&payload[..n]).await?;
                }
                remaining -= n;
            }
            writer.shutdown().await?;
            tokio::time::timeout(TIMEOUT, reader)
                .await
                .context("write path reader timed out")?
                .context("write path reader task failed")??;
        }
        let notes = if zerocopy {
            format!("zerocopy_reports={used_zerocopy}, fallback_reports={fallback}")
        } else {
            "baseline userspace write_all".to_string()
        };
        Ok(Row {
            name,
            bytes,
            iterations,
            elapsed: start.elapsed(),
            notes,
        })
    }

    async fn bench_splice_path(
        name: &'static str,
        bytes: usize,
        iterations: usize,
        payload: &[u8],
        policy: SpliceBackendPolicy,
    ) -> Result<Row> {
        let start = Instant::now();
        for _ in 0..iterations {
            let (mut client, mut inbound) = tcp_pair().await?;
            let (mut outbound, sink) = tcp_pair().await?;
            let sink = tokio::spawn(async move { drain_tcp(sink, bytes).await });
            let relay = tokio::spawn(async move {
                splice_bidirectional_with_backend(&mut inbound, &mut outbound, policy).await
            });

            let mut remaining = bytes;
            while remaining > 0 {
                let n = remaining.min(payload.len());
                client.write_all(&payload[..n]).await?;
                remaining -= n;
            }
            client.shutdown().await?;

            tokio::time::timeout(TIMEOUT, sink)
                .await
                .context("splice sink timed out")?
                .context("splice sink task failed")??;
            let (up, down) = tokio::time::timeout(TIMEOUT, relay)
                .await
                .context("splice relay timed out")?
                .context("splice relay task failed")??;
            anyhow::ensure!(
                up == bytes as u64 || down == bytes as u64,
                "splice relayed unexpected byte counts: up={up} down={down}"
            );
        }
        Ok(Row {
            name,
            bytes,
            iterations,
            elapsed: start.elapsed(),
            notes: format!("policy={policy:?}"),
        })
    }

    fn bench_af_xdp_open(interface: &str) -> Result<Row> {
        // AF_XDP binds exclusive state to a NIC queue. Repeated open/drop cycles
        // can race queue teardown on virtual NICs, so measure one bring-up unless
        // a caller explicitly asks for stress iterations.
        let iterations = env_usize("BLACKWIRE_AF_XDP_ITERS").unwrap_or(1);
        let mut zero_copy_available = None;
        let start = Instant::now();
        for _ in 0..iterations {
            let backend = AfXdpBackend::open(&TunAfXdpConfig {
                interface: Some(interface.to_string()),
                ..TunAfXdpConfig::default()
            })?;
            zero_copy_available = Some(backend.capabilities().zero_copy_available);
        }
        Ok(Row {
            name: "af_xdp_open",
            bytes: 0,
            iterations,
            elapsed: start.elapsed(),
            notes: format!(
                "interface={interface}, zero_copy_available={}",
                zero_copy_available.unwrap_or(false)
            ),
        })
    }

    async fn tcp_pair() -> Result<(TcpStream, TcpStream)> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let (client, accepted) = tokio::join!(TcpStream::connect(addr), listener.accept());
        Ok((client?, accepted?.0))
    }

    async fn drain_tcp(mut stream: TcpStream, expected: usize) -> Result<()> {
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0usize;
        while total < expected {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            total += n;
        }
        anyhow::ensure!(
            total == expected,
            "drained {total} bytes, expected {expected}"
        );
        Ok(())
    }

    fn env_usize(name: &str) -> Option<usize> {
        env::var(name).ok()?.parse().ok()
    }

    fn print_row(row: &Row) {
        let elapsed_ms = row.elapsed.as_secs_f64() * 1000.0;
        let throughput = if row.bytes == 0 {
            "n/a".to_string()
        } else {
            format!("{:.2}", row.throughput_mib_s())
        };
        println!(
            "| {} | {} | {:.2} | {} |",
            row.name,
            throughput,
            elapsed_ms,
            escape_md(&row.notes)
        );
    }

    fn escape_md(value: &str) -> String {
        value.replace('|', "\\|")
    }
}

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    linux::main().await
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("linux_extreme_paths is only available on Linux");
}
