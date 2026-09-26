// Types shared with the Rust shell (serde uses camelCase). Keep in sync with the Rust
// structs named in each comment. Money is always whole paise, never floating-point rupees.

export type Paise = number;
/** Unix seconds (UTC). */
export type Timestamp = number;
/** Local calendar date, `YYYY-MM-DD`. */
export type IsoDate = string;

export type Role = 'ADMIN' | 'RECEPTIONIST';

/** clinic_services::Session */
export interface Session {
  userId: number;
  username: string;
  fullName: string;
  role: Role;
  hasPin: boolean;
}

/** commands::session::AppStatus */
export interface AppStatus {
  needsSetup: boolean;
  clinicName: string;
  session: Session | null;
  locked: boolean;
  idleLockMinutes: number;
  appVersion: string;
}

/** clinic_services::settings::ClinicSettings */
export interface ClinicSettings {
  name: string;
  addressLines: string[];
  phone: string;
  email: string;
  gstin: string;
  receiptFooter: string;
  idleLockMinutes: number;
  invoicePrefix: string;
  receptionistDiscountCapPercent: number;
  roundToRupee: boolean;
  utcOffsetMinutes: number;
  returnWindowDays: number;
}

/** clinic_services::auth::NewAccount */
export interface NewAccount {
  username: string;
  fullName: string;
  password: string;
  pin?: string | null;
}

export interface SetupInput {
  clinic: ClinicSettings;
  admin: NewAccount;
  secondAdmin: NewAccount | null;
}

/** clinic_services::users::UserSummary */
export interface UserSummary {
  id: number;
  username: string;
  fullName: string;
  role: Role;
  isActive: boolean;
  hasPin: boolean;
  isLocked: boolean;
  lastLoginAt: Timestamp | null;
  createdAt: Timestamp;
}

export interface UserUpdate {
  fullName: string;
  role: Role;
  isActive: boolean;
}

/** clinic_services::audit::AuditEntry */
export interface AuditEntry {
  id: number;
  occurredAt: Timestamp;
  username: string | null;
  action: string;
  entityType: string | null;
  entityId: string | null;
  details: unknown;
}

// ---- Inventory ---------------------------------------------------------------------------------

export interface Category {
  id: number;
  name: string;
  isActive: boolean;
}

export interface Supplier {
  id: number;
  name: string;
  phone: string;
  gstin: string;
  isActive: boolean;
}

export const PRODUCT_TYPES = ['MEDICINE', 'TABLET', 'CAPSULE', 'CREAM', 'OINTMENT', 'GEL', 'MEDICAL_SUPPLY', 'CONSUMABLE', 'OTHER'] as const;
export type ProductType = (typeof PRODUCT_TYPES)[number];

/** repo::inventory::ProductRow */
export interface ProductRow {
  id: number;
  sku: string;
  name: string;
  genericName: string;
  categoryId: number | null;
  categoryName: string | null;
  productType: ProductType;
  manufacturer: string;
  unit: string;
  gstRateBp: number;
  defaultSellingPricePaise: Paise;
  defaultPurchasePricePaise: Paise;
  minStock: number;
  requiresExpiry: boolean;
  isActive: boolean;
  notes: string;
  createdAt: Timestamp;
  updatedAt: Timestamp;
  totalQty: number;
  sellableQty: number;
  nextExpiry: IsoDate | null;
}

export type ProductInput = Omit<ProductRow, 'id' | 'categoryName' | 'createdAt' | 'updatedAt' | 'totalQty' | 'sellableQty' | 'nextExpiry'> & {
  id: number | null;
};

export type StockFilter = 'ALL' | 'LOW' | 'OUT';

export interface ProductFilter {
  text?: string;
  categoryId?: number | null;
  includeInactive?: boolean;
  stock?: StockFilter;
}

/** repo::inventory::BatchRow */
export interface BatchRow {
  id: number;
  productId: number;
  productName: string;
  batchNo: string;
  expiryDate: IsoDate | null;
  supplierId: number | null;
  supplierName: string | null;
  purchasePricePaise: Paise;
  sellingPricePaise: Paise;
  quantity: number;
  createdAt: Timestamp;
}

export interface ProductDetail {
  product: ProductRow;
  batches: BatchRow[];
}

export interface ExpiringBatch extends BatchRow {
  /** Negative: expired that many days ago. */
  daysLeft: number;
}

export interface StockInInput {
  productId: number;
  batchNo: string;
  expiryDate: IsoDate | null;
  supplierId: number | null;
  purchasePricePaise: Paise;
  sellingPricePaise: Paise;
  qty: number;
  opening: boolean;
  note: string;
}

