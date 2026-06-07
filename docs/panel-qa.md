# Black UI Panel QA Results

These are manual/desktop QA summaries for Black UI structured editors.

## Outbounds Panel

- Panel URL: `http://127.0.0.1:18180/`
- Disposable config path: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json`
- Report JSON: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-results-qa-mpz8zjmb.json`
- Passed: 21
- Failed: 0
- Skipped: 0

Passed cases:

- Freedom/default
- VLESS/TCP
- VLESS/WS
- VLESS/gRPC
- VLESS/HTTPUpgrade
- VLESS/SplitHTTP
- VLESS/KCP
- VLESS/QUIC
- VLESS/TCP+TLS
- VLESS/TCP+REALITY
- VMess/TCP
- Trojan/TCP
- Shadowsocks/TCP
- Hysteria2/TCP
- Advanced JSON preserve
- Invalid UUID guard
- Invalid JSON guard
- Missing password guard
- Delete flow
- Disabled outbound omission
- No-enabled fallback
- Adaptive routing workflow

Notes:

- The live panel was exercised through the structured outbound editor and
  Advanced Config routing workflow.
- Disk assertions were performed against the generated `config.json` after
  successful saves.

## Advanced Config Panel

- Panel URL: `http://127.0.0.1:18180`
- Disposable config path: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-advanced-config\qa-mpypnkcz-config.json`
- Report JSON: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-advanced-config\qa-results-qa-mpypnkcz.json`
- Passed: 11
- Failed: 0
- Skipped: 0

Passed cases:

- API structured editor
- Routing structured editor
- Routing adaptive template
- DNS structured editor
- TUN structured editor
- Metrics address structured editor
- Profile structured editor
- Fast structured editor
- Log raw JSON save
- Raw JSON guard for `limits`
- Raw JSON guard for `stats`

Restoration:

- QA outbounds seeded: `qa-mpypnkcz-adv-a`, `qa-mpypnkcz-adv-b`.
- Original enabled outbounds were temporarily disabled so the QA outbounds would
  be first in template order.
- Config sections restored to the original live values.
- Original outbounds restored to their saved enabled states.
- QA outbounds removed.
- Settings restored to their original values.

Notes:

- The live panel was exercised through the structured Advanced Config editor and
  raw JSON fallback sections.
- Disk assertions were performed against the generated `config.json` after
  successful saves.
