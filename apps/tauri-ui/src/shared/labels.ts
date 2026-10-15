export const platformCategories: Record<string, string> = {
  wallet: '支付钱包',
  bank: '银行账户',
  securities: '证券账户',
  exchange: '虚拟币交易所',
  cash: '现金',
  other: '其他',
};
export const sourceNames: Record<string, string> = {
  wechat: '微信',
  alipay: '支付宝',
  bank: '银行卡',
  notification: '通知',
  manual: '手动',
  recurring: '周期账单',
};
export const kindNames: Record<string, string> = {
  expense: '支出',
  income: '收入',
  transfer: '转账',
  refund: '退款',
  adjustment: '余额调整 / 撤销',
};
export const editableKinds = Object.entries(kindNames).filter(([id]) => id !== 'adjustment');
