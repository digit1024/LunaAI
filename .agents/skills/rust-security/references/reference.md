# Rust Security Reference

Detailed checklists, code examples, and tooling reference. Loaded on demand from `SKILL.md`.

## unsafe Code Examples

### Common UB Patterns

```rust
// BUG: dangling pointer -- CString temporary dropped immediately
extern "C" {
    fn c_function(s: *const std::os::raw::c_char);
}
unsafe { c_function(CString::new("hello").unwrap().as_ptr()); }

// FIX: bind CString to extend lifetime
let s = CString::new("hello").unwrap();
unsafe { c_function(s.as_ptr()); }
```

```rust
// BUG: transmuting to type with different validity invariants
let x: u8 = 2;
let b: bool = unsafe { std::mem::transmute(x) };
// UB: only 0 and 1 are valid bool values
```

```rust
// BUG: returning pointer to stack-allocated data across FFI
#[no_mangle]
pub extern "C" fn get_string() -> *const c_char {
    let s = CString::new("hello").unwrap();
    s.as_ptr() // BUG: s dropped when function returns
}

// FIX: transfer ownership to caller
#[no_mangle]
pub extern "C" fn get_string() -> *mut c_char {
    CString::new("hello").unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)); }
    }
}
```

```rust
// BUG: unsound Send/Sync on FFI wrapper
struct CHandle(*mut c_void);
unsafe impl Send for CHandle {} // UNSOUND if C library uses global state
unsafe impl Sync for CHandle {} // UNSOUND if C library is not thread-safe
```

### Accepting Pointers from C

```rust
#[no_mangle]
pub extern "C" fn process(ptr: *const u8, len: usize) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    // ... process slice ...
    0
}
```

### Panic Safety at FFI Boundaries

```rust
// Abort on panic (safe default, Rust 1.81+; prior toolchains: UB):
#[no_mangle]
pub extern "C" fn callback() {
    panic!("this aborts the process");
}

// Catch panics and return error code (defensive pattern):
#[no_mangle]
pub extern "C" fn safe_callback() -> i32 {
    match std::panic::catch_unwind(|| {
        do_work()
    }) {
        Ok(result) => result,
        Err(_) => -1,
    }
}

// Allow unwinding (only when both sides expect it):
#[no_mangle]
pub extern "C-unwind" fn callback_unwind() {
    panic!("this unwinds across the FFI boundary");
}
```

### FFI Struct Layout

```rust
// Required for FFI -- without #[repr(C)], field order and padding are unspecified
#[repr(C)]
struct Point {
    x: f64,
    y: f64,
}
```

## Integer Overflow Examples

```rust
// VULNERABLE in release mode -- silent wrapping
fn allocate_buffer(count: usize, item_size: usize) -> Vec<u8> {
    let total_size = count * item_size; // wraps silently!
    vec![0u8; total_size]               // allocates too-small buffer
}

// SAFE -- checked arithmetic
fn allocate_buffer(count: usize, item_size: usize) -> Option<Vec<u8>> {
    let total_size = count.checked_mul(item_size)?;
    Some(vec![0u8; total_size])
}
```

```rust
// Enable overflow checks in release builds (Cargo.toml)
// [profile.release]
// overflow-checks = true
```

## Panic Safety Examples

```rust
// VULNERABLE -- all of these panic on untrusted input
let v = vec![1, 2, 3];
let _ = v[user_index];          // index out of bounds
let _ = v.len() / user_divisor; // divide by zero (integers)
let _ = option_val.unwrap();    // unwrap on None

// SAFE alternatives
let _ = v.get(user_index);          // Returns Option<&T>
let _ = v.len().checked_div(user_divisor); // Returns Option
let _ = option_val.unwrap_or(default);
let _ = option_val?;                 // Propagates error
```

### Recursive Parser Depth Limiting

```rust
const MAX_DEPTH: usize = 128;

fn parse(input: &[u8], depth: usize) -> Result<Node, Error> {
    if depth > MAX_DEPTH {
        return Err(Error::TooDeep);
    }
    // ... parse children ...
    parse(rest, depth + 1)?;
    Ok(node)
}
```

### Regex Safety

