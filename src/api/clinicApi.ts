import type * as T from './types';

/**
 * Everything the UI can ask the application to do (one method per Rust command).
 *
 * Screens depend only on this interface, never on Tauri directly, so a future multi-computer
 * version can swap in an HTTP implementation without touching them.
 */
export interface ClinicApi {
  // Session
  getAppStatus(): Promise<T.AppStatus>;
  completeSetup(input: T.SetupInput): Promise<T.AppStatus>;
  login(username: string, password: string): Promise<T.AppStatus>;
  logout(): Promise<T.AppStatus>;
  lockScreen(): Promise<T.AppStatus>;
  unlockWithPin(pin: string): Promise<T.AppStatus>;
  unlockWithPassword(password: string): Promise<T.AppStatus>;
  heartbeat(): Promise<void>;
  changeOwnPassword(currentPassword: string, newPassword: string): Promise<void>;
  setOwnPin(currentPassword: string, pin: string | null): Promise<T.AppStatus>;
  // Users, settings, audit
  listUsers(): Promise<T.UserSummary[]>;
  createUser(account: T.NewAccount, role: T.Role): Promise<T.UserSummary>;
  updateUser(userId: number, change: T.UserUpdate): Promise<T.UserSummary>;
  resetUserPassword(userId: number, newPassword: string): Promise<void>;
  getClinicSettings(): Promise<T.ClinicSettings>;
  updateClinicSettings(settings: T.ClinicSettings): Promise<T.ClinicSettings>;
  listAudit(limit: number, beforeId: number | null): Promise<T.AuditEntry[]>;
  // Inventory
  listCategories(): Promise<T.Category[]>;
  saveCategory(input: { id: number | null; name: string; isActive: boolean }): Promise<T.Category[]>;
  listSuppliers(): Promise<T.Supplier[]>;
  saveSupplier(input: { id: number | null; name: string; phone: string; gstin: string; isActive: boolean }): Promise<T.Supplier[]>;
  listProducts(filter: T.ProductFilter): Promise<T.ProductRow[]>;
  getProduct(productId: number): Promise<T.ProductDetail>;
  saveProduct(input: T.ProductInput): Promise<T.ProductRow>;
  stockIn(input: T.StockInInput): Promise<T.BatchRow>;
  /** Inventory → Add Inventory (receptionists too). */
  addInventory(input: T.AddInventoryInput): Promise<T.BatchRow>;
  /** Inventory → Import from CSV (admin): checks the file, changes nothing. */
  previewStockImport(text: string): Promise<T.ImportPreview>;
  /** Imports every row of the file at once (all or none). */
  importStock(text: string, requestKey: string, importAgain: boolean): Promise<T.ImportResult>;
  /** Saves the empty import sheet and opens it (normally in Excel); returns where it was saved. */
  openStockImportTemplate(): Promise<string>;
  /** Inventory → Delete (administrators): archives a product with history, removes an unused one. */
  deleteProduct(productId: number): Promise<T.DeleteOutcome>;
  adjustStock(input: T.AdjustInput): Promise<T.BatchRow>;
  listStockLedger(productId: number | null, limit: number): Promise<T.LedgerRow[]>;
  listExpiring(withinDays: number): Promise<T.ExpiringBatch[]>;
  searchProductsForSale(text: string): Promise<T.SaleProduct[]>;
  recentProductsForSale(): Promise<T.SaleProduct[]>;
  // Clients
  searchClients(text: string, includeInactive: boolean): Promise<T.ClientRow[]>;
  saveClient(input: T.ClientInput): Promise<T.ClientRow>;
  getClientProfile(clientId: number, fromDate: string | null, toDate: string | null): Promise<T.ClientProfile>;
  /** Possible existing clients for the details typed in a "new client" form. */
  checkClientDuplicates(query: T.DuplicateQuery): Promise<T.DuplicateMatch[]>;
  findDuplicateClients(): Promise<T.DuplicateGroup[]>;
  /** Administrators: bills of `secondaryId` move to `primaryId`; nothing is deleted. */
  mergeClients(primaryId: number, secondaryId: number): Promise<T.MergeResult>;
  // Consultations & procedures
  listServices(includeInactive: boolean): Promise<T.ServiceRow[]>;
  saveService(input: T.ServiceInput): Promise<T.ServiceRow>;
  // Billing
  /** `correctingBillId`: the bill being corrected (its stock counts as available again). */
  quoteBill(lines: T.BillLineInput[], services: T.ServiceLineInput[], discount: T.Discount, correctingBillId: number | null): Promise<T.Quote>;
  finalizeBill(input: T.BillInput): Promise<T.BillDetail>;
  listBills(filter: T.BillFilter): Promise<T.BillRow[]>;
  getBill(billId: number): Promise<T.BillDetail>;
  cancelBill(billId: number, reason: string): Promise<T.BillDetail>;
  returnBillItems(input: T.ReturnInput): Promise<T.ReturnResult>;
  correctBill(input: T.CorrectionInput): Promise<T.BillDetail>;
  getReceipt(billId: number): Promise<T.ReceiptData>;
  /** Returns the saved file name (in the exports folder). */
  exportReceiptPdf(billId: number): Promise<string>;
  openExport(fileName: string): Promise<void>;
  /** Saves the PDF, copies it to the clipboard and opens WhatsApp on the client's chat. */
  openWhatsApp(billId: number): Promise<T.WhatsAppHandoff>;
  // Reports
  getDashboard(): Promise<T.Dashboard>;
  salesReport(range: T.DateRange): Promise<T.SalesReport>;
  productSalesReport(range: T.DateRange): Promise<T.ProductSales[]>;
  stockReport(): Promise<T.StockReport>;
  /** Dashboard: what sells most, and the consultation / procedures / medicines split. */
  topSellers(range: T.DateRange): Promise<T.TopSellers>;
  // System
  getSystemInfo(): Promise<T.SystemInfo>;
  getHealth(): Promise<T.HealthReport>;
  listBackups(): Promise<T.BackupFile[]>;
  createBackup(): Promise<T.BackupFile>;
  /** `fileName` must be one returned by `listBackups`. Signs everyone out. */
  /** Needs the administrator's password again. */
  restoreBackup(fileName: string, password: string): Promise<T.RestoreResult>;
  openFolder(folder: T.AppFolder): Promise<void>;
}
