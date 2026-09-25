# ClinicApp — Design Proposal (v0.2, for approval)

> Status: **DRAFT — awaiting approval.** No application code has been written.
> Scope: local-first Clinic Inventory, Billing & Client Management for a single small clinic.
> Platforms: **Windows 10/11 (x64)** and **macOS (Apple Silicon; Intel optional)**.
> Date: 2026-09-25

Legend used in this document: **[FACT]** verified/decided constraint · **[DEFAULT]** proposed default you can override · **[Q]** open question needing your answer.

---

## 1. Architecture

```
 Clinic PC (Windows or macOS)
 ┌───────────────────────────────────────────────────────────────────┐
 │  Browser (Chrome / Edge)  →  http://localhost:4580                │
 │        │   React SPA (static files served by the same process)    │
 │        ▼   REST/JSON + httpOnly session cookie                    │
 │  ┌─────────────────────────────────────────────────────────────┐  │
 │  │ ClinicApp server  (single Node.js process, OS service)      │  │
 │  │   routes (thin) → services (business rules) → repositories  │  │
 │  │   jobs: daily backup · integrity check · expiry scan        │  │
 │  │   logger: rotating files, PII redacted                      │  │
 │  └───────────────────────────┬─────────────────────────────────┘  │
 │                              ▼                                    │
 │   SQLite  (WAL, foreign_keys=ON, synchronous=FULL)                │
 │   <APP_HOME>/data/clinic.db   →   <APP_HOME>/Backups/*.db         │
 └───────────────────────────────────────────────────────────────────┘
```

Design choices:

- **One process, one port, one DB file.** No Docker, Redis, microservices, cloud services or external APIs. Works fully offline. [FACT]
- **Browser UI instead of Electron/Tauri.** Browser printing already supports A4/A5/thermal printers and "Save as PDF" on both OSs. Electron would add ~150 MB per platform, a separate update channel and signing complexity for one benefit (silent printing), which can instead be achieved with a Chrome shortcut using `--kiosk-printing`. [DEFAULT]
- **Binds to `127.0.0.1` only** — not reachable from the network, and macOS will not show a firewall prompt. [DEFAULT] (see Q12)
- **Layered backend:** HTTP routes contain no business logic; services own rules and transactions; repositories own SQL. Money/tax calculation lives in a shared package used by both UI (preview) and server (authoritative).

---

## 2. Technology Stack

| Layer | Choice | Rationale |
|---|---|---|
| Runtime | Node.js 24 LTS + TypeScript (strict) | Cross-platform; bundled as a portable runtime per OS so the clinic needs no Node install. |
| HTTP | Fastify 5 + Zod | Small, fast, schema validation, structured error handling. |
| Database | SQLite via **better-sqlite3** | Synchronous transactions (no interleaving inside a bill finalize), mature, prebuilt binaries for win-x64 / darwin-arm64 / darwin-x64, **online `.backup()` API** for safe hot backups. |
| ORM / migrations | **Drizzle ORM + drizzle-kit** | Type-safe, parameterised, no separate query-engine binary (simpler packaging than Prisma). Prisma is a valid alternative if you prefer it. [DEFAULT] |
| Password hashing | Argon2id (`@node-rs/argon2`) | Modern algorithm, prebuilt for all three targets. |
| Frontend | React 19 + Vite + TypeScript, **Ant Design 5**, TanStack Query, React Router | Strong tables/forms/autocomplete/notifications for POS-style UI. |
| Shared | `packages/shared` — Zod schemas, money/GST math, types | Single source of truth for validation and calculation. |
| Scheduling | In-process scheduler (e.g. `croner`) | No OS cron dependency. |
| Logging | pino + rotating file | Structured; redacts PII/secrets. |
| Tests | Vitest (unit + integration on temp SQLite files), Playwright (one E2E billing flow) | |
| Service wrapper | Windows: **WinSW** · macOS: **launchd LaunchAgent** | Auto-start + auto-restart. |
| CI | GitHub Actions matrix: `windows-latest`, `macos-14` (arm64), optional `macos-13` (x64) | Tests + packaging per OS. |

Dependency policy: every dependency must justify itself; lockfile committed; `npm audit` + SCA in CI.

---

## 3. Cross-Platform Strategy (Windows + macOS)

| Concern | Windows | macOS |
|---|---|---|
| App home (`APP_HOME`) | `C:\ClinicApp\` | `~/Library/Application Support/ClinicApp/` |
| Override | `CLINICAPP_HOME` env var | same |
| Sub-folders | `data\`, `Backups\`, `logs\`, `config\`, `app\`, `runtime\` | same names |
| Service | WinSW Windows service (auto-start at boot, restart on crash) | LaunchAgent `~/Library/LaunchAgents/com.clinicapp.server.plist` (`RunAtLoad`, `KeepAlive`), runs as logged-in user, **not root** |
| Launcher | Desktop + Start-menu shortcut → opens browser | `ClinicApp.app` (tiny launcher) in Applications/Dock → `open http://localhost:4580` |
| Installer | `install.ps1` in zip; optional Inno Setup `.exe` | `install.sh` in zip; optional signed `.pkg` |
| File permissions | `icacls` restrict data/Backups/config to clinic user + Administrators | `chmod 700` folders, `0600` DB/backup/config files |
| Disk encryption (recommended) | BitLocker | FileVault |
| Code signing | Authenticode cert recommended (SmartScreen) | Developer ID + notarization recommended (Gatekeeper); otherwise documented `xattr -dr com.apple.quarantine` step |
| System backup interplay | — | Exclude `data/` from Time Machine, include `Backups/` |