```rust
use regex::RegexBuilder;

// Safe: set resource limits when compiling user-supplied patterns
let re = RegexBuilder::new(&user_pattern)
    .size_limit(1 << 20)      // 1 MB compiled regex size limit
    .dfa_size_limit(1 << 20)  // 1 MB DFA cache limit
    .build()?;

// NOTE: The `regex` crate guarantees O(m*n) -- resistant to catastrophic backtracking.
// Still limit compiled size for user-supplied patterns.
// The `fancy-regex` crate falls back to backtracking and IS vulnerable to ReDoS.
```

## Filesystem & Path Safety Examples

### Path Traversal Prevention

```rust
use std::path::{Path, PathBuf};
use std::fs;

// VULNERABLE -- Path::join discards base on absolute input
let base = PathBuf::from("/safe/directory");
let path = base.join(user_input);
// If user_input = "/etc/passwd", path = "/etc/passwd"

// SAFE -- allowlist path components, reject traversal and absolute paths
fn safe_path(base: &Path, user_input: &str) -> Result<PathBuf, &'static str> {
    use std::path::Component;

    let user_path = Path::new(user_input);
    let mut result = base.to_path_buf();

    for component in user_path.components() {
        match component {
            Component::Normal(seg) => result.push(seg),
            // Reject .., /, prefix (C:\), and curdir (.) components
            _ => return Err("path traversal attempt"),
        }
    }

    // Optional: canonicalize for symlink resolution (requires path to exist)
    // let canonical = fs::canonicalize(&result).map_err(|_| "path does not exist")?;
    // let canonical_base = fs::canonicalize(base).map_err(|_| "base does not exist")?;
    // if !canonical.starts_with(&canonical_base) { return Err("symlink escape"); }

    Ok(result)
}
```

### Archive Extraction Safety

```rust
use std::path::{Path, PathBuf};
use std::io::Read;
use zip::ZipArchive;

fn extract_zip_safely<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    dest_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        // Use enclosed_name() (zip >= 0.5.9) -- returns None if path
        // escapes the archive root (absolute paths, ".." traversal)
        let rel_path = match file.enclosed_name() {
            Some(name) => name.to_owned(),
            None => return Err(format!("illegal path in archive: {}", file.name()).into()),
        };

        let target = dest_dir.join(&rel_path);

        // Reject symlinks in archive (CVE-2025-29787: symlink-based Zip-Slip)
        if file.is_symlink() {
            continue; // or return error
        }

        if file.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&target)?;
            // Limit extracted file size to prevent decompression bombs
            let mut limited = file.take(100 * 1024 * 1024); // 100 MB limit
            std::io::copy(&mut limited, &mut outfile)?;
        }
    }
    Ok(())
}
```

## Web Framework Hardening Examples

### axum Body Limits

```rust
use axum::{Router, routing::post, extract::DefaultBodyLimit};
use tower_http::limit::RequestBodyLimitLayer;

// API routes: 10 MiB hard cap
let api = Router::new()
    .route("/api/data", post(handler))
    .layer(DefaultBodyLimit::disable())
    .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024));

// Upload route: 50 MiB hard cap
let upload = Router::new()
    .route("/upload", post(upload_handler))
    .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
    .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024));

let app = api.merge(upload);
```

### axum Timeout

```rust
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;
use axum::error_handling::HandleErrorLayer;
use axum::http::StatusCode;

let app = Router::new()
    .route("/", get(handler))
    .layer(
        ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|_: tower::BoxError| async {
                StatusCode::REQUEST_TIMEOUT
            }))
            .layer(TimeoutLayer::new(Duration::from_secs(30)))
    );
```

### axum Auth Middleware

```rust
use axum::{extract::Request, middleware::{self, Next}, response::Response};
use axum::http::StatusCode;

async fn auth_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
    let auth_header = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(token) if token.starts_with("Bearer ") => {
            // Validate JWT
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

let app = Router::new()
    .route("/protected", get(protected_handler))
    .layer(middleware::from_fn(auth_middleware));
```

### axum Rate Limiting

```rust
use tower_governor::{GovernorConfigBuilder, GovernorLayer};

let governor_conf = GovernorConfigBuilder::default()
    .per_second(2)
    .burst_size(5)
    .finish()
    .unwrap();

let app = Router::new()
    .route("/api", get(handler))
    .layer(GovernorLayer { config: governor_conf });
```

### actix-web Configuration

```rust
use actix_web::{web, App, HttpServer};
use std::time::Duration;

HttpServer::new(|| {
    App::new()
        .app_data(web::PayloadConfig::new(1024 * 1024))    // 1 MB global
        .app_data(web::JsonConfig::default().limit(4096))    // 4 KB JSON
        .app_data(web::FormConfig::default().limit(8192))    // 8 KB forms
})
.keep_alive(Duration::from_secs(75))
.client_request_timeout(Duration::from_secs(60))
.client_disconnect_timeout(Duration::from_secs(5))
```

