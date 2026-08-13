---
name: rust-security
description: Guides secure Rust coding practices and security reviews. Use when writing,
  reviewing, or auditing Rust code for vulnerabilities including unsafe misuse, FFI
  boundary bugs, integer overflow, panic-based DoS, path traversal, injection, deserialization
  attacks, cryptographic misuse, concurrency pitfalls, and supply-chain risks.
---

# Rust Security

Covers Rust 2021+ edition with awareness of recent compiler and ecosystem changes. Covers both writing secure Rust code and reviewing existing code for vulnerabilities. Check the project's `Cargo.toml` (`rust-version` field and `edition`) to determine the target Rust version and adjust advice accordingly.

Rust's ownership model eliminates many memory-safety bugs in safe code (use-after-free, double-free, buffer overflows, data races). This skill focuses on what Rust does **not** prevent: unsafe misuse, logic bugs, integer overflow, panic DoS, injection, supply-chain attacks, and the boundaries where safety guarantees break down.

**Freshness note**: Advisory statuses and crate APIs evolve. When citing a specific advisory (RUSTSEC-*) or CVE in a review, verify its current status at [rustsec.org/advisories](https://rustsec.org/advisories/) or via `cargo audit` before recommending action.

## Review Workflow

When reviewing Rust code for security issues, follow this sequence:

1. **Triage dangerous primitives** -- Scan for the high-risk signals in the table below. Their presence determines which deeper checklists to apply.
2. **Check unsafe boundaries** -- For any `unsafe` block or FFI code: verify invariants, aliasing rules, lifetime correctness, and `Send`/`Sync` implementations.
3. **Check web/network hardening** -- For any service exposing HTTP: body limits, timeouts, auth middleware, CSRF, SSRF prevention.
4. **Check data handling** -- Deserialization strictness, SQL parameterization, template escaping, integer arithmetic.
5. **Check supply chain** -- Dependency advisories, build script risks, CI pipeline tooling.
6. **Consult detailed checklists** -- For any flagged area, read `references/reference.md` for expanded guidance and code examples.

## Danger Signal Triage

Scan for these primitives first. If present, apply the corresponding checklist section.

| Signal | What to look for | Risk |
|---|---|---|
| `unsafe` | `unsafe` blocks, `unsafe impl`, `unsafe fn` | UB: aliasing violations, dangling pointers, invalid values, uninitialized memory |
| FFI | `extern "C"`, `#[no_mangle]`, `*const`/`*mut` across boundaries | Null deref, lifetime escape, layout mismatch, panic across FFI |
| `transmute` | `std::mem::transmute`, `transmute_copy` | Type confusion, invalid discriminants, padding UB |
| `Send`/`Sync` impls | `unsafe impl Send`, `unsafe impl Sync` | Data races if the type is not actually thread-safe |
| Raw pointers | `*const T`, `*mut T`, `ptr::read`, `ptr::write`, `offset` | Use-after-free, out-of-bounds, alignment violations |
| `std::process::Command` | `Command::new` with user-controlled args | Command injection (especially `.bat`/`.cmd` on Windows -- CVE-2024-24576) |
| File I/O with user input | `Path::join`, `fs::read`, `fs::write`, `File::open` | Path traversal (`Path::join` discards base on absolute input) |
| Archive extraction | `zip`, `tar`, `flate2` with user-supplied archives | Zip-Slip, symlink escape, decompression bombs |
| `serde` deserialization | `serde_json::from_str`, `serde_yaml`, `bincode::deserialize` | Stack overflow, allocation bombs, type confusion |
| Raw SQL | `sql_query`, `query()` with `format!`, `execute_unprepared` | SQL injection |
| `text/template` equiv | `|safe` filter in tera/askama, `PreEscaped()` in maud | XSS (escaping disabled) |
| Integer arithmetic | `+`, `-`, `*` on user-controlled values in release builds | Silent wrapping (two's complement), size miscalculation |
| `.unwrap()` / `[index]` | Direct indexing, `unwrap()`, `expect()` on untrusted input | Panic-based DoS |
| `reqwest`/`hyper` client | Outbound HTTP with user-controlled URLs | SSRF, header leakage on redirects, proxy env injection |
| TLS config | `danger_accept_invalid_certs`, `danger_accept_invalid_hostnames` | MitM (all certificate validation disabled) |
| `rand::rngs::SmallRng` | Non-crypto RNG used for tokens, keys, nonces | Predictable randomness |

## unsafe and FFI

### unsafe Blocks

- Treat every `unsafe` block as a manual proof obligation. The compiler cannot verify aliasing rules, pointer validity, type invariants, or lifetime correctness within `unsafe`.
- Each `unsafe` block should have a `// SAFETY:` comment documenting why the invariants hold. Enable `clippy::undocumented_unsafe_blocks` in CI.
- Prefer one unsafe operation per block (`clippy::multiple_unsafe_ops_per_block`) to make each proof obligation explicit.
- Common UB patterns: creating references from invalid pointers, breaking `Pin` guarantees on `!Unpin` types, transmuting to types with different validity invariants (e.g., `u8(2)` to `bool`), calling `unreachable_unchecked` on reachable paths.

### FFI Boundaries

- All FFI calls are inherently `unsafe`. Validate every pointer from C: null check, alignment, and lifetime.
- Use `#[repr(C)]` on all structs shared across FFI. Default Rust layout is unspecified.
- **CString lifetime bug**: `c_function(CString::new("hello").unwrap().as_ptr())` is UB -- the temporary is dropped immediately. Bind the `CString` to a variable first.
- **Panics across `extern "C"`**: As of Rust 1.81, panics in `extern "C"` functions abort the process; prior toolchains treat unwinding across the boundary as UB. Use `catch_unwind` at FFI boundaries to return error codes instead (only works with `panic=unwind`, not `panic=abort`). Use `extern "C-unwind"` only when both sides expect unwinding.
- **`Send`/`Sync` on FFI wrappers**: Do not implement `Send`/`Sync` on C handle wrappers unless you have verified the C library is thread-safe. This is a frequent source of soundness bugs (RUDRA found 264 such bugs across the ecosystem).

### Soundness Awareness

- The Rust compiler and stdlib have had (rare) soundness bugs (`I-unsound` label). The most notable (issue #25860) allows safe code to extend lifetimes to `'static`, causing use-after-free (exploited by the `cve-rs` crate). Don't assume "no `unsafe` in my crate" means UB is impossible -- keep the toolchain up to date.
- Any dependency may contain `unsafe` internally. A crate exposing only safe APIs can still cause UB if its `unsafe` code is unsound. This is why `cargo audit` and supply-chain review matter.

## Integer Overflow & Arithmetic

- In debug mode, integer overflow panics. In **release mode, it wraps silently** (two's complement). This is a real security concern: `price * quantity` can wrap to near-zero.
- Use explicit arithmetic methods for security-sensitive calculations:

| Method | Behavior | Use when |
|---|---|---|
| `checked_add`, `checked_mul`, etc. | Returns `None` on overflow | Buffer sizes, financial calculations, user-controlled inputs |
| `saturating_add`, etc. | Clamps to type bounds | Counters, progress values |
| `wrapping_add`, etc. | Explicit wrapping (documents intent) | Hash functions, ring buffers |
| `overflowing_add`, etc. | Returns `(value, did_overflow)` | Need both result and overflow flag |

- For defense-in-depth, set `overflow-checks = true` in `[profile.release]` in `Cargo.toml`. This panics on overflow in release builds (performance cost).

## Panic Safety & DoS

### Panic Vectors

- Direct indexing (`v[i]`), `unwrap()`, `expect()`, integer divide-by-zero, and integer overflow in debug mode all panic. In servers, unhandled panics crash the handler thread.
- Use `.get(i)`, `unwrap_or()`, `unwrap_or_default()`, and the `?` operator instead.
- **Stack overflow via recursion** is not catchable by `catch_unwind` -- it triggers SIGSEGV. Enforce depth limits in recursive parsers processing untrusted input.
- **Panic in `Drop`** during unwinding causes a double-panic, which aborts the process. Never panic in `Drop` implementations.

### Framework Isolation

- **tokio**: A panic in a spawned task aborts only that task, not the runtime. But `panic = abort` in `Cargo.toml` makes all panics terminate the process.
- **actix-web**: Isolates panics per worker; a panic in a handler does not bring down other workers.
- **axum**: Panics in handlers propagate to the connection task. Use `catch_unwind` middleware or tower's `CatchPanic` layer.

### Resource Exhaustion

- The `regex` crate is resistant to catastrophic backtracking by design (NFA/DFA, O(m*n) worst-case). Still limit compiled regex size and DFA cache for user-supplied patterns. `fancy-regex` falls back to backtracking and IS vulnerable to ReDoS.
- Set `size_limit` and `dfa_size_limit` on `RegexBuilder` when compiling user-supplied patterns.
- Limit request body sizes, connection counts, and allocation sizes for any user-controlled input.

## Filesystem & Path Safety

### Path Traversal

- **`Path::join` discards the base when the argument is absolute**: `PathBuf::from("/safe").join("/etc/passwd")` yields `/etc/passwd`. Reject (not strip) absolute paths and any non-`Component::Normal` segments (`..`, `.`, Windows prefixes). Use the component allowlist approach in `references/reference.md`.
- For symlink safety after joining, `canonicalize()` the result and verify it `starts_with()` the base directory.
- For symlink-aware safety, use `cap-std` for capability-based filesystem access.
- Clippy issue #10655 tracks detection of the `join` footgun.

### Archive Extraction

- Validate that extracted paths contain no `..` segments or absolute paths after cleaning.
- Check for symlinks in archives that could redirect writes outside the target directory (CVE-2025-29787 in the `zip` crate).
- Limit decompressed output size with `io::Read::take()` or equivalent to prevent decompression bombs.

### File Permissions & TOCTOU

- `exists()`, `is_file()`, `is_dir()` are subject to TOCTOU races. Prefer `open()` and handle errors instead of check-then-act.
- Use `std::fs::set_permissions` with restrictive modes for secrets (0o600).

## Web Framework Hardening

### Request Body Limits

| Framework | Default limit | Configuration |
|---|---|---|
| axum | 2 MB (since axum-core 0.3.x; RUSTSEC-2022-0055 fixed the prior no-limit default) | `DefaultBodyLimit::max(bytes)` or `RequestBodyLimitLayer` |
| actix-web | 256 KB | `web::PayloadConfig::new(bytes)`, `web::JsonConfig::default().limit(bytes)` |
| rocket | 32 KiB (forms) | `[default.limits]` in `Rocket.toml` |

`DefaultBodyLimit` in axum only applies to extractors that check it (`Json`, `Form`, `Bytes`). Use `tower_http::limit::RequestBodyLimitLayer` for a hard global limit.

### Timeouts

- **axum**: Use `tower_http::timeout::TimeoutLayer`. No built-in timeout.
- **actix-web**: `client_request_timeout` (header read, default 5s), `client_disconnect_timeout`, `keep_alive`.
- **rocket**: Configurable via `Rocket.toml` workers and keep-alive settings.

### CSRF Protection

No Rust web framework has built-in CSRF protection (except Rocket's `Shield` fairing for security headers). Use `axum-csrf-sync-pattern`, `axum_csrf`, or manual `Origin`/`Sec-Fetch-Site` header checking.

### Security Headers

- **Rocket**: `Shield` fairing (attached by default) injects X-Content-Type-Options, X-Frame-Options, Permissions-Policy. HSTS is enabled when TLS is active in non-debug builds.
- **axum**: Use `tower_http::set_header::SetResponseHeaderLayer` for each header.
- **actix-web**: Add headers via middleware wrapping.

### Auth Middleware

None of these frameworks enforce auth by default. Every route must be explicitly wrapped. Prefer a middleware chain that applies auth by default and opts out for public routes (allowlist, not denylist).

## HTTP Client Security: SSRF & Redirects

### SSRF Prevention

- Never make outbound HTTP requests to user-controlled URLs without IP validation.
- Validate resolved IPs against blocked ranges: loopback, private, link-local, unspecified for IPv4; loopback, unspecified, multicast, link-local (`fe80::/10`), unique-local (`fc00::/7`) for IPv6. Also check IPv4-mapped IPv6 (`::ffff:127.0.0.1`). Note: `is_private()` does not cover all internal ranges -- CGNAT (100.64.0.0/10), benchmarking (198.18.0.0/15), and other IANA special-purpose blocks require explicit checks. `is_global()` is nightly-only.
- **DNS rebinding**: Validate the IP at connection time, not at resolution time. Use a custom `reqwest::dns::Resolve` implementation that filters IPs after resolution. See `references/reference.md` for an implementation pattern (extend blocked ranges for your deployment).
- Block non-HTTP schemes (`file://`, `gopher://`, `ftp://`).
- Disable or re-validate on every redirect hop. An attacker can redirect from an external URL to `http://169.254.169.254/` (cloud metadata).

### Redirect Header Leakage

- `reqwest` strips `Authorization`, `Cookie`, and `Cookie2` headers on cross-origin redirects. But custom headers (e.g., `X-Api-Key`) are **not** stripped.
- Disable proxy env vars for security-sensitive clients with `.no_proxy()`. An attacker controlling environment variables (e.g., `HTTP_PROXY`) can intercept outbound traffic.

## SQL & Database Safety

- **Always** use parameterized queries or query builders. Never build queries with `format!` or string concatenation.
- Diesel's query builder and sqlx's `query!` macro (compile-time checked) are safe by default.
- **Dangerous APIs**: `diesel::sql_query` with `format!`, `sqlx::query()` with string concat, `sea_orm::execute_unprepared` with user input.
- Placeholder syntax varies by driver: `$1`/`$2` for PostgreSQL, `?` for MySQL/SQLite.
- **RUSTSEC-2024-0363** (sqlx) and **RUSTSEC-2024-0365** (diesel): Binary protocol misinterpretation via truncating casts on values >4 GiB. Upgrade to sqlx >= 0.8.1, diesel >= 2.2.3.

## Deserialization & Parsing

### serde Safety

- `serde_json` has a built-in recursion limit (~128 levels). Do not call `disable_recursion_limit()` on untrusted input.
- `bincode` is **unmaintained** (RUSTSEC-2025-0141; development permanently ceased). It also has **no default size limit** -- an attacker can craft a small message declaring a huge `Vec` length, causing OOM. For existing codebases still using bincode, always use `.with_limit::<N>()` for untrusted input. For new projects, prefer `postcard`, `rkyv`, or `bitcode`.
- Use `#[serde(deny_unknown_fields)]` for strict input validation on config structs.
- **`serde_yaml` is deprecated** (March 2024). Use `serde_yaml_ng` or another maintained fork. **RUSTSEC-2025-0068**: `serde_yml` (another fork) is declared unsound with potential segfaults.

### Untagged Enum Risks

- `#[serde(untagged)]` enums try variants in declaration order, returning the first match. This can cause silent type coercion: `"42"` may deserialize as `Int(42)` or `Str("42")` depending on variant order. In security-sensitive contexts (permissions, amounts), this ambiguity is dangerous.

## Template Injection / XSS

- **tera**: Auto-escapes in `.html`/`.htm`/`.xml` files. The `|safe` filter disables escaping -- never use with user input.
- **askama**: Compile-time templates. Auto-escapes based on file extension (`.html` = HTML escaping). The `|safe` filter and `HtmlSafe` trait bypass escaping.
- **maud**: Strongest default protection. Compile-time macro-based; all spliced values auto-escaped. Only `PreEscaped()` bypasses escaping.
- **None of these engines perform contextual escaping** (e.g., safe insertion into JS strings, CSS, or URL attributes requires manual handling).

## Cryptography Essentials

### Secure Randomness

- Use `OsRng` or `rand::rng()` (which uses `ThreadRng`, periodically reseeded from `OsRng`). Both implement `CryptoRng`.
- **Never use `SmallRng`** for security purposes -- it is not cryptographically secure.

### Crypto Library Selection

| Criterion | RustCrypto | ring | aws-lc-rs |
|---|---|---|---|
| Pure Rust | Yes | No (C/asm; derived from BoringSSL) | No (AWS-LC) |
| FIPS 140-3 | No | No | Yes (requires `fips` feature + validated build config) |
| rustls default | No | No (former) | Yes (current) |
| Maintenance | Active | Security-maintained by rustls team; original author on hiatus (RUSTSEC-2025-0007 withdrawn). Versions <0.17 are unmaintained (RUSTSEC-2025-0010). | Active (AWS-backed) |

### AEAD Nonce Management

- **Nonce reuse can reveal plaintext relationships and enable forgeries** for AES-GCM and ChaCha20Poly1305; treat any nonce reuse as a critical vulnerability.
- With random 96-bit nonces, collision risk is significant after ~2^48 messages (birthday bound).
- Prefer extended-nonce variants for random generation: `XChaCha20Poly1305` (192-bit nonce) or `AES-256-GCM-SIV` (nonce-misuse resistant).
- For counter-based nonces, ensure the counter is persistent and monotonic.

### Password Hashing

- Use `argon2` (Argon2id, OWASP defaults: m=19 MiB, t=2, p=1). Target 200-500ms for interactive auth.
- `bcrypt` truncates at 72 bytes. Use Argon2 for new projects.

### TLS Configuration

- Prefer `rustls` over `native-tls`. rustls is Rust-native (not OS TLS); its default crypto provider is `aws-lc-rs` (C/assembly), not pure Rust. Supports only TLS 1.2+ with AEAD ciphers and forward secrecy, and has no legacy cipher support. The `ring` feature provides an alternative provider. The `rustls-rustcrypto` provider exists but is not production-ready per its own README.
- **Never use** `danger_accept_invalid_certs(true)` in production. For internal CAs, use `add_root_certificate()` to trust specific certs.

### Constant-Time Operations

- Standard `==` on secrets is vulnerable to timing attacks. Use `subtle::ConstantTimeEq` for comparing secrets, MACs, and tokens.
- LLVM can optimize bitwise constant-time operations back into branches. Verify constant-time behavior at the target optimization level.

### Secret Handling

- Use the `zeroize` crate to zero secret data (keys, passwords, tokens) on drop. Derive `Zeroize` and `ZeroizeOnDrop` on types holding secrets.
- Use the `secrecy` crate (`Secret<T>`) to wrap secret values with `Debug`/`Display` redaction and auto-zeroize on drop. Prevents accidental logging of secrets.

## Concurrency Safety

### What Rust Prevents vs. Does Not

- **Prevented**: Data races (enforced by `Send`/`Sync` at compile time in safe code).
- **NOT prevented**: Deadlocks, logical races (TOCTOU), livelock, starvation.

### Tokio Pitfalls

- **Holding `std::sync::Mutex` across `.await`**: The mutex stays locked while the task is suspended, causing deadlock when another task on the same runtime thread needs the lock. Use `tokio::sync::Mutex` or minimize lock scope to synchronous blocks.
- **Blocking in async context**: Calling blocking I/O or `std::thread::sleep` starves the runtime. Use `tokio::task::spawn_blocking`.
- Incorrect `Send`/`Sync` on types used across `.await` boundaries can cause data races, since async tasks may migrate between threads.

## Supply Chain & CI Pipeline

### Dependency Scanning

- Run `cargo audit` in CI. Checks `Cargo.lock` against the RustSec advisory database.
- Run `cargo deny check` for advisories + license compliance + banned crates + source restrictions.
- Use `cargo vet` in high-assurance environments for human-audited supply chain verification.

### Build Script & Proc Macro Risks

- `build.rs` scripts and proc macros execute **arbitrary code at build/compile time** with full system access. This is Rust's equivalent of npm `install` scripts.
- `rust-analyzer` runs build scripts by default -- opening an untrusted project in an IDE can trigger code execution before any human review.
- Always commit `Cargo.lock` and build with `--locked` to ensure dependency integrity.

### Feature-Flag Risks

- Cargo features are additive and unioned across the dependency graph. A dependency enabling a feature (e.g., `serde/alloc`, `openssl/vendored`) can change behavior in your crate.
- Review `Cargo.lock` and `cargo tree -f '{p} {f}'` to understand which features are active. Security-relevant features (e.g., `danger_accept_invalid_certs` in some TLS crates) can be enabled transitively.

### Typosquatting

- Real-world incidents: `faster_log`/`async_println` (2025, crypto key exfiltration), `rustdecimal` (2022, CI malware). Average crate has 5.7 typosquatting candidates.
- Trusted Publishing (OIDC-based, rolled out July 2025) eliminates manually managed API tokens.

### Supply Chain Verification Checklist

- [ ] `Cargo.lock` committed and `--locked` used in CI builds
- [ ] `cargo audit` runs in CI (fail on any severity)
- [ ] `cargo deny check` configured for advisories, licenses, bans, and sources
- [ ] GitHub Actions pinned to full commit SHAs
- [ ] `build.rs` and proc macro dependencies reviewed for filesystem/network access
- [ ] `cargo tree -f '{p} {f}'` reviewed for unexpected feature activations

### Recommended CI Security Checks

```
cargo clippy --all-targets -- -D warnings \
  -W clippy::undocumented_unsafe_blocks \
  -W clippy::multiple_unsafe_ops_per_block
cargo test --all-features
cargo audit
cargo deny check
cargo +nightly miri test          # for crates with unsafe code
cargo +nightly fuzz run <target>  # for parsers processing untrusted input
```

Pin GitHub Actions to full commit SHAs, not tags. The March 2025 `tj-actions/changed-files` compromise affected 23,000+ repos when tags were redirected to malicious commits.

## Reference Files

| File | Contents | Load when |
|---|---|---|
| `references/reference.md` | Expanded code examples for each vulnerability category, tooling setup (Miri, cargo-fuzz, sanitizers, cargo-audit, cargo-deny, Clippy lints), SSRF-safe client implementation, serde attack patterns, crypto usage examples, notable CVE/advisory tables, CI pipeline templates | Needing code examples, tooling configuration, version-specific details, or implementation patterns for any checklist item above |