Rules enforced in code:
- No hard-coded OS paths; everything via a single `paths.ts` resolver using `path.join()`.
- No shell-specific logic in npm scripts — build/package/admin scripts are Node/TypeScript (`scripts/*.ts`). Only the thin installers are PowerShell/sh.
- `.gitattributes` for line endings; `forceConsistentCasingInFileNames` (APFS may be case-sensitive).
- One release package **per OS/arch** (native modules and Node runtime are platform-specific; `node_modules` is never copied across OSs).
- Restore procedure closes the DB connection before swapping files (Windows cannot replace an open file) — same sequence on both OSs.
- Macs sleep frequently → backup catch-up runs at startup **and** on a periodic check (if last backup > 20 h old).

---

## 4. Business Rules & Proposed Defaults

| # | Topic | Proposed default |
|---|---|---|
| D1 | Money | Stored as **integer paise** (no floating point). Currency symbol configurable (default ₹, INR). |
| D2 | GST | Selling price = **MRP inclusive of GST** (Indian pharmacy norm). Tax shown = `net × rate / (100 + rate)`. Setting to switch to tax-exclusive pricing, and a global "GST not applicable" switch. Per-product GST rate (0/5/12/18%). Receipt shows GSTIN and a CGST/SGST split when enabled. |
| D3 | Discount | Line discount and bill-level discount, each as % or ₹. Bill discount is **allocated proportionally to lines** (for correct tax and return refunds). Receptionist cap: 10% of bill (configurable); higher requires admin approval (admin enters credentials). |
| D4 | Rounding | Lines to the paisa; bill total rounded to nearest ₹1 with a visible "Round off" line (setting can disable). |
| D5 | Batches | Stock is held **per batch** (batch no, expiry, purchase price, MRP). Sale allocation is **FEFO** (first-expiry-first-out); a line can span multiple batches. Receptionist may manually pick a batch if needed. |
| D6 | Expiry | **Expired batches cannot be sold.** Warnings for batches expiring ≤ 30 days on the billing screen. Admin writes off expired stock via `EXPIRY` transaction. |
| D7 | Negative stock | Never. DB `CHECK (quantity >= 0)`. Admin adjustments set any value ≥ 0 with mandatory reason. |
| D8 | Units | Sold in a single base unit per product (strip, tube, piece, bottle). **No loose-tablet / pack conversion in v1.** |
| D9 | Invoice numbering | `{PREFIX}/{FY}/{SEQ}` e.g. `INV/26-27/000123`; gapless; resets every financial year (Apr–Mar); allocated **inside** the finalize transaction from a counter table. Cancelled bills keep their number. |
| D10 | Duplicate submission | Client generates an `idempotencyKey` (UUID) when a bill is opened; `UNIQUE` on `bill.idempotency_key`; a repeat request **returns the existing bill**. Finalize button disabled while pending. |
| D11 | Drafts | In-progress bill held in browser `localStorage` (survives refresh). Nothing written to DB until finalize. |
| D12 | Client on bill | Optional ("Walk-in" allowed). Phone not unique (families share phones); duplicates warned, not blocked. |
| D13 | Payments | Cash, UPI, Card, Other. **Split payments allowed**; sum must equal total. Cash "amount tendered → change" helper. No credit/dues in v1. |
| D14 | Cancellation | **Admin only**, mandatory reason. Bill → `CANCELLED` (never deleted); stock restored to the **same batches** via reversal transactions; audit logged. A bill with returns cannot be cancelled (return remaining items instead). |
| D15 | Returns | Against original bill line; qty ≤ sold − already returned; refund = original net line price (after discounts) pro-rata. Per item: **restock** (to original batch, if not expired) or **don't restock** (`RETURN` + `DAMAGE`). Receptionist within 7 days; admin anytime. |
| D16 | Medical data | **No diagnoses/prescriptions stored in v1.** Optional short notes on client and bill only. |
| D17 | Purchases | Lightweight **Stock Receipt ("Stock In")**: supplier, supplier invoice no/date, lines (product, batch, expiry, qty, cost, MRP). Creates batches + `PURCHASE` transactions. No purchase orders / supplier payables. |
| D18 | Suppliers | Simple master: name, phone, email, GSTIN, address. |
| D19 | Time | Stored UTC; "today", reports and receipts use clinic timezone (default `Asia/Kolkata`). |
| D20 | Backups | Automatic daily at 13:00 **plus** catch-up on start/wake if last > 20 h. Retention: 30 daily + 12 monthly. Every backup verified with `PRAGMA integrity_check`. Manual backup any time. |
| D21 | Users | Multiple named accounts (no shared logins), roles `ADMIN`, `RECEPTIONIST`. First admin created via first-run setup screen or `clinicapp create-admin` CLI. **No default password.** |
| D22 | Sessions | Idle timeout 60 min, absolute 12 h (configurable). Quick re-login keeps the bill draft. |
| D23 | Printer | Two receipt templates: **A5/A4** and **80 mm thermal**, selected in Settings. Printed via browser print dialog; PDF via "Save as PDF". |
| D24 | Stock ledger | `inventory_batch.quantity` is a cached balance; `inventory_transaction` is the authoritative, append-only ledger. Daily job verifies `SUM(ledger) = quantity` per batch and raises a dashboard alert on mismatch — **never auto-corrects**. |

### Role permissions

| Capability | ADMIN | RECEPTIONIST |
|---|---|---|
| Dashboard | ✓ | ✓ |
| Clients: create/edit/search/history | ✓ | ✓ |
| Create bill, print receipt | ✓ | ✓ |
| Discount above cap | ✓ | needs admin approval |
| Returns | ✓ anytime | ✓ within 7 days |
| Cancel bill | ✓ | ✗ |
| View inventory / stock / expiry | ✓ | ✓ (read-only; purchase price hidden) |
| Products, categories, suppliers CRUD | ✓ | ✗ |
| Stock In, adjustments, write-offs | ✓ | ✗ |
| Reports (sales, product sales, stock) | ✓ | Today's own sales only |
| Users, settings, backup/restore, audit log | ✓ | ✗ |

---

## 5. Database Schema

### 5.1 ERD

