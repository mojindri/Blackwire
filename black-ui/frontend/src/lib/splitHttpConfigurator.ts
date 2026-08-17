export type JsonObject = Record<string, unknown>;

export interface SplitHttpEditorState {
  path: string; hosts: string; method: string; mode: string; uplinkHttpMethod: string; headers: string;
  paddingKind: "none" | "fixed" | "range" | "bounds"; paddingFixed: string; paddingRange: string;
  paddingMin: string; paddingMax: string; paddingFrom: string; paddingTo: string;
  paddingMethod: string; paddingHeader: string; paddingKey: string; paddingPlacement: string;
  sessionPlacement: string; sessionKey: string; seqPlacement: string; seqKey: string;
  uplinkDataPlacement: string; uplinkDataKey: string; uplinkChunkSize: string; scMaxBufferedPosts: string;
  xmuxEnabled: boolean; xmuxMaxConcurrency: string; xmuxMaxConnections: string; xmuxCMaxReuseTimes: string;
  xmuxHMaxRequestTimes: string; xmuxHMaxReusableSecs: string; xmuxHKeepAlivePeriod: string;
  downloadEnabled: boolean; downloadNetwork: string; downloadSecurity: string;
}

export function readSplitHttp(value: unknown): SplitHttpEditorState {
  const source = object(value), padding = source.xPaddingBytes, bounds = object(padding), xmux = object(source.xmux), download = object(source.downloadSettings);
  const paddingKind = typeof padding === "number" ? "fixed" : typeof padding === "string" ? "range" : Object.keys(bounds).length ? "bounds" : "none";
  return {
    path: string(source.path), hosts: strings(source.host).join("\n"), method: string(source.method), mode: string(source.mode), uplinkHttpMethod: string(source.uplinkHTTPMethod), headers: headerText(object(source.headers)),
    paddingKind, paddingFixed: typeof padding === "number" ? String(padding) : "", paddingRange: typeof padding === "string" ? padding : "", paddingMin: number(bounds.min), paddingMax: number(bounds.max), paddingFrom: number(bounds.from), paddingTo: number(bounds.to),
    paddingMethod: string(source.xPaddingMethod), paddingHeader: string(source.xPaddingHeader), paddingKey: string(source.xPaddingKey), paddingPlacement: string(source.xPaddingPlacement),
    sessionPlacement: string(source.sessionPlacement), sessionKey: string(source.sessionKey), seqPlacement: string(source.seqPlacement), seqKey: string(source.seqKey), uplinkDataPlacement: string(source.uplinkDataPlacement), uplinkDataKey: string(source.uplinkDataKey), uplinkChunkSize: number(source.uplinkChunkSize), scMaxBufferedPosts: number(source.scMaxBufferedPosts),
    xmuxEnabled: Object.prototype.hasOwnProperty.call(source, "xmux"), xmuxMaxConcurrency: number(xmux.maxConcurrency), xmuxMaxConnections: number(xmux.maxConnections), xmuxCMaxReuseTimes: number(xmux.cMaxReuseTimes), xmuxHMaxRequestTimes: number(xmux.hMaxRequestTimes), xmuxHMaxReusableSecs: number(xmux.hMaxReusableSecs), xmuxHKeepAlivePeriod: number(xmux.hKeepAlivePeriod),
    downloadEnabled: Object.prototype.hasOwnProperty.call(source, "downloadSettings"), downloadNetwork: string(download.network), downloadSecurity: string(download.security)
  };
}

export function buildSplitHttp(state: SplitHttpEditorState, existing: unknown): JsonObject {
  const result = { ...object(existing) };
  setString(result, "path", state.path || "/"); setList(result, "host", state.hosts); setString(result, "method", state.method); setString(result, "mode", state.mode); setString(result, "uplinkHTTPMethod", state.uplinkHttpMethod);
  const headers = parseHeaders(state.headers); if (Object.keys(headers).length) result.headers = headers; else delete result.headers;
  if (state.paddingKind === "fixed" && state.paddingFixed.trim()) result.xPaddingBytes = Number(state.paddingFixed);
  else if (state.paddingKind === "range" && state.paddingRange.trim()) result.xPaddingBytes = state.paddingRange.trim();
  else if (state.paddingKind === "bounds") { const bounds: JsonObject = {}; setNumber(bounds, "min", state.paddingMin); setNumber(bounds, "max", state.paddingMax); setNumber(bounds, "from", state.paddingFrom); setNumber(bounds, "to", state.paddingTo); if (Object.keys(bounds).length) result.xPaddingBytes = bounds; else delete result.xPaddingBytes; }
  else delete result.xPaddingBytes;
  for (const [key, value] of [["xPaddingMethod",state.paddingMethod],["xPaddingHeader",state.paddingHeader],["xPaddingKey",state.paddingKey],["xPaddingPlacement",state.paddingPlacement],["sessionPlacement",state.sessionPlacement],["sessionKey",state.sessionKey],["seqPlacement",state.seqPlacement],["seqKey",state.seqKey],["uplinkDataPlacement",state.uplinkDataPlacement],["uplinkDataKey",state.uplinkDataKey]] as const) setString(result, key, value);
  setNumber(result, "uplinkChunkSize", state.uplinkChunkSize); setNumber(result, "scMaxBufferedPosts", state.scMaxBufferedPosts);
  if (state.xmuxEnabled) { const xmux: JsonObject = {}; for (const [key, value] of [["maxConcurrency",state.xmuxMaxConcurrency],["maxConnections",state.xmuxMaxConnections],["cMaxReuseTimes",state.xmuxCMaxReuseTimes],["hMaxRequestTimes",state.xmuxHMaxRequestTimes],["hMaxReusableSecs",state.xmuxHMaxReusableSecs],["hKeepAlivePeriod",state.xmuxHKeepAlivePeriod]] as const) setNumber(xmux, key, value); result.xmux = xmux; } else delete result.xmux;
  if (state.downloadEnabled) { const download: JsonObject = {}; setString(download, "network", state.downloadNetwork); setString(download, "security", state.downloadSecurity); result.downloadSettings = download; } else delete result.downloadSettings;
  return result;
}

function object(value: unknown): JsonObject { return value && typeof value === "object" && !Array.isArray(value) ? { ...(value as JsonObject) } : {}; }
function string(value: unknown) { return typeof value === "string" ? value : ""; }
function number(value: unknown) { return typeof value === "number" && Number.isFinite(value) ? String(value) : ""; }
function strings(value: unknown) { return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []; }
function setString(target: JsonObject, key: string, value: string) { if (value.trim()) target[key] = value.trim(); else delete target[key]; }
function setNumber(target: JsonObject, key: string, value: string) { if (value.trim() && Number.isFinite(Number(value))) target[key] = Number(value); else delete target[key]; }
function setList(target: JsonObject, key: string, value: string) { const list = value.split("\n").map((item) => item.trim()).filter(Boolean); if (list.length) target[key] = list; else delete target[key]; }
function headerText(value: JsonObject) { return Object.entries(value).map(([key, item]) => `${key}: ${String(item)}`).join("\n"); }
function parseHeaders(value: string) { const result: Record<string,string> = {}; for (const line of value.split("\n")) { const at = line.indexOf(":"); if (at > 0) result[line.slice(0, at).trim()] = line.slice(at + 1).trim(); } return result; }
