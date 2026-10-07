//! Explicitly whitelisted payment app notification parser dispatch.
pub const WECHAT_PACKAGE: &str = "com.tencent.mm";
pub const ALIPAY_PACKAGE: &str = "com.eg.android.AlipayGphone";

pub(crate) mod alipay;
pub(crate) mod wechat;
