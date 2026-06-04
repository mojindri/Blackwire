# Advanced Config Panel QA Result

## Run Summary
- Panel URL: `http://127.0.0.1:18180`
- Disposable config path: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-advanced-config\qa-mpypnkcz-config.json`
- Report JSON: `C:\Users\moji\AppData\Local\Temp\black-ui-qa-advanced-config\qa-results-qa-mpypnkcz.json`
- Passed: 11
- Failed: 0
- Skipped: 0

## Results
- PASS: API structured editor - API listener persisted as 127.0.0.1:62789
- PASS: Routing structured editor - routing rules, balancer, and geo files persisted
- PASS: Routing adaptive template - adaptive template generated a schema-compatible routing config
- PASS: DNS structured editor - dns servers and fake_ip persisted
- PASS: TUN structured editor - TUN runtime fields persisted
- PASS: Metrics address structured editor - metricsAddr persisted as 127.0.0.1:19090
- PASS: Profile structured editor - profile persisted as fast
- PASS: Fast structured editor - fast profile tuning persisted
- PASS: Log raw JSON save - log section saved as raw JSON
- PASS: Raw JSON guard (limits) - limits rejected malformed JSON before save
- PASS: Raw JSON guard (stats) - stats rejected malformed JSON before save

## Skipped Cases
- None.

## Restoration
- QA outbounds seeded: qa-mpypnkcz-adv-a, qa-mpypnkcz-adv-b.
- Original enabled outbounds were temporarily disabled so the QA outbounds would be first in template order.
- Config sections restored to the original live values.
- Original outbounds restored to their saved enabled states.
- QA outbounds removed: qa-mpypnkcz-adv-a, qa-mpypnkcz-adv-b.
- Settings restored to their original values.

## Notes
- The live panel was exercised through the structured Advanced Config editor and raw JSON fallback sections.
- Disk assertions were performed against the generated `config.json` after successful saves.