### Rocket Configuration

```toml
# Rocket.toml
[default.limits]
form = "64 kB"
json = "1 MiB"
msgpack = "2 MiB"
"file/jpg" = "5 MiB"
```

Rocket's `Shield` fairing (attached by default) provides X-Content-Type-Options, X-Frame-Options, and Permissions-Policy headers. HSTS is enabled automatically when TLS is active in non-debug builds.

## SSRF-Safe HTTP Client

### reqwest with Custom DNS Resolver

```rust
use reqwest::dns::{Resolve, Resolving};
use std::net::IpAddr;
use std::sync::Arc;

struct SsrfSafeResolver;

impl Resolve for SsrfSafeResolver {
    fn resolve(&self, name: hyper::client::connect::dns::Name) -> Resolving {
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((name.as_str(), 0)).await?;
            let safe_addrs: Vec<_> = addrs
                .filter(|addr| is_safe_target(addr.ip()))
                .collect();
            if safe_addrs.is_empty() {
                return Err("resolved to blocked IP range".into());
            }
            Ok(Box::new(safe_addrs.into_iter()) as Box<dyn Iterator<Item = _> + Send>)
        })
    }
}

// NOTE: This is an implementation pattern -- extend the blocked ranges
// based on your deployment's IANA special-purpose requirements.
fn is_safe_target(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(ip) => is_safe_v4(ip),
        IpAddr::V6(ip) => {
            // Handle IPv4-mapped IPv6 (::ffff:127.0.0.1) via IPv4 checks
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_safe_v4(v4);
            }
            !ip.is_loopback()
            && !ip.is_unspecified()
            && !ip.is_multicast()
            && !is_ipv6_link_local(ip)    // fe80::/10
            && !is_ipv6_unique_local(ip)  // fc00::/7 (private)
        }
    }
}

fn is_safe_v4(ip: std::net::Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_private()
    && !ip.is_link_local() && !ip.is_broadcast()
    && !ip.is_unspecified()
    // is_private() misses CGNAT, benchmarking, and other IANA
    // special-purpose ranges:
    && !is_cgnat(ip)
}

fn is_cgnat(ip: std::net::Ipv4Addr) -> bool {
    // CGNAT: 100.64.0.0/10 (shared address space, RFC 6598)
    let octets = ip.octets();
    octets[0] == 100 && (octets[1] & 0xC0) == 64
}

// Stable-Rust helpers for IPv6 ranges (nightly has is_unicast_link_local, etc.)
fn is_ipv6_link_local(ip: std::net::Ipv6Addr) -> bool {
    // fe80::/10
    let seg = ip.segments();
    (seg[0] & 0xFFC0) == 0xFE80
}

fn is_ipv6_unique_local(ip: std::net::Ipv6Addr) -> bool {
    // fc00::/7 (IPv6 private address space)
    let seg = ip.segments();
    (seg[0] & 0xFE00) == 0xFC00
}

let client = reqwest::Client::builder()
    .dns_resolver(Arc::new(SsrfSafeResolver))
    .redirect(reqwest::redirect::Policy::none()) // Re-validate each redirect manually
    .no_proxy()                                   // Ignore HTTP_PROXY env vars
    .timeout(std::time::Duration::from_secs(10))
    .connect_timeout(std::time::Duration::from_secs(5))
    .build()?;
```

### Redirect Safety

```rust
// Strip sensitive custom headers on cross-origin redirects
let client = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > 5 {
            attempt.error("too many redirects")
        } else if attempt.url().host_str() != attempt.previous()[0].host_str() {
            attempt.stop() // stop on cross-origin redirect
        } else {
            attempt.follow()
        }
    }))
    .build()?;
```

Note: reqwest strips `Authorization`, `Cookie`, `Cookie2` on cross-origin redirects by default. But custom headers like `X-Api-Key` are **not** stripped.

## SQL Injection Examples

### Diesel

