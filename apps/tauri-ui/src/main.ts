import './styles.css';
import { api, type Account, type DayView } from './api';
import { money, today, moveDate, validDate, validAmount } from './format';

const root = document.querySelector<HTMLDivElement>('#app')!;
// This shell is static. All database and input text is assigned through textContent.
root.innerHTML = `
  <main class="app-shell"><nav class="side-nav" id="side-nav" aria-label="主导航" inert><button class="nav-item active" id="nav-ledger" type="button">账本</button><button class="nav-item" id="nav-records" type="button">明细</button><button class="nav-item" id="nav-platforms" type="button">平台管理</button></nav>
    <section class="ledger" aria-label="日常账本"><button class="back-button" id="back" type="button" aria-label="返回上一级">‹</button>
      <div class="ledger-toolbar"><div class="account-field"><label for="account">当前平台</label><div class="account-select-wrap"><span aria-hidden="true">▣</span><select id="account" aria-label="当前平台"></select></div></div><div class="toolbar-actions"><button class="button button-primary compact-add" id="add-entry"><span aria-hidden="true">＋</span> 记一笔</button></div></div>
      <div class="date-bar"><div class="date-controls"><button class="icon-button" id="previous" aria-label="前一天">‹</button><label class="date-label"><span class="sr-only">记账日期</span><input type="date" id="date" min="0001-01-01" max="9999-12-31"></label><button class="icon-button" id="next" aria-label="后一天">›</button></div><button class="today-button" id="today">回到今天</button></div>
      <div id="feedback" role="status" aria-live="polite"></div>
      <div id="day-content" aria-busy="false"></div>
    </section>
  </main>
  <div class="action-dock"><span class="dock-context" id="dock-context"></span></div>
  <dialog id="sheet" aria-labelledby="sheet-title"><div class="sheet-head"><div><h2 id="sheet-title"></h2></div><button class="icon-button" id="close-sheet" aria-label="关闭">×</button></div><div id="sheet-content"></div></dialog>
  <button class="menu-button" id="menu" type="button" aria-label="打开导航" aria-controls="side-nav" aria-expanded="false">☰</button>
  <div class="nav-scrim" id="nav-scrim"></div>
  <div class="toast" id="toast" role="status" aria-live="polite" hidden></div>`;

function get<T extends HTMLElement = HTMLElement>(id: string): T { return document.getElementById(id) as T; }
function node<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text = ''): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag); el.className = className; el.textContent = text; return el;
}
function button(text: string, className: string, action: () => void): HTMLButtonElement {
  const el = node('button', className, text); el.type = 'button'; el.addEventListener('click', action); return el;
}
function errorText(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return '操作暂时未完成，请稍后重试。';
}
const accountSelect = get<HTMLSelectElement>('account');
const dateInput = get<HTMLInputElement>('date');
const dialog = get<HTMLDialogElement>('sheet');
let accounts: Account[] = [];
let selectedAccount = '';
let selectedDate = today();
let day: DayView | null = null;
let loading = true;
let saving = false;
let requestId = 0;
let hasSheetHistory = false;
let activeView: 'ledger' | 'records' | 'platforms' = 'ledger';
let recordPage = 0;
let platformPage = 0;
let platformFilter = '';
const platformCategories: Record<string, string> = { wallet: '支付钱包', bank: '银行账户', securities: '证券账户', exchange: '虚拟币交易所', cash: '现金', other: '其他' };
function pageSize(): number { return Math.max(1, Math.min(6, Math.floor((window.innerHeight - 330) / 62))); }
let toastTimer: ReturnType<typeof setTimeout>;
dateInput.value = selectedDate;

