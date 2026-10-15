import { api } from './api';
import { money, validAmount, validDate } from './format';

function element<K extends keyof HTMLElementTagNameMap>(tag:K,text='',cls=''):HTMLElementTagNameMap[K]{const n=document.createElement(tag);n.textContent=text;n.className=cls;return n;}
function input(form:HTMLElement,label:string,type='text',value=''):HTMLInputElement{const wrap=element('label',label);wrap.className='form-field';const field=element('input');field.type=type;field.value=value;field.autocomplete='off';wrap.append(field);form.append(wrap);return field;}
function select(form:HTMLElement,label:string,values:[string,string][],value=''):HTMLSelectElement{const wrap=element('label',label);wrap.className='form-field';const field=element('select');for(const[id,text]of values){const option=element('option',text);option.value=id;field.append(option);}field.value=value;wrap.append(field);form.append(wrap);return field;}
function button(parent:HTMLElement,label:string,run:()=>void,cls='button button-secondary'):HTMLButtonElement{const item=element('button',label);item.type='button';item.className=cls;item.addEventListener('click',run);parent.append(item);return item;}
function modal(title:string):{dialog:HTMLDialogElement;body:HTMLDivElement}{const dialog=element('dialog');const head=element('div');head.className='sheet-head';head.append(element('h2',title));const close=element('button','×');close.type='button';close.className='icon-button';close.setAttribute('aria-label','关闭');close.onclick=()=>dialog.close();head.append(close);const body=element('div');body.className='sheet-form';dialog.append(head,body);dialog.onclose=()=>dialog.remove();document.body.append(dialog);dialog.showModal();return{dialog,body};}
function cents(amount:string):bigint{const found=/^(-?)(\d+)(?:\.(\d{1,2}))?$/.exec(amount);if(!found)return 0n;const value=BigInt(found[2])*100n+BigInt((found[3]??'').padEnd(2,'0'));return found[1]?-value:value;}
function impact(kind:string,amount:string,original:string,accountChanged:boolean,dateChanged:boolean):string{
 const delta=cents(amount)-cents(original);const abs=delta<0n?-delta:delta;const value=`¥ ${abs/100n}.${String(abs%100n).padStart(2,'0')}`;
 const balance=kind==='expense'?(delta>0n?'减少':'增加'):(delta>0n?'增加':'减少');
 const report=kind==='expense'?(delta>0n?'支出增加':'支出减少'):kind==='refund'?(delta>0n?'支出减少':'支出增加'):(delta>0n?'收入增加':'收入减少');
 const amountEffect=delta===0n?'金额不变。':kind==='transfer'?`转出账户余额${delta>0n?'减少':'增加'} ${value}，转入账户余额${delta>0n?'增加':'减少'} ${value}；报表不变。`:`账户余额${balance} ${value}，报表${report} ${value}。`;
 return `保存后${amountEffect}${accountChanged?'账户归属会调整。':''}${dateChanged?'发生日期对应的余额与报表会重新计算。':''}`;
}
const kinds:[string,string][]=[['expense','支出'],['income','收入'],['transfer','转账'],['refund','退款']];

