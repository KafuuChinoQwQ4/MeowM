import { useState } from 'react';

export interface WatchItem {
  code: string;
  label: string;
}

function valid(provider: 'binance' | 'ifind', code: unknown): code is string {
  return (
    typeof code === 'string' &&
    (provider === 'binance'
      ? /^[\p{L}\p{N}]{1,64}$/u.test(code)
      : /^\d{6}\.(?:SH|SZ|BJ)$/.test(code))
  );
}

export function useWatchlist(provider: 'binance' | 'ifind') {
  const key = `meowm.market.${provider}.watchlist.v1`;
  const [state, setState] = useState<{ items: WatchItem[]; error: string }>(() => {
    try {
      const value: unknown = JSON.parse(localStorage.getItem(key) ?? '[]');
      if (!Array.isArray(value) || value.length > 100) throw new Error();
      const items = value.filter(
        (item): item is WatchItem =>
          item &&
          valid(provider, item.code) &&
          typeof item.label === 'string' &&
          item.label.length <= 100,
      );
      return { items: [...new Map(items.map((item) => [item.code, item])).values()], error: '' };
    } catch {
      return { items: [], error: '无法读取本机自选列表，请重新添加或检查存储权限。' };
    }
  });
  const toggle = (item: WatchItem) => {
    if (!valid(provider, item.code)) return;
    const exists = state.items.some((row) => row.code === item.code);
    if (!exists && state.items.length >= 100) {
      setState({ ...state, error: '最多可保存 100 个自选，请先取消部分收藏。' });
      return;
    }
    const items = exists
      ? state.items.filter((row) => row.code !== item.code)
      : [...state.items, item];
    try {
      localStorage.setItem(key, JSON.stringify(items));
      setState({ items, error: '' });
    } catch {
      setState({ ...state, error: '无法保存本机自选，请检查存储空间或权限后重试。' });
    }
  };
  return { ...state, toggle };
}
