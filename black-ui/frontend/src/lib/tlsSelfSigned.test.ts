import { describe, expect, it } from "vitest";
import { buildTlsSelfSignedInput, defaultTlsSelfSignedValues, expectedTlsSelfSignedPaths } from "./tlsSelfSigned";

describe("tls self-signed helper", () => {
  it("builds the backend payload from the server name", () => {
    const payload = buildTlsSelfSignedInput(defaultTlsSelfSignedValues("panel.example.com"));

    expect(payload).toEqual({ serverName: "panel.example.com", days: 365 });
  });

  it("predicts deterministic server-side paths for public IP deployments", () => {
    const paths = expectedTlsSelfSignedPaths("169.40.15.126");

    expect(paths).toEqual({
      certificateFile: "/etc/blackwire/certs/169.40.15.126.crt",
      keyFile: "/etc/blackwire/certs/169.40.15.126.key"
    });
  });

  it("clamps invalid validity days for the backend request", () => {
    expect(buildTlsSelfSignedInput({ serverName: "", days: "bad" })).toEqual({ serverName: "example.com", days: 365 });
    expect(buildTlsSelfSignedInput({ serverName: "example.com", days: "99999" })).toEqual({ serverName: "example.com", days: 3650 });
  });

  it("sanitizes predicted filenames", () => {
    expect(expectedTlsSelfSignedPaths("[2001:db8::1]")).toEqual({
      certificateFile: "/etc/blackwire/certs/2001-db8-1.crt",
      keyFile: "/etc/blackwire/certs/2001-db8-1.key"
    });
  });
});
