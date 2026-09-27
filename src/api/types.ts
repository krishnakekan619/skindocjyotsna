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
  /** Placeholders: {name} {clinic} {billNo} {total}. */
  whatsappMessage: string;
  /** Ticked by default on every bill's medicines; 0 = none. At most the receptionist limit. */
  defaultMedicineDiscountPercent: number;
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
  /** Set when this record was merged into another client. */
  mergedIntoClientId: number | null;
}

export type ClientInput = Omit<ClientRow, 'id' | 'clientCode' | 'lastVisitAt' | 'createdAt' | 'updatedAt' | 'mergedIntoClientId'> & {
  id: number | null;
  /** The user saw the possible duplicates and chose to create a new client anyway. */
  allowDuplicate?: boolean;
};

/** clinic_services::clients::DuplicateQuery */
export interface DuplicateQuery {
  fullName: string;
  phone: string;
  dateOfBirth: IsoDate | null;
  excludeId: number | null;
}

export type DuplicateReason = 'PHONE' | 'NAME' | 'SIMILAR_NAME';

export interface DuplicateMatch {
  client: ClientRow;
  reason: DuplicateReason;
  /** Likely the same person: creating a new client needs an explicit confirmation. */
  strong: boolean;
}

export interface DuplicateGroup {
  reason: 'PHONE' | 'NAME';
  clients: (ClientRow & { billCount: number })[];
}

export interface MergeResult {
  client: ClientRow;
  movedBills: number;
}

export interface Visit {
  bill: BillRow;
  items: BillItemRow[];
  services: ServiceItemRow[];
}

export interface ClientProfile {
  client: ClientRow;
  visits: Visit[];
  visitCount: number;
  billCount: number;
  totalSpentPaise: Paise;
}

// ---- Consultations & procedures --------------------------------------------------------------

export type ServiceKind = 'CONSULTATION' | 'PROCEDURE';

/** repo::services::ServiceRow */
export interface ServiceRow {
  id: number;
  kind: ServiceKind;
  name: string;
  defaultPricePaise: Paise;
  gstRateBp: number;
  discountEligible: boolean;
  isActive: boolean;
  sortOrder: number;
}

export type ServiceInput = Omit<ServiceRow, 'id'> & { id: number | null };

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

/**
 * A consultation or procedure: one from the list (`serviceId`), or a name typed on the bill
 * (`serviceId` null + `kind` + `name`), which joins the list at Finalize.
 * `unitPricePaise` null = the usual price.
 */
export interface ServiceLineInput {
  serviceId: number | null;
  kind: ServiceKind | null;
  name: string | null;
  qty: number;
  unitPricePaise: Paise | null;
}

/** A client typed on the New Bill screen; saved together with the bill. */
export interface NewClientInput {
  fullName: string;
  phone: string;
  allowDuplicate: boolean;
}

export interface BillInput {
  idempotencyKey: string;
  clientId: number | null;
  /** When `clientId` is null: a new client typed on the bill. */
  newClient: NewClientInput | null;
  lines: BillLineInput[];
  services: ServiceLineInput[];
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

export interface QuoteServiceLine {
  serviceId: number;
  kind: ServiceKind;
  name: string;
  qty: number;
  unitPricePaise: Paise;
  defaultPricePaise: Paise;
  grossPaise: Paise;
  discountSharePaise: Paise;
  netPaise: Paise;
  taxPaise: Paise;
}

export interface Quote {
  lines: QuoteLine[];
  serviceLines: QuoteServiceLine[];
  consultationPaise: Paise;
  proceduresPaise: Paise;
  productsPaise: Paise;
  /** What the discount applies to (medicines & products). */
  eligibleSubtotalPaise: Paise;
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

/** A consultation or procedure line of a bill. */
export interface ServiceItemRow {
  id: number;
  billId: number;
  lineNo: number;
  serviceId: number;
  kind: ServiceKind;
  name: string;
  qty: number;
  unitPricePaise: Paise;
  defaultPricePaise: Paise;
  discountEligible: boolean;
  discountSharePaise: Paise;
  gstRateBp: number;
  taxPaise: Paise;
  lineTotalPaise: Paise;
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
  services: ServiceItemRow[];
  payments: PaymentRow[];
}

/** commands::billing::WhatsAppHandoff */
export interface WhatsAppHandoff {
  billNo: string;
  clientName: string;
  fileName: string;
  /** The PDF is on the clipboard: paste it into the chat. */
  pdfCopied: boolean;
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
  /** "Consultation", "Procedures", "Medicines & Products", or "" (no headings). */
  section: string;
}

export interface ReceiptTotal {
  label: string;
  amount: Paise;
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
  /** Section subtotals shown instead of "Subtotal" when the bill has consultations/procedures. */
  breakdown: ReceiptTotal[];
  discountLabel: string;
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
  notice: string | null;
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

export interface SectionSales {
  consultationPaise: Paise;
  proceduresPaise: Paise;
  medicinesPaise: Paise;
}

export interface ServiceSales {
  serviceId: number;
  kind: ServiceKind;
  name: string;
  qty: number;
  revenuePaise: Paise;
}

export interface TopSellers {
  split: SectionSales;
  medicines: ProductSales[];
  procedures: ServiceSales[];
  consultations: ServiceSales[];
}

export interface Dashboard {
  today: string;
  salesToday: SalesTotals;
  netSalesTodayPaise: Paise;
  salesSplitToday: SectionSales;
  activeProducts: number;
  lowStock: number;
  outOfStock: number;
  expiringSoon: number;
  /** Out of stock first, then low stock (a few of each). */
  stockAlerts: ProductRow[];
  /** Expired or expiring within 30 days, soonest first. */
  expiringBatches: ExpiringBatch[];
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

export type BackupKind = 'manual' | 'auto' | 'pre-restore' | 'pre-upgrade';

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
