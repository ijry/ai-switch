import { X } from "lucide-react";
import { useEffect, useRef, type FormEvent, type ReactNode } from "react";

export type QuickEditDialogProps = {
  /** Rendered above the field; names the account being edited. */
  subtitle: string;
  title: string;
  children: ReactNode;
  error?: string | null;
  saving?: boolean;
  onClose: () => void;
  onSubmit: () => void;
};

/**
 * One-field editor for a value the account list already shows. Deliberately not
 * a popover: the list lives in an `overflow-x-hidden` scroll region and the row
 * name sits in a `truncate` box, both of which would clip an anchored panel.
 *
 * Borders are invisible in this app (no CSS reset, so `border-*` only sets a
 * width), so the header and footer are separated by fill, not hairlines, and the
 * dialog itself is outlined with `ring-1`.
 */
export function QuickEditDialog({
  subtitle,
  title,
  children,
  error,
  saving = false,
  onClose,
  onSubmit,
}: QuickEditDialogProps) {
  const dialogRef = useRef<HTMLFormElement>(null);
  const closeRef = useRef(onClose);
  const savingRef = useRef(saving);

  closeRef.current = onClose;
  savingRef.current = saving;

  useEffect(() => {
    const previousFocus =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    // The field, not the dialog: the whole point is to type a number and press
    // Enter without reaching for the mouse.
    const field = dialogRef.current?.querySelector<HTMLElement>("input, select");
    (field ?? dialogRef.current)?.focus();
    if (field instanceof HTMLInputElement) {
      field.select();
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !savingRef.current) {
        event.preventDefault();
        closeRef.current();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      previousFocus?.focus();
    };
  }, []);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSubmit();
  };

  return (
    <div
      className="motion-overlay fixed inset-0 z-[80] flex items-center justify-center bg-stone-950/45 p-4 backdrop-blur-sm"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !saving) {
          onClose();
        }
      }}
    >
      <form
        aria-labelledby="quick-edit-dialog-title"
        aria-modal="true"
        className="w-full max-w-sm overflow-hidden rounded-lg bg-white shadow-2xl ring-1 ring-stone-300/70"
        // The field carries `min`/`step` for the spinner, but validation is ours:
        // native interactive validation would silently swallow the submit and pop
        // a browser bubble instead of the message this dialog renders.
        noValidate
        onSubmit={submit}
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header className="flex items-start justify-between gap-3 bg-stone-50 px-4 py-3">
          <div className="min-w-0">
            {/* No `uppercase` here even though the app's dialog eyebrows use it:
                this one is an account name, and mangling its case makes it read
                like a different account. */}
            <p className="truncate text-[11px] font-semibold tracking-wide text-stone-400">
              {subtitle}
            </p>
            <h2
              className="mt-0.5 text-[14px] font-semibold text-stone-950"
              id="quick-edit-dialog-title"
            >
              {title}
            </h2>
          </div>
          <button
            aria-label={`关闭${title}`}
            className="grid h-7 w-7 shrink-0 place-items-center text-stone-500 motion-control hover:bg-stone-200 hover:text-stone-900 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 disabled:opacity-50"
            disabled={saving}
            onClick={onClose}
            title="关闭"
            type="button"
          >
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </header>
        <div className="grid gap-2 px-4 py-4">{children}</div>
        {error ? (
          <p
            className="mx-4 mb-3 rounded-lg bg-red-50 px-3 py-2 text-[12px] font-semibold text-red-700"
            role="alert"
          >
            {error}
          </p>
        ) : null}
        <footer className="flex items-center justify-end gap-2 bg-stone-50 px-4 py-3">
          <button
            className="rounded-lg bg-white px-3 py-1.5 text-[12px] font-semibold text-stone-700 ring-1 ring-stone-300 motion-control hover:bg-stone-100 disabled:opacity-50"
            disabled={saving}
            onClick={onClose}
            type="button"
          >
            取消
          </button>
          <button
            className="rounded-lg bg-stone-900 px-3 py-1.5 text-[12px] font-semibold text-white motion-control hover:bg-stone-800 disabled:opacity-50"
            disabled={saving}
            type="submit"
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </footer>
      </form>
    </div>
  );
}
