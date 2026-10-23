import assert from 'node:assert/strict';
import { test } from 'node:test';
import { resolvePlatformTheme, platformThemes } from '../src/shared/platformThemes.ts';

test('brand aliases support account suffixes, fullwidth text and old names', () => {
  for (const [name, id] of [
    ['币安现货', 'binance'],
    ['幣安 合约', 'binance'],
    ['我的ＢＩＮＡＮＣＥ钱包', 'binance'],
    ['OKEx 资金', 'okx'],
    ['欧易', 'okx'],
    ['同花順股票', 'ths'],
    ['ＰａｙＰａｌ 美元', 'paypal'],
    ['微信钱包', 'wechat'],
    ['Alipay 余额', 'alipay'],
    ['AlipayHK 港币', 'alipayhk'],
    ['TransferWise EUR', 'wise'],
    ['Gate.io 现货', 'gate'],
    ['Crypto.com USD', 'cryptocom'],
    ['招商证券', 'cmsecurities'],
    ['招商银行信用卡', 'cmb'],
    ['工商银行', 'icbc'],
    ['国泰君安君弘', 'guotaihaitong'],
    ['中信银行', 'citicbank'],
    ['中信证券', 'citic'],
  ])
    assert.equal(resolvePlatformTheme(name).id, id, name);
});

test('unrecognized platforms stay white and short aliases do not match inside words', () => {
  for (const name of [
    '',
    '未知小众钱包',
    '民生证券',
    '民生期货',
    'gateway',
    'unwise',
    'paypalish',
    'ok',
    'abc',
    'citizen',
    '123456789',
  ]) {
    const theme = resolvePlatformTheme(name);
    assert.equal(theme.id, 'default', name);
    assert.equal(theme.background, '#ffffff', name);
  }
  assert.equal(resolvePlatformTheme('备用零钱', 'cash').id, 'cash');
  assert.equal(resolvePlatformTheme('现金钱包').id, 'cash');
});

test('platform labels remain readable on every curated background', () => {
  function luminance(hex) {
    const [r, g, b] = hex
      .slice(1)
      .match(/../g)
      .map((s) => parseInt(s, 16) / 255)
      .map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  }
  for (const theme of platformThemes) {
    const a = luminance(theme.background),
      b = luminance(theme.foreground);
    assert.ok((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05) >= 4.5, theme.name);
  }
});
