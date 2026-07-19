export const buttonBase =
  "inline-flex min-h-9 max-w-full items-center justify-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-semibold tracking-tight break-words transition-[color,background-color,border-color,box-shadow,transform] duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--focus)] disabled:cursor-not-allowed disabled:opacity-50 motion-reduce:transition-none";

export const secondaryButton = `${buttonBase} border-[var(--button-border)] bg-[var(--button-bg)] text-[var(--button-text)] shadow-[var(--shadow-sm)] hover:border-[var(--button-hover-border)] hover:bg-[var(--button-hover-bg)]`;

export const dangerButton = `${buttonBase} border-[var(--danger-border)] bg-[var(--danger-bg)] text-[var(--danger-text)] shadow-[var(--shadow-sm)] hover:border-[var(--danger-hover-border)] hover:bg-[var(--danger-hover-bg)]`;

export const ghostButton = `${buttonBase} border-transparent bg-transparent text-[var(--muted)] hover:bg-[var(--button-hover-bg)] hover:text-[var(--text)]`;
