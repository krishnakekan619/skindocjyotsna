import { useState } from 'react';
import { Alert, Box, Button, Dialog, DialogActions, DialogContent, DialogTitle, Stack, Typography } from '@mui/material';
import { api, type BillDetail } from '../../api';
import { useApp } from '../../app/AppContext';
import { ConfirmDialog, ErrorAlert, Loading, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { rupees } from '../../lib/money';
import { ReceiptPreview } from '../receipt/ReceiptPreview';
import { BillStatusChip } from './BillStatusChip';
import { ReturnDialog } from './ReturnDialog';

/** One bill: the receipt as printed, plus PDF / print / return / correct / cancel. */
export function BillDetailDialog({ billId, onClose, onChanged }: { billId: number; onClose: () => void; onChanged: () => void }) {
  const { isAdmin, navigate, notify } = useApp();
  const [version, setVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [cancelling, setCancelling] = useState(false);
  const [returning, setReturning] = useState(false);
  const { data, error: loadError } = useLoader(async () => {
    const [detail, receipt] = await Promise.all([api.getBill(billId), api.getReceipt(billId)]);
    return { detail, receipt };
  }, [billId, version]);

  const changed = () => {
    setVersion((v) => v + 1);
    onChanged();
  };

  const savePdf = async () => {
    setBusy(true);
    setError(null);
    try {
      const fileName = await api.exportReceiptPdf(billId);
      notify(`${t.bills.savePdf}: ${fileName}`);
      await api.openExport(fileName);
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  };

  const correct = (detail: BillDetail) => {
    onClose();
    navigate({ name: 'newBill', correcting: detail });
  };

  const bill = data?.detail.bill;
  const returnable = data?.detail.items.some((i) => i.qty - i.returnedQty > 0) ?? false;
  const refunded = data?.detail.payments.filter((p) => p.direction === 'REFUND').reduce((s, p) => s + p.amountPaise, 0) ?? 0;

  return (
    <Dialog open onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle className="no-print">
        <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
          <span>
            {t.bills.detail} {bill?.billNo}
          </span>
          {bill && <BillStatusChip status={bill.status} returned={bill.returnedPaise > 0} />}
        </Stack>
      </DialogTitle>
      <DialogContent>
        {!data ? (
          loadError ? <ErrorAlert error={loadError} /> : <Loading />
        ) : (
          <Stack spacing={2}>
            <Stack spacing={1} className="no-print">
              {bill?.replacesBillNo && <Alert severity="info">{t.bills.replaces}: {bill.replacesBillNo}</Alert>}
              {bill?.correctedByBillNo && <Alert severity="warning">{t.bills.correctedBy}: {bill.correctedByBillNo}</Alert>}
              {bill?.cancelReason && (
                <Alert severity="error">
                  {t.bills.cancelReason}: {bill.cancelReason}
                </Alert>
              )}
              {refunded > 0 && (
                <Typography color="text.secondary">
                  {t.bills.refunded}: {rupees(refunded)}
                </Typography>
              )}
              <ErrorAlert error={error} />
            </Stack>
            <Box sx={{ display: 'flex', justifyContent: 'center' }}>
              <ReceiptPreview data={data.receipt} />
            </Box>
          </Stack>
        )}
      </DialogContent>
      <DialogActions className="no-print">
        {data && bill?.status === 'FINALIZED' && (
          <>
            {returnable && <Button onClick={() => setReturning(true)}>{t.bills.returnItems}</Button>}
            <Button onClick={() => correct(data.detail)}>{t.bills.correct}</Button>
            {isAdmin && (
              <Button color="error" onClick={() => setCancelling(true)}>
                {t.bills.cancel}
              </Button>
            )}
          </>
        )}
        <Box sx={{ flexGrow: 1 }} />
        <Button onClick={() => window.print()} disabled={!data}>
          {t.bills.print}
        </Button>
        <Button onClick={savePdf} disabled={!data || busy}>
          {t.bills.savePdf}
        </Button>
        <Button variant="contained" onClick={onClose}>
          {t.common.close}
        </Button>
      </DialogActions>
      <ConfirmDialog
        open={cancelling}
        title={t.bills.cancel}
        text={t.bills.cancelConfirm}
        confirmLabel={t.bills.cancel}
        danger
        requireReason
        onConfirm={async (reason) => {
          await api.cancelBill(billId, reason);
          notify(`${bill?.billNo ?? ''}: ${t.bills.statuses.CANCELLED}`);
          changed();
        }}
        onClose={() => setCancelling(false)}
      />
      {returning && data && (
        <ReturnDialog
          detail={data.detail}
          onClose={() => setReturning(false)}
          onDone={(message) => {
            setReturning(false);
            notify(message);
            changed();
          }}
        />
      )}
    </Dialog>
  );
}
