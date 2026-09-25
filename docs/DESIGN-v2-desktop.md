# ClinicApp — Cross-Platform Desktop Design (v2.0, for approval)

> **Status: DRAFT — awaiting approval. No application code has been written.**
> Supersedes `docs/DESIGN.md` (v0.2, browser + local-server design), which was written against the earlier brief. The new brief requires a native desktop app with `.exe`/`.dmg` installers, no terminal and no separately installed runtime, so the architecture changes.
> Date: 2026-09-25 · Targets: Windows 10/11 x64, macOS (Apple Silicon + Intel)

**Tags used throughout**

| Tag | Meaning |
|---|---|
| **[REQ]** | Confirmed requirement from the brief |
| **[ASSUMPTION]** | My assumption — please confirm or correct |
| **[REC]** | My recommendation (a design decision) |
| **[FUTURE]** | Optional future feature — designed for, **not** implemented in v1 |

---

## 0. The one decision that shapes everything

The brief prefers **Tauri** and suggests **Prisma**. These two don't fit together well:

- Tauri's backend is **Rust**. Its frontend is a system WebView (WebView2 on Windows, WKWebView on macOS). There is no Node.js runtime inside a Tauri app.
- Prisma is a **Node.js** library. To use it inside Tauri you would have to ship a bundled Node.js "sidecar" process plus Prisma's engine binaries for each OS. That brings back most of the size and complexity Tauri is meant to avoid. It also adds a second process that must be code-signed, notarized and kept alive on macOS, and a local socket/HTTP channel between the UI and the sidecar that must itself be secured.
- The obvious shortcut — Tauri's `plugin-sql` called directly from the React UI — is **not acceptable here**. It gives the UI raw SQL access, and its connection pool does not guarantee that `BEGIN … COMMIT` run on the same connection. That breaks the atomic-billing requirement (§13 of the brief).

So there are two coherent options:

| | **Option A — Tauri 2 + Rust core** *(recommended)* | **Option B — Electron + TypeScript core** |
|---|---|---|
| UI | React + TypeScript + MUI | React + TypeScript + MUI |
| Business logic & DB | Rust (`clinic-core` crate), SQLite via `rusqlite` (bundled SQLite), SQL migrations | TypeScript in Electron main process, `better-sqlite3` + Drizzle (or Prisma) |
| Installer size | ~10–20 MB | ~90–150 MB |
| Memory at runtime | ~80–150 MB | ~200–350 MB |
| Printing / PDF | System WebView print dialog on each OS; PDF generated in Rust (identical on both OSs) | Chromium on both OSs → `webContents.print()` / `printToPDF()` identical everywhere |
| Rendering consistency | WebView2 (Chromium) vs WKWebView (Safari engine) — small CSS differences possible | Identical (bundled Chromium) |
| Languages to maintain | **Two** (TypeScript + Rust) | **One** (TypeScript) |
| Attack surface | Small; commands are whitelisted per window via Tauri capabilities | Larger; needs careful hardening (contextIsolation, sandbox, no nodeIntegration, IPC validation) |
| Future local clinic server | Same `clinic-core` crate behind an `axum` HTTP server | Same TS core behind Fastify |
| Prisma | Not used (justified above) | Possible, but `better-sqlite3` is simpler and has synchronous transactions + an online backup API |

**[REC] Option A — Tauri 2 with a Rust core.** Reasons:
- It meets the brief's stated preference.
- There is no significant technical limitation: printing, PDF, autostart, single-instance, file dialogs and installers are all supported on both OSs.
- It gives the strongest data-integrity story. The bundled SQLite is the same version on both OSs, transactions are synchronous with one writer connection, the online backup API is available, and strict typing crosses the UI boundary via generated bindings.
- It has the smallest attack surface.

**The real cost is Rust.** Whoever maintains this app long-term must be comfortable with it.

- **If nobody on your side will maintain Rust, choose Option B.** It is a perfectly sound design for this app.
- **Q1 below asks you to decide.** The rest of this document assumes Option A. Everything except §2, §8 and §13 carries over almost unchanged to Option B.

---

## 1. Recommended Architecture

```
┌──────────────────────────── ClinicApp (one desktop process) ────────────────────────────┐
│                                                                                          │
│  ┌─────────────── UI layer: React + TypeScript + MUI (inside system WebView) ─────────┐  │
│  │ pages → feature components → hooks (TanStack Query) → ClinicApi interface          │  │
│  │ • no business rules, no money arithmetic, no SQL                                   │  │
│  │ • ClinicApi has one implementation today: TauriTransport (IPC)                     │  │
│  └───────────────────────────────┬────────────────────────────────────────────────────┘  │
│                                  │ typed IPC (tauri-specta generated bindings)           │
│  ┌───────────────────────────────▼────────────────────────────────────────────────────┐  │
│  │ Application shell: src-tauri (Rust)                                                │  │
│  │ • commands = thin adapters: authenticate → authorize → validate → call service     │  │
│  │ • session state, idle lock, error mapping (domain error → user message + log ref)  │  │
│  │ • platform services: paths, printing, file dialogs, open-folder, autostart         │  │
│  └───────────────────────────────┬────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────▼────────────────────────────────────────────────────┐  │
│  │ clinic-core (pure Rust library, no Tauri/UI dependency)                            │  │
│  │ domain types · pricing/GST engine · FEFO allocator · services · repository traits  │  │
│  └───────────────────────────────┬────────────────────────────────────────────────────┘  │
│  ┌───────────────────────────────▼────────────────────────────────────────────────────┐  │
│  │ clinic-sqlite: repository implementations, migrations, backup/restore, integrity   │  │
│  │ 1 writer connection (serialised) + small read pool · WAL · synchronous=FULL · FKs  │  │
│  └───────────────────────────────┬────────────────────────────────────────────────────┘  │
└──────────────────────────────────┼───────────────────────────────────────────────────────┘
                                   ▼
               <app-local-data>/data/clinic.db      <app-local-data>/backups/*.clinicbak
```

### Key properties

- **One process, no servers, no open ports** [REC]:
  - Nothing listens on the network, so there is no CSRF or port-hijack surface.
  - There are no background services.
  - The app is fully offline [REQ §3].
- **Single-instance lock** [REC] (`tauri-plugin-single-instance`). A second launch focuses the existing window instead of opening a second writer on the database.
- **Layering** [REQ §2]:
  - UI → shell commands → services → repositories → SQLite.
  - Services depend on repository **traits**, never on SQLite directly.

### Future multi-computer path [FUTURE — not implemented]

```
v1:   React ─IPC→ Tauri shell → clinic-core → SqliteRepos → clinic.db

v2:   React ─IPC→ Tauri shell → HttpBackend ──LAN/TLS──→ clinic-server (axum)
                                                          → clinic-core → SqliteRepos (or Postgres)
```
Moving to v2 means adding:
- a `clinic-server` binary that reuses `clinic-core` unchanged;
- an `HttpBackend` implementation of the same command set.

UI code does not change, because it only knows the `ClinicApi` interface. The v1 decisions that make this cheap:
- Entities have stable public IDs (ULID `public_id`) alongside integer keys.
- The UI never does calculations.
- All writes go through services.

---

## 2. Why Tauri 2 is appropriate

| Need | How Tauri 2 meets it |
|---|---|
| Native installers, no runtime install [REQ §28] | Built-in bundler: **NSIS `.exe`** (Windows) and **`.dmg`** (macOS). Single native binary; the web engine is the OS WebView. |
| Windows WebView | WebView2 ships with Windows 11 and is on current Windows 10 via Edge (WebView2 153.x is present on this dev PC). The installer includes the **offline WebView2 installer** as a fallback, so installing works without internet [REC]. |
| macOS Intel + Apple Silicon | **Universal binary** (`universal-apple-darwin`) — one `.dmg` for both [REC]. |
| Double-click to open, login screen [REQ §29] | Standard app; Start-menu/desktop shortcut on Windows, Applications/Dock on macOS. |
| Autostart option [REQ §29] | `tauri-plugin-autostart`: Windows registry Run key, macOS LaunchAgent / login item. |
| OS data directories [REQ §30] | Tauri path resolver: `app_local_data_dir`, `app_log_dir`, `document_dir`. |
| Printing on both OSs [REQ §21] | WebView print dialog (the OS-native dialog on each platform). Needs a Phase-1 spike (§13). |
| PDF | Generated in Rust — identical bytes on both OSs, no WebView dependency. |
| Security | Capabilities/permissions whitelist exactly which commands each window may call; strict CSP; no Node in the renderer. |
| Size/overhead | ~10–20 MB installer vs ~100+ MB for Electron. |

Limitations I've identified, and how they are handled:
- **WebView rendering differs between OSs.**
  - Receipts for print use simple, well-supported CSS and are tested on both OSs.
  - PDF output is not WebView-rendered at all.
- **No WebDriver for WKWebView on macOS.**
  - Automated end-to-end UI tests run on Windows.
  - On macOS, UI is covered by component tests plus a scripted manual test checklist (§14).
