//! Conservative rules for the supported WeChat and Alipay notification formats.
use ledger_domain::TransactionKind;
use regex::Regex;
use std::sync::LazyLock;

pub fn direction(text: &str) -> Option<TransactionKind> {
    if [
        "优惠",
        "广告",
        "满减",
        "失败",
        "取消",
        "待付款",
        "付款码",
        "验证码",
        "支付请求",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        return None;
    }
    if ["退款到账", "退款成功", "退回"]
        .iter()
        .any(|s| text.contains(s))
    {
        return Some(TransactionKind::Refund);
    }
    if text.contains("转账") {
        return Some(TransactionKind::Transfer);
    }
    if ["收款", "收到", "收钱", "入账"]
        .iter()
        .any(|s| text.contains(s))
    {
        return Some(TransactionKind::Income);
    }
    if ["付款", "支付成功", "消费", "支出", "扣款"]
        .iter()
        .any(|s| text.contains(s))
    {
        return Some(TransactionKind::Expense);
    }
    None
}
pub fn merchant(text: &str) -> Option<String> {
    static MERCHANT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:向([^\n，,。；;：:￥¥\d]{1,100}?)(?:付款|支付)|商户[：:]\s*([^\n，,。；;￥¥\d]{1,100}))").unwrap()
    });
    let captures = MERCHANT.captures(text)?;
    let name = captures.get(1).or_else(|| captures.get(2))?.as_str().trim();
    (!name.is_empty()).then(|| name.to_owned())
}