function context(): string { return `${accounts.find(a => a.id === selectedAccount)?.name ?? ''} · ${selectedDate}`; }
function notify(message: string): void {
  const toast = get('toast'); toast.textContent = message; toast.hidden = false;
  clearTimeout(toastTimer); toastTimer = setTimeout(() => { toast.hidden = true; }, 3200);
}
function toggleNavigation(open: boolean): void {
  get('side-nav').classList.toggle('open', open); get('side-nav').inert = !open;
  get('nav-scrim').classList.toggle('open', open);
  get('menu').setAttribute('aria-expanded', String(open));
  get('menu').setAttribute('aria-label', open ? '关闭导航' : '打开导航');
  get<HTMLButtonElement>('back').disabled = !open && activeView === 'ledger';
  if (!open) get('menu').focus();
}
function setView(view: 'ledger' | 'records' | 'platforms'): void { activeView = view; get('nav-ledger').classList.toggle('active', view === 'ledger'); get('nav-records').classList.toggle('active', view === 'records'); get('nav-platforms').classList.toggle('active', view === 'platforms'); toggleNavigation(false); renderDay(); }
function controls(): void {
  get<HTMLButtonElement>('back').disabled = saving || (activeView === 'ledger' && !get('side-nav').classList.contains('open'));
  document.querySelector('.ledger-toolbar')!.classList.toggle('platform-mode', activeView === 'platforms');
  document.querySelector('.date-bar')!.classList.toggle('platform-mode', activeView === 'platforms');
  accountSelect.disabled = saving || accounts.length === 0;
  dateInput.disabled = saving;
  for (const id of ['previous', 'next', 'today']) get<HTMLButtonElement>(id).disabled = saving;
  get<HTMLButtonElement>('previous').disabled ||= selectedDate <= '0001-01-01';
  get<HTMLButtonElement>('next').disabled ||= selectedDate >= '9999-12-31';
  get<HTMLButtonElement>('add-entry').disabled = saving || loading || !selectedAccount || !day;
  get('dock-context').textContent = selectedAccount ? context() : '先创建一个平台，开始记录';
  get('day-content').setAttribute('aria-busy', String(loading));
}
function feedback(message = '', retry?: () => void): void {
  const el = get('feedback'); el.replaceChildren(); el.className = message ? 'feedback' : '';
  if (message) el.append(node('span', '', message));
  if (retry) el.append(button('重新加载', 'text-button', retry));
}
function renderDay(): void {
  const content = get('day-content'); content.replaceChildren(); controls();
  if (activeView === 'platforms' && !loading) { renderPlatforms(content); return; }
  if (loading) {
    const state = node('div', 'empty-state loading-state'); state.append(node('span', 'loader'), node('p', '', '正在加载…')); content.append(state); return;
  }
  if (!selectedAccount) {
    const state = node('div', 'empty-state'); state.append(node('span', 'empty-art', '＋'), node('h2', '', '从第一个平台开始'), node('p', '', '为微信、支付宝或银行卡建一个独立账本。'), button('创建我的第一个平台', 'button button-primary', openAccount)); content.append(state); return;
  }
  if (!day) return;
  const summary = node('div', 'summary-grid');
  for (const [label, value, kind, symbol] of [['当日收入', day.income, 'income', '↙'], ['当日支出', day.expense, 'expense', '↗'], ['当日净额', day.net, 'net', '≈']]) {
    const card = node('section', `summary-card ${kind}`); const title = node('div', 'summary-title'); title.append(node('span', '', label), node('span', 'summary-icon', symbol));
    const amount = node('div', 'summary-amount'); amount.append(node('span', 'currency', '¥'), node('span', '', money(value))); card.append(title, amount); summary.append(card);
  }
  if (activeView === 'ledger') content.append(summary);
  const balance = node('section', 'balance-panel'); const balanceInfo = node('div', 'balance-info');
  balanceInfo.append(node('span', 'balance-label', '日末余额'), node('strong', 'balance-value', day.balance === null ? '尚未设置' : `¥ ${money(day.balance)}`));
  balanceInfo.append(node('p', 'balance-hint', day.reference ? `基于 ${day.reference.date} 的参考余额 ¥ ${money(day.reference.amount)}` : '设置参考余额后，即可查看每日结余。'));
  const balanceActions = node('div', 'balance-actions'); balanceActions.append(button(day.reference ? '调整参考余额' : '设置参考余额', 'text-button', openReference));
  if (day.reference) balanceActions.append(button('清除', 'text-button muted-button', openClear));
  balance.append(balanceInfo, balanceActions); if (activeView === 'ledger') content.append(balance);
  const records = node('section', 'records'); const heading = node('div', 'records-heading');
  const title = node('h2', '', '当日明细'); title.append(node('span', 'count-badge', String(day.entries.length))); heading.append(title); records.append(heading);
  if (!day.entries.length) {
    const state = node('div', 'empty-state entries-empty'); state.append(node('span', 'empty-art', '≋'), node('h3', '', '暂无记录'), node('p', '', '')); records.append(state);
  } else {
    const list = node('ul', 'entry-list');
    const size = pageSize();
    recordPage = Math.min(recordPage, Math.max(0, Math.ceil(day.entries.length / size) - 1));
    for (const entry of day.entries.slice(recordPage * size, (recordPage + 1) * size)) {
      const income = entry.kind === 'income'; const item = node('li', 'entry');
      const icon = node('span', `entry-icon ${income ? 'income' : 'expense'}`, income ? '↙' : '↗'); icon.setAttribute('aria-hidden', 'true');
      const body = node('div', 'entry-body'); body.append(node('h3', '', entry.note || (income ? '收入' : '支出')), node('p', '', `${income ? '收入' : '支出'} · ${entry.time}`));
      const amount = node('span', `entry-amount ${income ? 'income-text' : ''}`, `${income ? '+' : '−'} ¥${money(entry.amount)}`); item.append(icon, body, amount); list.append(item);
    }
    records.append(list);
    const pages = Math.ceil(day.entries.length / size);
    const pager = node('div', 'record-pager');
    const prev = button('上一页', 'text-button', () => { recordPage--; renderDay(); });
    const next = button('下一页', 'text-button', () => { recordPage++; renderDay(); });
    prev.disabled = recordPage === 0; next.disabled = recordPage >= pages - 1;
    pager.append(prev, node('span', '', `${recordPage + 1} / ${pages}`), next); records.append(pager);
  }
  if (activeView === 'records') content.append(records);
}

