import { useEffect, useRef } from 'react';
export type View = 'ledger' | 'records' | 'pending' | 'platforms' | 'reports' | 'budgets';
const views: [View, string][] = [
  ['ledger', '账本'],
  ['records', '明细'],
  ['pending', '待确认'],
  ['platforms', '平台管理'],
  ['reports', '报表总览'],
  ['budgets', '预算管理'],
];
export function Navigation({
  view,
  open,
  onOpen,
  onSelect,
}: {
  view: View;
  open: boolean;
  onOpen: (open: boolean) => void;
  onSelect: (view: View) => void;
}) {
  const menu = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onOpen(false);
        menu.current?.focus();
      }
    };
    document.addEventListener('keydown', escape);
    return () => document.removeEventListener('keydown', escape);
  }, [open, onOpen]);
  return (
    <>
      <nav
        className={'side-nav' + (open ? ' open' : '')}
        id="side-nav"
        aria-label="主导航"
        inert={!open}
      >
        {views.map(([id, label]) => (
          <button
            className={'nav-item' + (view === id ? ' active' : '')}
            id={'nav-' + id}
            key={id}
            onClick={() => {
              onSelect(id);
              menu.current?.focus();
            }}
          >
            {label}
          </button>
        ))}
      </nav>
      <button
        ref={menu}
        className="menu-button"
        id="menu"
        aria-label={open ? '关闭导航' : '打开导航'}
        aria-controls="side-nav"
        aria-expanded={open}
        onClick={() => onOpen(!open)}
      >
        ☰
      </button>
      <div className={'nav-scrim' + (open ? ' open' : '')} onClick={() => onOpen(false)} />
    </>
  );
}
