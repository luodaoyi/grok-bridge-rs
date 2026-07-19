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