```rust
// SAFE: query builder generates parameterized queries
let results = users::table
    .filter(users::email.eq(user_input))
    .load::<User>(&mut conn)?;

// DANGEROUS: sql_query with format!
let query = format!("SELECT * FROM users WHERE name = '{}'", user_input);
let results = diesel::sql_query(query).load::<User>(&mut conn)?;

// SAFE: sql_query with bind parameters
let results = diesel::sql_query("SELECT * FROM users WHERE name = $1")
    .bind::<diesel::sql_types::Text, _>(user_input)
    .load::<User>(&mut conn)?;
```

### sqlx

```rust
// SAFE: compile-time checked, parameterized
let user = sqlx::query_as!(User,
    "SELECT id, name, email FROM users WHERE email = $1",
    user_email
).fetch_one(&pool).await?;

// DANGEROUS: query() with string concat
let q = format!("SELECT * FROM users WHERE name = '{}'", user_input);
let rows = sqlx::query(&q).fetch_all(&pool).await?;

// SAFE: query() with bind
let rows = sqlx::query("SELECT * FROM users WHERE name = $1")
    .bind(user_input)
    .fetch_all(&pool).await?;
```

### Sea-ORM

```rust
// SAFE: query builder
let user = Entity::find()
    .filter(Column::Email.eq(user_email))
    .one(&db).await?;

// SAFE: raw SQL with parameters
use sea_orm::{ConnectionTrait, Statement, DatabaseBackend};
let results = db.query_all(Statement::from_sql_and_values(
    DatabaseBackend::Postgres,
    "SELECT * FROM users WHERE email = $1",
    [user_email.into()],
)).await?;

// DANGEROUS: execute_unprepared with user input
db.execute_unprepared(&format!(
    "SELECT * FROM users WHERE name = '{}'", user_input
)).await?;
```

## Deserialization Attack Examples

### serde_json Recursion

```rust
// Default: recursion limit is active (safe)
let value: serde_json::Value = serde_json::from_str(&untrusted_json)?;

// DANGEROUS: disabling recursion limit
let mut de = serde_json::Deserializer::from_str(&untrusted_json);
de.disable_recursion_limit();
// Even Display/Debug/Drop on deeply nested serde_json::Value can stack overflow
```

### bincode Size Limits

```rust
// DANGEROUS: no size limit (default)
let config = bincode::config::standard();
let value: Vec<u8> = bincode::decode_from_slice(&untrusted_bytes, config)?.0;
// Attacker sends small message declaring Vec length = 2^64 -> OOM

// SAFE: explicit size limit
let config = bincode::config::standard()
    .with_limit::<{ 1024 * 1024 }>(); // 1 MB limit
```

### deny_unknown_fields

```rust
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    name: String,
    port: u16,
}
// Rejects input with extra fields not in the struct.
// Limitations: incompatible with #[serde(flatten)] in some cases.
```

### Untagged Enum Type Confusion

```rust
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Value {
    Int(i64),    // tried first
    Str(String), // tried second
}
// "42" may deserialize as Int(42) depending on format behavior.
// In security-sensitive contexts, use tagged enums instead.
```

## Template Injection Examples

### tera

```html
{# Auto-escaped by default in .html files -- safe #}
{{ user_input }}

{# DANGEROUS: |safe filter disables escaping #}
{{ user_input | safe }}
```

### askama

```html
{# Auto-escaped in .html templates -- safe #}
{{ user_input }}

{# DANGEROUS: |safe filter bypasses escaping #}
{{ user_input|safe }}
```

### maud

```rust
use maud::{html, PreEscaped};

let safe = html! { p { (user_input) } };            // Escaped
let dangerous = html! { p { (PreEscaped(user_input)) } };  // NOT escaped
```

## Cryptography Examples

### AEAD Encryption (AES-GCM)

```rust
use aes_gcm::{Aes256Gcm, Nonce};
use aes_gcm::aead::{Aead, KeyInit, OsRng};

let key = Aes256Gcm::generate_key(&mut OsRng);
let cipher = Aes256Gcm::new(&key);

// CRITICAL: nonce must NEVER be reused with the same key
let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bit (12-byte)

let ciphertext = cipher.encrypt(&nonce, b"plaintext".as_ref())
    .expect("encryption failure");
let plaintext = cipher.decrypt(&nonce, ciphertext.as_ref())
    .expect("decryption failure");
```

### Extended-Nonce AEAD (Recommended for Random Nonces)

```rust
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng};

let key = XChaCha20Poly1305::generate_key(&mut OsRng);
let cipher = XChaCha20Poly1305::new(&key);
// 192-bit nonce: safe to generate randomly for each message
let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
let ciphertext = cipher.encrypt(&nonce, b"plaintext".as_ref()).unwrap();
```

