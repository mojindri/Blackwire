# Outbounds Panel QA Result

## Run Summary
- Panel URL: `http://127.0.0.1:18180/`
- Disposable config path: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json`
- Report JSON: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-results-qa-mpz8zjmb.json`
- Passed: 21
- Failed: 0
- Skipped: 0

## Results
- PASS: Freedom/default - qa-mpz8zjmb-freedom persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/TCP - qa-mpz8zjmb-vless-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/WS - qa-mpz8zjmb-vless-ws persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/gRPC - qa-mpz8zjmb-vless-grpc persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/HTTPUpgrade - qa-mpz8zjmb-vless-httpupgrade persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/SplitHTTP - qa-mpz8zjmb-vless-splithttp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/KCP - qa-mpz8zjmb-vless-kcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/QUIC - qa-mpz8zjmb-vless-quic persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/TCP+TLS - qa-mpz8zjmb-vless-tls persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VLESS/TCP+REALITY - qa-mpz8zjmb-vless-reality persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: VMess/TCP - qa-mpz8zjmb-vmess-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: Trojan/TCP - qa-mpz8zjmb-trojan-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: Shadowsocks/TCP - qa-mpz8zjmb-shadowsocks-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: Hysteria2/TCP - qa-mpz8zjmb-hysteria2-tcp persisted to C:\Users\moji\AppData\Local\Temp\black-ui-qa-outbounds\qa-mpz8zjmb-config.json
- PASS: Advanced JSON preserve - qa-mpz8zjmb-advanced-preserve preserved unknown keys while structured fields changed
- PASS: Invalid UUID guard - invalid UUID blocked before save
- PASS: Invalid JSON guard - malformed advanced JSON blocked before save
- PASS: Missing password guard - trojan password guard shown
- PASS: Delete flow - qa-mpz8zjmb-delete removed from config output
- PASS: Disabled outbound omission - qa-mpz8zjmb-toggle omitted from generated config when disabled
- PASS: No-enabled fallback - generated config fell back to synthetic freedom outbound

## Routing
- PASS: Adaptive routing workflow - routing balancer references 15 enabled outbounds

## Skipped Cases
- None.

## Notes
- The live panel was exercised through the structured outbound editor and Advanced Config routing workflow.
- Disk assertions were performed against the generated `config.json` after successful saves.