```
user ─────< session
user ─────< audit_log
category ─< product >─ supplier (default supplier)
product ──< inventory_batch ──< inventory_transaction >── user
supplier ─< stock_receipt ──< stock_receipt_item >── inventory_batch
client ───< bill ──< bill_item >── product
                      bill_item ──< bill_item_batch >── inventory_batch
            bill ──< payment
            bill ──< sales_return ──< return_item >── bill_item
                                        return_item >── inventory_batch
inventory_transaction ─(nullable FK)→ bill | sales_return | stock_receipt
setting (key/value) · invoice_counter (series, fy) · backup_log · schema_migrations
```

### 5.2 Tables

All tables: `id INTEGER PRIMARY KEY`, `created_at`, `updated_at` (ISO-8601 UTC TEXT) unless noted. Money columns are `INTEGER` paise. Rates are `INTEGER` basis points (18% = 1800).

**user**
| column | type | notes |
|---|---|---|
| username | TEXT | UNIQUE, case-insensitive (`COLLATE NOCASE`) |
| full_name | TEXT | |
| password_hash | TEXT | Argon2id |
| role | TEXT | CHECK IN ('ADMIN','RECEPTIONIST') |
| is_active | INTEGER | |
| failed_login_count, locked_until, last_login_at, password_changed_at | | lockout after 5 failures / 15 min |

**session** — `id TEXT PK` (SHA-256 of 256-bit random token), `user_id FK`, `expires_at`, `last_seen_at`, `user_agent`.

**category** — `name UNIQUE NOCASE`, `is_active`.

**supplier** — `name`, `phone`, `email`, `gstin`, `address`, `notes`, `is_active`.

**product**
| column | notes |
|---|---|
| sku | UNIQUE NOCASE (auto-generated `P-00001` if blank) |
| name, generic_name, manufacturer | indexed for search |
| category_id | FK category |
| product_type | CHECK IN ('MEDICINE','TABLET_CAPSULE','CREAM_OINTMENT','MEDICAL_PRODUCT','CONSUMABLE','OTHER') |
| unit | e.g. strip, tube, piece, bottle |
| hsn_code | optional |
| gst_rate_bp | INTEGER, CHECK 0..2800 |
| default_selling_price_paise | used when creating new batches |
| min_stock | low-stock threshold (default from settings) |
| default_supplier_id | FK supplier, nullable |
| is_active, notes | |

*Current stock is derived:* `SUM(inventory_batch.quantity)` over active, non-expired batches (a SQL view `v_product_stock`).

**inventory_batch**
| column | notes |
|---|---|
| product_id | FK |
| batch_no | UNIQUE(product_id, batch_no) |
| expiry_date | DATE, nullable for non-expiring consumables |
| purchase_price_paise, selling_price_paise | per-batch MRP |
| quantity | INTEGER **CHECK (quantity >= 0)** |
| is_active | |

**inventory_transaction** (append-only: SQLite triggers `RAISE(ABORT)` on UPDATE/DELETE)
| column | notes |
|---|---|
| batch_id | FK |
| type | CHECK IN ('INITIAL_STOCK','PURCHASE','SALE','RETURN','ADJUSTMENT','DAMAGE','EXPIRY') |
| qty_change | signed integer |
| previous_qty, new_qty | CHECK (new_qty = previous_qty + qty_change) |
| reason | required for ADJUSTMENT/DAMAGE/EXPIRY |
| bill_id, sales_return_id, stock_receipt_id | nullable FKs |
| user_id | FK |
| created_at | |

**stock_receipt** — `supplier_id FK`, `supplier_invoice_no`, `invoice_date`, `notes`, `user_id FK`.
**stock_receipt_item** — `stock_receipt_id FK`, `batch_id FK`, `qty`, `unit_cost_paise`.

**client**
| column | notes |
|---|---|
| client_code | UNIQUE, `CL-000001` |
| full_name | indexed |
| phone | indexed, not unique |
| email, date_of_birth, gender (CHECK IN ('MALE','FEMALE','OTHER','UNSPECIFIED')), address | optional |
| emergency_contact_name, emergency_contact_phone | optional |
| notes, last_visit_at, is_active | |

**bill**
| column | notes |
|---|---|
| bill_no | UNIQUE |
| idempotency_key | UNIQUE |
| client_id | FK, nullable (walk-in) |
| status | CHECK IN ('FINALIZED','CANCELLED') |
| subtotal_paise, line_discount_paise, bill_discount_paise, tax_paise, round_off_paise, total_paise | |
| bill_discount_type, bill_discount_value | 'PERCENT'/'AMOUNT' |
| pricing_mode | 'INCLUSIVE'/'EXCLUSIVE' snapshot |
| notes | |
| created_by, discount_approved_by | FK user |
| cancelled_by, cancelled_at, cancel_reason | |
| finalized_at | |

**bill_item** — `bill_id FK`, `product_id FK`, snapshots `product_name`, `sku`, `unit`, `hsn_code`; `unit_price_paise`, `qty CHECK > 0`, `line_discount_paise`, `allocated_bill_discount_paise`, `gst_rate_bp`, `taxable_paise`, `tax_paise`, `line_total_paise`, `returned_qty CHECK (returned_qty <= qty)`.

**bill_item_batch** — `bill_item_id FK`, `batch_id FK`, `qty`. Records exactly which batches were sold (used for cancellation/return restocking).

**payment** — `bill_id FK`, `method CHECK IN ('CASH','UPI','CARD','OTHER')`, `amount_paise CHECK > 0`, `reference`.

**sales_return** — `return_no UNIQUE`, `bill_id FK`, `reason`, `refund_total_paise`, `refund_method`, `user_id FK`.
**return_item** — `sales_return_id FK`, `bill_item_id FK`, `batch_id FK`, `qty`, `refund_paise`, `restock INTEGER`.

