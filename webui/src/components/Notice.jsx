export function Notice({ notice }) {
  if (!notice) return null;
  const isError = notice.tone === "error";
  return (
    <div
      className={`alert ${isError ? "alert-danger" : "alert-success"}`}
      role="status"
      aria-live="polite"
    >
      {notice.text}
    </div>
  );
}
