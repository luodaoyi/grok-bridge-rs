export function MetaField({ label, children, wide = false, title }) {
  return (
    <div className={`meta-field ${wide ? "meta-field-wide" : ""}`}>
      <span className="subheader meta-label">
        {label}
      </span>
      <code
        className="meta-value"
        title={title}
      >
        {children}
      </code>
    </div>
  );
}
