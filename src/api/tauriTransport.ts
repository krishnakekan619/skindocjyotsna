import { invoke } from '@tauri-apps/api/core';
import type { ClinicApi } from './clinicApi';
import { isCommandError, type CommandError } from './types';

/** Fired when a command finds the session locked or ended; App re-reads the status. */
export const SESSION_CHANGED_EVENT = 'skindoc:session-changed';

// Argument names are camelCase here; Tauri maps them to the Rust snake_case parameters.
async function call<R>(command: string, args?: Record<string, unknown>): Promise<R> {
  try {
    return await invoke<R>(command, args);
  } catch (error) {
    if (isCommandError(error)) {
      if (error.code === 'LOCKED' || error.code === 'NOT_SIGNED_IN') window.dispatchEvent(new Event(SESSION_CHANGED_EVENT));
      throw error;
    }
    // Tauri rejects with a plain string when it cannot pass the arguments to Rust.
    console.error(`${command} failed`, error);
    const ipcError: CommandError = { code: 'IPC', message: 'The app could not process this request. Please try again, and restart the app if it keeps happening.' };
    throw ipcError;
  }
}

/** ClinicApi implementation that calls the Rust shell over Tauri IPC. */
export const tauriApi: ClinicApi = {
  getAppStatus: () => call('get_app_status'),
  completeSetup: (input) => call('complete_setup', { input }),
  login: (username, password) => call('login', { username, password }),
  logout: () => call('logout'),
  lockScreen: () => call('lock_screen'),
  unlockWithPin: (pin) => call('unlock_with_pin', { pin }),
  unlockWithPassword: (password) => call('unlock_with_password', { password }),
  heartbeat: () => call('heartbeat'),
  changeOwnPassword: (currentPassword, newPassword) => call('change_own_password', { currentPassword, newPassword }),
  setOwnPin: (currentPassword, pin) => call('set_own_pin', { currentPassword, pin }),

  listUsers: () => call('list_users'),
  createUser: (account, role) => call('create_user', { account, role }),
  updateUser: (userId, change) => call('update_user', { userId, change }),
  resetUserPassword: (userId, newPassword) => call('reset_user_password', { userId, newPassword }),
  getClinicSettings: () => call('get_clinic_settings'),
  updateClinicSettings: (settings) => call('update_clinic_settings', { settings }),
  listAudit: (limit, beforeId) => call('list_audit', { limit, beforeId }),

  listCategories: () => call('list_categories'),
  saveCategory: (input) => call('save_category', { input }),
  listSuppliers: () => call('list_suppliers'),
  saveSupplier: (input) => call('save_supplier', { input }),
  listProducts: (filter) => call('list_products', { filter }),
  getProduct: (productId) => call('get_product', { productId }),
  saveProduct: (input) => call('save_product', { input }),
  stockIn: (input) => call('stock_in', { input }),
  addInventory: (input) => call('add_inventory', { input }),
  previewStockImport: (text, mode) => call('preview_stock_import', { text, mode }),
  importStock: (text, mode, requestKey, importAgain) => call('import_stock', { text, mode, requestKey, importAgain }),
  exportStockCsv: () => call('export_stock_csv'),
  openStockImportTemplate: () => call('open_stock_import_template'),
  deleteProduct: (productId) => call('delete_product', { productId }),
  adjustStock: (input) => call('adjust_stock', { input }),
  listStockLedger: (productId, limit) => call('list_stock_ledger', { productId, limit }),
  listExpiring: (withinDays) => call('list_expiring', { withinDays }),
  searchProductsForSale: (text) => call('search_products_for_sale', { text }),
  recentProductsForSale: () => call('recent_products_for_sale'),

  searchClients: (text, includeInactive) => call('search_clients', { text, includeInactive }),
  saveClient: (input) => call('save_client', { input }),
  getClientProfile: (clientId, fromDate, toDate) => call('get_client_profile', { clientId, fromDate, toDate }),
  checkClientDuplicates: (query) => call('check_client_duplicates', { query }),
  findDuplicateClients: () => call('find_duplicate_clients'),
  mergeClients: (primaryId, secondaryId) => call('merge_clients', { primaryId, secondaryId }),
  listServices: (includeInactive) => call('list_services', { includeInactive }),
  saveService: (input) => call('save_service', { input }),

  quoteBill: (lines, services, discount, correctingBillId) => call('quote_bill', { lines, services, discount, correctingBillId }),
  finalizeBill: (input) => call('finalize_bill', { input }),
  listBills: (filter) => call('list_bills', { filter }),
  getBill: (billId) => call('get_bill', { billId }),
  cancelBill: (billId, reason) => call('cancel_bill', { billId, reason }),
  returnBillItems: (input) => call('return_bill_items', { input }),
  correctBill: (input) => call('correct_bill', { input }),
  getReceipt: (billId) => call('get_receipt', { billId }),
  exportReceiptPdf: (billId) => call('export_receipt_pdf', { billId }),
  openExport: (fileName) => call('open_export', { fileName }),
  openWhatsApp: (billId) => call('open_whatsapp', { billId }),

  getDashboard: () => call('get_dashboard'),
  salesReport: (range) => call('sales_report', { range }),
  productSalesReport: (range) => call('product_sales_report', { range }),
  stockReport: () => call('stock_report'),
  topSellers: (range) => call('top_sellers', { range }),

  getSystemInfo: () => call('get_system_info'),
  getHealth: () => call('get_health'),
  listBackups: () => call('list_backups'),
  createBackup: () => call('create_backup'),
  restoreBackup: (fileName, password) => call('restore_backup', { fileName, password }),
  openFolder: (folder) => call('open_folder', { folder }),
};