- **Silent (no-dialog) printing to a thermal printer is not uniform across OSs.**
  - v1 uses the normal print dialog; the app can remember the last printer where the OS allows it.
  - Silent printing is [FUTURE].

---

## 3. Technology Stack

| Layer | Choice | Justification |
|---|---|---|
| Desktop shell | **Tauri 2** | §2 |
| UI | **React 19 + TypeScript (strict)**, **Vite** | [REQ]; fast builds |
| Component library | **MUI (Material UI) v7** + **MUI X DataGrid (MIT version)** + **MUI X Date Pickers** | [REQ] suggested; mature, accessible, keyboard-friendly; DataGrid handles server-side pagination/sort/filter |
| UI data fetching | **TanStack Query** | Caching/invalidation, loading/error states |
| Forms & validation (UI side) | **react-hook-form + Zod** | Fast forms and immediate inline messages. The server-side (Rust) validation is authoritative. |
| Routing | **React Router** (hash/memory routing) | No URLs exposed outside the app; client data never in URLs [REQ §25] |
| IPC typing | **tauri-specta** | Generates TypeScript types and command bindings from Rust — one source of truth, compile-time checked on both sides |
| Core language | **Rust (stable)** | Tauri backend language; strong typing, no runtime GC pauses |
| Database | **SQLite (bundled via `rusqlite` feature `bundled`)** | Same SQLite version on both OSs; FTS5 for fast search (to verify enabled in bundled build during Phase 1) |
| DB layer | **`rusqlite`** + hand-written, parameterised SQL in repository modules; **`rusqlite_migration`** for versioned migrations | Mature; synchronous transactions (`TransactionBehavior::Immediate`), online backup API, `integrity_check`. Prisma is not used (§0). An ORM adds little for ~20 tables. |
| Password hashing | **`argon2`** crate (Argon2id) | Modern, OWASP-recommended |
| Money | **`i64` minor units (paise)** + integer basis-point rates | No floating point anywhere in money paths [REQ §12] |
| IDs | Integer PK + **ULID** `public_id` | Stable IDs for future sync/server mode |
| PDF | **`printpdf`** (or `genpdf` on top of it) with embedded **Noto Sans** font (has ₹ glyph) | Deterministic, offline, identical output on both OSs |
| Logging | **`tracing` + `tracing-appender`** (daily rolling, JSON) | Structured logs; retention cleanup |
| Secure storage | **`keyring`** crate → Windows Credential Manager / macOS Keychain | Used only if DB encryption is enabled (Q12) |
| Tauri plugins | `single-instance`, `autostart`, `dialog` (save/open file), `opener` (open folder), `window-state`, optional `updater` [FUTURE] | Each one adds a needed capability; nothing else is included |
| Tests | Rust `cargo test` (+ `proptest` for pricing), **Vitest + React Testing Library**, **WebdriverIO + tauri-driver** (Windows E2E) | §14 |
| CI/CD | **GitHub Actions** (`windows-latest`, `macos-latest`) + **tauri-action** | Builds, tests, signs and packages on both OSs |

**Dev machine prerequisites** (developers only, never the clinic PC):
- Rust toolchain.
- Windows: MSVC Build Tools. macOS: Xcode Command Line Tools.
- Node 24 + npm.

*This PC currently has Node 24 and git, but **not** Rust or MSVC Build Tools. Phase 1 will list the exact install steps.*

---

## 4. Component Diagram

```
UI (React)                         Shell (src-tauri)                 clinic-core                         clinic-sqlite
─────────────                      ─────────────────                 ───────────                         ─────────────
LoginPage ───────────┐             auth_cmds ─────────────┐          AuthService ──────┐                 UserRepoSqlite
DashboardPage ───────┤             dashboard_cmds ────────┤          DashboardService ─┤                 ProductRepoSqlite
ClientsPages ────────┤  ClinicApi  client_cmds ───────────┤          ClientService ────┤  Repository     BatchRepoSqlite
BillingPages ────────┼──(IPC)────▶ billing_cmds ──────────┼────────▶ BillingService ───┼──traits───────▶ LedgerRepoSqlite
InventoryPages ──────┤             inventory_cmds ────────┤          InventoryService ─┤  (UnitOfWork)   BillRepoSqlite
ReportsPages ────────┤             report_cmds ───────────┤          ReportService ────┤                 ClientRepoSqlite
SettingsPages ───────┘             settings/backup_cmds ──┤          BackupService ────┤                 AuditRepoSqlite
                                   system_cmds ───────────┘          AuditService ─────┘                 SettingsRepoSqlite
                                   │                                 PricingEngine (pure)                Migrations
                                   ├─ SessionManager (idle lock)      FefoAllocator (pure)               BackupEngine (online API)
                                   ├─ Authorizer (permission matrix)  InvoiceNumbering                   IntegrityChecker
                                   ├─ ErrorMapper → user message      Clock trait (testable time)
                                   └─ PlatformServices: Paths, Printer, PdfRenderer, FolderOpener, Autostart
```

---

## 5. Business Rules & Defaults

### 5.1 Confirmed requirements (from the brief)
- **Batches and FEFO**
  - A product has multiple batches.
  - Sales use FEFO.
  - Expired stock is never sold.
  - There are near-expiry warnings.
- **Inventory ledger**
  - Every stock movement creates an immutable ledger entry.
  - Ledger types: `INITIAL_STOCK`, `PURCHASE`, `SALE`, `RETURN`, `ADJUSTMENT`, `DAMAGE`, `EXPIRY`, `CANCELLATION`.
- **Finalizing a bill**
  - Finalize is atomic.
  - Duplicate protection covers double-click, retry, restart and error cases.
- **Cancelled bills**
  - A cancelled bill is kept.
  - Its stock is restored.
  - Who, why and when are all recorded.
- **Returns and roles**
  - Returns are linked to the original bill and batch.
  - There are two roles, ADMIN and RECEPTIONIST, with the capabilities in §19 of the brief.
- **Money and data handling**
  - Money calculation is decimal-safe.
  - No telemetry.
  - Client data stays out of logs, URLs and localStorage.

### 5.2 Assumptions & recommendations (please confirm)

