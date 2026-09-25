import { useEffect, useState } from 'react';
import { Alert, Button, Card, CardContent, CardHeader, CircularProgress, Stack } from '@mui/material';
import { api, errorMessage, type ReceiptData } from '../../api';
import { ReceiptPreview } from './ReceiptPreview';

/** Phase 0 spike: receipt preview, PDF export (primary) and print (kept ready, DEC-004). */
export function ReceiptCard() {
  const [data, setData] = useState<ReceiptData | null>(null);
  const [notice, setNotice] = useState<{ severity: 'success' | 'error' | 'info'; text: string } | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api
      .getSampleReceipt()
      .then(setData)
      .catch((error: unknown) => setNotice({ severity: 'info', text: errorMessage(error, 'Could not load the sample receipt.') }));
  }, []);

  const savePdf = async () => {
    setSaving(true);
    try {
      setNotice({ severity: 'success', text: `PDF saved: ${await api.exportSampleReceiptPdf()}` });
    } catch (error) {
      setNotice({ severity: 'error', text: errorMessage(error, 'The PDF could not be created.') });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card variant="outlined">
      <CardHeader
        title="Receipt (sample)"
        action={
          <Stack direction="row" spacing={1}>
            <Button variant="contained" onClick={savePdf} disabled={!data || saving}>
              Save PDF
            </Button>
            <Button onClick={() => window.print()} disabled={!data} title="Uses the system print dialog (A5)">
              Print
            </Button>
          </Stack>
        }
      />
      <CardContent>
        <Stack spacing={2}>
          {notice && <Alert severity={notice.severity}>{notice.text}</Alert>}
          {data ? <ReceiptPreview data={data} /> : !notice && <CircularProgress aria-label="Loading receipt" />}
        </Stack>
      </CardContent>
    </Card>
  );
}