export type AdjustmentKind = 'ADJUSTMENT' | 'DAMAGE' | 'EXPIRY';

export interface AdjustInput {
  batchId: number;
  countedQty: number;
  kind: AdjustmentKind;
  reason: string;
}

/** repo::inventory::LedgerRow */
export interface LedgerRow {
  id: number;
  occurredAt: Timestamp;
  productId: number;
  productName: string;
  batchId: number;
  batchNo: string;
  kind: string;
  qtyChange: number;
  previousQty: number;
  newQty: number;
  reason: string;
  billId: number | null;
  billNo: string | null;
  username: string;
}

/** clinic_services::inventory::SaleProduct */
export interface SaleProduct {
  productId: number;
  name: string;
  genericName: string;
  sku: string;
  unit: string;
  gstRateBp: number;
  pricePaise: Paise;
  availableQty: number;
  nextExpiry: IsoDate | null;
  expiresSoon: boolean;
}

// ---- Clients -----------------------------------------------------------------------------------

export type Gender = 'MALE' | 'FEMALE' | 'OTHER' | 'UNDISCLOSED';

/** repo::clients::ClientRow */
export interface ClientRow {
  id: number;
  clientCode: string;
  fullName: string;
  phone: string;
  email: string;
  dateOfBirth: IsoDate | null;
  gender: Gender;
  address: string;
  emergencyContact: string;
  notes: string;
  lastVisitAt: Timestamp | null;
  isActive: boolean;
  createdAt: Timestamp;
  updatedAt: Timestamp;
}

export type ClientInput = Omit<ClientRow, 'id' | 'clientCode' | 'lastVisitAt' | 'createdAt' | 'updatedAt'> & { id: number | null };

export interface Visit {
  bill: BillRow;
  items: BillItemRow[];
}

export interface ClientProfile {
  client: ClientRow;
  visits: Visit[];
  visitCount: number;
  totalSpentPaise: Paise;
}

// ---- Billing -----------------------------------------------------------------------------------

export type BillStatus = 'FINALIZED' | 'CANCELLED' | 'CORRECTED';
export type PaymentMethod = 'CASH' | 'UPI' | 'CARD' | 'OTHER';
export const PAYMENT_METHODS: PaymentMethod[] = ['CASH', 'UPI', 'CARD', 'OTHER'];

/** clinic_core::pricing::Discount (percent value is basis points: 10% = 1000). */
export type Discount = { kind: 'NONE' } | { kind: 'PERCENT'; value: number } | { kind: 'AMOUNT'; value: Paise };

export interface BillLineInput {
  productId: number;
  qty: number;
  notSuppliedQty: number;
}

export interface PaymentInput {
  method: PaymentMethod;
  amountPaise: Paise;
  reference: string;
}

export interface BillInput {
  idempotencyKey: string;
  clientId: number | null;
  lines: BillLineInput[];
  discount: Discount;
  payments: PaymentInput[];
  amountReceivedPaise: Paise | null;
  note: string;
  approval: { username: string; password: string } | null;
}

export interface QuoteLine {
  productId: number;
  productName: string;
  unit: string;
  qty: number;
  notSuppliedQty: number;
  unitPricePaise: Paise;
  gstRateBp: number;
  grossPaise: Paise;
  discountSharePaise: Paise;
  netPaise: Paise;
  taxPaise: Paise;
  batchNos: string[];
  expiresSoon: boolean;
}

export interface Quote {
  lines: QuoteLine[];
  subtotalPaise: Paise;
  discountPaise: Paise;
  taxPaise: Paise;
  roundOffPaise: Paise;
  totalPaise: Paise;
  discountRateBp: number;
  needsApproval: boolean;
}

/** repo::billing::BillRow */
export interface BillRow {
  id: number;
  billNo: string;
  clientId: number | null;
  clientName: string | null;
  clientCode: string | null;
  status: BillStatus;
  subtotalPaise: Paise;
  discountPaise: Paise;
  taxPaise: Paise;
  roundOffPaise: Paise;
  totalPaise: Paise;
  amountReceivedPaise: Paise | null;
  changePaise: Paise | null;
  returnedPaise: Paise;
  note: string;
  createdBy: number;
  createdByName: string;
  finalizedAt: Timestamp;
  cancelledAt: Timestamp | null;
  cancelReason: string | null;
  replacesBillId: number | null;
  replacesBillNo: string | null;
  correctedByBillId: number | null;
  correctedByBillNo: string | null;
}

export interface ItemBatchRow {
  id: number;
  billItemId: number;
  batchId: number;
  batchNo: string;
  expiryDate: IsoDate | null;
  qty: number;
  returnedQty: number;
}

