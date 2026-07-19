export function MetaField({ label, children, wide = false, title }) {
  return (
    <div className={`min-w-0 ${wide ? "flex-1 basis-80" : "basis-36"}`}>
      <span className="mb-0.5 block text-[10px] font-bold tracking-wide text-[var(--faint)] uppercase">
        {label}
      </span>
      <code
        className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-[var(--code-text)]"
        title={title}
      >
        {children}
      </code>
    </div>
  );
}