### Password Hashing (Argon2)

```rust
use argon2::{
    password_hash::{
        rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};

// Hash
let salt = SaltString::generate(&mut OsRng);
let argon2 = Argon2::default(); // Argon2id, OWASP: m=19MiB, t=2, p=1
let hash = argon2.hash_password(b"hunter2", &salt)
    .expect("hashing failed")
    .to_string();

// Verify
let parsed = PasswordHash::new(&hash).unwrap();
assert!(Argon2::default().verify_password(b"hunter2", &parsed).is_ok());
```

### Constant-Time Comparison

```rust
use subtle::ConstantTimeEq;

// VULNERABLE: standard == short-circuits on first difference
if user_token == stored_token { /* ... */ }

// SAFE: constant-time comparison
if user_token.as_bytes().ct_eq(stored_token.as_bytes()).into() {
    // authenticated
}
```

### TLS Configuration with rustls

```rust
// DANGEROUS: disables all certificate validation
let client = reqwest::Client::builder()
    .danger_accept_invalid_certs(true)
    .build()?;

// SAFE: trust specific internal CA
let cert = reqwest::Certificate::from_pem(&pem_bytes)?;
let client = reqwest::Client::builder()
    .add_root_certificate(cert)
    .build()?;
```

### FIPS 140 with aws-lc-rs

```toml
# Cargo.toml
[dependencies]
aws-lc-rs = { version = "1", features = ["fips"] }
# Or via rustls:
rustls = { version = "0.23", features = ["fips"] }
```

## Concurrency Examples

### Tokio Mutex Deadlock

```rust
// VULNERABLE: std::sync::Mutex held across .await
async fn handle_request(state: Arc<std::sync::Mutex<State>>) {
    let guard = state.lock().unwrap();
    some_async_operation().await; // guard still held, task may be suspended
    // Another task on the same thread needing this mutex -> deadlock
}

// SAFE: minimize lock scope to synchronous block
async fn handle_request(state: Arc<std::sync::Mutex<State>>) {
    let data = {
        let guard = state.lock().unwrap();
        guard.clone() // or extract needed data
    }; // guard dropped here
    some_async_operation().await;
}

// SAFE: use tokio::sync::Mutex (designed for async)
async fn handle_request(state: Arc<tokio::sync::Mutex<State>>) {
    let guard = state.lock().await;
    some_async_operation().await;
}
```

### Blocking in Async Context

```rust
// VULNERABLE: blocks the tokio runtime thread
async fn handle() {
    std::thread::sleep(std::time::Duration::from_secs(5)); // starves runtime
}

// SAFE: offload to blocking thread pool
async fn handle() {
    tokio::task::spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_secs(5));
    }).await.unwrap();
}
```

## Tooling Setup

### Miri (Undefined Behavior Detection)

```bash
# Install (requires nightly)
rustup +nightly component add miri

# Run tests under Miri
cargo +nightly miri test

# Use Tree Borrows (more permissive aliasing model)
MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri test

# Disable isolation (needed for env vars, file I/O)
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test

# Check for memory leaks
MIRIFLAGS="-Zmiri-leak-check" cargo +nightly miri test
```

**Detects:** Out-of-bounds access, use-after-free, uninitialized reads, aliasing violations (Stacked/Tree Borrows), data races, invalid type invariants, memory leaks.

**Limitations:** ~100x slowdown, cannot test FFI code, limited platform API support.

### cargo-careful (Extra Debug Assertions)

```bash
cargo install cargo-careful
cargo +nightly careful test
```

Faster than Miri. FFI-compatible. Catches invalid values passed to stdlib functions.

### Sanitizers (ASan, MSan, TSan)

```bash
# AddressSanitizer -- out-of-bounds, use-after-free, double-free
RUSTFLAGS="-Zsanitizer=address" cargo +nightly test --target x86_64-unknown-linux-gnu

# MemorySanitizer -- uninitialized memory reads (Linux x86_64 only)
RUSTFLAGS="-Zsanitizer=memory" cargo +nightly test --target x86_64-unknown-linux-gnu

# ThreadSanitizer -- data races
RUSTFLAGS="-Zsanitizer=thread" cargo +nightly test --target x86_64-unknown-linux-gnu

# For mixed Rust/C codebases, rebuild stdlib:
RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu
```

