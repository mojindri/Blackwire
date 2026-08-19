# Fast Profile

Fast is a latency-first runtime profile. It deliberately accepts a narrower
set of configurations than `compat`, Blackwire's default broad-compatibility
profile. It does not weaken authentication, TLS/REALITY validation, timeouts,
or parser checks.

Configure the saved profile in **Black UI → Settings → Runtime & observability**.
Blackwire stores that selection in the desired MySQL revision. For a temporary
process-level override, start the runtime with:

```sh
blackwire run --profile fast
```

The CLI override wins for that process only; it does not edit your stored
revision.

## When You Should Use It

Use `compat` unless you have measured a latency or throughput problem on a
supported path. Choose Fast only when you can accept a reduced protocol and
transport surface in exchange for a simpler hot path.

| Profile | Purpose |
| --- | --- |
| `compat` | Default. Broad protocol/transport compatibility and interop work. |
| `fast` | Narrow, latency-first path with stricter validation. |

The other profiles shown in Settings are workload-oriented defaults. They do
not make an untested protocol or transport supported; consult the
[Feature Matrix](feature-matrix.md) first.

## Fast-Profile Rules

When Fast is selected, Blackwire validates the stored revision at startup.

| Setting | Fast behavior |
| --- | --- |
| VLESS inbound | Allowed. |
| VMess inbound | Rejected. |
| TCP transport | Allowed. |
| WebSocket, gRPC, SplitHTTP, or TUN | Rejected. |
| TLS or REALITY | Allowed. |
| No transport security | Rejected in strict production mode; warned about only when you explicitly disable strict production for a lab. |
| Sniffing or FakeIP | Rejected. |
| `IpOnDemand` routing strategy | Rejected. |
| Large routing rule sets | Warned about; keep your rules focused. |
| Freedom or VLESS outbound | Allowed. |

Validation is fail-closed: an invalid Fast revision does not start. Runtime
status explains the problem so you can adjust the revision in Black UI.

## Fast-Path Tuning

In **Black UI → Settings → Performance policies**, you can enable
**Fast-path tuning**. Leave it off unless you are investigating a measured
problem. The normal profile choice is safer than manually changing relay
internals.

When enabled, these controls are stored relationally with the revision:

| Control | Default guidance |
| --- | --- |
| Strict production mode | Keep on for public deployments. |
| Pool policy | Leave at the default unless a targeted benchmark proves it helps your workload. |
| Splice policy | Keep adaptive; it uses the best available safe relay path. |
| Relay engine and flush policy | Keep the supplied defaults unless profiling identifies a bottleneck. |
| Buffer sizes | Do not increase them blindly; they affect memory per active connection. |
| Linux zero-copy and io_uring | Experimental or host-dependent; enable only on prepared Linux hosts with measurements. |

Fast Profile keeps extra compatibility work off the hot path. It never skips
REALITY key checks, TLS certificate validation, UUID/password validation, or
protocol error handling.

## Security And Operations

- Do not expose an unauthenticated or unencrypted inbound to the public
  internet.
- Treat `security: none` as an internal disposable-lab case, never an operator
  recipe.
- Check Runtime after changing profiles to confirm the automatic reload or
  in-process handover completed.
- Benchmark in an environment resembling your deployment before tuning. A
  setting that improves one destination or connection pattern can harm another.

## Lab Fixtures

The repository has JSON fixtures for wire tests and latency labs. They are
loaded only into explicitly disposable MySQL databases by lab scripts. They are
not deployable configuration files and must not be copied into production.

See [Latency Lab](latency-lab.md) for benchmark methodology and
[Configuration For Dummies](08-config-for-dummies.md) for the normal Black UI
workflow.
