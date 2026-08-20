import type { Hysteria2TuningState } from "../../lib/hysteria2Tuning";
import { Input } from "../atoms/Input";
import { Switch } from "../atoms/Switch";
import { Field } from "../molecules/Field";

export function Hysteria2TuningFields({
  direction,
  value,
  onChange
}: {
  direction: "inbound" | "outbound";
  value: Hysteria2TuningState;
  onChange: (update: Partial<Hysteria2TuningState>) => void;
}) {
  return <>
    <p className="field-hint">This {direction} overrides Blackwire's automatic QUIC socket sizing. Native datagrams are managed automatically.</p>
    <div className="configurator-grid">
      {direction === "outbound" ? <Field label="Endpoint shards" hint="Optional parallel QUIC endpoints, 1-64.">
        <Input value={value.hysteria2EndpointShards ?? "1"} onChange={(e) => onChange({ hysteria2EndpointShards: e.target.value })} placeholder="1" />
      </Field> : null}
      <Switch checked={value.hysteria2QuicReusePort} onChange={(hysteria2QuicReusePort) => onChange({ hysteria2QuicReusePort })} label="QUIC reuse port" />
      <Field label="QUIC endpoints" hint="Number or cpu. Leave empty for default.">
        <Input value={value.hysteria2QuicEndpoints} onChange={(e) => onChange({ hysteria2QuicEndpoints: e.target.value })} placeholder="1" />
      </Field>
      <Field label="Receive buffer bytes">
        <Input value={value.hysteria2QuicRecvBufferBytes} onChange={(e) => onChange({ hysteria2QuicRecvBufferBytes: e.target.value })} placeholder="8388608" />
      </Field>
      <Field label="Send buffer bytes">
        <Input value={value.hysteria2QuicSendBufferBytes} onChange={(e) => onChange({ hysteria2QuicSendBufferBytes: e.target.value })} placeholder="8388608" />
      </Field>
    </div>
  </>;
}