| # | Topic | Default | Tag |
|---|---|---|---|
| D1 | Currency | INR (₹), 2 decimals, stored as paise (`i64`). Currency symbol configurable. | ASSUMPTION |
| D2 | GST mode | Selling price is **MRP inclusive of GST** (Indian retail norm). Tax is back-calculated per line: `tax = round(net × r / (10000 + r))`, r in basis points. A Setting switches to **exclusive** mode, and a switch disables GST entirely for unregistered clinics. | ASSUMPTION |
| D3 | GST rates | Per product: 0 / 5 / 12 / 18 / 28 % (editable list in Settings). Receipt shows a CGST/SGST split (half each) when GST is enabled. **IGST, e-invoicing and GSTR reports are not in v1.** | ASSUMPTION / FUTURE |
| D4 | Rounding | Half-up at paise per line; bill total rounded to nearest ₹1 with an explicit "Round off" line (configurable). | REC |
| D5 | Line discount | % or ₹ per line. | REC |
| D6 | Bill discount | % or ₹ on the bill, **allocated pro-rata to lines** (largest-remainder method, so paise always add up). This keeps GST and return refunds correct. | REC |
| D7 | Discount limits | Receptionist ≤ 10 % of bill (configurable). Above that, an ADMIN enters their password in an approval dialog; the approver is recorded. | ASSUMPTION |
| D8 | Price source | Selling price comes from the **batch** (MRP differs by batch). The product holds a default that pre-fills new batches. Receptionists cannot override price. Admins may, with the override audited. | REC |
| D9 | FEFO | Allocate from the non-expired batch with the earliest expiry. Batches without expiry (consumables) come last, oldest received first. One bill line can span several batches, and the receipt shows each batch. A receptionist may pick a batch manually but cannot pick an expired one. | REC |
| D10 | Expiry | Expiry means end of the stated day; month-only expiry ("12/2026") is stored as the month's last day. Batches with expiry **today or earlier** are unsellable. Billing warns when the allocated batch expires within **30 days** (configurable). Alert buckets: expired / 30 / 60 / 90 days. Admin writes off expired stock via an `EXPIRY` entry. | REC |
| D11 | Negative stock | Impossible: `CHECK (quantity >= 0)` in the database. An admin adjustment sets a new count ≥ 0, with a mandatory reason. | REQ / REC |
| D12 | Units | Each product is sold in a single base unit (strip, tube, bottle, piece). **No strip→tablet conversion in v1.** | ASSUMPTION (Q5) |
| D13 | Payment methods | Cash, UPI, Card, Other. **Split payment allowed** (e.g. cash + UPI). For cash: amount received → change due. The sum of payments must equal the total. **No credit/dues in v1.** | ASSUMPTION (Q6) |
| D14 | Invoice numbering | `{PREFIX}/{FY}/{SEQ:6}`, e.g. `INV/26-27/000123`. Gapless, resets every Indian financial year (1 April). Allocated **inside** the finalize transaction. Cancelled bills keep their number. Returns use `RET/26-27/000001`, credit notes `CN/…` [FUTURE]. | ASSUMPTION (Q8) |
| D15 | Bill drafts | A bill in progress is saved to the **database** (`bill_draft`), not localStorage, so it survives crash/restart without putting client data in the WebView. The draft holds the idempotency key. | REC |
| D16 | Walk-in | A client on a bill is **optional** ("Walk-in"). Phone numbers are not unique (families share); a possible duplicate triggers a warning, not a block. | ASSUMPTION |
| D17 | Cancellation | **ADMIN only**, mandatory reason. Status becomes `CANCELLED`, stock returns to the **exact batches** sold via `CANCELLATION` ledger entries, and an audit entry is written. A bill with any return cannot be cancelled; the remaining items are returned instead. No time limit for admins. | REC |
| D18 | Returns | Per bill line. Qty ≤ sold − already returned. Refund = original net line amount (after all discounts) pro-rata. Each returned item is either **Restock** (to the original batch, only if not expired) or **Do not restock** (`RETURN` + `DAMAGE` entries). Receptionist: within **7 days** of the bill; admin: any time. Refund method is recorded. | ASSUMPTION (Q7) |
| D19 | Users | Named accounts, no shared logins. First run: an **Initial Setup** screen creates the first ADMIN (no default password exists). 1–10 users expected. | ASSUMPTION |
| D20 | Sessions | Login per app launch. Screen **locks after 15 min idle** (password to unlock; the draft bill is kept). Switch-user is available. | REC |
| D21 | Receipt size | Two templates: **A5 portrait** (normal printer) and **80 mm thermal**, chosen in Settings. PDF is always A5. 58 mm is [FUTURE]. | ASSUMPTION (Q10) |
| D22 | Backups | Automatic **daily** (option: weekly). Runs while the app is open at the scheduled time, **or at app start if one was missed**, **and on app exit** if the last backup is older than 12 h. Keep 14 daily + 8 weekly + 12 monthly (configurable). | REC |
| D23 | Prescriptions / diagnoses | **Not stored in v1.** Optional short free-text note on the bill (e.g. "per Dr. S."). The schema leaves room for a future `prescription` module. | ASSUMPTION (Q9) / FUTURE |
| D24 | Suppliers | Simple supplier master (name, phone, email, GSTIN, address) linked to batches/purchases. | REC |
| D25 | Purchases | Lightweight **Stock In (Goods Receipt)**: supplier, supplier invoice no/date, lines (product, batch, expiry, qty, cost, MRP). Creates batches + `PURCHASE` entries. **No purchase orders or supplier payables in v1.** | REC / FUTURE |
| D26 | Barcode | **Not in v1.** An optional `barcode` column on product (indexed) is included. A USB scanner acts as a keyboard, so it will work in the product search box with no extra code. Barcode label printing is [FUTURE]. | REC / FUTURE |
| D27 | Timezone | Stored as UTC ISO-8601. "Today", reports, FY and receipts use the clinic timezone (default `Asia/Kolkata`). | ASSUMPTION |
| D28 | Data location | Per OS user account (the account the clinic uses on that PC). | ASSUMPTION (Q14) |

### 5.3 Permission matrix (enforced in the Rust shell on every command)

| Permission | ADMIN | RECEPTIONIST |
|---|---|---|
| View dashboard | ✓ | ✓ |
| Client create / edit / search / view history | ✓ | ✓ |
| Create bill, print / PDF receipt | ✓ | ✓ |
| Discount above cap | ✓ | admin approval |
| Return items | ✓ | ✓ (≤ 7 days) |
| Cancel bill | ✓ | ✗ |
| View products & available stock | ✓ | ✓ (purchase price hidden) |
| Create/edit/deactivate products, categories, suppliers | ✓ | ✗ |
| Stock In, adjustment, damage, expiry write-off | ✓ | ✗ |
| View stock transactions ledger | ✓ | ✗ |
| Reports | ✓ | Own "today" summary only |
| Users, settings, backup/restore, audit log, open data folders | ✓ | ✗ |

Roles are stored in a `role` table (seeded, not editable in v1). Permissions are a compile-time matrix in code, which keeps it simple and makes custom roles possible later [FUTURE].

---

## 6. Database ERD

```
role ──< user ──< audit_log
          │
          ├──< (created_by / approved_by / cancelled_by on bill, sales_return, stock_*)
          │
category ──< product >── supplier (default_supplier_id, nullable)
               │
               └──< inventory_batch >── supplier
                        │
                        ├──< inventory_transaction >── (bill | sales_return | stock_receipt | stock_adjustment)
                        │
supplier ──< stock_receipt ──< stock_receipt_item >── inventory_batch
            stock_adjustment ──< stock_adjustment_item >── inventory_batch

client ──< bill ──< bill_item >── product
            │         └──< bill_item_batch >── inventory_batch
            ├──< payment
            └──< sales_return ──< return_item >── bill_item
                                     └──────────> inventory_batch

bill_draft (1 per open bill, holds idempotency_key)      app_setting (key/value)
number_sequence (series, fy)      backup_record      schema_migrations (managed by migrator)
```

Cascade rules [REC]:
- **`ON DELETE RESTRICT` everywhere** on financial and ledger tables.
- Nothing financial is ever deleted.
- `ON DELETE CASCADE` only for `bill_draft → bill_draft_item`.

---

## 7. Detailed Database Schema

Conventions:
- **Keys:** every table has `id INTEGER PRIMARY KEY` and a `public_id TEXT UNIQUE` (ULID) where the entity is user-visible.
- **Types:** timestamps are `TEXT` ISO-8601 UTC; money is `INTEGER` paise (`*_paise`); rates are `INTEGER` basis points (`*_bp`, 18 % = 1800); booleans are `INTEGER` 0/1 with a `CHECK`.
- **Tables:** declared `STRICT`, so SQLite enforces column types.

### 7.1 Identity & audit

**role** — `code TEXT UNIQUE CHECK (code IN ('ADMIN','RECEPTIONIST'))`, `name`.

**user**
| column | type / constraint |
|---|---|
| username | TEXT NOT NULL UNIQUE COLLATE NOCASE |
| full_name | TEXT NOT NULL |
| password_hash | TEXT NOT NULL (Argon2id PHC string) |
| role_id | INTEGER NOT NULL → role(id) RESTRICT |
| is_active | INTEGER NOT NULL DEFAULT 1 |
| must_change_password | INTEGER NOT NULL DEFAULT 0 |
| failed_login_count / locked_until | INTEGER / TEXT — lockout 5 attempts → 5 min |
| last_login_at, created_at, updated_at | TEXT |

**audit_log** (append-only — `BEFORE UPDATE/DELETE` triggers `RAISE(ABORT)`)
| column | notes |
|---|---|
| occurred_at, user_id → user | |
| action | e.g. `LOGIN`, `LOGIN_FAILED`, `LOGOUT`, `SCREEN_UNLOCK`, `USER_CREATE`, `USER_UPDATE`, `PASSWORD_RESET`, `PRODUCT_CREATE/UPDATE/DEACTIVATE`, `PRICE_OVERRIDE`, `STOCK_RECEIPT`, `STOCK_ADJUST`, `BILL_FINALIZE`, `BILL_CANCEL`, `DISCOUNT_APPROVE`, `RETURN_CREATE`, `SETTINGS_UPDATE`, `BACKUP_CREATE`, `BACKUP_FAILED`, `RESTORE`, `INTEGRITY_CHECK`, `REPORT_EXPORT` |
| entity_type, entity_id | e.g. `bill`, 123 |
| details_json | IDs, before/after values of non-sensitive fields — **never** client name/phone/address |
| Index | (occurred_at), (entity_type, entity_id), (user_id) |

### 7.2 Catalogue & inventory

**category** — `name UNIQUE COLLATE NOCASE`, `is_active`.

**supplier** — `name NOT NULL`, `phone`, `email`, `gstin`, `address`, `notes`, `is_active`. Index (name).

**product**
| column | type / constraint |
|---|---|
| sku | TEXT NOT NULL UNIQUE COLLATE NOCASE (auto `P-000001` if blank) |
| barcode | TEXT UNIQUE NULL (optional, D26) |
| name | TEXT NOT NULL · index `(name COLLATE NOCASE)` |
| generic_name | TEXT NULL · index |
| category_id | → category RESTRICT |
| product_type | TEXT CHECK IN (`MEDICINE`,`TABLET`,`CAPSULE`,`CREAM`,`OINTMENT`,`GEL`,`MEDICAL_SUPPLY`,`CONSUMABLE`,`OTHER`) |
| manufacturer | TEXT · index |
| unit | TEXT NOT NULL (strip, tube, bottle, piece, box) |
| requires_expiry | INTEGER NOT NULL DEFAULT 1 |
| default_selling_price_paise, default_purchase_price_paise | INTEGER ≥ 0 |
| gst_rate_bp | INTEGER CHECK (gst_rate_bp BETWEEN 0 AND 2800) |
| hsn_code | TEXT NULL |
| min_stock | INTEGER ≥ 0 (default from settings) |
| default_supplier_id | → supplier NULL |
| is_active, notes, created_at, updated_at | |

