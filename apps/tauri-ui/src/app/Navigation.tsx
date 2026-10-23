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
  | 'budgets'
  | 'notifications'
  | 'recurring'
  | 'search'
  | 'reconciliation'
  | 'data'
  | 'privacy';
const primaryViews: View[] = ['ledger', 'records', 'pending', 'finance', 'settings'];
const destinations: Record<
  View,
  { title: string; description: string; parent?: View; children?: View[] }
> = {
  ledger: { title: '账本', description: '查看每日收支与余额' },
  records: { title: '明细', description: '查看和编辑交易流水' },
  pending: { title: '待确认', description: '检查导入和采集的候选记录' },
  finance: {
    title: '财务管理',
    description: '查看统计、管理预算，整理账单和流水。',
    children: ['reports', 'budgets', 'recurring', 'search', 'reconciliation'],
  },
  settings: {
    title: '设置',
    description: '按类别管理平台、采集、隐私与本机数据。',
    children: ['sources', 'security'],
  },
  sources: {
    title: '平台与采集',
    description: '管理记账平台和自动采集来源。',
    parent: 'settings',
    children: ['platforms', 'notifications'],
  },
  security: {
    title: '安全与数据',
    description: '设置隐私保护，备份、恢复或清理数据。',
    parent: 'settings',
    children: ['privacy', 'data'],
  },
  platforms: { title: '平台管理', description: '新增、编辑和选择记账平台', parent: 'sources' },
  notifications: {
    title: '通知采集',
    description: '通知来源、采集权限与屏幕识别',
    parent: 'sources',
  },
  privacy: {
    title: '隐私与数据',
    description: '应用锁、金额隐藏与数据清理',
    parent: 'security',
  },
  data: { title: '备份与导出', description: '加密备份、恢复与筛选导出', parent: 'security' },
  reports: { title: '报表总览', description: '收支趋势、分类与商户统计', parent: 'finance' },
  budgets: { title: '预算管理', description: '设置预算并查看使用进度', parent: 'finance' },
  recurring: { title: '周期账单', description: '管理重复账单与到期提醒', parent: 'finance' },
  search: { title: '搜索与分类', description: '搜索流水、整理分类和匹配规则', parent: 'finance' },
  reconciliation: { title: '账户对账', description: '核对平台余额与历史流水', parent: 'finance' },
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
      <p>{destination.description}</p>
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
              <small>{destinations[id].description}</small>
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
