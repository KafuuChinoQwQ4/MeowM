let amountsHidden = false;
export function setAmountsHidden(hidden: boolean): void {
  amountsHidden = hidden;
}
export function displayAmount(value: string): string {
  return amountsHidden ? '••••' : value;
}
const currencyExponents: Record<string, number> = { JPY: 0, KRW: 0 };
export function currencyExponent(code: string): number {
  return currencyExponents[code] ?? 2;
}
export function money(value: string, exponent = 2): string {
  if (amountsHidden) return '••••';
  const [integer, decimal = ''] = value.split('.');
  const fraction = exponent ? `.${decimal.slice(0, exponent).padEnd(exponent, '0')}` : '';
  return `${integer.replace(/\B(?=(\d{3})+(?!\d))/g, ',')}${fraction}`;
}
export function formatCurrency(
  value: string,
  code = 'CNY',
  exponent = currencyExponent(code),
): string {
  const amount = money(value, exponent);
  return code === 'CNY' ? `¥ ${amount}` : `${code} ${amount}`;
}
export function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}
export function moveDate(value: string, days: number): string {
  const d = new Date(`${value}T12:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}
export function validDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value < '0001-01-01') return false;
  const date = new Date(`${value}T12:00:00Z`);
  return !Number.isNaN(date.getTime()) && date.toISOString().slice(0, 10) === value;
}
export function validAmount(value: string, reference = false, exponent = 2): boolean {
  const fraction = exponent ? `(?:\\.\\d{1,${exponent}})?` : '';
  return (
    new RegExp(`^${reference ? '-?' : ''}\\d+${fraction}$`).test(value) &&
    (reference || /[1-9]/.test(value))
  );
}
