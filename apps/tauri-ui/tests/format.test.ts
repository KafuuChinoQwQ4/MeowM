import { test } from 'node:test';
import assert from 'node:assert/strict';
import { money, moveDate, validDate, validAmount } from '../src/format.ts';

test('money keeps every cent beyond Number safe integer precision', () => {
  assert.equal(money('9007199254740991.25'), '9,007,199,254,740,991.25');
  assert.equal(money('-12.5'), '-12.50');
});
test('calendar navigation crosses leap day and year boundary', () => {
  assert.equal(moveDate('2024-02-28', 1), '2024-02-29');
  assert.equal(moveDate('2025-01-01', -1), '2024-12-31');
  assert.equal(validDate('2025-02-29'), false);
  assert.equal(validDate('2024-02-29'), true);
});
test('entry amounts are positive decimals with at most two places; references allow zero and negative', () => {
  for (const value of ['0', '-1', '1.001', '1e3', '', 'NaN'])
    assert.equal(validAmount(value), false, value);
  for (const value of ['0.01', '12', '12.50']) assert.equal(validAmount(value), true, value);
  assert.equal(validAmount('0', true), true);
  assert.equal(validAmount('-12.50', true), true);
});