export async function openTransaction(accountId:string,date:string,refresh:()=>Promise<void>,id?:string):Promise<void>{
 const accounts=await api.accounts();const categories=await api.listCategories();const detail=id?await api.transactionDetail(id):null;const tx=detail?.transaction??null;
 const {dialog,body}=modal(tx?'交易详情':'记一笔');
 if(tx&&detail){
  body.append(element('p',`${tx.date} · ${tx.status} · ${tx.kind} · ¥ ${money(tx.amount)}`),element('p',`账户：${accounts.find(a=>a.id===tx.account_id)?.name??tx.account_id}`),element('p',`分类：${categories.find(c=>c.id===tx.category_id)?.name??'未分类'} · 商户：${tx.merchant??'—'} · 备注：${tx.description??'—'}`));
  const linked=[...new Set([...detail.related_ids,...(tx.original_transaction_id?[tx.original_transaction_id]:[]),...(detail.reversal_of?[detail.reversal_of]:[]),...(detail.reversal_id?[detail.reversal_id]:[])])];const links=element('section');links.append(element('h3','关联记录'));if(linked.length)for(const link of linked)button(links,`打开关联记录 ${link.slice(0,8)}`,()=>{dialog.close();void openTransaction(accountId,date,refresh,link).catch(error=>alert(errorText(error)));});else links.append(element('p','没有关联记录'));body.append(links);
  const sources=element('section');sources.append(element('h3','来源'));for(const source of detail.sources)sources.append(element('p',`${source.source}${source.external_id?` · ${source.external_id}`:''}${source.digest?` · 摘要 ${source.digest}`:''}`));body.append(sources);
  const history=element('section');history.append(element('h3','变更历史'));for(const event of detail.history)history.append(element('p',`${event.action} · ${event.time} · ${event.detail}`));body.append(history);
 }
 const form=element('form');body.append(form);const kind=select(form,'类型',kinds,tx?.kind??'expense');if(tx)kind.disabled=true;
 const source=select(form,'账户',accounts.map(a=>[a.id,a.name]),tx?.account_id??accountId);
 const destination=select(form,'转入账户（转账时必填）',[['','请选择账户'],...accounts.map(a=>[a.id,a.name] as [string,string])],tx?.to_account_id??'');
 const day=input(form,'日期','date',tx?.date??date);const amount=input(form,'金额（元）','text',tx?.amount??'');amount.inputMode='decimal';amount.setAttribute('aria-label','金额（元）');amount.required=true;
 const category=select(form,'分类',[['','未分类'],...categories.map(c=>[c.id,c.name] as [string,string])],tx?.category_id??'');
 const merchant=input(form,'商户','text',tx?.merchant??'');merchant.maxLength=100;const description=input(form,'备注','text',tx?.description??'');description.maxLength=200;
 const original=input(form,'退款原消费 ID（退款必填）','text',tx?.original_transaction_id??'');
 if(tx)original.disabled=true;
 const toggleKind=():void=>{destination.closest('label')!.hidden=kind.value!=='transfer';original.closest('label')!.hidden=kind.value!=='refund';};kind.addEventListener('change',toggleKind);toggleKind();
 const error=element('p');error.className='form-error';error.hidden=true;error.setAttribute('role','alert');form.append(error);
 let previewValues='';
 const values=():string=>JSON.stringify([kind.value,source.value,destination.value,day.value,amount.value,category.value,merchant.value,description.value,original.value]);
 const preview=element('button','查看影响并保存');preview.type='button';preview.className='button button-secondary';const confirm=element('button','确认保存修改');confirm.type='button';confirm.className='button button-primary';confirm.hidden=true;form.append(preview,confirm);
 if(tx){body.insertBefore(element('p','编辑会按版本号保存。导入来源字段保持不变。','sheet-description'),form);form.hidden=true;if(tx.status==='confirmed'&&!detail?.reversal_id&&!detail?.reversal_of)button(body,'编辑交易',()=>{form.hidden=false;});else body.append(element('p','此记录不可编辑。','field-hint'));}
 preview.addEventListener('click',()=>{error.hidden=true;if(!validAmount(amount.value)||!validDate(day.value)){error.textContent='请输入有效的正金额和日期。';error.hidden=false;return;}if(kind.value==='transfer'&&(!destination.value||destination.value===source.value)){error.textContent='转账需要选择不同的转入账户。';error.hidden=false;return;}if(kind.value==='refund'&&!original.value.trim()){error.textContent='退款必须关联原消费。';error.hidden=false;return;}body.querySelector('.impact-preview')?.remove();body.insertBefore(element('p',tx?impact(kind.value,amount.value,tx.amount,source.value!==tx.account_id||destination.value!==(tx.to_account_id??''),day.value!==tx.date):`保存后将新增一笔${kind.selectedOptions[0].textContent} ¥ ${money(amount.value)}。${kind.value==='transfer'?'转账只改变两端余额。':kind.value==='refund'?'退款会抵减原消费。':''}`,'impact-preview'),form);previewValues=values();preview.hidden=true;confirm.hidden=false;});
 confirm.addEventListener('click',async()=>{if(values()!==previewValues){error.textContent='内容已修改，请重新查看影响。';error.hidden=false;confirm.hidden=true;preview.hidden=false;return;}confirm.disabled=true;try{await api.saveTransaction({id:tx?.id,expected_version:detail?.edit_version??0,kind:kind.value,amount:amount.value,date:day.value,account_id:source.value,to_account_id:kind.value==='transfer'?destination.value:undefined,category_id:category.value||undefined,merchant:merchant.value.trim()||undefined,description:description.value.trim()||undefined,original_transaction_id:kind.value==='refund'?original.value.trim():undefined});dialog.close();await refresh();}catch(cause){error.textContent=errorText(cause);error.hidden=false;confirm.disabled=false;}});
 if(tx&&detail&&tx.status==='confirmed'&&!detail.reversal_id&&!detail.reversal_of)button(body,'撤销交易',()=>{const reason=window.prompt('填写撤销原因');if(!reason?.trim())return;if(!window.confirm('确认撤销？原交易会保留，并创建可追溯的反向记录。'))return;void api.reverseTransaction(tx.id,reason.trim()).then(()=>{dialog.close();return refresh();}).catch(error=>{errorBox(body,error);});},'button button-danger');
 if(tx&&tx.status!=='confirmed')for(const field of form.querySelectorAll<HTMLInputElement|HTMLSelectElement|HTMLButtonElement>('input,select,button'))field.disabled=true;
}
function errorText(error:unknown):string{return error instanceof Error?error.message:String(error);}
function errorBox(parent:HTMLElement,error:unknown):void{const box=element('p',errorText(error));box.className='form-error';parent.append(box);}