**audit_log** (append-only) — `user_id`, `action`, `entity`, `entity_id`, `details_json` (no PII beyond IDs), `created_at`.
Actions: `LOGIN`, `LOGIN_FAILED`, `LOGOUT`, `USER_CREATE/UPDATE/DISABLE`, `PASSWORD_RESET`, `BILL_FINALIZE`, `BILL_CANCEL`, `RETURN_CREATE`, `DISCOUNT_OVERRIDE`, `STOCK_ADJUST`, `STOCK_RECEIPT`, `PRODUCT_UPDATE`, `SETTINGS_UPDATE`, `BACKUP_CREATE`, `RESTORE`, `REPORT_EXPORT`.

**setting** — `key TEXT PK`, `value_json`.
**invoice_counter** — `series TEXT` ('BILL','RETURN'), `fy TEXT`, `next_seq INTEGER`, PK(series, fy).
**backup_log** — `file_name`, `size_bytes`, `sha256`, `kind` ('MANUAL','AUTO','PRE_RESTORE','PRE_MIGRATION'), `verified`, `created_by`.

Notes on requested entities:
- **StockAdjustment** is represented as `inventory_transaction` rows of type `ADJUSTMENT`/`DAMAGE`/`EXPIRY` with mandatory reason and user — a separate table would duplicate the ledger.
- **Return/ReturnItem** → `sales_return` / `return_item` (`RETURN` is a SQL keyword).
- No hard deletes on financial tables; products/clients/users are **deactivated**.

### 5.3 SQLite configuration
`PRAGMA journal_mode=WAL; synchronous=FULL; foreign_keys=ON; busy_timeout=5000;` — `FULL` chosen over `NORMAL` because clinic PCs can lose power; the performance cost is negligible at this volume.

---

## 6. Critical Transaction Flows

### 6.1 Finalize bill (single `BEGIN IMMEDIATE` transaction)
```
1. If bill with idempotencyKey exists → return it (HTTP 200, no new bill).
2. Load products + active, non-expired batches (FEFO order).
3. Validate each line: product active, qty > 0, total available ≥ qty
   → else ROLLBACK, 409 INSUFFICIENT_STOCK { product, available }.
4. Recalculate all amounts server-side (client totals ignored).
5. Validate discount cap / approval; validate payments sum = total.
6. Allocate bill_no from invoice_counter.
7. INSERT bill, bill_items, bill_item_batches, payments.
8. For each batch allocation:
     UPDATE inventory_batch SET quantity = quantity - :n
      WHERE id = :id AND quantity >= :n            -- must affect exactly 1 row
     INSERT inventory_transaction (SALE, -n, prev, new, bill_id, user)
9. UPDATE client.last_visit_at; INSERT audit_log.
COMMIT   (any exception → ROLLBACK; nothing persisted)
```
The conditional `UPDATE … AND quantity >= n` guarantees no negative stock even under concurrent sales.

### 6.2 Cancel bill (admin)
One transaction: verify `FINALIZED` and no returns → set `CANCELLED` + reason → for each `bill_item_batch`, add qty back and insert `RETURN` transaction referencing the bill (reason "Bill cancelled") → audit.

### 6.3 Return
One transaction: validate qty ≤ `qty − returned_qty` per line → compute pro-rata refund → insert `sales_return`/`return_item` → increment `bill_item.returned_qty` → restock to original batch (`RETURN`) or, if not restockable/expired, `RETURN` then `DAMAGE` → audit.

### 6.4 Stock adjustment (admin)
One transaction: set batch qty to a new value ≥ 0 → `ADJUSTMENT`/`DAMAGE`/`EXPIRY` transaction with reason → audit.

### 6.5 Calculation (shared package, inclusive mode)
```
gross        = unit_price × qty
line_net     = gross − line_discount − allocated_bill_discount
tax          = round(line_net × rate / (10000 + rate))      (rate in bp)
taxable      = line_net − tax
bill_total   = Σ line_net ; round_off = round_to_rupee(bill_total) − bill_total
```
Allocation of bill discount uses the largest-remainder method so paise always sum exactly.

---

## 7. API Design (REST, `/api/v1`, JSON)

Conventions: JSON bodies validated with Zod; pagination `?page=&pageSize=` (max 100); errors `{ "error": { "code", "message", "details"? } }`; state-changing requests require session cookie + same-origin check.

| Area | Method & path | Role |
|---|---|---|
| Setup | `GET /setup/status` · `POST /setup/admin` (only when no users exist) | public |
| Auth | `POST /auth/login` · `POST /auth/logout` · `GET /auth/me` · `POST /auth/change-password` · `POST /auth/approve` (admin credentials for discount override) | any |
| Dashboard | `GET /dashboard/summary` | any |
| Products | `GET /products?q=&categoryId=&manufacturer=&type=&status=&stock=low|out` · `POST /products` · `GET /products/:id` · `PATCH /products/:id` · `POST /products/:id/deactivate` · `POST /products/:id/activate` | read: any · write: ADMIN |
| Product search (billing) | `GET /products/search?q=` → name/generic/SKU/batch with available qty & price | any |
| Batches | `GET /products/:id/batches` · `PATCH /batches/:id` (price/expiry correction, audited) · `POST /batches/:id/adjust` | ADMIN |
| Stock In | `POST /stock-receipts` · `GET /stock-receipts` · `GET /stock-receipts/:id` | ADMIN |
| Transactions | `GET /inventory/transactions?productId=&batchId=&type=&from=&to=` | ADMIN (receptionist: read) |
| Stock views | `GET /inventory/low-stock` · `GET /inventory/out-of-stock` · `GET /inventory/expiry?bucket=expired|30|60|90` | any |
| Categories | `GET/POST /categories` · `PATCH /categories/:id` | write: ADMIN |
| Suppliers | `GET/POST /suppliers` · `GET/PATCH /suppliers/:id` | ADMIN |
| Clients | `GET /clients?q=` (name/phone/code) · `POST /clients` · `GET /clients/:id` · `PATCH /clients/:id` · `GET /clients/:id/history` | any |
| Billing | `POST /bills/quote` (compute only, no writes) · `POST /bills` (finalize, requires `idempotencyKey`) · `GET /bills?from=&to=&status=&clientId=&q=` · `GET /bills/:id` · `GET /bills/:id/receipt` | any |
| Cancel | `POST /bills/:id/cancel { reason }` | ADMIN |
| Returns | `POST /bills/:id/returns` · `GET /returns?from=&to=` · `GET /returns/:id` | any (time-limited for receptionist) |
| Reports | `GET /reports/sales?range=today|yesterday|week|month|custom&from=&to=` · `/reports/product-sales` · `/reports/stock` · `/reports/expiry` · `/reports/client-history?clientId=` — all support `&format=csv` | ADMIN (receptionist: today) |
| Settings | `GET /settings` · `PUT /settings` | read: any (public subset) · write: ADMIN |
| Users | `GET/POST /users` · `PATCH /users/:id` · `POST /users/:id/reset-password` | ADMIN |
| Backup | `GET /backups` · `POST /backups` · `POST /backups/:id/verify` · `POST /backups/restore { backupId, confirmText, password }` | ADMIN |
| Audit | `GET /audit?from=&to=&action=&userId=` | ADMIN |
| Health | `GET /health` → db ok, integrity status, last backup age, version | public (no PII) |

