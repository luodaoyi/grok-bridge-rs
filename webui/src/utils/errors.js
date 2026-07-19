export function errorMessage(error) {
  if (!error) return "unknown error";
  if (error.name === "AbortError") return "请求超时，正在自动重试";
  return error.message || String(error);
}
