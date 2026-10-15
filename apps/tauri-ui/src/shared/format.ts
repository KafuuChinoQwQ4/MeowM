export function money(value: string): string {
  const [integer, decimal = ''] = value.split('.');
  return `${integer.replace(/\B(?=(\d{3})+(?!\d))/g, ',')}.${decimal.padEnd(2, '0')}`;
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
export function validAmount(value: string, reference = false): boolean {
  return (
    (reference ? /^-?\d+(\.\d{1,2})?$/ : /^\d+(\.\d{1,2})?$/).test(value) &&
    (reference || /[1-9]/.test(value))
  );
}