Always pass `--target` explicitly to avoid sanitizing build scripts.

### cargo-fuzz (Fuzzing)

```bash
# Install (requires nightly)
cargo install cargo-fuzz

# Initialize
cargo fuzz init

# Run
cargo +nightly fuzz run fuzz_target_1

# Time-limited run
cargo +nightly fuzz run fuzz_target_1 -- -max_total_time=300
```

ASan is enabled by default in cargo-fuzz.

**Structured fuzzing with Arbitrary:**

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct MyInput {
    size: u16,
    data: Vec<u8>,
    flag: bool,
}

fuzz_target!(|input: MyInput| {
    my_crate::process(input.size, &input.data, input.flag);
});
```

### cargo-audit

```bash
cargo install cargo-audit
cargo audit

# Check for yanked crates too
cargo audit --deny warnings

# Audit compiled binaries (with cargo-auditable)
cargo install cargo-auditable
cargo auditable build --release
cargo audit bin target/release/my_binary
```

### cargo-deny

```bash
cargo install cargo-deny
cargo deny init    # creates deny.toml
cargo deny check   # run all checks

# Individual checks
cargo deny check advisories  # vulnerability advisories
cargo deny check licenses    # license compliance
cargo deny check bans        # banned crate enforcement
cargo deny check sources     # allowed registry sources
```

### cargo-vet (Human-Audited Supply Chain)

```bash
cargo vet                              # check all deps against audits
cargo vet inspect serde 1.0.200        # inspect a crate
cargo vet diff serde 1.0.199 1.0.200   # diff between versions
cargo vet certify serde 1.0.200        # record audit
```

### Clippy Security Lints

Key lints for security-sensitive code:

| Lint | Default | Purpose |
|---|---|---|
| `undocumented_unsafe_blocks` | Allow | Requires `// SAFETY:` comment before `unsafe` blocks |
| `multiple_unsafe_ops_per_block` | Allow | One unsafe operation per block |
| `unsafe_derive_deserialize` | Allow | Warns on Deserialize derive with unsafe methods |
| `ptr_as_ptr` | Allow | Prefer `pointer::cast()` over `as` casts |
| `cast_ptr_alignment` | Allow | Flags alignment-changing pointer casts |
| `transmute_ptr_to_ref` | Warn | Prefer `&*ptr` over transmute |
| `crosspointer_transmute` | Warn | Flags transmute between raw ptr and reference |
| `transmute_int_to_bool` | Warn | Flags integer to bool transmute |

Recommended CI invocation:

```bash
cargo clippy --all-targets -- -D warnings \
    -W clippy::undocumented_unsafe_blocks \
    -W clippy::multiple_unsafe_ops_per_block \
    -W clippy::unsafe_derive_deserialize \
    -W clippy::ptr_as_ptr \
    -W clippy::cast_ptr_alignment
```

## CI Pipeline Template

```yaml
# .github/workflows/security.yml
name: Security
on:
  push:
    branches: [main]
  pull_request:
  schedule:
    - cron: '0 6 * * 0'  # weekly scan

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<pin-to-full-sha>
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - run: |
          cargo clippy --all-targets --all-features -- -D warnings \
            -W clippy::undocumented_unsafe_blocks \
            -W clippy::multiple_unsafe_ops_per_block

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<pin-to-full-sha>
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --all-features

  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<pin-to-full-sha>
      - uses: actions-rust-lang/audit@<pin-to-full-sha>

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<pin-to-full-sha>
      - uses: EmbarkStudios/cargo-deny-action@<pin-to-full-sha>

  miri:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<pin-to-full-sha>
      - uses: dtolnay/rust-toolchain@nightly
        with:
          components: miri
      - run: cargo miri setup
      - run: cargo miri test

  fuzz:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<pin-to-full-sha>
      - uses: dtolnay/rust-toolchain@nightly
      - run: cargo install cargo-fuzz
      - run: cargo fuzz run fuzz_target -- -max_total_time=300
```

Pin all GitHub Actions to full commit SHAs. The March 2025 `tj-actions/changed-files` compromise affected 23,000+ repos when tags were redirected to malicious commits.

## Notable CVEs and Advisories

### Standard Library CVEs

