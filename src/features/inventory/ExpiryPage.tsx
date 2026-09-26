import { useState } from 'react';
import { Card, MenuItem, Stack, Table, TableBody, TableCell, TableHead, TableRow, TextField } from '@mui/material';
import { api } from '../../api';
import { EmptyState, ErrorAlert, Loading, PageHeader, StatusChip, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { formatExpiry } from '../../lib/dates';

const WINDOWS = [30, 60, 90, 180];

/** Batches in stock that are expired or expire soon, soonest first. */
export function ExpiryPage() {
  const [within, setWithin] = useState(30);
  const { data, error, loading } = useLoader(() => api.listExpiring(within), [within]);
  return (
    <>
      <PageHeader title={t.expiry.title} />
      <Stack direction="row" sx={{ mb: 2 }}>
        <TextField select label={t.expiry.within} value={within} onChange={(e) => setWithin(Number(e.target.value))} sx={{ width: 280 }}>
          {WINDOWS.map((d) => (
            <MenuItem key={d} value={d}>
              {t.expiry.days(d)}
            </MenuItem>
          ))}
        </TextField>
      </Stack>
      <ErrorAlert error={error} />
      <Card variant="outlined">
        {loading && !data ? (
          <Loading />
        ) : !data || data.length === 0 ? (
          <EmptyState text={t.expiry.empty} />
        ) : (
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>{t.products.name}</TableCell>
                <TableCell>{t.products.batchNo}</TableCell>
                <TableCell>{t.products.expiry}</TableCell>
                <TableCell>{t.expiry.daysLeft}</TableCell>
                <TableCell align="right">{t.common.qty}</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {data.map((b) => (
                <TableRow key={b.id}>
                  <TableCell>{b.productName}</TableCell>
                  <TableCell>{b.batchNo}</TableCell>
                  <TableCell>{formatExpiry(b.expiryDate)}</TableCell>
                  <TableCell>
                    {b.daysLeft < 0 ? (
                      <StatusChip label={t.expiry.expiredAgo(-b.daysLeft)} color="error" />
                    ) : (
                      <StatusChip label={t.expiry.days(b.daysLeft)} color={b.daysLeft <= 30 ? 'warning' : 'default'} />
                    )}
                  </TableCell>
                  <TableCell align="right">{b.quantity}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Card>
    </>
  );
}
