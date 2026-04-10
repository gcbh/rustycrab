# Dependencies & Build Configuration Review

## CRITICAL

### DEP-C1: Duplicate `reqwest` Versions (0.12 and 0.13)
- **File:** `Cargo.toml:27`, `Cargo.lock`
- **Description:** Workspace declares `reqwest = "0.12"` but `chromiumoxide 0.9.1` depends on `reqwest 0.13.2`. Two copies compiled into the binary. Increased compile times and binary size.
- **Fix:** Upgrade workspace reqwest to 0.13 to unify.

### DEP-C2: Duplicate `rand` Versions (0.8 and 0.9)
- **File:** `Cargo.toml:61`, `Cargo.lock`
- **Description:** Workspace pins `rand = "0.8"`, but transitive dependencies pull in `rand 0.9`. Creates duplicate `rand_core` versions and potential type incompatibility. All call sites use `thread_rng()` which is the 0.8 API (removed in 0.9).
- **Fix:** Audit transitive deps and either upgrade to 0.9 or pin to prevent duplication.

### DEP-C3: `chromiumoxide` `default-features = false` Without Runtime Feature
- **File:** `Cargo.toml:72`
- **Description:** Disabling default features risks losing WebSocket support if a future patch makes the runtime feature optional. Intent should be explicit.
- **Fix:** Explicitly specify `features = ["tokio-runtime"]` if available.

## HIGH

### DEP-H1: `serde_yaml` is Deprecated/Unmaintained
- **File:** `Cargo.toml:33`, `Cargo.lock: 0.9.34+deprecated`
- **Description:** Lock file explicitly marks as deprecated. Author recommends `serde_yml`.
- **Fix:** Migrate to `serde_yml` or another maintained YAML library.

### DEP-H2: `sled` 0.34 is Effectively Unmaintained
- **File:** `Cargo.toml:46`
- **Description:** No release since 2022. sled 1.0 will be a complete rewrite. Used as the persistence layer for both `rustykrab-store` and `rustykrab-memory`. Data corruption bugs will never be fixed.
- **Fix:** Consider migrating to `redb`, `sqlite` (via `rusqlite`), or another maintained embedded DB.

### DEP-H3: `rustykrab-memory` Crate is Orphaned
- **File:** All `crates/*/Cargo.toml` files
- **Description:** Listed as workspace member but no other crate depends on it. `rustykrab-tools` has its own `memory_backend.rs` trait, `rustykrab-store` has its own `memory.rs`. Appears to be dead code or incomplete migration.
- **Fix:** Integrate or remove.

### DEP-H4: No Release Profile Optimizations
- **File:** `Cargo.toml`
- **Description:** No `[profile.release]` section. Missing: `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"`.
- **Fix:** Add appropriate release profile settings.

### DEP-H5: No CI/CD Configuration
- **Description:** No `.github/workflows/`, CircleCI, or other CI config. For a project with crypto, browser automation, and shell execution, automated `cargo clippy` / `cargo audit` is essential.
- **Fix:** Add GitHub Actions workflow.

## MEDIUM

### DEP-M1: Minimal `.gitignore`
- **File:** `.gitignore`
- **Description:** Only contains `/target`. Missing `.env`, `*.pem`, `*.key`, `.idea/`, `.vscode/`, `.DS_Store`.
- **Fix:** Add standard entries.

### DEP-M2: Blocking IMAP Crate in Async Context
- **File:** `crates/rustykrab-tools/src/gmail.rs`
- **Description:** `imap` v2 uses `std::net::TcpStream`. Should use `spawn_blocking` everywhere (one call site missed).

### DEP-M3: Dual TLS Implementations
- **File:** `Cargo.toml:27, 76-77`
- **Description:** `reqwest` uses `rustls-tls`; `imap`/`lettre` use `native-tls`. Binary links both OpenSSL and rustls.
- **Fix:** Standardize on one TLS implementation.

### DEP-M4: `rustykrab-providers` Missing Direct Tokio Dependency
- **File:** `crates/rustykrab-providers/Cargo.toml`
- **Description:** Uses reqwest (needs tokio) but no direct tokio dependency. Works via transitive dependency only.

### DEP-M5: No `rust-toolchain.toml` File
- **Description:** No pinned Rust compiler version. Different developers may get different behavior.
- **Fix:** Add `rust-toolchain.toml` with a pinned stable channel.

### DEP-M6: No MSRV (`rust-version`) Field
- **File:** `Cargo.toml:16-20`
- **Description:** No minimum supported Rust version declared.

## LOW

### DEP-L1: `tokio` `features = ["full"]` Overly Broad
- **File:** `Cargo.toml:24`
- **Description:** Enables all features. Specifying only needed features reduces compile time.

### DEP-L2: `security-framework` OSX_10_15 Feature
- **File:** `Cargo.toml:68`
- **Description:** macOS 10.15 Catalina no longer supported by Apple.

### DEP-L3: `codesign.sh` Only Signs CLI Binary
- **File:** `scripts/codesign.sh`
- **Description:** Only handles `rustykrab-cli`. Other binaries would need script updates.

### DEP-L4: No Workspace Dev-Dependencies
- **File:** `Cargo.toml`
- **Description:** `tempfile = "3"` declared independently in multiple crates instead of at workspace level.