Error codes (examples) → user message:
| code | user-facing message |
|---|---|
| `INSUFFICIENT_STOCK` | Insufficient stock. Available quantity: {n} |
| `BATCH_EXPIRED` | This batch has expired and cannot be sold. |
| `PRODUCT_INACTIVE` | The selected product is no longer available. |
| `DUPLICATE_SKU` | A product with this SKU already exists. |
| `DISCOUNT_APPROVAL_REQUIRED` | Discount above {cap}% needs administrator approval. |
| `PAYMENT_MISMATCH` | Payment total does not match bill total. |
| `RETURN_QTY_EXCEEDED` | Only {n} units can still be returned. |
| `FORBIDDEN` | You do not have permission to perform this action. |
| `INTERNAL` | Unable to complete the operation. Please try again or contact the administrator. (ref: {logId}) |

---

## 8. Folder Structure

```
clinic-app/
├─ package.json                 # npm workspaces
├─ .gitattributes  .editorconfig  .nvmrc  tsconfig.base.json
├─ apps/
│  ├─ server/
│  │  ├─ src/
│  │  │  ├─ config/             # env loading, paths.ts (per-OS APP_HOME), settings cache
│  │  │  ├─ db/                 # connection + pragmas, schema.ts, migrations/, seed/
│  │  │  ├─ lib/                # errors, logger, auth guards, idempotency, time/FY helpers
│  │  │  ├─ modules/
│  │  │  │  ├─ auth/  users/  settings/  audit/
│  │  │  │  ├─ products/  inventory/  suppliers/  categories/
│  │  │  │  ├─ clients/  billing/  returns/
│  │  │  │  ├─ reports/  backup/  dashboard/
│  │  │  │  │   └─ each: *.routes.ts, *.service.ts, *.repo.ts, *.schema.ts, *.test.ts
│  │  │  ├─ jobs/               # backup, integrity check, expiry scan
│  │  │  ├─ cli.ts              # create-admin, backup, restore, doctor, reset-password
│  │  │  └─ server.ts
│  │  └─ test/                  # integration tests (temp SQLite per test)
│  └─ web/
│     ├─ src/
│     │  ├─ app/                # router, layout, auth context, theme
│     │  ├─ pages/              # dashboard, clients, billing, inventory, reports, settings
│     │  ├─ components/         # ProductSearch, ClientSearch, MoneyText, ConfirmDialog…
│     │  ├─ features/billing/   # bill draft store, keyboard shortcuts
│     │  ├─ print/              # ReceiptA5.tsx, ReceiptThermal80.tsx, print.css
│     │  └─ api/                # typed client + TanStack Query hooks
│     └─ e2e/                   # Playwright
├─ packages/
│  └─ shared/                   # zod schemas, money.ts, gst.ts, types, error codes
├─ scripts/                     # build.ts, package.ts (per OS/arch), fetch-node-runtime.ts
├─ packaging/
│  ├─ windows/                  # winsw.xml, install.ps1, uninstall.ps1, inno-setup.iss
│  └─ macos/                    # com.clinicapp.server.plist, install.sh, uninstall.sh, launcher app
├─ docs/
│  ├─ DESIGN.md  ARCHITECTURE.md
│  ├─ install-windows.md  install-macos.md
│  ├─ user-guide-receptionist.md  admin-guide.md
│  ├─ backup-restore.md  configuration.md  troubleshooting.md  runbook.md
│  └─ CHANGELOG.md
└─ .github/workflows/ci.yml     # lint, typecheck, test, audit, package (win + mac)
```

Runtime layout on a clinic PC (`APP_HOME`):
```
APP_HOME/
├─ app/        # versioned release (app-1.0.0/), 'current' pointer
├─ runtime/    # bundled Node for this OS/arch
├─ config/     # clinicapp.env (PORT, HOST, SESSION_SECRET generated at install)
├─ data/       # clinic.db, clinic.db-wal, clinic.db-shm
├─ Backups/    # clinic-2026-09-25_1300.db (+ .sha256)
└─ logs/       # app-YYYY-MM-DD.log
```

---

## 9. UI Design & Wireframes

Navigation (left sidebar, collapsible; receptionist sees only permitted items):
```
Dashboard
Clients      ├ Client List  ├ Add Client  └ Client Details
Billing      ├ New Bill (F1 from anywhere)  └ Bill History
Inventory    ├ Products  ├ Add Product  ├ Stock  ├ Stock In  ├ Stock Transactions  ├ Low Stock  └ Expiry
Reports      ├ Sales  ├ Product Sales  ├ Inventory  └ Client History
Settings     ├ Clinic  ├ Users  ├ Backup & Restore  └ System Settings
```
Visual: light background, Ant Design default theme with one accent colour, 15–16 px base font, large primary buttons, money right-aligned in tabular numerals.

