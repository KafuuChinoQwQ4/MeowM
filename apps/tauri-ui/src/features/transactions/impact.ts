function cents(amount: string): bigint {
  const found = /^(-?)(\d+)(?:\.(\d{1,2}))?$/.exec(amount);
  if (!found) return 0n;
  const value = BigInt(found[2]) * 100n + BigInt((found[3] ?? '').padEnd(2, '0'));
  return found[1] ? -value : value;
}
export function impact(
  kind: string,
  amount: string,
  original: string,
  accountChanged: boolean,
  dateChanged: boolean,
): string {
  const delta = cents(amount) - cents(original);
  const abs = delta < 0n ? -delta : delta;
  const value = `¥ ${abs / 100n}.${String(abs % 100n).padStart(2, '0')}`;
  const balance =
    kind === 'expense' ? (delta > 0n ? '减少' : '增加') : delta > 0n ? '增加' : '减少';
  const report =
    kind === 'expense'
      ? delta > 0n
        ? '支出增加'
        : '支出减少'
      : kind === 'refund'
        ? delta > 0n
          ? '支出减少'
          : '支出增加'
        : delta > 0n
          ? '收入增加'
          : '收入减少';
  const amountEffect =
    delta === 0n
      ? '金额不变。'
      : kind === 'transfer'
        ? `转出账户余额${delta > 0n ? '减少' : '增加'} ${value}，转入账户余额${delta > 0n ? '增加' : '减少'} ${value}；报表不变。`
        : `账户余额${balance} ${value}，报表${report} ${value}。`;
  return `保存后${amountEffect}${accountChanged ? '账户归属会调整。' : ''}${dateChanged ? '发生日期对应的余额与报表会重新计算。' : ''}`;
}
