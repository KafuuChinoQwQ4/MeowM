import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { errorText } from '../shared/errors';
const DialogContext = createContext<{
  busy: boolean;
  setBusy: (value: boolean) => void;
  close: () => void;
} | null>(null);
export function useDialog() {
  const context = useContext(DialogContext);
  if (!context) throw new Error('Dialog context missing');
  return context;
}
export function Dialog({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [busy, updateBusy] = useState(false);
  const busyRef = useRef(false);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const ownsHistory = useRef(false);
  const setBusy = (value: boolean) => {
    busyRef.current = value;
    updateBusy(value);
  };
  const close = () => {
    if (busyRef.current) return;
    if (ownsHistory.current) history.back();
    else onCloseRef.current();
  };
  useEffect(() => {
    const element = ref.current!;
    const previous = document.activeElement as HTMLElement | null;
    element.showModal();
    history.pushState({ meowmSheet: true }, '');
    ownsHistory.current = true;
    const back = () => {
      ownsHistory.current = false;
      if (busyRef.current) {
        history.pushState({ meowmSheet: true }, '');
        ownsHistory.current = true;
        return;
      }
      onCloseRef.current();
    };
    window.addEventListener('popstate', back);
    return () => {
      window.removeEventListener('popstate', back);
      element.close();
      previous?.focus();
    };
  }, []);
  return (
    <DialogContext value={{ busy, setBusy, close }}>
      <dialog
        id="sheet"
        ref={ref}
        aria-labelledby="sheet-title"
        onCancel={(event) => {
          event.preventDefault();
          close();
        }}
        onClick={(event) => {
          if (event.target !== event.currentTarget) return;
          const rect = event.currentTarget.getBoundingClientRect();
          if (
            event.clientX < rect.left ||
            event.clientX > rect.right ||
            event.clientY < rect.top ||
            event.clientY > rect.bottom
          )
            close();
        }}
      >
        <div className="sheet-head">
          <h2 id="sheet-title">{title}</h2>
          <button className="icon-button" aria-label="关闭" disabled={busy} onClick={close}>
            ×
          </button>
        </div>
        <div id="sheet-content">{children}</div>
      </dialog>
    </DialogContext>
  );
}
export function useFormAction(onSaved: () => void) {
  const { busy, setBusy, close } = useDialog();
  const [error, setError] = useState('');
  const running = useRef(false);
  async function run(operation: () => Promise<unknown>) {
    if (running.current || busy) return;
    running.current = true;
    setBusy(true);
    setError('');
    try {
      await operation();
      setBusy(false);
      close();
      onSaved();
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  return { busy, error, setError, run };
}
