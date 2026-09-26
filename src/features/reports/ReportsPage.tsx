import { useState } from 'react';
import { Box, Button, Card, CardContent, CardHeader, Stack, Tab, Table, TableBody, TableCell, TableHead, TableRow, Tabs, TextField, Typography } from '@mui/material';
import { api, type DateRange } from '../../api';
import { EmptyState, ErrorAlert, Loading, PageHeader, useLoader } from '../../components/common';
import { t } from '../../i18n/en';
import { addDaysIso, formatExpiry, monthStartIso, todayIso } from '../../lib/dates';
import { rupees } from '../../lib/money';

type ReportTab = 'sales' | 'products' | 'stock';

/** Sales, product sales and stock reports (admin). "Print" uses the browser print dialog. */
export function ReportsPage() {
  const [tab, setTab] = useState<ReportTab>('sales');
  const today = todayIso();
  const [range, setRange] = useState<DateRange>({ from: today, to: today });
  const presets: [string, DateRange][] = [
    [t.reports.today, { from: today, to: today }],
    [t.reports.yesterday, { from: addDaysIso(today, -1), to: addDaysIso(today, -1) }],
    [t.reports.week, { from: addDaysIso(today, -6), to: today }],
    [t.reports.month, { from: monthStartIso(), to: today }],
  ];

  return (
    <>
      <PageHeader title={t.reports.title} actions={<Button onClick={() => window.print()}>{t.reports.print}</Button>} />
      <Tabs value={tab} onChange={(_, v: ReportTab) => setTab(v)} sx={{ mb: 2 }} className="no-print">
        <Tab value="sales" label={t.reports.sales} />
        <Tab value="products" label={t.reports.productSales} />
        <Tab value="stock" label={t.reports.stock} />
      </Tabs>
      {tab !== 'stock' && (
        <Stack direction="row" spacing={1} sx={{ mb: 2, alignItems: 'center', flexWrap: 'wrap' }} useFlexGap>
          {presets.map(([label, r]) => (
            <Button key={label} size="small" variant={r.from === range.from && r.to === range.to ? 'contained' : 'outlined'} onClick={() => setRange(r)}>
              {label}
            </Button>
          ))}
          <TextField size="small" type="date" label={t.reports.from} value={range.from} onChange={(e) => e.target.value && setRange({ ...range, from: e.target.value })} slotProps={{ inputLabel: { shrink: true } }} />
          <TextField size="small" type="date" label={t.reports.to} value={range.to} onChange={(e) => e.target.value && setRange({ ...range, to: e.target.value })} slotProps={{ inputLabel: { shrink: true } }} />
        </Stack>
      )}
      {tab === 'sales' && <SalesReportView range={range} />}
      {tab === 'products' && <ProductSalesView range={range} />}
      {tab === 'stock' && <StockReportView />}
    </>
  );
}

function SalesReportView({ range }: { range: DateRange }) {
  const { data, error, loading } = useLoader(() => api.salesReport(range), [range.from, range.to]);
  if (loading && !data) return <Loading />;
  if (!data) return <ErrorAlert error={error} />;
  const rows: [string, string][] = [
    [t.reports.billCount, String(data.totals.billCount)],
    [t.reports.clients, String(data.totals.clientsServed)],
    [t.reports.gross, rupees(data.totals.subtotalPaise)],
    [t.reports.discounts, rupees(data.totals.discountPaise)],
    [t.reports.totalSales, rupees(data.totals.totalPaise)],
    [t.reports.returns, rupees(data.totals.returnedPaise)],
    [t.reports.net, rupees(data.netPaise)],
    [t.reports.tax, rupees(data.totals.taxPaise)],
  ];
  return (
    <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2 }}>
      <Card variant="outlined">
        <CardContent>
          <Table size="small">
            <TableBody>
              {rows.map(([label, value]) => (
                <TableRow key={label}>
                  <TableCell component="th">{label}</TableCell>
                  <TableCell align="right" sx={{ fontWeight: label === t.reports.net ? 700 : 400 }}>
                    {value}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
      <Card variant="outlined">
        <CardHeader title={t.reports.byMethod} />
        <CardContent>
          {data.byMethod.length === 0 ? (
            <EmptyState text={t.bills.empty} />
          ) : (
            <Table size="small">
              <TableHead>
                <TableRow>
                  <TableCell>{t.billing.payment}</TableCell>
                  <TableCell align="right">{t.reports.received}</TableCell>
                  <TableCell align="right">{t.reports.returns}</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {data.byMethod.map((m) => (
                  <TableRow key={m.method}>
                    <TableCell>{t.billing.methods[m.method]}</TableCell>
                    <TableCell align="right">{rupees(m.receivedPaise)}</TableCell>
                    <TableCell align="right">{rupees(m.refundedPaise)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </Box>
  );
}

function ProductSalesView({ range }: { range: DateRange }) {
  const { data, error, loading } = useLoader(() => api.productSalesReport(range), [range.from, range.to]);
  if (loading && !data) return <Loading />;
  if (!data) return <ErrorAlert error={error} />;
  return (
    <Card variant="outlined">
      {data.length === 0 ? (
        <EmptyState text={t.bills.empty} />
      ) : (
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>{t.products.name}</TableCell>
              <TableCell align="right">{t.reports.qtySold}</TableCell>
              <TableCell align="right">{t.reports.revenue}</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {data.map((p) => (
              <TableRow key={p.productId}>
                <TableCell>{p.productName}</TableCell>
                <TableCell align="right">{p.qtySold}</TableCell>
                <TableCell align="right">{rupees(p.revenuePaise)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </Card>
  );
}

function StockReportView() {
  const { data, error, loading } = useLoader(() => api.stockReport(), []);
  if (loading && !data) return <Loading />;
  if (!data) return <ErrorAlert error={error} />;
  const value = data.batches.reduce((s, b) => s + b.quantity * b.purchasePricePaise, 0);
  return (
    <Stack spacing={2}>
      <Typography color="text.secondary">
        {t.products.batches}: {data.batches.length} · {t.products.purchasePrice}: {rupees(value)}
      </Typography>
      <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2 }}>
        <ProductList title={t.reports.lowStock} rows={data.lowStock.map((p) => [p.name, `${p.sellableQty} / ${p.minStock} ${p.unit}`])} />
        <ProductList title={t.reports.outOfStock} rows={data.outOfStock.map((p) => [p.name, p.sku])} />
      </Box>
      <Card variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>{t.products.name}</TableCell>
              <TableCell>{t.products.batchNo}</TableCell>
              <TableCell>{t.products.expiry}</TableCell>
              <TableCell align="right">{t.common.qty}</TableCell>
              <TableCell align="right">{t.common.price}</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {data.batches.map((b) => (
              <TableRow key={b.id}>
                <TableCell>{b.productName}</TableCell>
                <TableCell>{b.batchNo}</TableCell>
                <TableCell>{formatExpiry(b.expiryDate)}</TableCell>
                <TableCell align="right">{b.quantity}</TableCell>
                <TableCell align="right">{rupees(b.sellingPricePaise)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Card>
    </Stack>
  );
}

function ProductList({ title, rows }: { title: string; rows: [string, string][] }) {
  return (
    <Card variant="outlined">
      <CardHeader title={`${title} (${rows.length})`} />
      <CardContent>
        {rows.length === 0 ? (
          <EmptyState text={t.common.none} />
        ) : (
          <Table size="small">
            <TableBody>
              {rows.map(([name, detail]) => (
                <TableRow key={name + detail}>
                  <TableCell>{name}</TableCell>
                  <TableCell align="right">{detail}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
