import { entriesApi, type PageEntry, type PageQuery } from '../../api/entries';
import { invoke } from '../../api/client';
function cell(value: string | null | undefined): string {
  const safe = value ?? '';
  const guarded = /^[\s]*[=+@-]/.test(safe) ? "'" + safe : safe;
  return '"' + guarded.replaceAll('"', '""') + '"';
}
export async function exportSearch(query: PageQuery): Promise<void> {
  const rows: PageEntry[] = [];
  for (let offset = 0; ; offset += 200) {
    const page = await entriesApi.query({ ...query, offset, limit: 200 });
    rows.push(...page.items);
    if (rows.length >= page.total || !page.items.length) break;
  }
  const header = ['日期', '状态', '类型', '金额', '商户', '备注', '来源', '流水 ID'];
  const lines = [
    header,
    ...rows.map((r) => [
      r.occurred_at,
      r.status,
      r.kind,
      r.amount ?? '',
      r.merchant ?? '',
      r.description ?? '',
      r.sources.join('、'),
      r.id,
    ]),
  ];
  const content = '\uFEFF' + lines.map((row) => row.map(cell).join(',')).join('\r\n');
  if (/Android/i.test(navigator.userAgent)) {
    await invoke<void>('save_search_csv', { csv: content });
    return;
  }
  const blob = new Blob([content], {
    type: 'text/csv;charset=utf-8',
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = 'meowm-search.csv';
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
