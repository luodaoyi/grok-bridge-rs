/** Decode a standard base64 string into raw bytes. Empty/invalid → empty buffer. */
export function decodeBase64ToUint8Array(base64) {
  if (typeof base64 !== "string" || base64.length === 0) {
    return new Uint8Array(0);
  }
  try {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  } catch {
    return new Uint8Array(0);
  }
}

/**
 * Encode a JS string as UTF-8 bytes then standard base64.
 * Matches server decode_write_data (raw byte path, no charset munging).
 */
export function encodeUtf8ToBase64(text) {
  const bytes = new TextEncoder().encode(String(text ?? ""));
  return encodeBytesToBase64(bytes);
}

/** Encode raw bytes as standard base64. */
export function encodeBytesToBase64(bytes) {
  if (!bytes || bytes.length === 0) return "";
  const chunkSize = 0x8000;
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    const slice = bytes.subarray(offset, offset + chunkSize);
    binary += String.fromCharCode(...slice);
  }
  return btoa(binary);
}

/** Max raw payload size accepted by Runtime write_raw (64 KiB). */
export const MAX_WRITE_BYTES = 64 * 1024;

/**
 * Split UTF-8 text into base64 chunks that each decode to ≤ maxBytes.
 * Splits on byte boundaries (not JS string indices).
 */
export function chunkUtf8ToBase64(text, maxBytes = MAX_WRITE_BYTES) {
  const bytes = new TextEncoder().encode(String(text ?? ""));
  if (bytes.length === 0) return [];
  const limit = Math.max(1, Number(maxBytes) || MAX_WRITE_BYTES);
  const chunks = [];
  for (let offset = 0; offset < bytes.length; offset += limit) {
    chunks.push(encodeBytesToBase64(bytes.subarray(offset, offset + limit)));
  }
  return chunks;
}
