// Only date labels cross IPC. Rust resolves the device's local timezone to UTC.
export function monthRange(month: string): [string, string] {
  if (!/^\d{4}-\d{2}$/.test(month)) return ['', ''];
  const [year, number] = month.split('-').map(Number);
  if (year < 1 || year > 9998 || number < 1 || number > 12) return ['', ''];
  const next =
    number === 12
      ? `${String(year + 1).padStart(4, '0')}-01`
      : `${String(year).padStart(4, '0')}-${String(number + 1).padStart(2, '0')}`;
  return [month + '-01', next + '-01'];
}