async function loadDay(): Promise<void> {
  recordPage = 0;
  const id = ++requestId; const accountId = selectedAccount; const date = selectedDate;
  loading = true; day = null; feedback(); renderDay();
  if (!accountId) { loading = false; renderDay(); return; }
  try { const result = await api.day(accountId, date); if (id !== requestId) return; day = result; }
  catch (error) { if (id !== requestId) return; feedback(`账本加载失败：${errorText(error)}`, () => void loadDay()); }
  finally { if (id === requestId) { loading = false; renderDay(); } }
}
async function loadAccounts(): Promise<void> {
  const id = ++requestId; loading = true; feedback(); renderDay();
  try {
    const result = await api.accounts(); if (id !== requestId) return; accounts = result;
    if (!accounts.some(a => a.id === selectedAccount)) selectedAccount = accounts[0]?.id ?? '';
    renderAccounts(); await loadDay();
  } catch (error) {
    if (id !== requestId) return; loading = false; day = null; renderDay();
    feedback(`无法打开本地账本：${errorText(error)}`, () => void loadAccounts());
  }
}
function renderAccounts(): void {
  accountSelect.replaceChildren();
  if (!accounts.length) { const option = node('option', '', '尚无平台'); option.value = ''; accountSelect.append(option); }
  for (const account of accounts) { const option = node('option', '', account.name); option.value = account.id; accountSelect.append(option); }
  accountSelect.value = selectedAccount; controls();
}
function selectDate(date: string): void {
  if (saving || !validDate(date)) { dateInput.value = selectedDate; return; }
  selectedDate = date; dateInput.value = date; void loadDay();
}
accountSelect.addEventListener('change', () => { if (saving) return; selectedAccount = accountSelect.value; void loadDay(); });
dateInput.addEventListener('change', () => selectDate(dateInput.value));
get('previous').addEventListener('click', () => selectDate(moveDate(selectedDate, -1)));
get('next').addEventListener('click', () => selectDate(moveDate(selectedDate, 1)));
get('today').addEventListener('click', () => selectDate(today()));
get('nav-platforms').addEventListener('click', () => setView('platforms'));
get('add-entry').addEventListener('click', openEntry);
get('nav-ledger').addEventListener('click', () => setView('ledger'));
get('nav-records').addEventListener('click', () => setView('records'));
get('menu').addEventListener('click', () => { toggleNavigation(!get('side-nav').classList.contains('open')); });
document.addEventListener('keydown', e => { if (e.key === 'Escape' && get('side-nav').classList.contains('open')) { e.preventDefault(); toggleNavigation(false); } });
get('nav-scrim').addEventListener('click', () => { toggleNavigation(false); });
get('back').addEventListener('click', () => { if (get('side-nav').classList.contains('open')) toggleNavigation(false); else if (activeView !== 'ledger') setView('ledger'); });
window.addEventListener('resize', () => { if (!dialog.open) renderDay(); });

