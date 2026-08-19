import type { Hysteria2TuningState } from "../../lib/hysteria2Tuning";
import { Input, Select } from "../atoms/Input";
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
    <p className="field-hint">This {direction} now overrides Blackwire's global Hysteria2 transport defaults.</p>
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
    <div className="configurator-grid">
      <Switch checked={value.hysteria2DatagramEnabled} onChange={(hysteria2DatagramEnabled) => onChange({ hysteria2DatagramEnabled })} label="Datagram UDP relay" />
      <Switch checked={value.hysteria2DatagramUdpOverDatagram} onChange={(hysteria2DatagramUdpOverDatagram) => onChange({ hysteria2DatagramUdpOverDatagram })} label="UDP over datagram" />
      <Field label="Datagram policy">
        <Select value={value.hysteria2DatagramPolicy} onChange={(e) => onChange({ hysteria2DatagramPolicy: e.target.value })}>
          <option value="standard">standard</option>
          <option value="h2-plus">h2-plus</option>
        </Select>
      </Field>
      <Field label="FEC mode">
        <Select value={value.hysteria2FecMode} onChange={(e) => onChange({ hysteria2FecMode: e.target.value })}>
          <option value="off">off</option>
          <option value="auto">auto</option>
          <option value="xor1-of-n">xor1-of-n</option>
          <option value="reed-solomon">reed-solomon</option>
          <option value="raptor-like">raptor-like</option>
        </Select>
      </Field>
      <Field label="FEC overhead percent">
        <Input value={value.hysteria2FecMaxOverheadPercent} onChange={(e) => onChange({ hysteria2FecMaxOverheadPercent: e.target.value })} placeholder="20" />
      </Field>
    </div>
  </>;
}
