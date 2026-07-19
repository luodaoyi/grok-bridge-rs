import { describe, expect, it } from "vitest";
import { decodeBase64ToUint8Array } from "./base64.js";

describe("decodeBase64ToUint8Array", () => {
  it("decodes ASCII payload", () => {
    const bytes = decodeBase64ToUint8Array(btoa("hello"));
    expect(Array.from(bytes)).toEqual([104, 101, 108, 108, 111]);
  });

  it("returns empty for empty or invalid input", () => {
    expect(decodeBase64ToUint8Array("").length).toBe(0);
    expect(decodeBase64ToUint8Array(null).length).toBe(0);
    expect(decodeBase64ToUint8Array("@@@").length).toBe(0);
  });
});