export interface BillItemRow {
  id: number;
  billId: number;
  lineNo: number;
  productId: number;
  productName: string;
  sku: string;
  unit: string;
  qty: number;
  notSuppliedQty: number;
  unitPricePaise: Paise;
  discountSharePaise: Paise;
  gstRateBp: number;
  taxPaise: Paise;
  lineTotalPaise: Paise;
  returnedQty: number;
  batches: ItemBatchRow[];
}

export interface PaymentRow {
  id: number;
  method: PaymentMethod;
  amountPaise: Paise;
  direction: 'IN' | 'REFUND';
  reference: string;
  createdAt: Timestamp;
}

export interface BillDetail {
  bill: BillRow;
  items: BillItemRow[];
  payments: PaymentRow[];
}

export interface BillFilter {
  fromDate?: IsoDate | null;
  toDate?: IsoDate | null;
  status?: BillStatus | null;
  clientId?: number | null;
  text?: string;
}

export interface ReturnInput {
  billId: number;
  lines: { billItemId: number; qty: number; restock: boolean }[];
  reason: string;
  refundMethod: PaymentMethod;
}

export interface ReturnResult {
  returnNo: string;
  refundPaise: Paise;
  bill: BillDetail;
}

export interface CorrectionInput {
  originalBillId: number;
  reason: string;
  bill: BillInput;
}

/** clinic_pdf::ReceiptData */
export interface ReceiptLine {
  name: string;
  detail: string | null;
  qty: number;
  unitPrice: Paise;
  amount: Paise;
  notSuppliedQty: number;
}

export interface ReceiptPayment {
  method: string;
  amount: Paise;
}

export interface ReceiptData {
  clinicName: string;
  clinicAddressLines: string[];
  clinicPhone: string | null;
  clinicGstin: string | null;
  statusBanner: string | null;
  billNo: string;
  dateTime: string;
  clientLabel: string | null;
  lines: ReceiptLine[];
  subtotal: Paise;
  discount: Paise;
  taxLabel: string;
  tax: Paise;
  roundOff: Paise;
  total: Paise;
  payments: ReceiptPayment[];
  amountReceived: Paise | null;
  changeDue: Paise | null;
  billedBy: string | null;
  footer: string | null;
}

// ---- Reports -----------------------------------------------------------------------------------

export interface SalesTotals {
  billCount: number;
  subtotalPaise: Paise;
  discountPaise: Paise;
  taxPaise: Paise;
  totalPaise: Paise;
  returnedPaise: Paise;
  clientsServed: number;
}

export interface Dashboard {
  today: string;
  salesToday: SalesTotals;
  netSalesTodayPaise: Paise;
  activeProducts: number;
  lowStock: number;
  outOfStock: number;
  expiringSoon: number;
  recentBills: BillRow[];
  recentClients: ClientRow[];
  hasBackupAdmin: boolean;
  ledgerProblems: number;
}

export interface DateRange {
  from: IsoDate;
  to: IsoDate;
}

export interface SalesReport {
  totals: SalesTotals;
  netPaise: Paise;
  byMethod: { method: PaymentMethod; receivedPaise: Paise; refundedPaise: Paise }[];
}

export interface ProductSales {
  productId: number;
  productName: string;
  qtySold: number;
  revenuePaise: Paise;
}

export interface StockReport {
  batches: BatchRow[];
  lowStock: ProductRow[];
  outOfStock: ProductRow[];
}

// ---- System ------------------------------------------------------------------------------------

export interface DatabaseStatus {
  sqliteVersion: string;
  schemaVersion: number;
  journalMode: string;
  integrityOk: boolean;
  fts5Available: boolean;
}

export interface SystemInfo {
  appVersion: string;
  platform: string;
  arch: string;
  dataDir: string;
  databaseFile: string;
  backupDir: string;
  exportDir: string;
  logDir: string;
  database: DatabaseStatus;
}

export interface HealthReport {
  integrityOk: boolean;
  ledgerMismatchBatchIds: number[];
}

export type BackupKind = 'manual' | 'auto' | 'pre-restore';

export interface BackupFile {
  fileName: string;
  sizeBytes: number;
  kind: BackupKind | null;
  createdUtc: string | null;
  appVersion: string | null;
  problem: string | null;
}

export interface RestoreResult {
  restoredFrom: BackupFile;
  safetyBackup: BackupFile;
}

export type AppFolder = 'backups' | 'exports' | 'data' | 'logs';

/** Error shape returned by every Rust command. `message` is safe to show to staff. */
export interface CommandError {
  code: string;
  message: string;
  field?: string;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as Record<string, unknown>)['code'] === 'string' &&
    typeof (value as Record<string, unknown>)['message'] === 'string'
  );
}