### 9.1 Dashboard
```
┌ ClinicApp ─────────────────────────── 🔍 Search clients / products / bills   Priya (Reception) ▾ ┐
│ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐           │
│ │Today's sales │ │Bills today   │ │Clients today │ │Low stock     │ │Out of stock  │           │
│ │  ₹ 12,450    │ │     34       │ │     29       │ │   7  ⚠       │ │   2  ⛔      │           │
│ └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘           │
│ [ + New Bill (F1) ]   [ + New Client ]              Expiring ≤30 days: 5 ›   Items: 214        │
│                                                                                                │
│ Recent bills                                   │ Recent clients                                │
│ INV/26-27/000123  John Doe     ₹150   10:42 ›  │ John Doe       98xxxx1234   today ›           │
│ INV/26-27/000122  Walk-in      ₹80    10:31 ›  │ Meera Shah     97xxxx8811   today ›           │
│ ...                                            │ ...                                           │
│ ⚠ Last backup: today 13:00 ✓                                                                   │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 9.2 New Bill (primary workflow, keyboard-first)
```
┌ New Bill ───────────────────────────────────────────────────────────── [Clear bill] ┐
│ Client [🔍 name / phone / code        ▾]  John Doe · 98xxxx1234 · last visit 20-Sep  │
│        (Alt+C)                       [+ New client]   [Walk-in]                       │
│──────────────────────────────────────────────────────────────────────────────────────│
│ Product [🔍 type name, generic, SKU, batch…  (F2)]   Qty [ 1 ]  [ Add ↵ ]            │
│   ┌──────────────────────────────────────────────────────────────┐                   │
│   │ Paracetamol 500mg  · strip · ₹20.00 · Stock 97 · Exp 03/2027 │ ← autocomplete    │
│   │ Paracetamol 650mg  · strip · ₹30.00 · Stock 12 · Exp 11/2026 │                   │
│   └──────────────────────────────────────────────────────────────┘                   │
│──────────────────────────────────────────────────────────────────────────────────────│
│ #  Product              Batch     Qty   Price     Disc    GST   Total          │
│ 1  Paracetamol 500mg    B2231     [2]   ₹20.00    ₹0      12%   ₹40.00    ✕    │
│ 2  Pain Relief Cream    C119      [1]   ₹120.00   ₹0      18%   ₹120.00   ✕    │
│──────────────────────────────────────────────────────────────────────────────────────│
│ Bill discount [ 10 ] (₹ | %)            Subtotal            ₹160.00                   │
│ Notes [                    ]            Discount           −₹10.00                   │
│                                         GST (incl.)          ₹17.19                   │
│ Payment  (●Cash) (○UPI) (○Card) [Split] Round off            ₹0.00                   │
│ Tendered [ 200 ]  Change ₹50.00         TOTAL              ₹150.00                   │
│                                                                                      │
│                              [ Finalize & Print  (F9) ]   ← disabled while saving    │
└──────────────────────────────────────────────────────────────────────────────────────┘
```
Interaction target: **select client → type product → Enter → (repeat) → F9 → print dialog** ≈ 6–8 keystrokes/clicks for a two-item bill. Inline errors (e.g. "Insufficient stock. Available quantity: 2") appear on the qty field before finalize, and are re-checked on the server.

### 9.3 Receipt (A5 / 80 mm variants share this content)
```
          SKIN DOC CLINIC            (from Settings)
     12 MG Road, Pune · 020-xxxxxxx
          GSTIN: 27XXXXX1234X1Z5
------------------------------------------------
Bill No: INV/26-27/000123     25-Sep-2026 10:42
Client : John Doe (CL-000045)
------------------------------------------------
Item                 Qty    Price      Total
Paracetamol 500mg     2     20.00      40.00
 Batch B2231 Exp 03/27
Pain Relief Cream     1    120.00     120.00
------------------------------------------------
Subtotal                              160.00
Discount                              -10.00
GST incl. (CGST 8.60 / SGST 8.59)      17.19
TOTAL                             ₹   150.00
Paid: Cash 200.00   Change 50.00
------------------------------------------------
Billed by: Priya
{Receipt footer from Settings}  Thank you
```
Buttons after finalize: **[Print]  [Save PDF]  [New Bill (F1)]**. Cancelled bills reprint with a "CANCELLED" watermark.

### 9.4 Client details
```
┌ John Doe  CL-000045 ───────────────────────────── [Edit] [+ New Bill for client] ┐
│ 98xxxx1234 · john@example.com · DOB 12-Mar-1985 (41) · Male · Pune               │
│ Client since 02-Jan-2026 · Last visit 25-Sep-2026 · Visits 6 · Total spent ₹1,840 │
│ Tabs:  [Visits]  [Products provided]  [Profile]                                  │
│──────────────────────────────────────────────────────────────────────────────────│
│ 25-Sep-2026  INV/26-27/000123   ₹150   FINALIZED                         ›       │
│    Paracetamol 500mg × 2 · Pain Relief Cream × 1                                 │
│ 20-Sep-2026  INV/26-27/000119   ₹230   PARTIALLY RETURNED                ›       │
│    Amoxicillin 500mg × 1 · Antiseptic Cream × 2   (returned: Antiseptic × 1)     │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### 9.5 Products / Stock
```
┌ Products ─────────────────────────────────────────────────────── [+ Add Product] [Stock In] ┐
│ 🔍 name / generic / SKU / batch   Category [All ▾]  Manufacturer [All ▾]  Stock [All|Low|Out] │
│ SKU     Name               Category   Type     Stock  Min  Nearest exp  Price   Status       │
│ P-00001 Paracetamol 500mg  Analgesic  Tablet    97    20   03/2027      ₹20.00  Active   ⋯   │
│ P-00014 Bandage 5cm        Dressing   Consum.    4 ⚠  10   —            ₹35.00  Active   ⋯   │
│ P-00021 Antiseptic Cream   Topical    Cream      0 ⛔  5   —            ₹90.00  Active   ⋯   │
│ ⋯ menu: View batches · Stock movements · Adjust stock · Edit · Deactivate                    │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 9.6 Backup & Restore
```
┌ Backup & Restore ──────────────────────────────────────────────────────────────┐
│ Location: C:\ClinicApp\Backups   (macOS: ~/Library/Application Support/…)       │
│ Automatic daily backup: [On] at [13:00]  Keep: [30] daily, [12] monthly         │
│ Last backup: 25-Sep-2026 13:00 ✓ verified            [ Back up now ]            │
│─────────────────────────────────────────────────────────────────────────────────│
│ File                          Size    Type     Verified                          │
│ clinic-2026-09-25_1300.db     4.1 MB  AUTO     ✓     [Restore…] [Show in folder] │
│ clinic-2026-09-24_1300.db     4.1 MB  AUTO     ✓     [Restore…]                  │
│ Restore requires: typing RESTORE + admin password. A pre-restore backup is       │
│ always created first. The app restarts automatically.                           │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 10. Security Considerations

