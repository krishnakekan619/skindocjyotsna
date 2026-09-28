import { describe, expect, it } from 'vitest';
import { shownBatchNo } from './batch';

describe('shownBatchNo', () => {
  it('hides lot numbers made up by Add Inventory', () => {
    expect(shownBatchNo('LOT-000123')).toBe('');
  });

  it('keeps batch numbers typed by hand', () => {
    expect(shownBatchNo('A23')).toBe('A23');
    expect(shownBatchNo('LOT-A1')).toBe('LOT-A1');
  });
});
