# Outbounds Panel QA Result

## Run Summary
- Panel URL: `http://127.0.0.1:18180/`
- Disposable config path: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json`
- Report JSON: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-results-qa-mpyotapi.json`
- Passed: 19
- Failed: 0
- Skipped: 2

## Results
- PASS: Freedom/default - qa-mpyotapi-freedom persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: VLESS/TCP - qa-mpyotapi-vless-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: VLESS/WS - qa-mpyotapi-vless-ws persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: VLESS/gRPC - qa-mpyotapi-vless-grpc persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: VLESS/HTTPUpgrade - qa-mpyotapi-vless-httpupgrade persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: VLESS/SplitHTTP - qa-mpyotapi-vless-splithttp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- SKIPPED: VLESS/KCP - kcp transport selector could not be reached cleanly
- SKIPPED: VLESS/QUIC - quic transport selector could not be reached cleanly
- PASS: VLESS/TCP+TLS - qa-mpyotapi-vless-tls persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: VLESS/TCP+REALITY - qa-mpyotapi-vless-reality persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: VMess/TCP - qa-mpyotapi-vmess-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: Trojan/TCP - qa-mpyotapi-trojan-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: Shadowsocks/TCP - qa-mpyotapi-shadowsocks-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: Hysteria2/TCP - qa-mpyotapi-hysteria2-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpyotapi-config.json
- PASS: Advanced JSON preserve - qa-mpyotapi-advanced-preserve preserved unknown keys while structured fields changed
- PASS: Invalid UUID guard - invalid UUID blocked before save
- PASS: Invalid JSON guard - malformed advanced JSON blocked before save
- PASS: Missing password guard - trojan password guard shown
- PASS: Delete flow - qa-mpyotapi-delete removed from config output
- PASS: Disabled outbound omission - qa-mpyotapi-toggle omitted from generated config when disabled
- PASS: No-enabled fallback - generated config fell back to synthetic freedom outbound

## Routing
- PASS: Adaptive routing workflow - routing balancer references 14 enabled outbounds

## Skipped Cases
- VLESS/KCP: kcp transport selector could not be reached cleanly
- VLESS/QUIC: quic transport selector could not be reached cleanly

## Notes
- The live panel was exercised through the structured outbound editor and Advanced Config routing workflow.
- Disk assertions were performed against the generated `config.json` after successful saves.