**product_search** — FTS5 virtual table (name, generic_name, sku, manufacturer, barcode), kept in sync by triggers → instant prefix search over 10k+ products.

**inventory_batch**
| column | type / constraint |
|---|---|
| product_id | → product RESTRICT |
| batch_no | TEXT NOT NULL · `UNIQUE(product_id, batch_no)` · index (batch_no) |
| expiry_date | TEXT NULL (`YYYY-MM-DD`); `CHECK` NOT NULL when product requires expiry (enforced in service + trigger) · index (expiry_date) |
| supplier_id | → supplier NULL |
| purchase_date | TEXT NULL |
| purchase_price_paise, selling_price_paise | INTEGER ≥ 0 |
| quantity | INTEGER NOT NULL **CHECK (quantity >= 0)** |
| is_active | INTEGER |
| Index | (product_id, expiry_date) for FEFO; partial index `WHERE quantity > 0` |

**inventory_transaction** — the immutable ledger (triggers block UPDATE/DELETE)
| column | type / constraint |
|---|---|
| occurred_at | TEXT NOT NULL |
| product_id, batch_id | → product / inventory_batch RESTRICT (product denormalised for fast reporting) |
| type | CHECK IN (`INITIAL_STOCK`,`PURCHASE`,`SALE`,`RETURN`,`ADJUSTMENT`,`DAMAGE`,`EXPIRY`,`CANCELLATION`) |
| qty_change | INTEGER NOT NULL, ≠ 0 (signed) |
| previous_qty, new_qty | INTEGER NOT NULL · `CHECK (new_qty = previous_qty + qty_change AND new_qty >= 0)` |
| reason | TEXT — NOT NULL for `ADJUSTMENT`,`DAMAGE`,`EXPIRY`,`CANCELLATION` |
| bill_id, sales_return_id, stock_receipt_id, stock_adjustment_id | nullable FKs |
| user_id | → user NOT NULL |
| Index | (batch_id, occurred_at), (product_id, occurred_at), (bill_id), (type, occurred_at) |

**stock_receipt** (Stock In header) — `receipt_no UNIQUE`, `supplier_id`, `supplier_invoice_no`, `supplier_invoice_date`, `notes`, `user_id`, `created_at`.
**stock_receipt_item** — `stock_receipt_id`, `batch_id`, `qty > 0`, `unit_cost_paise`.

