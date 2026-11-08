import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
export type View =
  | 'ledger'
  | 'records'
  | 'pending'
  | 'finance'
  | 'settings'
  | 'sources'
  | 'security'
  | 'platforms'
  | 'reports'
  | 'exchange'
  | 'markets'
  | 'budgets'
  | 'notifications'
  | 'connections'
  | 'recurring'
  | 'search'
  | 'reconciliation'
  | 'data'
  | 'privacy';
const primaryViews: View[] = [
  'ledger',
  'records',
  'pending',
  'exchange',
  'markets',
  'finance',
  'settings',
];
const destinations: Record<View, { title: string; parent?: View; children?: View[] }> = {
  ledger: { title: '账本' },
  records: { title: '明细' },
  pending: { title: '待确认' },
  finance: {
    title: '财务管理',
    children: ['reports', 'budgets', 'recurring', 'search', 'reconciliation'],
  },
  settings: {
    title: '设置',
    children: ['sources', 'security'],
  },
  sources: {
    title: '平台与采集',
    parent: 'settings',
    children: ['platforms', 'notifications', 'connections'],
  },
  security: {
    title: '安全与数据',
    parent: 'settings',
    children: ['privacy', 'data'],
  },
  platforms: {
    title: '平台管理',
    parent: 'sources',
  },
  notifications: {
    title: '通知采集',
    parent: 'sources',
  },
  connections: {
    title: '支付 API',
    parent: 'sources',
  },
  privacy: {
    title: '隐私与数据',
    parent: 'security',
  },
  data: { title: '备份与导出', parent: 'security' },
  reports: { title: '报表总览', parent: 'finance' },
  exchange: { title: '法币汇率' },
  markets: { title: '交易所' },
  budgets: { title: '预算管理', parent: 'finance' },
  recurring: { title: '周期账单', parent: 'finance' },
  search: { title: '搜索与分类', parent: 'finance' },
  reconciliation: { title: '账户对账', parent: 'finance' },
};

export function parentView(view: View) {
  return destinations[view].parent;
}

function primaryView(view: View): View {
  const parent = parentView(view);
  return parent ? primaryView(parent) : view;
}

export function NavigationDirectory({
  view,
  onSelect,
}: {
  view: View;
  onSelect: (view: View) => void;
}) {
  const destination = destinations[view];
  if (!destination.children) return null;
  return (
    <section className="navigation-directory" aria-label={destination.title}>
      <h2>{destination.title}</h2>
      <div className="directory-entries">
        {destination.children.map((id) => (
          <button
            className="directory-entry"
            id={'nav-' + id}
            key={id}
            onClick={() => onSelect(id)}
          >
            <span>
              <strong>{destinations[id].title}</strong>
            </span>
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="m9 5 7 7-7 7" />
            </svg>
          </button>
        ))}
      </div>
    </section>
  );
}
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
  const nav = useRef<HTMLElement>(null);
  const wasOpen = useRef(false);
  useEffect(() => {
    if (!open) {
      if (wasOpen.current) menu.current?.focus();
      wasOpen.current = false;
      return;
    }
    wasOpen.current = true;
    nav.current?.querySelector<HTMLButtonElement>('.nav-item.active')?.focus();
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onOpen(false);
      }
    };
    document.addEventListener('keydown', escape);
    return () => document.removeEventListener('keydown', escape);
  }, [open, onOpen]);
  return (
    <>
      {createPortal(
        <>
          <nav
            ref={nav}
            className={'side-nav' + (open ? ' open' : '')}
            id="side-nav"
            aria-label="主导航"
            inert={!open}
          >
            {primaryViews.map((id) => (
              <button
                className={'nav-item' + (primaryView(view) === id ? ' active' : '')}
                aria-current={primaryView(view) === id ? 'page' : undefined}
                id={'nav-' + id}
                key={id}
                onClick={() => {
                  onSelect(id);
                }}
              >
                {destinations[id].title}
              </button>
            ))}
          </nav>
          <div className={'nav-scrim' + (open ? ' open' : '')} onClick={() => onOpen(false)} />
        </>,
        document.body,
      )}
      <button
        ref={menu}
        className="menu-button"
        id="menu"
        aria-label="打开导航"
        aria-controls="side-nav"
        aria-expanded={open}
        hidden={open}
        onClick={() => onOpen(true)}
      >
        ☰
      </button>
    </>
  );
}