| Area | Measure |
|---|---|
| Network exposure | Bind `127.0.0.1`; no inbound ports; no outbound calls. |
| Authentication | Named accounts; Argon2id (OWASP params); min 8 chars; lockout 5 attempts / 15 min; forced change on reset. |
| Sessions | Random 256-bit token, stored hashed; cookie `HttpOnly; SameSite=Strict; Path=/`; idle + absolute timeouts; logout invalidates server-side. |
| CSRF | SameSite=Strict + `Origin`/`Host` header validation on state-changing requests. |
| Authorization | Role guard on every route (deny by default); receptionist-visible fields filtered server-side (e.g. purchase price). |
| Input validation | Zod on every request; strict types, lengths, ranges. |
| SQL injection | Drizzle parameterised queries only; raw SQL banned by lint rule except reviewed repos. |
| XSS | React escaping; no `dangerouslySetInnerHTML`; strict CSP (`default-src 'self'`), `X-Content-Type-Options`, `frame-ancestors 'none'`. |
| Secrets | No secrets in source. `SESSION_SECRET` generated at install into `APP_HOME/config/clinicapp.env` (0600 / ACL-restricted). `.env*` git-ignored; `.env.example` with placeholders only. |
| Data at rest | OS disk encryption (BitLocker/FileVault) recommended; folder ACLs; backups inherit restricted permissions. Optional backup encryption → Q11. |
| Logging | No passwords, tokens, phone numbers or client names in logs; errors logged with correlation ID. |
| Audit | Append-only `audit_log` for sensitive operations (see §5.2). |
| Supply chain | Lockfile, `npm audit`/SCA in CI, pinned Node runtime with checksum verification during packaging, signed releases where certificates are available. |
| Restore safety | Admin + password re-entry + typed confirmation; backup file integrity & schema-version check before swap; automatic pre-restore backup. |

---

## 11. Error Handling & Observability

- Central error mapper: domain errors → codes/messages (§7); SQLite constraint errors mapped to friendly messages; unknown errors → generic message + log reference ID.
- Database errors are **never swallowed**; every transaction failure is logged at `error` with context (no PII).
- Logs: `APP_HOME/logs/`, daily rotation, 30-day retention.
- `/health` endpoint and a "System status" panel in Settings (DB OK, integrity check result, last backup age, disk free space, version).
- `clinicapp doctor` CLI: checks paths, permissions, port availability, DB integrity, service status.

---

## 12. Backup & Restore

- **Mechanism:** better-sqlite3 online backup API → consistent snapshot while the app runs → `PRAGMA integrity_check` on the copy → SHA-256 recorded in `backup_log`.
- **Schedule:** daily at configured time + catch-up on start/wake (> 20 h). Also automatic backup **before every migration/update** and **before every restore**.
- **Retention:** 30 daily + 12 monthly (configurable); pre-restore/pre-migration backups kept 90 days.
- **Off-machine copy (strongly recommended):** configurable secondary folder (USB drive / NAS / OneDrive-synced folder) — copied after each successful backup; failure raises a dashboard warning, never blocks billing.
- **Restore:** Settings → Backup & Restore → select → verify → confirm (typed + password) → pre-restore backup → close DB → move current `clinic.db*` aside → copy backup → reopen → integrity check → restart service. CLI equivalent `clinicapp restore <file>` for when the UI can't start.

---

## 13. Deployment Strategy

### Windows
1. Download `ClinicApp-<ver>-win-x64.zip` (or `.exe` installer).
2. Run `install.ps1` as Administrator → creates `C:\ClinicApp\…`, sets ACLs, generates config/secret, runs migrations, installs & starts **ClinicApp** service (WinSW, automatic start), creates desktop/Start shortcut.
3. Open shortcut → first-run screen → create admin → enter clinic settings.
- Start/stop: Start-menu "Start/Stop ClinicApp", or `sc start ClinicApp` / `sc stop ClinicApp`.

### macOS
1. Download `ClinicApp-<ver>-macos-arm64.zip` (or `.pkg`).
2. Run `install.sh` → creates `~/Library/Application Support/ClinicApp/…`, sets permissions, generates config/secret, runs migrations, installs LaunchAgent, installs `ClinicApp.app` launcher.
3. Open ClinicApp from Applications → first-run admin setup.
- Start/stop: `launchctl kickstart -k gui/$UID/com.clinicapp.server` / `launchctl bootout gui/$UID/com.clinicapp.server`, wrapped by `clinicapp start|stop|status`.

### Updates (both)
New version extracted to `app/app-<ver>/` → automatic pre-update backup → stop service → switch `current` → migrations run on start → health check → on failure, switch back and restore pre-update backup (documented one-command rollback). Semantic versioning; CHANGELOG per release.