function openSheet(title: string): HTMLDivElement {
  get('sheet-title').textContent = title; const content = get<HTMLDivElement>('sheet-content'); content.replaceChildren();
  if (!dialog.open) { dialog.showModal(); history.pushState({ meowmSheet: true }, ''); hasSheetHistory = true; }
  return content;
}
function closeSheet(): void {
  if (saving) return;
  if (hasSheetHistory) { history.back(); } else dialog.close();
}
window.addEventListener('popstate', () => {
  hasSheetHistory = false;
  if (saving && dialog.open) { history.pushState({ meowmSheet: true }, ''); hasSheetHistory = true; return; }
  dialog.close();
});
get('close-sheet').addEventListener('click', closeSheet);
dialog.addEventListener('cancel', e => { e.preventDefault(); closeSheet(); });
dialog.addEventListener('click', e => { if (e.target === dialog) { const r = dialog.getBoundingClientRect(); if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) closeSheet(); } });
function formBase(content: HTMLElement, submitLabel: string): { form: HTMLFormElement; fields: HTMLFieldSetElement; error: HTMLElement } {
  const form = node('form', 'sheet-form'); const fields = node('fieldset'); const error = node('p', 'form-error'); error.setAttribute('role', 'alert'); error.hidden = true;
  const submit = node('button', 'button button-primary submit-button', submitLabel); submit.type = 'submit';
  form.append(fields, error, submit); content.append(form); return { form, fields, error };
}
function field(parent: HTMLElement, label: string, name: string, placeholder: string, type = 'text'): HTMLInputElement {
  const wrapper = node('label', 'form-field', label); const input = node('input'); input.name = name; input.type = type; input.placeholder = placeholder;
  input.autocomplete = 'off'; wrapper.append(input); parent.append(wrapper); return input;
}
async function save(form: HTMLFormElement, error: HTMLElement, operation: () => Promise<void>, success: string, refreshAccounts = false): Promise<void> {
  if (saving) return;
  saving = true; error.hidden = true; controls();
  for (const el of form.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLTextAreaElement>('input,button,textarea,select')) el.disabled = true;
  get<HTMLButtonElement>('close-sheet').disabled = true;
  const submit = form.querySelector<HTMLButtonElement>('[type="submit"]')!; const label = submit.textContent; submit.textContent = '正在保存…';
  try {
    await operation(); saving = false; closeSheet(); notify(success);
    if (refreshAccounts) await loadAccounts(); else await loadDay();
  } catch (cause) { error.textContent = errorText(cause); error.hidden = false; }
  finally {
    saving = false; controls(); get<HTMLButtonElement>('close-sheet').disabled = false;
    for (const el of form.querySelectorAll<HTMLInputElement | HTMLButtonElement | HTMLTextAreaElement>('input,button,textarea,select')) el.disabled = false;
    submit.textContent = label;
  }
}
function invalid(error: HTMLElement, message: string, input: HTMLElement): void { error.textContent = message; error.hidden = false; input.focus(); }
function renderPlatforms(content: HTMLElement): void {
  const heading = node('div', 'platform-heading'); heading.append(node('h2', '', '平台管理'), button('＋ 新建平台', 'button button-primary', () => openAccount())); content.append(heading);
  const filter = node('select', 'platform-filter'); filter.setAttribute('aria-label', '筛选平台分类');
  const all = node('option', '', '全部分类'); all.value = ''; filter.append(all);
  for (const [value, text] of Object.entries(platformCategories)) { const option = node('option', '', text); option.value = value; filter.append(option); }
  filter.value = platformFilter; filter.addEventListener('change', () => { platformFilter = filter.value; platformPage = 0; renderDay(); }); content.append(filter);
  const filtered = accounts.filter(a => !platformFilter || a.category === platformFilter);
  const size = Math.max(1, Math.min(6, Math.floor((innerHeight - 270) / 84)));
  const pages = Math.max(1, Math.ceil(filtered.length / size)); platformPage = Math.min(platformPage, pages - 1);
  const list = node('div', 'platform-list');
  for (const account of filtered.slice(platformPage * size, (platformPage + 1) * size)) {
    const row = node('div', 'platform-row'); const info = node('div', 'platform-info');
    info.append(node('strong', '', account.name), node('span', '', platformCategories[account.category] ?? '其他'));
    const actions = node('div', 'platform-row-actions');
    actions.append(button('编辑', 'text-button', () => openAccount(account)), button('进入', 'text-button', () => { selectedAccount = account.id; renderAccounts(); setView('ledger'); void loadDay(); }));
    row.append(info, actions); list.append(row);
  }
  if (!filtered.length) list.append(node('p', 'empty-platforms', '暂无平台'));
  content.append(list);
  const pager = node('div', 'record-pager');
  const prev = button('上一页', 'text-button', () => { platformPage--; renderDay(); }); const next = button('下一页', 'text-button', () => { platformPage++; renderDay(); });
  prev.disabled = platformPage === 0; next.disabled = platformPage === pages - 1;
  pager.append(prev, node('span', '', `${platformPage + 1} / ${pages}`), next); content.append(pager);
}
function openAccount(existing?: Account): void {
  if (saving) return;
  const content = openSheet(existing ? '编辑平台' : '新建平台');
  const { form, fields, error } = formBase(content, existing ? '保存平台' : '创建平台'); const name = field(fields, '平台名称', 'name', '例如：微信、支付宝、银行卡'); name.required = true; name.maxLength = 40; name.value = existing?.name ?? '';
  const categoryLabel = node('label', 'form-field', '平台分类'); const category = node('select'); category.name = 'category';
  for (const [value, text] of Object.entries(platformCategories)) { const option = node('option', '', text); option.value = value; category.append(option); }
  category.value = existing?.category ?? 'wallet'; categoryLabel.append(category); fields.append(categoryLabel);
  fields.append(node('p', 'field-hint', '当前按人民币记账；分类不代表已接入行情或平台同步。'));
  form.addEventListener('submit', e => { e.preventDefault(); const value = name.value.trim(); if (!value) return invalid(error, '请输入平台名称。', name);
    const selectedCategory = category.value;
    void save(form, error, async () => { if (existing) await api.updatePlatform(existing.id, value, selectedCategory); else { const account = await api.createAccount(value, selectedCategory); selectedAccount = account.id; platformFilter = ''; platformPage = 0; } }, existing ? '平台已更新' : '平台已创建', true);
  }); queueMicrotask(() => name.focus());
}
function openEntry(): void {
  if (saving || loading || !selectedAccount || !day) return;
  const accountId = selectedAccount; const date = selectedDate;
  const content = openSheet('记一笔'); content.append(node('p', 'sheet-context', context()));
  const { form, fields, error } = formBase(content, '保存这一笔');
  const types = node('div', 'type-picker'); types.setAttribute('role', 'group'); types.setAttribute('aria-label', '收支类型');
  let income = false;
  const expenseButton = button('↗ 支出', 'type-option active', () => setKind(false));
  const incomeButton = button('↙ 收入', 'type-option', () => setKind(true));
  function setKind(value: boolean): void { income = value; expenseButton.classList.toggle('active', !value); incomeButton.classList.toggle('active', value); expenseButton.setAttribute('aria-pressed', String(!value)); incomeButton.setAttribute('aria-pressed', String(value)); }
  setKind(false); types.append(expenseButton, incomeButton); fields.append(types);
  const amount = field(fields, '金额（元）', 'amount', '0.00'); amount.inputMode = 'decimal'; amount.required = true; amount.className = 'amount-input';
  const noteLabel = node('label', 'form-field', '备注（选填）'); const note = node('textarea'); note.name = 'note'; note.placeholder = '备注'; note.rows = 3; note.maxLength = 500; noteLabel.append(note); fields.append(noteLabel);
  form.addEventListener('submit', e => { e.preventDefault(); const value = amount.value.trim(); if (!validAmount(value)) return invalid(error, '请输入大于 0 的金额，最多保留两位小数。', amount);
    const noteValue = note.value.trim(); const entryIncome = income;
    void save(form, error, () => api.saveEntry(accountId, date, value, noteValue, entryIncome), '已保存');
  }); queueMicrotask(() => amount.focus());
}
function openReference(): void {
  if (saving || !day || !selectedAccount) return;
  const accountId = selectedAccount; const date = selectedDate;
  const content = openSheet('设置参考余额'); content.append(node('p', 'sheet-context', context()), node('p', 'sheet-description', '输入这一天结束时的实际余额。我们会结合收支记录，推算其他日期的日末余额。'));
  const { form, fields, error } = formBase(content, '保存参考余额'); const amount = field(fields, '当日日末余额（元）', 'balance', '0.00'); amount.inputMode = 'text'; amount.required = true; amount.className = 'amount-input';
  if (day.reference?.date === date) amount.value = day.reference.amount;
  fields.append(node('p', 'field-hint', '可以是零或负数；新设置会替换此平台原有的参考余额。'));
  form.addEventListener('submit', e => { e.preventDefault(); const value = amount.value.trim(); if (!validAmount(value, true)) return invalid(error, '请输入有效余额，最多保留两位小数。', amount);
    void save(form, error, () => api.setReference(accountId, date, value), '参考余额已更新');
  }); queueMicrotask(() => amount.focus());
}
function openClear(): void {
  if (saving || !day?.reference || !selectedAccount) return;
  const accountId = selectedAccount; const content = openSheet('清除参考余额？'); content.append(node('p', 'sheet-context', context()), node('p', 'sheet-description', '清除后，此平台将不再显示日末余额。已有的收入和支出记录会保留，之后可以重新设置。'));
  const { form, fields, error } = formBase(content, '确认清除'); fields.append(button('保留参考余额', 'button button-secondary', closeSheet));
  form.addEventListener('submit', e => { e.preventDefault(); void save(form, error, () => api.clearReference(accountId), '参考余额已清除'); });
}
void loadAccounts();
