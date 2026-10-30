// Only date labels cross IPC. Rust resolves the device's local timezone to UTC.
export function recentDaysRange(anchorDate: string, count: number): [string, string] {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(anchorDate) || !Number.isInteger(count) || count < 1)
    return ['', ''];
  const anchor = new Date(`${anchorDate}T00:00:00Z`);
  if (!Number.isFinite(anchor.getTime()) || anchor.toISOString().slice(0, 10) !== anchorDate)
    return ['', ''];
  const from = new Date(anchor);
  from.setUTCDate(from.getUTCDate() - count + 1);
  const until = new Date(anchor);
  until.setUTCDate(until.getUTCDate() + 1);
  return [from.toISOString().slice(0, 10), until.toISOString().slice(0, 10)];
}

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

function shiftMonth(month: string, delta: number): string {
  const [year, number] = month.split('-').map(Number);
  const serial = year * 12 + number - 1 + delta;
  const nextYear = Math.floor(serial / 12);
  const nextMonth = (serial % 12) + 1;
  return `${String(nextYear).padStart(4, '0')}-${String(nextMonth).padStart(2, '0')}`;
}

export function recentMonthRange(anchorMonth: string, count = 12): [string, string] {
  if (!/^\d{4}-\d{2}$/.test(anchorMonth) || count < 1) return ['', ''];
  const fromMonth = shiftMonth(anchorMonth, -(count - 1));
  return [monthRange(fromMonth)[0], monthRange(anchorMonth)[1]];
}

export function recentYearRange(anchorDate: string, count = 6): [string, string] {
  const year = Number(anchorDate.slice(0, 4));
  if (!Number.isInteger(year) || year < 1 || count < 1) return ['', ''];
  return [
    `${String(year - count + 1).padStart(4, '0')}-01-01`,
    `${String(year + 1).padStart(4, '0')}-01-01`,
  ];
}
