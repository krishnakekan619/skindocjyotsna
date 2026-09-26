import { useEffect, useRef, useState } from 'react';
import { Alert, Box, Button, Dialog, DialogActions, DialogContent, DialogContentText, DialogTitle, Stack, Typography } from '@mui/material';
import { api, isCommandError, type BillDetail, type ClientRow } from '../../api';
import { useApp } from '../../app/AppContext';
import { ConfirmDialog, ErrorAlert, Loading, useLoader } from '../../components/common';
import { MOD_KEY, t } from '../../i18n/en';
import { todayIso, toIso } from '../../lib/dates';
import { rupees } from '../../lib/money';
import { ClientDialog } from '../clients/ClientDialog';
import { ReceiptPreview } from '../receipt/ReceiptPreview';
import { BillStatusChip } from './BillStatusChip';
import { ReturnDialog } from './ReturnDialog';

/**
 * One bill: the receipt as printed, plus print / PDF / WhatsApp / return / correct / cancel.
 * Right after Finalize, `savedMessage` is shown on top and `autoPrint` opens the print dialog.
 */
export function BillDetailDialog({
  billId,
  onClose,
  onChanged,
  savedMessage,
  autoPrint = false,
}: {
  billId: number;
  onClose: () => void;
  onChanged: () => void;
  savedMessage?: string | undefined;
  autoPrint?: boolean;
}) {
  const { isAdmin, navigate, notify } = useApp();
  const [version, setVersion] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [cancelling, setCancelling] = useState(false);
  const [returning, setReturning] = useState(false);
  const [noPhone, setNoPhone] = useState<string | null>(null);
  const [editingClient, setEditingClient] = useState<ClientRow | null>(null);
  const printed = useRef(false);
  const { data, error: loadError } = useLoader(async () => {
    const [detail, receipt] = await Promise.all([api.getBill(billId), api.getReceipt(billId)]);
    return { detail, receipt };
  }, [billId, version]);

  // "Finalize & print": print as soon as the receipt is on screen (once).
  useEffect(() => {
    if (autoPrint && data && !printed.current) {
      printed.current = true;
      window.setTimeout(() => window.print(), 300);
    }
  }, [autoPrint, data]);

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

  /** Opens the clinic's WhatsApp on the client's chat with the message typed and the PDF copied. */
  const sendWhatsApp = async () => {
    setBusy(true);
    setError(null);
    try {
      const handoff = await api.openWhatsApp(billId);
      notify(handoff.pdfCopied ? t.bills.whatsappReady(handoff.clientName, MOD_KEY) : t.bills.whatsappDrag(handoff.clientName), 'info');
    } catch (e) {
      if (isCommandError(e) && e.code === 'NO_PHONE') setNoPhone(e.message);
      else setError(e);
    } finally {
      setBusy(false);
    }
  };

  const addMobileNumber = async () => {
    const clientId = data?.detail.bill.clientId;
    setNoPhone(null);
    if (!clientId) return;
    try {
      setEditingClient((await api.getClientProfile(clientId, null, null)).client);
    } catch (e) {
      setError(e);
    }
  };

  const correct = (detail: BillDetail) => {
    onClose();
    navigate({ name: 'newBill', correcting: detail });
  };

  const bill = data?.detail.bill;
  // Bills with returns can't be cancelled or corrected (return the rest instead); receptionists
  // correct same-day bills only. The backend enforces both; this just hides dead-end buttons.
  const hasReturns = (bill?.returnedPaise ?? 0) > 0 || (data?.detail.items.some((i) => i.returnedQty > 0) ?? false);
  const sameDay = bill ? toIso(new Date(bill.finalizedAt * 1000)) === todayIso() : false;
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
              {savedMessage && (
                <Alert severity="success" sx={{ fontSize: '1.05rem' }}>
                  {savedMessage}
                </Alert>
              )}
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
      <DialogActions className="no-print" sx={{ flexWrap: 'wrap', gap: 1 }}>
        {data && bill?.status === 'FINALIZED' && !savedMessage && (
          <>
            {returnable && <Button onClick={() => setReturning(true)}>{t.bills.returnItems}</Button>}
            {!hasReturns && (isAdmin || sameDay) && <Button onClick={() => correct(data.detail)}>{t.bills.correct}</Button>}
            {isAdmin && !hasReturns && (
              <Button color="error" onClick={() => setCancelling(true)}>
                {t.bills.cancel}
              </Button>
            )}
          </>
        )}
        <Box sx={{ flexGrow: 1 }} />
        <Button variant="outlined" onClick={() => window.print()} disabled={!data}>
          {t.bills.print}
        </Button>
        <Button variant="outlined" onClick={savePdf} disabled={!data || busy}>
          {t.bills.savePdf}
        </Button>
        {bill?.status === 'FINALIZED' && (
          <Button variant="outlined" color="success" onClick={() => void sendWhatsApp()} disabled={!data || busy}>
            {t.bills.sendWhatsApp}
          </Button>
        )}
        <Button variant="contained" onClick={onClose}>
          {savedMessage ? t.nav.newBill : t.common.close}
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
      {noPhone !== null && (
        <Dialog open onClose={() => setNoPhone(null)} maxWidth="xs" fullWidth>
          <DialogTitle>{t.bills.noPhoneTitle}</DialogTitle>
          <DialogContent>
            <DialogContentText>{noPhone}</DialogContentText>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => setNoPhone(null)}>{t.bills.continueWithoutWhatsApp}</Button>
            {bill?.clientId && (
              <Button variant="contained" onClick={() => void addMobileNumber()}>
                {t.bills.addMobile}
              </Button>
            )}
          </DialogActions>
        </Dialog>
      )}
      {editingClient && (
        <ClientDialog
          client={editingClient}
          onClose={() => setEditingClient(null)}
          onSaved={() => {
            setEditingClient(null);
            void sendWhatsApp(); // number added: carry on straight to WhatsApp
          }}
        />
      )}
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
