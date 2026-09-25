import { describe, expect, it } from 'vitest';
import { formatPaise } from './money';

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
});
