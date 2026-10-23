# 平台品牌配色表

整理日期：2026-10-21。当前收录 127 个常见平台：41 个支付钱包、27 个加密资产交易所、32 个证券/基金/行情软件及证券交易所、27 家银行。

## 取色与识别规则

- 参考各平台官方品牌资料、官方页面的标识，以及官方开发者发布的 App Store 图标；每行附原始来源。
- 下表是本应用采用的界面色值。部分品牌有多种地区版本或辅助色，这里选取代表性组合，并为小字号文字调整亮度，不宣称全部色值都是官方标准 HEX。
- 币安采用黑金、欧易黑白、同花顺红白、PayPal 深蓝白；总和保留彩色渐变，现金和未识别的平台采用白底深字。
- 根据平台名称自动识别，不依赖所选分类。中英文、常用别名、全角字母和大小写均支持；可添加“现货”“美元”等中文或用空格分开的英文后缀。纯英文别名不会匹配到更长的英文单词内部，例如 gateway、unwise 不会染色。
- 优先匹配较长的名字，例如 AlipayHK 不会误用 Alipay；国泰君安、海通证券等旧称归入国泰海通。名称中没有品牌时，现金分类也使用白色。
- 现有平台直接生效；新增和编辑时可输入名称或别名筛选品牌候选，点选后填入标准名称和分类，也可展开完整列表浏览；小众 App 可直接填写自定义名称。输入时实时预览配色，保存后平台列表和账本顶部保持一致。运行时完全使用本地颜色表。
- 这是常见品牌的整理，未按全球全部软件或市场份额作穷尽统计。后续可在 `apps/tauri-ui/src/shared/platformThemes.ts` 增补别名及来源；不为未收录的名字随机分配颜色。