| CVE | Version Range | Vulnerability |
|---|---|---|
| CVE-2024-24576 | < 1.77.2 | `Command` argument escaping on Windows batch files (BatBadBut, CVSS 10.0) |
| CVE-2022-21658 | < 1.58.1 | `remove_dir_all` TOCTOU race condition (symlink attack) |
| CVE-2021-31162 | 1.48-1.52 | Double free in `Vec::from_iter` specialization on drop panic |
| CVE-2021-28879 | 1.14-1.52 | Buffer overflow in `Zip` iterator |
| CVE-2020-36323 | 1.28-1.52 | `join()` on iterators of `&[u8]` could expose uninitialized memory |
| CVE-2020-36318 | 1.48-1.49 | `VecDeque::make_contiguous` use-after-free |
| CVE-2018-1000810 | 1.26-1.29 | Buffer overflow in `str::repeat()` |

### Ecosystem Advisories

| Advisory | Crate | Issue | Fix |
|---|---|---|---|
| RUSTSEC-2024-0003 | h2 | HTTP/2 resource exhaustion via unbounded reset frame queueing (OOM) | >= 0.3.24 |
| RUSTSEC-2024-0336 | rustls | Infinite loop on `close_notify` after `client_hello` (100% CPU) | >= 0.23.5 |
| RUSTSEC-2024-0363 | sqlx | Binary protocol overflow on >4 GiB values | >= 0.8.1 |
| RUSTSEC-2024-0365 | diesel | Binary protocol overflow on >4 GiB values | >= 2.2.3 |
| RUSTSEC-2022-0093 | ed25519-dalek | Private key extraction via signing oracle | >= 2.0.0-rc2 |
| RUSTSEC-2022-0055 | axum-core | No default body size limit (DoS) | >= 0.3.0 |
| RUSTSEC-2022-0013 | regex | Compiling untrusted patterns: large repetitions of empty sub-expressions cause parse-time DoS. Matching untrusted text with trusted patterns is not affected. | >= 1.5.5 |
| RUSTSEC-2025-0007 | ring | Advisory withdrawn; rustls team has taken security maintenance responsibility. Original author on hiatus. | Evaluate aws-lc-rs for new projects |
| RUSTSEC-2025-0010 | ring | Versions <0.17 are unmaintained | Upgrade to ring >= 0.17 |
| RUSTSEC-2025-0068 | serde_yml | Unsound and unmaintained (segfaults via emitter) | Use serde_yaml_ng |
| RUSTSEC-2021-0124 | tokio | Data race in oneshot channel | >= 1.8.4 |
| RUSTSEC-2021-0081 | actix-http | HTTP request smuggling (CVSS 7.5) | >= 2.2.1 |
| RUSTSEC-2020-0159 | chrono | Segfault in `localtime_r` due to thread-unsafe env var access | >= 0.4.20 |
| CVE-2025-29787 | zip | Path traversal via symlinks during extraction (Zip-Slip) | >= 2.3.0 |
| CVE-2025-62518 | tokio-tar / astral-tokio-tar | TAR entry smuggling via PAX header desync (TARmageddon, CVSS 8.1). tokio-tar is unmaintained and will not be patched. | Use astral-tokio-tar >= 0.5.6 |

### Common Vulnerability Patterns (by Frequency)

1. **Denial of Service** -- Most common category. HTTP/2 attacks, regex bombs, infinite loops, resource exhaustion.
2. **Memory corruption via unsafe** -- Use-after-free, OOB writes, double-free in FFI wrappers and low-level data structures.
3. **Unsoundness** -- Safe APIs that trigger UB due to incorrect unsafe implementations. RUDRA found 264 such bugs leading to 76 CVEs.
4. **HTTP request smuggling** -- Multiple advisories across hyper, actix-http, async-h1.
5. **Cryptographic failures** -- Timing side-channels, oracle attacks, incorrect nonce handling.
6. **Path traversal** -- Zip-Slip variants, `Path::join` with absolute paths.

## Security Review Workflow Summary

1. **Static analysis**: `cargo clippy` with strict unsafe lints
2. **Dependency audit**: `cargo audit` + `cargo deny check`
3. **Miri**: `cargo +nightly miri test` for unsafe code (aliasing, use-after-free, invalid values)
4. **cargo-careful**: `cargo +nightly careful test` for stdlib misuse (FFI-compatible)
5. **Fuzz**: `cargo +nightly fuzz run` on public APIs processing untrusted input
6. **Sanitizers**: ASan + TSan for mixed Rust/C codebases
7. **Manual review**: Focus on `unsafe` blocks, `Send`/`Sync` impls, `transmute`, raw pointers, FFI boundaries