### Demo data
`clinicapp seed --demo` loads sample categories, suppliers, ~30 products with multiple batches (incl. expired / near-expiry / low-stock cases), 10 clients, and historical bills — **only on an empty database**, never in production by default.

---

## 14. Testing Strategy

| Level | Coverage |
|---|---|
| Unit (shared) | Subtotal, line/bill discount, proportional allocation (paise exact), GST inclusive/exclusive, rounding, FY invoice numbering, expiry bucketing, low-stock calculation. |
| Integration (server, temp SQLite) | Add product; stock in; sale deducts stock; FEFO multi-batch split; insufficient stock rejects with no side effects; expired batch blocked; **atomicity** (fault injected after bill insert → no bill, no stock change, no counter increment); idempotent duplicate submit returns same bill; concurrent last-unit sale; cancel restores exact batches; partial returns & over-return rejection; restock vs damage; adjustment ≥ 0; ledger invariant; client create/update/search/history; RBAC per route; login lockout; backup + restore round-trip. |
| E2E (Playwright) | Login → new bill → finalize → receipt shown → stock reduced; double-click Finalize creates one bill. |
| Cross-platform | Full suite on Windows and macOS in CI; path resolver & restore tests per OS. |
| Security | `npm audit`, lint rules (no raw SQL, no `dangerouslySetInnerHTML`), dependency review. |

---

## 15. Implementation Phases

| Phase | Deliverable | Exit check |
|---|---|---|
| 1 | Monorepo, tooling, CI, paths resolver, SQLite + migrations, auth (setup/login/sessions/RBAC), users, settings, audit, app shell | App starts on Win + mac; tests green |
| 2 | Categories, suppliers, products, batches, Stock In, adjustments, transactions ledger, low-stock/expiry views | Inventory tests green |
| 3 | Clients CRUD, search, profile | Client tests green |
| 4 | Billing: quote, finalize (transaction + idempotency), bill history, cancel, returns | Billing/atomicity tests green |
| 5 | Receipt templates (A5 + 80 mm), print, PDF | Manual print check on both OSs |
| 6 | Dashboard + reports + CSV export | Report tests green |
| 7 | Backup/restore (UI + CLI + scheduler + retention + secondary copy) | Round-trip test green |
| 8 | Packaging (win-x64, macos-arm64[, x64]), installers, service setup, docs, E2E | Clean install on fresh Win + mac |

Each phase ends with: tests passing, app running, short summary, and your approval before the next phase.

---

## 16. Assumptions & Open Questions

Please answer, or reply "accept defaults" (optionally "except Qn: …").

| # | Question | Default if not answered |
|---|---|---|
| Q1 | Is the clinic GST-registered? Should prices be tax-inclusive (MRP)? | Yes, inclusive (D2); GST toggle available |
| Q2 | Discount cap for receptionists? | 10% of bill; above needs admin (D3) |
| Q3 | Round bill total to nearest ₹1? | Yes (D4) |
| Q4 | Batch/expiry tracking required for all products, including consumables? | Required for medicines; optional expiry for consumables (D5) |
| Q5 | Do you sell loose tablets from strips (unit conversion)? | No, v1 sells base unit only (D8) |
| Q6 | Credit/pending payments (patient pays later)? | Not in v1 (D13) |
| Q7 | Who may cancel bills? Receptionist return window? | Admin only; receptionist returns ≤ 7 days (D14/D15) |
| Q8 | Invoice format & FY reset? | `INV/26-27/000001`, resets April 1 (D9) |
| Q9 | Store diagnoses / prescriptions / doctor name? | No (D16) — adding later is non-breaking |
| Q10 | Printer: A4/A5 laser, or 80 mm/58 mm thermal? | Both A5 and 80 mm templates (D23) |
| Q11 | Encrypt backup files with a password? (Adds recovery risk if password is lost.) | No app-level encryption; rely on BitLocker/FileVault + ACLs; revisit if backups go to USB/cloud |
| Q12 | Will other PCs/tablets in the clinic need access over LAN? | No — localhost only. (LAN mode would require HTTPS, firewall rules and stronger session controls.) |
| Q13 | How many staff accounts; is one PC shared by several people? | 1–5 named accounts, shared PC, auto-logout after 60 min idle |
| Q14 | Mac hardware: Apple Silicon only, or Intel too? Minimum macOS version? | Apple Silicon only; minimum macOS = whatever Node 24 officially supports (to be verified, believed 13.5+) |
| Q15 | Budget for code-signing certs (Apple Developer ID ~$99/yr; Windows Authenticode)? | Unsigned builds with documented first-launch steps |
| Q16 | macOS: single user account on the Mac? | Yes → per-user LaunchAgent |
| Q17 | Off-machine backup target available (USB / NAS / cloud-synced folder)? | Configurable secondary folder, off by default, strongly recommended |
| Q18 | Clinic name/branding for demo data & receipt (e.g. "Skin Doc Clinic")? | Placeholder "Demo Clinic" |
| Q19 | Is there an existing product/client list (Excel/CSV) to import? | CSV import for products & clients in Phase 2/3 (small scope) |
| Q20 | Git repository / hosting (GitHub?) for CI? | GitHub + Actions; repo initialised locally in Phase 1 |

---

## 17. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Single PC hardware/disk failure | Data loss | Daily verified backups + off-machine copy (Q17); restore runbook |
| Power loss mid-transaction | Corruption | WAL + `synchronous=FULL`; atomic transactions; integrity checks |
| Unsigned binaries blocked by Gatekeeper/SmartScreen | Install friction | Signing (Q15) or documented steps |
| Native module / Node runtime mismatch per OS | App won't start | Per-OS/arch packages built & tested in CI |
| Staff share one login | No accountability | Named accounts, idle logout, audit trail |
| Stock ledger drift due to future code bugs | Wrong stock | DB constraints, append-only ledger, daily invariant check |
| Scope creep (prescriptions, appointments, accounting) | Delay/complexity | Explicitly out of v1; schema leaves room to extend |
