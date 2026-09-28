/**
 * Batch numbers as staff see them. Add Inventory makes up a lot number (`LOT-000123`) so each
 * delivery is tracked separately; it means nothing to staff and is not shown (DEC-036). A batch
 * number someone typed by hand is shown as typed.
 */
export function shownBatchNo(batchNo: string): string {
  return /^LOT-\d+$/.test(batchNo) ? '' : batchNo;
}
