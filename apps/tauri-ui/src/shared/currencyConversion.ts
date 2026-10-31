import { currencyExponent } from './format';

interface DecimalParts {
  coefficient: bigint;
  scale: number;
}

const TEN = 10n;

function powerOfTen(exponent: number): bigint {
  return TEN ** BigInt(exponent);
}

function decimalParts(value: string): DecimalParts | null {
  if (!/^\d+(?:\.\d+)?$/.test(value)) return null;
  const [whole, fraction = ''] = value.split('.');
  try {
    return { coefficient: BigInt(whole + fraction), scale: fraction.length };
  } catch {
    return null;
  }
}

function formatScaled(value: bigint, scale: number): string {
  if (scale === 0) return value.toString();
  const digits = value.toString().padStart(scale + 1, '0');
  const whole = digits.slice(0, -scale);
  const fraction = digits.slice(-scale).replace(/0+$/, '');
  return fraction ? `${whole}.${fraction}` : whole;
}

export function currencyPairRate(baseRatePerCny: string, quoteRatePerCny: string): string | null {
  const base = decimalParts(baseRatePerCny);
  const quote = decimalParts(quoteRatePerCny);
  if (!base || !quote || base.coefficient === 0n || quote.coefficient === 0n) return null;

  const outputScale = 12;
  const numerator = quote.coefficient * powerOfTen(base.scale + outputScale);
  const denominator = base.coefficient * powerOfTen(quote.scale);
  const rounded = (numerator + denominator / 2n) / denominator;
  return formatScaled(rounded, outputScale);
}

function amountMinorUnits(value: string, exponent: number): bigint | null {
  if (value.length > 40 || !/^\d+(?:\.\d+)?$/.test(value)) return null;
  const [whole, fraction = ''] = value.split('.');
  if (fraction.length > exponent) return null;
  try {
    return BigInt(whole) * powerOfTen(exponent) + BigInt(fraction.padEnd(exponent, '0') || '0');
  } catch {
    return null;
  }
}

function formatMinorUnits(value: bigint, exponent: number): string {
  if (exponent === 0) return value.toString();
  const digits = value.toString().padStart(exponent + 1, '0');
  return `${digits.slice(0, -exponent)}.${digits.slice(-exponent)}`;
}

export function convertCurrencyAmount(
  amount: string,
  baseCurrency: string,
  quoteCurrency: string,
  baseRatePerCny: string,
  quoteRatePerCny: string,
): string | null {
  const baseExponent = currencyExponent(baseCurrency);
  const quoteExponent = currencyExponent(quoteCurrency);
  const sourceMinor = amountMinorUnits(amount, baseExponent);
  const baseRate = decimalParts(baseRatePerCny);
  const quoteRate = decimalParts(quoteRatePerCny);
  if (
    sourceMinor === null ||
    !baseRate ||
    !quoteRate ||
    baseRate.coefficient === 0n ||
    quoteRate.coefficient === 0n
  ) {
    return null;
  }

  const numerator =
    sourceMinor * quoteRate.coefficient * powerOfTen(baseRate.scale + quoteExponent);
  const denominator = baseRate.coefficient * powerOfTen(quoteRate.scale + baseExponent);
  const convertedMinor = (numerator + denominator / 2n) / denominator;
  return formatMinorUnits(convertedMinor, quoteExponent);
}