参考品牌规范包括 [Binance 官方媒体资料](https://www.binance.com/en-GB/press)、[OKX 品牌更新](https://www.okx.com/learn/okex-rebrands-to-okx)、[PayPal 品牌设计方资料](https://www.pentagram.com/work/paypal)、[Wise 官方换色说明](https://wise.com/community/en-US/brand-new-look)、[OSL 官方品牌色](https://www.osl.com/en/brand-assets)、[BHIM 官方品牌指南](https://www.bhimupi.org.in/files/bhim/BHIM-UPI-Guidelines.pdf)。

## 支付钱包

| 平台 | 可识别名称/别名 | 背景 | 文字 | 官方参考 |
| --- | --- | --- | --- | --- |
| 微信 | 微信支付、微信、WeChat Pay、WeChat、Weixin | `#0a8547` | `#ffffff` | [品牌 / App](https://pay.weixin.qq.com/) |
| 支付宝 | 支付宝、Alipay | `#1677d2` | `#ffffff` | [品牌 / App](https://www.alipay.com/) |
| AlipayHK | 支付宝香港、支付寶香港、AlipayHK、Alipay HK | `#6252c7` | `#ffffff` | [品牌 / App](https://apps.apple.com/hk/app/alipayhk-%E6%94%AF%E4%BB%98%E5%AF%B6%E9%A6%99%E6%B8%AF/id1210638245) |
| 云闪付 | 云闪付、云闪、銀聯、银联、UnionPay、QuickPass | `#d71920` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E4%BA%91%E9%97%AA%E4%BB%98-%E9%93%B6%E8%A1%8C%E4%B8%9A%E7%BB%9F%E4%B8%80%E7%A7%BB%E5%8A%A8%E6%94%AF%E4%BB%98app/id600273928) |
| PayPal | PayPal、贝宝、貝寶 | `#003087` | `#ffffff` | [品牌 / App](https://www.paypal.com/) |
| Venmo | Venmo | `#0078bd` | `#ffffff` | [品牌 / App](https://venmo.com/) |
| Cash App | Cash App、CashApp | `#00d64f` | `#003b16` | [品牌 / App](https://cash.app/) |
| Apple Pay | Apple Pay、ApplePay、苹果支付、蘋果支付 | `#111111` | `#ffffff` | [品牌 / App](https://www.apple.com/apple-pay/) |
| Google Pay | Google Pay、GooglePay、Google Wallet、谷歌支付、谷歌钱包 | `#e8f0fe` | `#174ea6` | [品牌 / App](https://pay.google.com/) |
| Samsung Pay | Samsung Pay、SamsungPay、Samsung Wallet、三星支付、三星钱包 | `#1428a0` | `#ffffff` | [品牌 / App](https://www.samsung.com/us/samsung-wallet/) |
| Wise | Wise、TransferWise | `#9fe870` | `#163300` | [品牌 / App](https://wise.com/community/en-US/brand-new-look) |
| Revolut | Revolut | `#191c1f` | `#ffffff` | [品牌 / App](https://apps.apple.com/us/app/revolut-send-spend-and-save/id932493382) |
| Payoneer | Payoneer、派安盈 | `#ff6c2c` | `#21130b` | [品牌 / App](https://www.payoneer.com/) |
| Skrill | Skrill | `#862165` | `#ffffff` | [品牌 / App](https://www.skrill.com/) |
| NETELLER | NETELLER | `#83ba25` | `#172600` | [品牌 / App](https://www.neteller.com/) |
| Stripe | Stripe | `#635bff` | `#ffffff` | [品牌 / App](https://stripe.com/) |
| Square | Square、Block Square | `#151515` | `#ffffff` | [品牌 / App](https://squareup.com/) |
| ShopeePay | ShopeePay、Shopee Pay、虾皮支付、蝦皮支付 | `#d03d12` | `#ffffff` | [品牌 / App](https://apps.apple.com/id/app/shopeepay-dompet-serba-bisa/id6455990519) |
| GrabPay | GrabPay、Grab Pay、Grab钱包 | `#007e3e` | `#ffffff` | [品牌 / App](https://apps.apple.com/sg/app/grab-food-delivery-taxi-ride/id647268330) |
| GCash | GCash、G Cash | `#005ce5` | `#ffffff` | [品牌 / App](https://gcash.com/) |
| Maya | Maya、PayMaya | `#80ee40` | `#163300` | [品牌 / App](https://www.maya.ph/) |
| Touch ’n Go | Touch n Go、Touch 'n Go、Touch ’n Go、TNG eWallet、TNG钱包 | `#0055a4` | `#ffffff` | [品牌 / App](https://www.touchngo.com.my/) |
| LINE Pay | LINE Pay、LINEPay | `#07823b` | `#ffffff` | [品牌 / App](https://pay.line.me/) |
| Kakao Pay | Kakao Pay、KakaoPay、카카오페이 | `#ffeb00` | `#222222` | [品牌 / App](https://www.kakaopay.com/) |
| Naver Pay | Naver Pay、NaverPay、네이버페이 | `#00de5a` | `#003d1a` | [品牌 / App](https://pay.naver.com/) |
| PayPay | PayPay | `#d92d3b` | `#ffffff` | [品牌 / App](https://paypay.ne.jp/) |
| Rakuten Pay | Rakuten Pay、RakutenPay、乐天支付、楽天ペイ | `#bf0000` | `#ffffff` | [品牌 / App](https://pay.rakuten.co.jp/) |
| Amazon Pay | Amazon Pay、AmazonPay、亚马逊支付 | `#ff9900` | `#232f3e` | [品牌 / App](https://pay.amazon.com/) |
| 京东金融 | 京东金融、京东支付、京東金融、京東支付、JD Pay、JDPay | `#c91623` | `#ffffff` | [品牌 / App](https://jr.jd.com/) |
| 抖音支付 | 抖音支付、抖音钱包、Douyin Pay | `#161823` | `#69f5ef` | [品牌 / App](https://www.douyin.com/) |
| 美团支付 | 美团支付、美团钱包、美團支付、Meituan Pay | `#ffd100` | `#3b3000` | [品牌 / App](https://www.meituan.com/) |
| 数字人民币 | 数字人民币、數字人民幣、e-CNY、eCNY | `#d41e25` | `#ffffff` | [品牌 / App](https://www.pbcdci.cn/) |
| QQ 钱包 | QQ钱包、QQ錢包、QQ Wallet、QQ Pay、QQPay、财付通、財付通、Tenpay | `#1677d2` | `#ffffff` | [品牌 / App](https://www.tenpay.com/) |
| 翼支付 | 翼支付、BestPay | `#d81e42` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E7%BF%BC%E6%94%AF%E4%BB%98/id519952689) |
| 沃钱包 | 沃钱包、沃錢包、沃支付、Wo Pay | `#ffac00` | `#3e2800` | [品牌 / App](https://apps.apple.com/cn/app/%E6%B2%83%E9%92%B1%E5%8C%85-5g%E6%99%BA%E4%BB%98%E7%94%B1%E6%88%91/id513192980) |
| 壹钱包 | 壹钱包、壹錢包、壹支付 | `#c84907` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E5%A3%B9%E9%92%B1%E5%8C%85/id745097904) |
| 八达通 | 八达通、八達通、Octopus | `#ef8e00` | `#3e2600` | [品牌 / App](https://www.octopus.com.hk/) |
| Paytm | Paytm | `#002e6e` | `#ffffff` | [品牌 / App](https://paytm.com/) |
| PhonePe | PhonePe、Phone Pe | `#5f259f` | `#ffffff` | [品牌 / App](https://www.phonepe.com/) |
| BHIM | BHIM、BHIM UPI | `#007d43` | `#ffffff` | [品牌 / App](https://apps.apple.com/in/app/bhim-bharats-own-payments-app/id1200315258) |
| Toss | Toss、토스、Toss Pay | `#0064ff` | `#ffffff` | [品牌 / App](https://toss.im/) |

## 加密资产交易所

| 平台 | 可识别名称/别名 | 背景 | 文字 | 官方参考 |
| --- | --- | --- | --- | --- |
| 币安 | 币安、幣安、Binance | `#181a20` | `#f0b90b` | [品牌 / App](https://apps.apple.com/hk/app/%E5%B9%A3%E5%AE%89-%E8%B2%B7%E6%AF%94%E7%89%B9%E5%B9%A3-%E5%B0%B1%E7%94%A8%E5%85%A8%E7%90%83%E7%AC%AC%E4%B8%80%E6%8A%95%E8%B3%87%E7%90%86%E8%B2%A1%E9%A6%96%E9%81%B8%E5%8A%A0%E5%AF%86%E8%B2%A8%E5%B9%A3%E4%BA%A4%E6%98%93%E6%89%80/id1436799971) |
| 欧易 | 欧易、歐易、OKX、OKEx | `#101010` | `#ffffff` | [品牌 / App](https://www.okx.com/learn/okex-rebrands-to-okx) |
| Bybit | Bybit | `#17181d` | `#f7a600` | [品牌 / App](https://www.bybit.com/) |
| Bitget | Bitget | `#101b20` | `#00f0ff` | [品牌 / App](https://www.bitget.com/) |
| Coinbase | Coinbase | `#0052ff` | `#ffffff` | [品牌 / App](https://apps.apple.com/us/app/coinbase-buy-crypto-stocks/id886427730) |
| Kraken | Kraken | `#7132f5` | `#ffffff` | [品牌 / App](https://www.kraken.com/) |
| KuCoin | KuCoin、库币、庫幣 | `#007b63` | `#ffffff` | [品牌 / App](https://www.kucoin.com/) |
| Gate | Gate、Gate.io、芝麻开门、芝麻開門 | `#2354e6` | `#ffffff` | [品牌 / App](https://apps.apple.com/us/app/gate-buy-btc-eth/id1294998195) |
| 火币 HTX | 火币、火幣、HTX、Huobi | `#102452` | `#ffffff` | [品牌 / App](https://www.htx.com/) |
| MEXC | MEXC、抹茶交易所、抹茶 | `#2354e6` | `#ffffff` | [品牌 / App](https://www.mexc.com/) |
| Crypto.com | Crypto.com、Crypto com | `#0b1426` | `#ffffff` | [品牌 / App](https://crypto.com/) |
| Gemini | Gemini、双子星交易所、雙子星交易所 | `#ff5a00` | `#251000` | [品牌 / App](https://apps.apple.com/us/app/gemini-markets-trade-it-all/id1408914447) |
| Bitfinex | Bitfinex | `#074d3b` | `#ffffff` | [品牌 / App](https://www.bitfinex.com/) |
| Bitstamp | Bitstamp | `#00875a` | `#ffffff` | [品牌 / App](https://www.bitstamp.net/) |
| BitMart | BitMart | `#171b26` | `#ffffff` | [品牌 / App](https://apps.apple.com/us/app/bitmart-buy-crypto-stocks/id1396382871) |
| CoinEx | CoinEx | `#007f72` | `#ffffff` | [品牌 / App](https://www.coinex.com/) |
| Phemex | Phemex | `#111111` | `#76dfb5` | [品牌 / App](https://apps.apple.com/us/app/phemex-buy-crypto-stocks/id1499601684) |
| BingX | BingX | `#1a55ed` | `#ffffff` | [品牌 / App](https://bingx.com/) |
| Upbit | Upbit、업비트 | `#093687` | `#ffffff` | [品牌 / App](https://upbit.com/) |
| Bithumb | Bithumb、빗썸 | `#f37321` | `#271300` | [品牌 / App](https://www.bithumb.com/) |
| bitbank | bitbank、ビットバンク | `#07527c` | `#ffffff` | [品牌 / App](https://bitbank.cc/) |
| bitFlyer | bitFlyer、ビットフライヤー | `#1763ae` | `#ffffff` | [品牌 / App](https://apps.apple.com/jp/app/%E3%83%93%E3%83%83%E3%83%88%E3%83%95%E3%83%A9%E3%82%A4%E3%83%A4%E3%83%BC-%E3%83%93%E3%83%83%E3%83%88%E3%82%B3%E3%82%A4%E3%83%B3-%E6%9A%97%E5%8F%B7%E8%B3%87%E7%94%A3-%E4%BB%AE%E6%83%B3%E9%80%9A%E8%B2%A8-%E5%8F%96%E5%BC%95/id1164758997) |
| Coincheck | Coincheck、コインチェック | `#087c82` | `#ffffff` | [品牌 / App](https://coincheck.com/) |
| HashKey | HashKey、HashKey Exchange | `#17171c` | `#a67dff` | [品牌 / App](https://apps.apple.com/hk/app/hashkey-exchange-%E6%B8%AF%E5%B9%A3-%E7%BE%8E%E5%85%83%E4%BA%A4%E6%98%93%E8%99%9B%E6%93%AC%E8%B3%87%E7%94%A2/id6448627365) |
| OSL | OSL、OSL Exchange | `#000000` | `#5dce00` | [品牌 / App](https://www.osl.com/en/brand-assets) |
| Poloniex | Poloniex、P网、P網 | `#147c67` | `#ffffff` | [品牌 / App](https://poloniex.com/) |
| CoinW 币赢 | CoinW、币赢、幣贏 | `#5730f5` | `#ffffff` | [品牌 / App](https://apps.apple.com/us/app/coinw-buy-btc-eth-crypto/id1494077068) |

## 证券、基金、行情软件与交易所

| 平台 | 可识别名称/别名 | 背景 | 文字 | 官方参考 |
| --- | --- | --- | --- | --- |
| 同花顺 | 同花顺、同花順、10jqka、Hithink | `#d92332` | `#ffffff` | [品牌 / App](https://download.10jqka.com.cn/phone/) |
| 东方财富 | 东方财富、東方財富、Eastmoney | `#c83f0e` | `#ffffff` | [品牌 / App](https://www.eastmoney.com/) |
| 天天基金 | 天天基金、1234567 | `#c83f0e` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E5%A4%A9%E5%A4%A9%E5%9F%BA%E9%87%91-%E5%9F%BA%E9%87%91%E6%8A%95%E8%B5%84%E7%90%86%E8%B4%A2/id531212368) |
| 雪球 | 雪球、Xueqiu | `#1469ca` | `#ffffff` | [品牌 / App](https://xueqiu.com/) |
| 富途牛牛 | 富途、富途牛牛、Futu、Futunn | `#ff8a00` | `#422300` | [品牌 / App](https://www.futunn.com/) |
| moomoo | moomoo | `#ff7f00` | `#3c2000` | [品牌 / App](https://apps.apple.com/us/app/moomoo-investing-trading/id1440255819) |
| 老虎证券 | 老虎证券、老虎證券、Tiger Brokers、Tiger Trade | `#ffdc00` | `#302900` | [品牌 / App](https://www.itiger.com/) |
| 微牛 | 微牛、Webull | `#0349f5` | `#ffffff` | [品牌 / App](https://apps.apple.com/us/app/webull-investing-trading/id1179213067) |
| Robinhood | Robinhood | `#ccff00` | `#1b2600` | [品牌 / App](https://robinhood.com/) |
| 盈透证券 | 盈透、IBKR、Interactive Brokers | `#cf202f` | `#ffffff` | [品牌 / App](https://www.interactivebrokers.com/) |
| 嘉信理财 | 嘉信、Charles Schwab、Schwab | `#0074b8` | `#ffffff` | [品牌 / App](https://www.schwab.com/) |
| Fidelity | Fidelity、富达、富達 | `#326e17` | `#ffffff` | [品牌 / App](https://apps.apple.com/us/app/fidelity-investments/id348177453) |
| E*TRADE | E*TRADE、ETrade、E Trade | `#5627a8` | `#ffffff` | [品牌 / App](https://us.etrade.com/) |
| Trading 212 | Trading 212、Trading212 | `#101010` | `#00b9e6` | [品牌 / App](https://apps.apple.com/gb/app/trading-212/id566325832) |
| eToro | eToro | `#11d975` | `#003f1c` | [品牌 / App](https://www.etoro.com/) |
| TradingView | TradingView | `#131722` | `#ffffff` | [品牌 / App](https://www.tradingview.com/) |
| Saxo | Saxo、Saxo Bank、盛宝银行、盛寶銀行 | `#102d4e` | `#ffffff` | [品牌 / App](https://www.home.saxo/) |
| 中信证券 | 中信证券、中信證券、信e投、信E投、CITIC Securities | `#bb1526` | `#ffffff` | [品牌 / App](https://www.cs.ecitic.com/) |
| 华泰证券 | 华泰证券、華泰證券、涨乐财富通、漲樂財富通、Huatai Securities | `#d9233c` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E6%B6%A8%E4%B9%90%E8%B4%A2%E5%AF%8C%E9%80%9A-%E7%82%92%E8%82%A1%E7%A5%A8%E7%90%86%E8%B4%A2%E5%B0%B1%E9%80%89%E5%8D%8E%E6%B3%B0/id379594830) |
| 国泰海通 | 国泰海通、國泰海通、国泰君安、國泰君安、海通证券、海通證券、国泰君安君弘、君弘、Guotai Haitong | `#d9262f` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E5%9B%BD%E6%B3%B0%E6%B5%B7%E9%80%9A%E5%90%9B%E5%BC%98-%E8%82%A1%E7%A5%A8%E5%BC%80%E6%88%B7%E7%82%92%E8%82%A1%E8%AF%81%E5%88%B8%E4%BA%A4%E6%98%93/id952047557) |
| 广发证券 | 广发证券、廣發證券、广发易淘金、易淘金、GF Securities | `#c8202d` | `#ffffff` | [品牌 / App](https://www.gf.com.cn/) |
| 招商证券 | 招商证券、招商證券、招商智远、招商智遠、China Merchants Securities | `#c9162b` | `#ffffff` | [品牌 / App](https://www.cmschina.com/) |
| 平安证券 | 平安证券、平安證券、平安证券APP、Ping An Securities | `#bc4607` | `#ffffff` | [品牌 / App](https://stock.pingan.com/) |
| 中国银河证券 | 银河证券、銀河證券、银河玖乐、中国银河、Galaxy Securities | `#c51c30` | `#ffffff` | [品牌 / App](https://www.chinastock.com.cn/) |
| 华夏基金 | 华夏基金、華夏基金、ChinaAMC | `#c6192e` | `#ffffff` | [品牌 / App](https://www.chinaamc.com/) |
| 易方达 | 易方达、易方達、E Fund | `#cc202a` | `#ffffff` | [品牌 / App](https://www.efunds.com.cn/) |
| 中信建投 | 中信建投、蜻蜓点金、蜻蜓點金、China Securities | `#d91915` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E8%9C%BB%E8%9C%93%E7%82%B9%E9%87%91-%E4%B8%AD%E4%BF%A1%E5%BB%BA%E6%8A%95%E8%AF%81%E5%88%B8%E7%82%92%E8%82%A1%E5%BC%80%E6%88%B7-%E5%9F%BA%E9%87%91%E7%90%86%E8%B4%A2/id1121579071) |
| 上海证券交易所 | 上海证券交易所、上海證券交易所、上交所、SSE | `#c00014` | `#ffffff` | [品牌 / App](https://www.sse.com.cn/) |
| 深圳证券交易所 | 深圳证券交易所、深圳證券交易所、深交所、SZSE | `#005bac` | `#ffffff` | [品牌 / App](https://www.szse.cn/) |
| 香港交易所 | 香港交易所、港交所、HKEX | `#d52440` | `#ffffff` | [品牌 / App](https://www.hkex.com.hk/) |
| 纳斯达克 | 纳斯达克、納斯達克、Nasdaq | `#007fa3` | `#ffffff` | [品牌 / App](https://www.nasdaq.com/) |
| 纽约证券交易所 | 纽约证券交易所、紐約證券交易所、纽交所、紐交所、NYSE | `#0875bf` | `#ffffff` | [品牌 / App](https://www.nyse.com/) |

## 银行

| 平台 | 可识别名称/别名 | 背景 | 文字 | 官方参考 |
| --- | --- | --- | --- | --- |
| 工商银行 | 工商银行、工商銀行、工行、ICBC | `#c7000b` | `#ffffff` | [品牌 / App](https://www.icbc.com.cn/) |
| 建设银行 | 建设银行、建設銀行、建行、CCB | `#005baa` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E4%B8%AD%E5%9B%BD%E5%BB%BA%E8%AE%BE%E9%93%B6%E8%A1%8C/id391965015) |
| 农业银行 | 农业银行、農業銀行、农行、農行、Agricultural Bank、ABC银行 | `#007b67` | `#ffffff` | [品牌 / App](https://www.abchina.com/) |
| 中国银行 | 中国银行、中國銀行、中行、Bank of China、BOC | `#a61b29` | `#ffffff` | [品牌 / App](https://www.boc.cn/) |
| 交通银行 | 交通银行、交通銀行、交行、Bank of Communications、BOCOM、买单吧、買單吧 | `#003c8f` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E4%BA%A4%E9%80%9A%E9%93%B6%E8%A1%8C/id337876534) |
| 招商银行 | 招商银行、招商銀行、招行、掌上生活、China Merchants Bank、CMB、招商银行APP | `#c4162b` | `#ffffff` | [品牌 / App](https://www.cmbchina.com/) |
| 邮储银行 | 邮储银行、郵儲銀行、邮政储蓄、郵政儲蓄、PSBC | `#007c46` | `#ffffff` | [品牌 / App](https://www.psbc.com/) |
| 浦发银行 | 浦发银行、浦發銀行、浦发、浦發、SPDB | `#172b70` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E6%B5%A6%E5%8F%91%E9%93%B6%E8%A1%8C/id471855847) |
| 兴业银行 | 兴业银行、興業銀行、兴业生活、CIB | `#004b93` | `#ffffff` | [品牌 / App](https://www.cib.com.cn/) |
| 民生银行 | 民生银行、民生銀行、CMBC | `#00775b` | `#ffffff` | [品牌 / App](https://www.cmbc.com.cn/) |
| 光大银行 | 光大银行、光大銀行、阳光惠生活、CEB | `#6a2585` | `#ffffff` | [品牌 / App](https://apps.apple.com/cn/app/%E5%85%89%E5%A4%A7%E9%93%B6%E8%A1%8C/id447733826) |
| 中信银行 | 中信银行、中信銀行、动卡空间、動卡空間、CITIC Bank | `#c51b30` | `#ffffff` | [品牌 / App](https://www.citicbank.com/) |
| 平安银行 | 平安银行、平安銀行、平安口袋银行、平安口袋銀行、Ping An Bank | `#bd4608` | `#ffffff` | [品牌 / App](https://bank.pingan.com/) |
| 广发银行 | 广发银行、廣發銀行、发现精彩、發現精彩、China Guangfa Bank、CGB | `#bd1225` | `#ffffff` | [品牌 / App](https://www.cgbchina.com.cn/) |
| 华夏银行 | 华夏银行、華夏銀行、Hua Xia Bank | `#c8202b` | `#ffffff` | [品牌 / App](https://www.hxb.com.cn/) |
| 汇丰银行 | 汇丰、滙豐、匯豐、HSBC | `#db0011` | `#ffffff` | [品牌 / App](https://www.hsbc.com/) |
| 渣打银行 | 渣打、Standard Chartered | `#0072aa` | `#ffffff` | [品牌 / App](https://www.sc.com/) |
| 花旗银行 | 花旗、Citibank、Citi Bank、Citi | `#00427e` | `#ffffff` | [品牌 / App](https://www.citigroup.com/) |
| Chase | Chase、JPMorgan、J.P. Morgan、摩根大通 | `#07539f` | `#ffffff` | [品牌 / App](https://www.chase.com/) |
| 美国银行 | 美国银行、美國銀行、Bank of America、BofA | `#c41230` | `#ffffff` | [品牌 / App](https://www.bankofamerica.com/) |
| Capital One | Capital One、CapitalOne | `#004977` | `#ffffff` | [品牌 / App](https://www.capitalone.com/) |
| 星展银行 | 星展、DBS | `#c90024` | `#ffffff` | [品牌 / App](https://www.dbs.com/) |
| 华侨银行 | 华侨银行、華僑銀行、OCBC | `#c81922` | `#ffffff` | [品牌 / App](https://www.ocbc.com/) |
| 大华银行 | 大华银行、大華銀行、UOB | `#003b71` | `#ffffff` | [品牌 / App](https://www.uobgroup.com/) |
| Nubank | Nubank、Nu Bank | `#820ad1` | `#ffffff` | [品牌 / App](https://nubank.com.br/) |
| Monzo | Monzo | `#ff4f5f` | `#2e1018` | [品牌 / App](https://monzo.com/) |
| N26 | N26 | `#056f69` | `#ffffff` | [品牌 / App](https://n26.com/) |
