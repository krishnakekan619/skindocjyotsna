import { describe, expect, it } from 'vitest';
import { formatPaise, paiseToInput, parseRupees, percentLabel, rupees } from './money';

describe('formatPaise', () => {
  it.each([
    [0, '0.00'],
    [5, '0.05'],
    [99_999, '999.99'],
    [100_000, '1,000.00'],
    [12_345_678, '1,23,456.78'],
    [1_234_567_890, '1,23,45,678.90'],
    [-150_000, '-1,500.00'],
  ])('%i paise -> %s', (paise, expected) => {
    expect(formatPaise(paise)).toBe(expected);
  });

  it('refuses fractional paise (floating-point money)', () => {
    expect(() => formatPaise(10.5)).toThrow(RangeError);
  });

  it('adds the rupee sign', () => {
    expect(rupees(15_000)).toBe('₹ 150.00');
  });
});

describe('parseRupees', () => {
  it.each([
    ['150', 15_000],
    ['150.5', 15_050],
    ['150.05', 15_005],
    ['1,500.00', 150_000],
    ['₹ 20', 2_000],
    ['0', 0],
    [' 7 ', 700],
  ])('%s -> %i paise', (text, expected) => {
    expect(parseRupees(text)).toBe(expected);
  });

  it.each(['', 'abc', '1.234', '-5', '1e3', '12.3.4'])('rejects %s', (text) => {
    expect(parseRupees(text)).toBeNull();
  });

  it('round-trips with paiseToInput', () => {
    for (const paise of [0, 5, 15_000, 15_050, 123_456]) expect(parseRupees(paiseToInput(paise))).toBe(paise);
  });
});

describe('percentLabel', () => {
  it('formats basis points', () => {
    expect(percentLabel(1_200)).toBe('12%');
    expect(percentLabel(250)).toBe('2.5%');
    expect(percentLabel(0)).toBe('0%');
  });
});
