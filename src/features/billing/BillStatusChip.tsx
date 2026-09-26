import type { BillStatus } from '../../api';
import { StatusChip } from '../../components/common';
import { t } from '../../i18n/en';

export function BillStatusChip({ status, returned }: { status: BillStatus; returned?: boolean }) {
  if (status === 'FINALIZED' && returned) return <StatusChip label={t.bills.returned} color="warning" />;
  const color = status === 'FINALIZED' ? 'success' : status === 'CANCELLED' ? 'error' : 'default';
  return <StatusChip label={t.bills.statuses[status]} color={color} />;
}
