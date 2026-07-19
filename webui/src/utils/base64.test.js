import { describe, expect, it } from "vitest";
import {
  MAX_WRITE_BYTES,
  chunkUtf8ToBase64,
  decodeBase64ToUint8Array,
  encodeUtf8ToBase64,
} from "./base64.js";

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

describe("encodeUtf8ToBase64", () => {
  it("round-trips UTF-8 including CJK and control sequences", () => {
    const samples = ["hello", "中文", "\u001b[A", "a\r\nb\t", "é"];
    for (const sample of samples) {
      const encoded = encodeUtf8ToBase64(sample);
      const decoded = new TextDecoder().decode(decodeBase64ToUint8Array(encoded));
      expect(decoded).toBe(sample);
    }
  });

  it("chunks large pastes under the write limit", () => {
    const text = "x".repeat(MAX_WRITE_BYTES + 100);
    const chunks = chunkUtf8ToBase64(text, MAX_WRITE_BYTES);
    expect(chunks.length).toBe(2);
    const joined = chunks
      .map((chunk) => decodeBase64ToUint8Array(chunk))
      .reduce((acc, part) => {
        const next = new Uint8Array(acc.length + part.length);
        next.set(acc);
        next.set(part, acc.length);
        return next;
      }, new Uint8Array(0));
    expect(joined.length).toBe(text.length);
    expect(chunks.every((chunk) => decodeBase64ToUint8Array(chunk).length <= MAX_WRITE_BYTES)).toBe(
      true,
    );
  });
});
