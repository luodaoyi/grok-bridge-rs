export function Notice({ notice }) {
  if (!notice) return null;
  const isError = notice.tone === "error";
  return (
    <div
      className={`mb-3 rounded-xl border px-3.5 py-2.5 text-xs shadow-[var(--shadow-sm)] ${
        isError
          ? "border-[var(--danger-border)] bg-[var(--notice-error-bg)] text-[var(--danger-text)]"
          : "border-[var(--notice-border)] bg-[var(--notice-bg)] text-[var(--notice-text)]"
      }`}
      role="status"
      aria-live="polite"
    >
      {notice.text}
    </div>
  );
}
