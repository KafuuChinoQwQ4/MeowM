export const statusLabel = { open: '待核对', reviewed: '已逐项核对', closed: '已关闭' };
export const localTime = (value: string) =>
  new Date(value).toLocaleString('zh-CN', { hour12: false });