**stock_adjustment** (header) — `adjustment_no UNIQUE`, `kind CHECK IN ('ADJUSTMENT','DAMAGE','EXPIRY')`, `reason NOT NULL`, `user_id`, `created_at`.
**stock_adjustment_item** — `stock_adjustment_id`, `batch_id`, `counted_qty` / `qty_change`.
*(Satisfies the brief's `StockAdjustment` entity. Quantities live only in the ledger, and the header groups a multi-line stock count under one reason.)*

### 7.3 Clients

**client**
| column | type / constraint |
|---|---|
| client_code | TEXT UNIQUE (`CL-000001`) |
| full_name | TEXT NOT NULL · index `(full_name COLLATE NOCASE)` |
| phone | TEXT NULL · index (normalised digits) |
| email, date_of_birth, gender (`MALE`,`FEMALE`,`OTHER`,`UNDISCLOSED`), address | optional |
| emergency_contact_name, emergency_contact_phone | optional |
| notes | TEXT (UI warns: "do not record diagnoses here") |
| last_visit_at | TEXT, index |
| is_active, created_at, updated_at | |

**client_search** — FTS5 (full_name, phone, client_code) for fast search across 50k+ clients.

### 7.4 Billing

**bill_draft** — `idempotency_key TEXT UNIQUE`, `client_id NULL`, `created_by`, `payload_json` (lines/discount/payments — IDs & quantities only), `updated_at`. Finalizing deletes the draft **in the same transaction**.

**bill**
| column | type / constraint |
|---|---|
| bill_no | TEXT NOT NULL UNIQUE |
| idempotency_key | TEXT NOT NULL **UNIQUE** |
| client_id | → client NULL (walk-in) · index (client_id, finalized_at) |
| status | CHECK IN (`FINALIZED`,`CANCELLED`) |
| pricing_mode | CHECK IN (`INCLUSIVE`,`EXCLUSIVE`) — snapshot |
| gross_paise, line_discount_paise, bill_discount_paise, taxable_paise, tax_paise, round_off_paise, total_paise | INTEGER |
| bill_discount_kind / bill_discount_value | `PERCENT_BP`/`AMOUNT` + value |
| amount_received_paise, change_paise | INTEGER |
| returned_total_paise | INTEGER DEFAULT 0 (maintained by returns) |
| note | TEXT NULL |
| finalized_at | TEXT NOT NULL · index |
| created_by, discount_approved_by | → user |
| cancelled_at, cancelled_by, cancel_reason | NULL unless cancelled; `CHECK` all-or-none |
| clinic_snapshot_json | clinic name/address/GSTIN at time of sale (reprints stay historically correct) |

**bill_item** — `bill_id`, `line_no`, `product_id`, **snapshots** (`product_name`, `sku`, `unit`, `hsn_code`), `unit_price_paise`, `qty > 0`, `line_discount_paise`, `allocated_bill_discount_paise`, `gst_rate_bp`, `taxable_paise`, `tax_paise`, `line_total_paise`, `returned_qty CHECK (returned_qty BETWEEN 0 AND qty)`. Index (product_id) for product-sales report.

**bill_item_batch** — `bill_item_id`, `batch_id`, `qty > 0`, `batch_no`/`expiry_date` snapshot.

**payment** — `bill_id`, `method CHECK IN ('CASH','UPI','CARD','OTHER')`, `amount_paise > 0`, `reference NULL`, `direction CHECK IN ('IN','REFUND')`, `sales_return_id NULL`.

**sales_return** — `return_no UNIQUE`, `bill_id`, `reason NOT NULL`, `refund_total_paise`, `refund_method`, `user_id`, `created_at`.
**return_item** — `sales_return_id`, `bill_item_id`, `batch_id`, `qty > 0`, `refund_paise`, `restock INTEGER`, `condition_note`.

### 7.5 System

**app_setting** — `key TEXT PRIMARY KEY`, `value_json`, `updated_by`, `updated_at`. Keys:
- clinic name, address, phone, email, GSTIN;
- pricing/GST settings;
- receipt size and footer;
- currency;
- invoice prefix;
- low-stock default;
- expiry warning days;
- discount cap;
- idle-lock minutes;
- backup schedule, destination and retention;
- autostart.

**number_sequence** — `series` (`BILL`,`RETURN`,`RECEIPT`,`ADJUSTMENT`,`CLIENT`,`PRODUCT`), `fy`, `next_value`; `PRIMARY KEY (series, fy)`.

**backup_record** — `file_name`, `path`, `kind` (`MANUAL`,`AUTO`,`PRE_RESTORE`,`PRE_MIGRATION`), `size_bytes`, `sha256`, `schema_version`, `app_version`, `verified`, `created_by`, `created_at`.

### 7.6 SQLite configuration [REC]

```
PRAGMA journal_mode = WAL;        -- crash-safe, readers don't block the writer
PRAGMA synchronous  = FULL;       -- survives power loss (clinic PCs lose power); cost negligible at this volume
PRAGMA foreign_keys = ON;
PRAGMA busy_timeout = 5000;
PRAGMA trusted_schema = OFF;
```
- A single writer connection behind a mutex serialises all writes, so there is no `SQLITE_BUSY` during billing.
- 2–4 read-only connections serve searches and reports.
- `PRAGMA quick_check` runs at startup and a full `integrity_check` weekly and after restore.

### 7.7 Performance targets [REQ §35]

The brief's sizing is 10k products, 50k clients, 100k bills and 500k bill items.

- **Search:**
  - Products and clients use FTS5 prefix search, debounced 150 ms, top 20 results.
  - All lists use keyset (cursor) pagination, 50 rows per page.
- **Reports:**
  - Reports aggregate in SQL, using indexes on `finalized_at` and `product_id`.
  - Target < 1 s for a month's range.
  - Nothing loads the full dataset into the UI.
- **Verification:** a synthetic dataset of this size is generated in Phase 10 to measure against these targets.

---

## 8. Application Folder Structure

```
clinic-app/
├─ package.json                 # UI workspace + scripts (dev, build, test, tauri)
├─ Cargo.toml                   # Rust workspace
├─ .github/workflows/           # ci.yml (lint/test both OSs) · release.yml (sign + package)
├─ src/                         # ── React UI ──
│  ├─ app/                      # AppShell, router, theme, providers, idle-lock, error boundary
│  ├─ api/
│  │  ├─ bindings.ts            # GENERATED by tauri-specta (do not edit)
│  │  ├─ clinicApi.ts           # ClinicApi interface (transport-agnostic)
│  │  └─ tauriTransport.ts      # v1 implementation
│  ├─ features/
│  │  ├─ auth/  dashboard/  clients/  billing/  inventory/  reports/  settings/
│  │  │   └─ each: pages/, components/, hooks/, *.test.tsx
│  ├─ components/               # shared: SearchBox, MoneyText, ConfirmDialog, EmptyState, DataTable…
│  └─ print/                    # ReceiptA5.tsx, ReceiptThermal80.tsx, print.css
├─ src-tauri/                   # ── Desktop shell ──
│  ├─ tauri.conf.json           # bundle config (NSIS, DMG), CSP, identifier
│  ├─ capabilities/             # per-window permission whitelist
│  ├─ icons/
│  └─ src/
│     ├─ main.rs  lib.rs        # builder, plugins, state wiring, single-instance
│     ├─ commands/              # auth.rs, clients.rs, billing.rs, inventory.rs, reports.rs, settings.rs, backup.rs, system.rs
│     ├─ session.rs  authz.rs  error_map.rs
│     └─ platform/              # paths.rs, printing.rs, folders.rs, autostart.rs
├─ crates/
│  ├─ clinic-core/              # ── domain + services (no Tauri, no SQLite) ──
│  │  └─ src/ domain/, pricing/, fefo/, numbering/, services/, repos (traits), errors.rs, clock.rs
│  ├─ clinic-sqlite/            # ── persistence ──
│  │  ├─ migrations/            # V001__init.sql, V002__… (versioned, forward-only)
│  │  └─ src/ db.rs, repos/, backup.rs, integrity.rs, seed.rs
│  └─ clinic-pdf/               # receipt PDF rendering + embedded font
├─ tests/
│  ├─ e2e/                      # WebdriverIO + tauri-driver (Windows)
│  └─ manual/                   # cross-platform release checklist (Win10, Win11, macOS ARM, macOS Intel)
├─ tools/                       # dataset generator (perf), seed-demo
└─ docs/
   ├─ architecture.md  database.md  development.md  testing.md  packaging.md
   ├─ user-guide.md  backup-restore.md  troubleshooting.md  recovery.md
   ├─ deploy-windows.md  deploy-macos.md
   └─ CHANGELOG.md
```

### Runtime data locations (resolved via the Tauri path API; nothing hard-coded) [REQ §30]

| | Windows | macOS |
|---|---|---|
| App identifier | `in.clinicapp.desktop` (placeholder) | same |
| Database | `%LOCALAPPDATA%\in.clinicapp.desktop\data\clinic.db` | `~/Library/Application Support/in.clinicapp.desktop/data/clinic.db` |
| Default backups | `%LOCALAPPDATA%\in.clinicapp.desktop\backups\` | `~/Library/Application Support/in.clinicapp.desktop/backups/` |
| Logs | `%LOCALAPPDATA%\in.clinicapp.desktop\logs\` | `~/Library/Logs/in.clinicapp.desktop/` |
| Installed program | `%LOCALAPPDATA%\Programs\ClinicApp\` (per-user NSIS) or Program Files (per-machine) | `/Applications/ClinicApp.app` |

- **Local, not Roaming, AppData** [REC]. A roaming profile would sync the live database file, which corrupts it.
- **Backups are not placed in Documents or Desktop by default** [REC]. On many PCs these folders are silently synced to OneDrive (Windows) or iCloud (macOS), which would upload patient data to the cloud. An *additional* backup destination (USB drive, NAS or a folder of your choice) is set in Settings.
- **Settings → System** has buttons to **Open database folder / Open backups folder / Open logs folder** (admin only).

---

## 9. Main Screens / Wireframes

Target laptop resolution **1366×768** and up. The left navigation collapses to icons. Light theme, 16 px base font, large 44 px primary buttons, and right-aligned tabular numerals for money.

### Global keyboard shortcuts

| Key | Action |
|---|---|
| `F1` | New Bill |
| `F2` | Product search |
| `F3` | Client search |
| `F9` | Finalize |
| `Ctrl/⌘+P` | Print last receipt |
| `Esc` | Close dialog |
| `Ctrl/⌘+K` | Global search |

### 9.1 Login / first-run
```
┌──────────────────────────────────────────┐
│             🩺  ClinicApp                 │
│   Username  [____________________]        │
│   Password  [____________________]        │
│             [        Sign in       ]      │
│   v1.0.0 · Offline · Database OK          │
└──────────────────────────────────────────┘
```
On first run the screen is **Initial Setup** instead: clinic name, then create an administrator account (username, name, password ×2), then optionally load demo data.

### 9.2 Dashboard
```
┌─☰ ClinicApp ───────────────── 🔍 Search clients, products, bills (Ctrl+K) ──── Priya · Reception ▾ ┐
│ Dashboard  │ ┌─────────────┐┌─────────────┐┌─────────────┐┌─────────────┐┌─────────────┐┌─────────┐│
│ Clients    │ │Today's sales││Bills today  ││Clients today││Low stock  ⚠ ││Out of stock ││Expiring ││
│ Billing    │ │ ₹12,450.00  ││     34      ││     29      ││      7      ││      2      ││ ≤30d: 5 ││
│ Inventory  │ └─────────────┘└─────────────┘└─────────────┘└─────────────┘└─────────────┘└─────────┘│
│ Reports    │  [ ＋ New Bill  F1 ]   [ ＋ New Client ]                Active products: 214        │
│ Settings   │                                                                                     │
│            │  Recent bills                              │ Recent clients                         │
│            │  10:42 INV/26-27/000123 John Doe   ₹150.00 │ John Doe      98•••••234   Today       │
│            │  10:31 INV/26-27/000122 Walk-in     ₹80.00 │ Meera Shah    97•••••811   Today       │
│            │  …                                   View all › │ …                          View all › │
│            │  ✓ Last backup: today 13:00 (verified)                                             │
└────────────┴────────────────────────────────────────────────────────────────────────────────────┘
```
Phone numbers are masked in lists; the full number shows on the profile [REC: privacy].

### 9.3 New Bill (primary workflow)
```
┌ New Bill ──────────────────────────────────────────────────── Draft saved ✓   [Discard draft] ┐
│ Client (F3) [🔍 name / phone / ID                 ▾] ✕   John Doe · CL-000045 · last 20-Sep   │
│             [Walk-in]  [＋ New client]                                                        │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ Product (F2) [🔍 name, generic, SKU, barcode…          ]   Qty [  1 ]   [ Add ↵ ]             │
│   ▸ Paracetamol 500mg   strip   ₹20.00   In stock 97   next exp 12/2026                       │
│     Paracetamol 650mg   strip   ₹30.00   In stock 12   next exp 03/2027                       │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ # Product              Batch(es)        Qty   Price    Disc   GST  Line total                 │
│ 1 Paracetamol 500mg    A23 (exp 12/26)  [ 2]  ₹20.00   –      12%  ₹40.00      ✕              │
│ 2 Pain Relief Cream    C19 (exp 08/27)  [ 1]  ₹120.00  –      18%  ₹120.00     ✕              │
│   ⚠ Batch A23 expires in 27 days                                                              │
├──────────────────────────────────────────────┬────────────────────────────────────────────────┤
│ Bill discount [ 10.00 ] [₹ | %]              │ Subtotal                          ₹160.00      │
│ Note [                              ]        │ Discount                         −₹10.00      │
│                                              │ GST (included)                     ₹17.19      │
│ Payment  [ Cash ] [ UPI ] [ Card ] [ Split ] │ Round off                           ₹0.00      │
│ Received [ 200.00 ]   Change ₹50.00          │ TOTAL                             ₹150.00      │
│                                              │                                                │
│                                              │     [  Finalize & Print   F9  ]               │
└──────────────────────────────────────────────┴────────────────────────────────────────────────┘
```
- **Validation before finalize:** entering a qty above available stock shows *"Insufficient stock. Available quantity: 2"* inline, and the stock is re-checked inside the transaction.
- **Duplicate protection:** Finalize shows a spinner and is disabled until the result comes back.
- **After finalize:** a dialog shows **Bill INV/26-27/000123 saved** → **[Print] [Save PDF] [New bill (F1)]**, with Print focused, so the whole thing is `F9 → Enter`.
- **Target:** a two-item cash bill for an existing client takes about 8 interactions.

### 9.4 Receipt (A5 / 80 mm share content; PDF = A5)
```
                 {CLINIC NAME}
         {Address line} · Ph {phone}
               GSTIN {gstin}
------------------------------------------------
Bill No : INV/26-27/000123
Date    : 25-Sep-2026 10:42
Client  : John Doe (CL-000045)
------------------------------------------------
Item                  Qty   Rate      Amount
Paracetamol 500mg      2    20.00      40.00
  Batch A23  Exp 12/26
Pain Relief Cream      1   120.00     120.00
  Batch C19  Exp 08/27
------------------------------------------------
Subtotal                               160.00
Discount                               -10.00
GST incl. (CGST 8.60 + SGST 8.59)       17.19
TOTAL                            ₹     150.00
Cash received 200.00      Change 50.00
------------------------------------------------
Billed by: Priya
{Receipt footer}              Thank you
```
Cancelled bills print with a **CANCELLED** watermark and the cancellation date/reason.

### 9.5 Client profile & history
```
┌ John Doe · CL-000045 ────────────────────────────────── [Edit]  [＋ New bill for this client] ┐
│ 📞 98765 43234 · ✉ john@example.com · DOB 12-Mar-1985 (41) · Male                             │
│ Client since 02-Jan-2026 · Last visit 25-Sep-2026 · 6 visits · Total ₹1,840.00                │
│ [ Visits ] [ Products received ] [ Details ]          From [01-Apr-2026] To [25-Sep-2026] 🔍  │
├───────────────────────────────────────────────────────────────────────────────────────────────┤
│ 25-Sep-2026  INV/26-27/000123   ₹150.00   Finalized                                   Open ›  │
│              Paracetamol 500mg × 2 · Pain Relief Cream × 1                                    │
│ 20-Sep-2026  INV/26-27/000119   ₹230.00   Partly returned (−₹45.00)                   Open ›  │
│              Amoxicillin 500mg × 1 · Antiseptic Cream × 2  (returned 1)                       │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 9.6 Products & stock
```
┌ Products ──────────────────────────────────────────────── [＋ Add product]  [＋ Stock in] ┐
│ 🔍 name / generic / SKU / batch    Category [All ▾]  Manufacturer [All ▾]  Show [All|Low|Out|Inactive] │
│ SKU      Name               Type    Category   In stock  Min  Next expiry  Price   Status    │
│ P-000001 Paracetamol 500mg  Tablet  Analgesic       97   20   12/2026      ₹20.00  Active ⋯  │
│ P-000014 Bandage 5 cm       Supply  Dressing     4 ⚠    10   —            ₹35.00  Active ⋯  │
│ P-000021 Antiseptic Cream   Cream   Topical      0 ⛔    5   —            ₹90.00  Active ⋯  │
│ ⋯ = View batches · Stock history · Adjust stock · Edit · Deactivate          ‹ 1 2 3 … ›     │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```
Product detail → **Batches** tab (batch, expiry, supplier, qty, MRP), **History** tab (ledger: date, type, ±qty, before→after, reason, bill link, user).

### 9.7 Backup & Restore (admin)
```
┌ Backup & Restore ──────────────────────────────────────────────────────────────────────────┐
│ Automatic backup  (●) Daily at [13:00]  (○) Weekly on [Mon ▾]     Keep: 14 daily · 8 weekly · 12 monthly │
│ Extra copy to     [ E:\ClinicBackups                ] [Browse…]   ✓ last copy OK          │
│ Last backup       25-Sep-2026 13:00 · verified ✓           [ Back up now ]  [ Open folder ]│
├────────────────────────────────────────────────────────────────────────────────────────────┤
│ Date / time         Type          Size     Verified                                        │
│ 25-Sep-2026 13:00   Automatic     4.1 MB   ✓        [ Restore… ]                           │
│ 24-Sep-2026 18:05   Manual        4.1 MB   ✓        [ Restore… ]                           │
│ [ Restore from file… ]   [ Run integrity check ]                                           │
└────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 10. Core Business Workflows

| Workflow | Steps |
|---|---|
| **Add product** (admin) | Products → Add → name/type/category/unit/GST/min stock/default prices → Save → optional "Add opening stock" (batch, expiry, qty) → `INITIAL_STOCK` entry |
| **Stock In** (admin) | Stock In → supplier + invoice no → add lines (product search, batch, expiry, qty, cost, MRP) → Save → batches created/updated + `PURCHASE` entries, one transaction |
| **Adjust stock** (admin) | Product → Batches → Adjust → counted qty + kind (Adjustment/Damage/Expiry) + reason → confirm → ledger entry |
| **New client** | Clients → New (or inside New Bill) → name + phone (other fields optional) → Save |
| **Create bill** | §9.3; server-side flow §11.1 |
| **Print / PDF** | After finalize, or from Bill History → Print (OS dialog) / Save PDF (OS save dialog) |
| **Cancel bill** (admin) | Bill History → bill → Cancel → reason → confirm dialog showing stock to be restored → §11.2 |
| **Return** | Bill History → bill → Return items → per line qty + restock? + reason → refund method → confirm → §11.3 → return slip print |
| **Reports** | Reports → pick report → period preset/custom → table + totals → Print / Export CSV (admin, audited) |
| **Backup / restore** | §12 |

---

## 11. Inventory / Billing Transaction Strategy

All multi-step writes run as **one `BEGIN IMMEDIATE` transaction on the single writer connection**. This takes the write lock up front, so nothing can interleave. Services receive a `UnitOfWork`. Any `Err` → the transaction is dropped → automatic **ROLLBACK**.

### 11.1 Finalize bill
```
BEGIN IMMEDIATE
 1  SELECT bill WHERE idempotency_key = :key
      → exists: COMMIT, return that bill (no new bill)         ← double-click / retry / restart
 2  Load draft lines; validate client active (if any), products active
 3  For each line: FEFO-allocate from non-expired active batches
      → shortfall: Err(InsufficientStock{product, available})  → ROLLBACK
 4  PricingEngine computes everything (integers only; UI totals ignored)
 5  Validate discount cap / approval; Σ payments == total; received ≥ cash due
 6  bill_no ← number_sequence('BILL', fy)  (UPDATE … RETURNING)
 7  INSERT bill, bill_item(s), bill_item_batch(es), payment(s)
 8  For each allocation:
      UPDATE inventory_batch SET quantity = quantity - :n
       WHERE id = :batch AND quantity >= :n RETURNING quantity   ← must return 1 row
      INSERT inventory_transaction(SALE, -n, prev, new, bill_id, user)
 9  UPDATE client.last_visit_at ; DELETE bill_draft ; INSERT audit_log(BILL_FINALIZE)
COMMIT
```

What this guarantees:
- **Bill and stock change together or not at all.** Either all of steps 6–9 are committed, or none are. There is never a bill without its stock deduction, or a deduction without its bill.
- **Crash after COMMIT** (before the UI hears back): on restart the draft is gone and the key maps to a finalized bill. The UI shows "This bill was already completed: INV/…".
- **Crash before COMMIT**: SQLite's WAL recovery discards the partial transaction. The draft still exists, so the receptionist simply presses Finalize again.
- **The UI cannot block duplicates on its own.** The in-progress state disables Finalize, but correctness rests on the `UNIQUE(idempotency_key)` database constraint.

### 11.2 Cancel bill
```
BEGIN IMMEDIATE
  bill must be FINALIZED and have no returns; user must be ADMIN; reason required
  UPDATE bill SET status='CANCELLED', cancelled_at, cancelled_by, cancel_reason
  for each bill_item_batch: batch.quantity += qty ; INSERT inventory_transaction(CANCELLATION, +qty, …, bill_id)
  INSERT payment(direction='REFUND', …) per original payment method ; audit_log(BILL_CANCEL)
COMMIT
```

### 11.3 Return
```
BEGIN IMMEDIATE
  per line: qty ≤ bill_item.qty − returned_qty ; receptionist ≤ 7 days
  refund = pro-rata of line_total (last unit takes remainder so totals match exactly)
  INSERT sales_return, return_item(s) ; UPDATE bill_item.returned_qty, bill.returned_total
  restock=true & batch not expired → batch += qty ; ledger RETURN(+qty)
  restock=false                    → ledger RETURN(+qty) then DAMAGE(−qty)  (net 0, fully explained)
  INSERT payment(REFUND) ; audit_log(RETURN_CREATE)
COMMIT
```

### 11.4 Stock In / Adjustment
One transaction per document. The ledger entry and the batch quantity update are always written together by a single repository function (`LedgerRepo::apply_movement`). **It is the only code path allowed to change `inventory_batch.quantity`** [REC].

### 11.5 Ledger integrity check
A weekly job, plus a button in Settings, verifies for every batch:

`quantity == SUM(qty_change)` over the ledger, **and** each entry's `previous_qty` equals the prior entry's `new_qty`.

- A mismatch raises an admin alert on the dashboard.
- It is **never auto-corrected** [REQ §9: no silent stock modifications].

---

## 12. Security & Privacy

| Area | Approach |
|---|---|
| Authentication | Named users; Argon2id (m=19 MiB, t=2, p=1 — OWASP minimum, tuned in Phase 1); min 8-char password; lockout after 5 failures for 5 min; admin reset forces change at next login. |
| Sessions | Session lives in Rust memory only (no token in WebView storage). Idle lock after 15 min; app close ends the session. |
| Authorization | Every command runs `require(permission)` against the matrix (§5.3) — deny by default. The UI hides what a role can't do, but the UI is never trusted. |
| IPC surface | Tauri **capabilities** whitelist only our commands. No `shell`, no broad `fs` plugin exposed to the WebView; file access only via Rust with OS save/open dialogs. |
| Input validation | Zod in the UI for fast feedback; **authoritative validation in Rust** (types, lengths, ranges, enums) before any service call. |
| SQL injection | Only parameterised statements (`params![]`); dynamic `ORDER BY` from an enum whitelist; CI lint check. |
| XSS | React escaping; `dangerouslySetInnerHTML` banned (lint rule); strict CSP in `tauri.conf.json` (`default-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost`); no remote content loaded. |
| CSRF | **Not applicable** — no HTTP server or cookies. Documented; revisit in future server mode (then: TLS + CSRF tokens). |
| Secrets | None needed in v1. No secrets in source; `.env*` git-ignored. Signing keys live only in CI secrets. If DB encryption is enabled (Q12), the key is stored in **Windows Credential Manager / macOS Keychain** via `keyring`, with a printed recovery key. |
| Data at rest | Recommend **BitLocker** (Windows) / **FileVault** (macOS). App data dir is per-user (OS ACLs). Optional SQLCipher (Q12). |
| Backups | Stored in the app data dir by default (not cloud-synced folders); warn if the chosen destination is inside OneDrive/iCloud; restore requires admin + password re-entry; optional encrypted backups (Q12). |
| Privacy / minimisation | Only name is required on a client; all else optional. No diagnoses in v1. No client data in logs, errors, URLs (in-app routing uses IDs), localStorage, or window titles. Phones masked in lists. **No telemetry, analytics or crash reporting.** CSV exports admin-only and audited. |
| Logging | `tracing` JSON logs; PII-carrying types implement a redacting `Debug`; SQL parameter values never logged; each user-facing error carries a short reference ID matching the log line. Daily rotation, 30 files kept. |
| Audit | Append-only `audit_log` (DB triggers block edits/deletes) for all sensitive actions (§7.1). Admin viewer with filters. |
| Supply chain | Lockfiles committed; `cargo audit` + `cargo deny` + `npm audit` in CI; Dependabot; minimal dependency set; signed release artifacts. |
| Code signing | Windows Authenticode + macOS Developer ID + notarization (Q15). |

### Error handling
Each domain error variant maps to a code and a friendly message. Examples:

| Code | Message shown |
|---|---|
| `INSUFFICIENT_STOCK` | "Insufficient stock. Available quantity: 2" |
| `PRODUCT_INACTIVE` | "Unable to complete the operation. The selected product is no longer available." |
| `BATCH_EXPIRED` | "This batch has expired and cannot be sold." |
| `DISCOUNT_APPROVAL_REQUIRED` | "Discounts above 10% need administrator approval." |
| `RETURN_QTY_EXCEEDED` | "Only 3 units can still be returned." |
| `PERMISSION_DENIED` | "You don't have permission to do this." |
| `DATABASE_BUSY` | "The system is busy, please try again." |
| `INTERNAL` | "Something went wrong. Reference: E-7F3K2 — please tell the administrator." |

SQLite error codes are never shown to users. Database errors are never swallowed; each is logged with context and returned as `INTERNAL`.

---

## 13. Backup & Recovery Strategy

### 13.1 Backup format
A `.clinicbak` file is a zip containing:
- `clinic.db` — a consistent snapshot taken with SQLite's **online backup API** while the app runs;
- `manifest.json` — app version, schema version, created_at, sha256 of `clinic.db`, row counts of key tables.

The SQLite file format is the same on Windows and macOS, so **a backup made on either OS restores on the other**.

### 13.2 Creating a backup
The steps, in order:
1. Take the snapshot to a temp file.
2. Run `PRAGMA integrity_check` on the snapshot.
3. Compute the checksum.
4. Zip it.
5. Write with an atomic rename.
6. Record it in `backup_record`.
7. Copy it to the extra destination, if one is configured.
8. Apply retention pruning — this **never deletes** the most recent 3 backups or any `PRE_RESTORE`/`PRE_MIGRATION` backup less than 90 days old.

A failed backup:
- is logged;
- is written to the audit log;
- shows a dashboard banner "Last backup failed — click to retry";
- **never blocks billing**.

### 13.3 When backups run
- **Manual:** Settings → Backup → [Back up now] or [Back up to…] (OS save dialog, any destination such as a USB drive).
- **Automatic:** daily or weekly (D22):
  - at the scheduled time while the app is open;
  - on start if a scheduled backup was missed;
  - on exit if the last backup is older than 12 h.
- **Before every migration** (app upgrade) and **before every restore**.

### 13.4 Restore (admin)
1. Select a backup from the list, or "Restore from file…".
2. **Validate:**
   - the zip opens;
   - the manifest checksum matches;
   - `integrity_check` passes;
   - the schema version is ≤ the current version (older backups are migrated forward after restore);
   - key row counts are shown for a sanity check.
3. **Confirm:** a dialog shows the backup date and counts. The admin must type `RESTORE` and re-enter their password.
4. **Automatic `PRE_RESTORE` backup** of the current database is taken.
5. The app enters maintenance mode, and the writer and readers close.
6. The swap:
   - move `clinic.db`, `-wal` and `-shm` aside;
   - copy in the restored db;
   - fsync;
   - atomic rename.
7. Reopen, run migrations, then run a full `integrity_check`.
   - On failure: roll back automatically to the pre-restore copy and show an error.
8. Audit, then restart the app.

### 13.5 Recovery (documented in `docs/recovery.md`)
- **Crash or power loss:** WAL recovery runs automatically at next start; `quick_check` runs at startup.
- **Startup detects corruption:** the app opens in **Recovery mode** (admin only):
  - it offers to restore the latest verified backup;
  - it moves the damaged file aside and keeps it, never deleting it.
- **App won't start at all:** manual procedure — copy a `.clinicbak`, unzip it, and replace `clinic.db` at the documented path per OS.
- **Lost admin password:** a documented reset procedure. The recovery reset code shown at install is used to create a new admin. [REC — Q16]

---

## 14. Windows / macOS Packaging Strategy

| | Windows | macOS |
|---|---|---|
| Installer | **NSIS `.exe`** (Tauri bundler); per-user install by default (no admin rights needed) — per-machine optional | **`.dmg`** with drag-to-Applications; universal binary (Apple Silicon + Intel) |
| Runtime deps | None to install; WebView2 is part of Windows 11/10 — installer embeds **offline WebView2 installer** fallback | None (WKWebView is part of macOS) |
| Minimum OS | Windows 10 (1809+) and 11, x64 | **macOS 12 Monterey+** [ASSUMPTION — confirm in Phase 1 spike; Tauri 2 supports older but we test what we support] |
| Signing | Authenticode code-signing certificate (OV, or Azure Trusted Signing) — avoids SmartScreen warnings | Apple Developer ID (≈ $99/yr) + **notarization** + stapling — avoids Gatekeeper blocking |
| Shortcuts | Start menu + optional desktop shortcut | Applications / Dock |
| Autostart | Setting toggles `tauri-plugin-autostart` (registry Run key) | Setting toggles login item/LaunchAgent |
| Upgrade | Run newer installer; app data untouched; on first start: **pre-migration backup → migrations → integrity check** | Replace app in Applications; same first-start sequence |
| Rollback | Reinstall previous version + restore the `PRE_MIGRATION` backup (documented) | same |
| Uninstall | Apps & Features. **Data is kept** unless user ticks "remove data" (with warning) | Drag to Bin; data remains in Application Support (documented removal steps) |
| Updates | v1: manual installer from you. `tauri-plugin-updater` (signed updates, needs hosting) [FUTURE] | same |
| Build | GitHub Actions `windows-latest` + `tauri-action` | GitHub Actions `macos-latest` + `tauri-action` (can build universal) |

**Important practical constraint:** macOS apps can only be built, signed and notarised on macOS. This dev PC is Windows, so macOS builds come from **GitHub Actions macOS runners** (or a Mac you provide). Real printing and installation tests on macOS need a physical Mac (Q17).

**Phase-1 technical spike** — de-risk before building features. Each of these is verified on both OSs:
1. WebView print dialog on Windows (WebView2) and macOS (WKWebView) — A5 and 80 mm layouts.
2. PDF with ₹ glyph and batch lines.
3. SQLite bundled build with FTS5 + online backup.
4. NSIS and DMG installers launch offline.
5. Autostart toggle.
6. Single-instance.
7. App-data paths.

If WKWebView printing proves unreliable, the fallback is to print the **generated PDF** via the OS (still cross-platform).

---

## 15. Testing Strategy

| Layer | Tooling | What it covers |
|---|---|---|
| Pricing engine (pure) | `cargo test` + **`proptest`** | Line totals, inclusive/exclusive GST, line + bill discount allocation (Σ allocations == discount, always), rounding/round-off, received/change, split payments — thousands of generated cases. |
| FEFO allocator (pure) | `cargo test` | Earliest-expiry first; skips expired; no-expiry last; multi-batch split; exact-fit; shortfall reports available qty. |
| Numbering | `cargo test` | FY boundary (31-Mar → 1-Apr), gapless under rollback (failed finalize does not consume a number). |
| Services + SQLite (integration) | `cargo test` against a temp-file DB per test (same pragmas as production) | **Inventory:** create product, stock in, adjustment, damage, expiry write-off, low-stock/out-of-stock queries, expiry buckets. **Billing:** finalize, stock deduction per batch, ledger rows correct, insufficient stock → no side effects, expired batch blocked. **Atomicity:** fault injection (a test repo that fails after bill insert / after first batch update / before commit) → assert no bill, no ledger rows, batch qty unchanged, sequence unchanged. **Idempotency:** same key twice → one bill; key re-used after restart → returns existing. **Concurrency:** two threads sell the last unit → exactly one succeeds. **Cancellation:** status, exact-batch restoration, `CANCELLATION` ledger rows, refunds; cancel after return rejected. **Returns:** partial/multiple returns, over-return rejected, restock vs damage, pro-rata refunds sum correctly. **Clients:** create, update, search (FTS), history with date filter. **Auth/authz:** lockout, every command × role matrix. **Backup/restore:** round-trip, corrupted/mismatched-checksum backup rejected, pre-restore backup created, older-schema backup migrated. **Ledger invariant:** holds after randomized operation sequences. |
| UI components | Vitest + React Testing Library (mocked `ClinicApi`) | Billing screen keyboard flow, validation messages, Finalize disabled while pending, empty/loading/error states. |
| E2E | WebdriverIO + `tauri-driver` (Windows CI) | Login → new bill → finalize → receipt dialog → stock reduced; double-press Finalize → one bill; restore flow. |
| Cross-platform | CI runs all Rust + UI tests on **Windows and macOS**; release checklist (`tests/manual/`) executed on **Win 10, Win 11, macOS Apple Silicon, macOS Intel** | Install, first run, login, DB creation, inventory, billing, **real printer print**, PDF, backup, restore across OSs (backup made on Windows restored on Mac and vice-versa), paths/open-folder, autostart, upgrade from previous version, uninstall, shutdown during billing. |
| Recovery | Scripted | Kill the process mid-finalize (repeated 100×) → DB consistent, invariant holds; corrupt DB file → Recovery mode offered. |
| Performance | `tools/` dataset generator (10k products, 50k clients, 100k bills, 500k items) | Product/client search < 100 ms, finalize < 150 ms, monthly sales report < 1 s, app start < 3 s. |
| Security | `cargo audit`, `cargo deny`, `npm audit`, ESLint security rules, Clippy | Known vulnerabilities, banned patterns (raw SQL string building, `dangerouslySetInnerHTML`). |

---

## 16. Deployment Architecture

```
Developer ──git push──▶ GitHub repo ──▶ GitHub Actions
                                          ├─ ci.yml:      lint · typecheck · cargo test · vitest · audit   (windows-latest, macos-latest)
                                          └─ release.yml (on tag vX.Y.Z):
                                               windows-latest → build → sign (Authenticode) → ClinicApp_X.Y.Z_x64-setup.exe
                                               macos-latest   → build universal → sign → notarize → ClinicApp_X.Y.Z_universal.dmg
                                               → GitHub Release (checksums + release notes)

Clinic PC (Windows or Mac) ◀── download installer (USB or browser) ── install ── double-click ── Login
    └─ all data stays on this machine; extra backup copy to USB/NAS
```
Versioning: SemVer; `CHANGELOG.md`; the schema version is tracked by migrations and shown in Settings → System along with the app version, data paths and last integrity check.

---

## 17. Implementation Phases (as per brief, with checkpoints)

| Phase | Scope | Exit criteria |
|---|---|---|
| 0 | **Spike** (≤ 2 days): items in §14 | Go/no-go on Tauri printing on macOS |
| 1 | Workspace, Tauri shell, React + MUI shell, SQLite + migrations, auth, roles, sessions, idle lock, audit, settings, first-run setup, logging, CI on both OSs | App builds & runs on Win + mac; tests green |
| 2 | Categories, suppliers, products, batches, Stock In, adjustments, ledger, low-stock, expiry | Inventory tests green |
| 3 | Clients CRUD, search, profile, history | Client tests green |
| 4 | Billing: drafts, product/client search, pricing engine, FEFO, finalize transaction, payments, bill history | Billing/atomicity/idempotency tests green |
| 5 | Receipts: A5 + 80 mm templates, print, PDF | Printed on real printers on both OSs |
| 6 | Dashboard + reports + CSV export | Report tests green |
| 7 | Returns & cancellation | Reversal tests green |
| 8 | Backup/restore/integrity/recovery mode | Round-trip + cross-OS restore green |
| 9 | Packaging, signing, installers | Clean install on Win 10/11 + macOS ARM/Intel |
| 10 | Final QA: E2E, recovery, performance, docs | All checklists passed |

I will stop for your review at the end of each phase.

---

## 18. Assumptions & Questions Requiring Clarification

Reply with answers, or "**accept defaults**" plus any exceptions (e.g. "accept defaults except Q1: Option B, Q10: 58 mm").

| # | Question | Default if accepted |
|---|---|---|
| **Q1** | **Tauri + Rust core (Option A) or Electron + TypeScript core (Option B)?** Who will maintain the code long-term, and are they comfortable with Rust? | Option A |
| Q2 | Is the clinic GST-registered? Are prices tax-inclusive (MRP)? | Yes, inclusive (D2) |
| Q3 | Receptionist discount cap? | 10 %, admin approval above (D7) |
| Q4 | Round bill total to nearest ₹1? | Yes (D4) |
| Q5 | Do you sell loose tablets from strips (unit conversion)? | No (D12) |
| Q6 | Credit/pay-later for patients? | No (D13) |
| Q7 | Return window for receptionists; can admins cancel bills from previous days/months? | 7 days; admin anytime (D17/D18) |
| Q8 | Invoice number format / FY reset (April)? | `INV/26-27/000123` (D14) |
| Q9 | Store prescriptions, diagnoses, or treating-doctor name? | No; free-text bill note only (D23) |
| Q10 | Printer(s): normal A4/A5 printer, 80 mm or 58 mm thermal? Model if known. | A5 + 80 mm (D21) |
| Q11 | Backup: daily or weekly; is a USB drive/NAS available for off-machine copies? | Daily; extra destination strongly recommended (D22) |
| Q12 | Encrypt the database/backups inside the app (SQLCipher + OS keychain)? Trade-off: stronger privacy if the PC/USB is stolen, but **data is unrecoverable if both the keychain and the recovery key are lost**. | No app-level encryption; require BitLocker/FileVault; revisit before Phase 8 |
| Q13 | Number of staff users; will several people share one PC? | 1–10 users, shared PC, idle lock 15 min |
| Q14 | Will the clinic PC use one OS user account for the app? | Yes — data stored under that account (D28) |
| Q15 | Budget for code signing (Apple Developer ID ≈ $99/yr; Windows cert ≈ $100–400/yr or Azure Trusted Signing)? Without it, staff see security warnings on install. | Unsigned builds for pilot, signed before production |
| Q16 | Admin password recovery: acceptable to show a one-time **recovery code** at setup that must be kept safely? | Yes |
| Q17 | Do you have a Mac (Apple Silicon and/or Intel) and the clinic's real printer for testing? | CI builds; you provide real-device testing |
| Q18 | Minimum macOS version to support? | macOS 12+ |
| Q19 | Clinic name/logo, app name and bundle identifier (e.g. `in.skindoc.clinicapp`)? | "ClinicApp", `in.clinicapp.desktop` (placeholders) |
| Q20 | Existing product/client data (Excel/CSV) to import? | CSV import for products & clients (small scope, Phases 2–3) |
| Q21 | Supplier management depth — just names on stock-in, or also supplier purchase history report? | Master + Stock In history (D24/D25) |
| Q22 | Barcode scanner in the near term? | Barcode column + keyboard-wedge scanner support only (D26) |
| Q23 | Source hosting for CI/releases — GitHub? Private repo? | Private GitHub repo + Actions |

### Optional future features (designed for, not built)
- Multi-computer / clinic server mode.
- Cloud backup.
- Purchase orders and supplier payables.
- Accounting and GST returns (GSTR).
- Prescriptions and diagnoses.
- Barcode label printing.
- Multiple branches.
- Silent thermal printing.
- Signed auto-updates.
- Custom roles.
- SMS/WhatsApp receipts.

### Risks

| Risk | Mitigation |
|---|---|
| Rust skill gap for future maintenance | Q1; Option B available |
| macOS WebView printing quirks | Phase-0 spike; PDF-print fallback |
| Unsigned builds blocked or warned by OS | Q15 |
| Single PC = single point of failure | Verified daily backups + off-machine copy; recovery mode; documented restore |
| No Mac for real-device testing | Q17 — CI covers build/test, but printing/installer QA needs hardware |
| Scope creep | Future list above is explicitly out of v1 |
