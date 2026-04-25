# Email and URL Detection Implementation Plan

> **Status**: 🟢 Historical rollout record — implemented, including scanner/runtime ingestion parity work
> **Current contract owner**: [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) for the current subsystem description and [`../../CLI_GUIDE.md`](../../CLI_GUIDE.md) for user-facing CLI workflows
> **Historical priority / effort**: P2, originally estimated at 1-2 weeks
> **Dependencies at implementation time**: None (independent of copyright and license detection)

## Table of Contents

- [Overview](#overview)
- [Python Reference Analysis](#python-reference-analysis)
- [Rust Architecture Design](#rust-architecture-design)
- [Implementation Phases](#implementation-phases)
- [Beyond-Parity Improvements](#beyond-parity-improvements)
- [Testing Strategy](#testing-strategy)
- [Success Criteria](#success-criteria)

---

## Overview

This file is retained as a completed implementation record. Treat future-tense rollout notes, old success criteria, and historical implementation phases below as historical context rather than as the live maintainer contract.

Email and URL detection extracts email addresses and URLs from text-bearing scan inputs for contact information and reference tracking. This primarily means source and text files, and in Rust it additionally includes supported-image EXIF/XMP metadata as a beyond-parity clue source. It is the simplest of the three text detection features (license, copyright, email/URL) — primarily regex-based with a filter pipeline to remove junk results.

### Scope

**In Scope:**

- Email address extraction (RFC-compliant regex)
- URL extraction (http, https, ftp, sftp, ssh, git, svn, hg, rsync + compound schemes)
- Git-style URLs (`git@github.com:user/repo.git`)
- Bare domain URLs (`www.example.com`, `ftp.example.com`)
- Junk filtering (example domains, private IPs, XML namespaces, CDN/PKI URLs)
- URL normalization (canonicalization, default port removal, punycode)
- User/password stripping from URLs
- Deduplication
- Threshold support (`--max-email`, `--max-url`)
- Scanner pipeline integration

**Out of Scope:**

- Email/URL validation against external services (no DNS/HTTP checks)
- Dead link detection
- Email obfuscation detection (the `AT`/`DOT` obfuscation in copyright context is handled by the copyright detector, not this module)
- Email/URL extraction from within copyright/license context (those detectors extract their own)

### Current State in Rust

**Implemented:**

- ✅ Finder module at `src/finder/` (`emails.rs`, `urls.rs`, `host.rs`, `junk_data.rs`)
- ✅ `OutputEmail` + `emails` field in `FileInfo`
- ✅ `OutputURL` includes `start_line`/`end_line`
- ✅ Scanner integration via `process_with_options(...)`
- ✅ CLI flags: `--email`, `--max-email`, `--url`, `--max-url`
- ✅ Local golden tests and fixtures in `tests/finder_golden.rs` and `testdata/plugin_email_url/`

**Notes:**

- Rust intentionally keeps several beyond-parity fixes (notably extended TLD length support and IPv6/private IP correctness).
- ✅ The live Rust scanner now feeds finder detection through a shared path-aware ingestion helper in `src/utils/file.rs`, covering decoded non-UTF text, PDFs with extractable text, printable strings from `.dll` / `.exe` inputs, and supported-image EXIF/XMP metadata before the normal finder pipeline runs. PEM certificate skipping remains a separate intentional divergence in the scanner.

---

## Python Reference Analysis

### Architecture Overview

The Python implementation spans three files:

| File                | Lines | Purpose                                                                |
| ------------------- | ----- | ---------------------------------------------------------------------- |
| `finder.py`         | 597   | Core detection: regex patterns, filter pipelines, URL canonicalization |
| `finder_data.py`    | 252   | Junk classification data: emails, hosts, IPs, URLs, domain suffixes    |
| `plugin_email.py`   | 59    | Scanner plugin: `--email`, `--max-email` CLI flags                     |
| `plugin_url.py`     | 55    | Scanner plugin: `--url`, `--max-url` CLI flags                         |
| `api.py` (relevant) | ~60   | `get_emails()`, `get_urls()` — thin wrappers with threshold            |

Total: ~1,023 lines (much simpler than copyright detection's ~4,675 lines).

### Pipeline Architecture

Both email and URL detection follow the same pattern:

```text
┌──────────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  1. Read     │───>│  2. Regex    │───>│  3. Filter   │───>│  4. Yield    │
│  Lines       │    │  Match       │    │  Pipeline    │    │  Results     │
│              │    │              │    │              │    │              │
│ numbered_    │    │ findall()    │    │ ordered      │    │ (value,      │
│ text_lines() │    │ per line     │    │ filter chain │    │  line_num)   │
└──────────────┘    └──────────────┘    └──────────────┘    └──────────────┘
```

#### Email Detection (`find_emails`)

**Regex:**

```python
r'\b[A-Z0-9._%-]+@[A-Z0-9.-]+\.[A-Z]{2,4}\b'  # case-insensitive
```

**Filter Pipeline (in order):**

1. **`junk_email_domains_filter`**: Removes emails with junk domains
   - Calls `is_good_email_domain()` → checks host via `is_good_host()`
   - Then checks domain (TLD+1) via `url_host_domain()` + `is_good_host()`
2. **`uninteresting_emails_filter`**: Removes emails matching `finder_data.classify_email()`
   - Checks against `JUNK_EMAILS` set, `JUNK_DOMAIN_SUFFIXES`, `JUNK_EXACT_DOMAIN_NAMES`
3. **`unique_filter`** (if `unique=True`): Deduplicates by `(key, match)` tuple

#### URL Detection (`find_urls`)

**Regex:**

```python
# Three alternatives OR'd together:
# 1. URLs with schemes: (https?|ftps?|sftp|rsync|ssh|svn|git|https?\+git|https?\+svn|https?\+hg)://[^\s<>\[\]"]+
# 2. Bare domain URLs: (www|ftp)\.[^\s<>\[\]"]+
# 3. Git-style URLs: git\@[^\s<>\[\]"]+:[^\s<>\[\]"]+\.git
```

**Filter Pipeline (in order — order IS important):**

1. **`verbatim_crlf_url_cleaner`**: Removes literal `\n`, `\r` from URLs (unless URL ends with `/`)
2. **`end_of_url_cleaner`**: Strips trailing junk (HTML entities, punctuation, brackets, quotes, backslashes)
3. **`empty_urls_filter`**: Removes empty/stub URLs (`"https"`, `"http"`, `"ftp"`, `"www"`)
4. **`scheme_adder`**: Adds `http://` to bare-domain URLs (e.g., `www.example.com` → `http://www.example.com`)
5. **`user_pass_cleaning_filter`**: Strips user:pass from URL host; drops URLs with no host
6. **`build_regex_filter(INVALID_URLS_PATTERN)`**: Removes invalid URLs matching `(scheme)://[$%*/_]+`
7. **`canonical_url_cleaner`**: Canonicalizes URLs (punycode, default port removal) via `urlpy.parse()`
8. **`junk_url_hosts_filter`**: Removes URLs with junk hosts/domains
   - Checks host via `is_good_host()` (private IPs, localhost, etc.)
   - Checks domain via `is_good_host()` (example.com, test.com, etc.)
9. **`junk_urls_filter`**: Removes URLs matching `finder_data.classify_url()`
   - Exact match against `JUNK_URLS` set
   - Prefix match against `JUNK_URL_PREFIXES`
   - Suffix match against `JUNK_DOMAIN_SUFFIXES` (`.png`, `.jpg`, `.gif`, `.jpeg`)
10. **`unique_filter`** (if `unique=True`): Deduplicates

#### Shared Components

**`is_good_host(host)`**: Central host validation:

- If IP address → check `is_private_ip()` and `classify_ip()`
- If hostname without `.` → reject (localhost, private hostnames)
- If domain → check `classify_host()`

**`is_private_ip(ip)`**: IPv4/IPv6 private address detection:

- IPv4: reserved, private, multicast, unspecified, loopback, link-local
- IPv6: multicast, reserved, link-local, site-local, private, unspecified, loopback

**`url_host_domain(url)`**: Extract (host, domain) from URL using `urlpy.parse()`.

**`classify()` (finder_data.py)**: Generic junk classifier:

- Lowercase + strip trailing `/`
- Check `@` → extract host and check against `ignored_hosts`
- Check if any string from `data_set` is a substring of input
- Check if input ends with any of `suffixes`

#### Junk Classification Data (`finder_data.py`)

| Dataset                   | Count | Purpose                                                                  |
| ------------------------- | ----- | ------------------------------------------------------------------------ |
| `JUNK_EMAILS`             | 7     | Exact junk email domains (test.com, example.com, localhost, etc.)        |
| `JUNK_HOSTS_AND_DOMAINS`  | 11    | Junk hosts (example.com, localhost, maps.google.com, 1.2.3.4, etc.)      |
| `JUNK_IPS`                | 1     | Junk IP addresses (1.2.3.4)                                              |
| `JUNK_EXACT_DOMAIN_NAMES` | 8     | Exact junk domains for email host filtering (test.com, sample.com, etc.) |
| `JUNK_URLS`               | 73    | Exact junk URLs (W3C schemas, DTDs, XML namespaces, etc.)                |
| `JUNK_URL_PREFIXES`       | 57    | URL prefixes to filter (Spring DTDs, Apple certs, Microsoft PKI, etc.)   |
| `JUNK_DOMAIN_SUFFIXES`    | 4     | Suffix patterns to filter (.png, .jpg, .gif, .jpeg)                      |

#### Output Format

**Emails** (`api.get_emails`):

```json
{
  "emails": [{ "email": "user@example.com", "start_line": 5, "end_line": 5 }]
}
```

**URLs** (`api.get_urls`):

```json
{
  "urls": [{ "url": "https://example.com", "start_line": 10, "end_line": 10 }]
}
```

Both support a `threshold` parameter (default 50, 0 = unlimited).

### Known Bugs and Issues in Python

1. **`is_private_ip` IPv6 bug** (finder.py line 493): `private(` instead of `private = (` — missing assignment, so IPv6 private detection is broken (returns `None` instead of boolean)
2. **TLD length limit**: Email regex `[A-Z]{2,4}` rejects valid TLDs longer than 4 chars (`.museum`, `.technology`, `.photography` — all valid per ICANN)
3. **FIXME comment** (finder.py line 286): `verbatim_crlf_url_cleaner` has `# FIXME: when is this possible and could happen?` — suggests the filter may be unnecessary
4. **`canonical_url` silently swallows errors** (finder.py line 413-417): `except Exception: pass` — masks potential bugs
5. **`classify` uses substring matching**: `any(d in s for d in data_set)` means `JUNK_HOSTS_AND_DOMAINS` entry `"a.b.c"` would match any URL/host containing `"a.b.c"` as a substring, not just exact matches
6. **`urlpy` dependency**: External URL parsing library used for canonicalization and host extraction — could be replaced with Rust's `url` crate
7. **No `start_line`/`end_line` distinction**: Both are always set to the same value (emails and URLs are single-line) — the distinction is vestigial
8. **Non-standard URL schemes**: `https?\\+git`, `https?\\+svn`, `https?\\+hg` are valid VCS URLs but may not be handled by standard URL parsers

### Python Dependencies

- **urlpy**: URL parsing, canonicalization, punycode, host/domain extraction
- **ipaddress**: IP address validation and classification
- **commoncode.text**: `toascii()` for Unicode→ASCII conversion
- **textcode.analysis**: `numbered_text_lines()` for reading files with line numbers

---

## Rust Architecture Design

### Design Philosophy

1. **Simple regex + filter pipeline** — same approach as Python, no NLP/grammar needed
2. **Replace urlpy with `url` crate** — Rust's standard URL parser (WHATWG URL standard)
3. **Replace ipaddress with `std::net`** — Rust stdlib has `Ipv4Addr`/`Ipv6Addr`
4. **Fix all known bugs** — TLD length, IPv6 private detection, error swallowing
5. **Thread-safe by design** — all state is immutable after initialization
6. **Compiled regex** — `regex` crate with `lazy_static!` or `OnceLock`

### High-Level Architecture

```text
┌─────────────────────────────────────────────────────────────────────┐
│                    Email/URL Detection Pipeline                      │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌──────────────────────┐    ┌──────────────────────┐              │
│  │ Email Detector        │    │ URL Detector          │              │
│  │                       │    │                       │              │
│  │ ┌───────────────────┐│    │ ┌───────────────────┐│              │
│  │ │ Regex Match       ││    │ │ Regex Match       ││              │
│  │ │ (RFC-ish email)   ││    │ │ (schemes + bare)  ││              │
│  │ └─────────┬─────────┘│    │ └─────────┬─────────┘│              │
│  │           │          │    │           │          │              │
│  │ ┌─────────▼─────────┐│    │ ┌─────────▼─────────┐│              │
│  │ │ Filter Pipeline   ││    │ │ Filter Pipeline   ││              │
│  │ │ • domain junk     ││    │ │ • crlf clean      ││              │
│  │ │ • uninteresting   ││    │ │ • trailing junk   ││              │
│  │ │ • unique          ││    │ │ • empty filter    ││              │
│  │ └─────────┬─────────┘│    │ │ • scheme add      ││              │
│  │           │          │    │ │ • user/pass strip ││              │
│  │           ▼          │    │ │ • invalid filter  ││              │
│  │  Vec<EmailDetection> │    │ │ • canonicalize    ││              │
│  └──────────────────────┘    │ │ • junk hosts      ││              │
│                              │ │ • junk URLs       ││              │
│                              │ │ • unique          ││              │
│                              │ └─────────┬─────────┘│              │
│                              │           │          │              │
│                              │           ▼          │              │
│                              │  Vec<UrlDetection>   │              │
│                              └──────────────────────┘              │
│                                                                      │
│  Shared: JunkData, HostValidator, IpClassifier                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Core Data Types

```rust
/// A detected email address with source location
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmailDetection {
    pub email: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// A detected URL with source location
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UrlDetection {
    pub url: String,
    pub start_line: usize,
    pub end_line: usize,
}

/// Configuration for detection thresholds
pub struct DetectionConfig {
    /// Maximum emails to return (0 = unlimited)
    pub max_emails: usize,
    /// Maximum URLs to return (0 = unlimited)
    pub max_urls: usize,
    /// Whether to deduplicate results
    pub unique: bool,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            max_emails: 50,
            max_urls: 50,
            unique: true,
        }
    }
}
```

### Junk Classification Data

```rust
/// Compiled junk classification data (initialized once, immutable)
pub struct JunkData {
    pub junk_emails: HashSet<&'static str>,
    pub junk_hosts_and_domains: HashSet<&'static str>,
    pub junk_ips: HashSet<&'static str>,
    pub junk_exact_domain_names: HashSet<&'static str>,
    pub junk_urls: HashSet<&'static str>,
    pub junk_url_prefixes: Vec<&'static str>,  // sorted for binary search
    pub junk_domain_suffixes: &'static [&'static str],
}
```

All junk data will be `const` or `lazy_static` — no runtime allocation.

### Key Design Decisions

#### 1. Use `url` Crate for URL Parsing (vs Python's `urlpy`)

**Python**: Uses `urlpy` for `parse()`, `sanitize()`, `punycode()`, `remove_default_port()`.

**Rust**: Use the [`url`](https://crates.io/crates/url) crate (WHATWG URL Standard implementation):

- `Url::parse()` for parsing
- `url.host()` for host extraction
- `url.domain()` for domain
- Built-in port handling, punycode, normalization

#### 2. Use `std::net` for IP Classification (vs Python's `ipaddress`)

**Python**: Uses `ipaddress.ip_address()`, `is_private`, `is_loopback`, etc.

**Rust**: `std::net::IpAddr`, `Ipv4Addr`, `Ipv6Addr` provide all the same methods:

- `is_loopback()`, `is_private()` (unstable but easy to implement), `is_multicast()`
- `is_unspecified()`, `is_link_local()` (IPv6)
- No external dependency needed

#### 3. Extended TLD Support (vs Python's 2-4 char limit)

**Python**: `[A-Z]{2,4}` — misses valid TLDs like `.museum` (6), `.technology` (10).

**Rust**: `[A-Z]{2,63}` — RFC 1035 maximum label length. This is a strict improvement.

#### 4. Filter Pipeline as Iterator Chain

**Python**: Filters are functions that take and return iterables.

**Rust**: Filters will be methods on an iterator adapter, composable via chaining:

```rust
matches
    .filter_junk_email_domains()
    .filter_uninteresting_emails()
    .unique()
    .take(config.max_emails)
    .collect()
```

This is idiomatic Rust and zero-allocation for the pipeline itself.

#### 5. `OutputURL` and `OutputEmail` Parity Fix

The existing `OutputURL` struct is missing `start_line`/`end_line` fields. Python outputs both. We need to:

- Add `start_line` and `end_line` to `OutputURL`
- Create `OutputEmail` struct with `email`, `start_line`, `end_line`
- Add `emails` field to `FileInfo`

### Module Structure

```text
src/
├── finder/
│   ├── mod.rs              # Public API: find_emails(), find_urls()
│   ├── emails.rs           # Email regex, email filter pipeline
│   ├── urls.rs             # URL regex, URL filter pipeline
│   ├── filters.rs          # Shared filter infrastructure
│   ├── junk_data.rs        # Junk classification data (const/static)
│   ├── host.rs             # Host/domain validation, IP classification
│   └── types.rs            # EmailDetection, UrlDetection, DetectionConfig
└── golden_test.rs          # Fixture-backed golden tests and related coverage
```

---

## Implementation Phases

### Phase 1: Data Types and Junk Data (1-2 days)

**Goal**: Establish types, junk classification data, and model updates.

**Deliverables:**

1. `types.rs`: `EmailDetection`, `UrlDetection`, `DetectionConfig`
2. `junk_data.rs`: All junk sets/lists from `finder_data.py` as `const`/`lazy_static`
3. Model updates:
   - Add `start_line`/`end_line` to `OutputURL`
   - Create `OutputEmail` struct
   - Add `emails: Vec<OutputEmail>` to `FileInfo`

**Testing**: Verify junk data matches Python reference (count + spot checks).

### Phase 2: Host and IP Validation (1-2 days)

**Goal**: Implement shared host/domain/IP validation logic.

**Deliverables:**

1. `host.rs`:
   - `is_good_host(host: &str) -> bool`
   - `is_private_ip(ip: IpAddr) -> bool`
   - `is_good_email_domain(email: &str) -> bool`
   - `url_host_domain(url: &str) -> Option<(String, String)>`
   - `classify_ip()`, `classify_host()`, `classify_email()`, `classify_url()`

**Key Fixes:**

- Fix IPv6 `is_private_ip` bug (Python missing assignment)
- Use `std::net::IpAddr` for proper IP parsing
- Use `url` crate for reliable host/domain extraction

**Testing**: Unit tests for all IP types (IPv4/IPv6, private/public), domain validation, email domain validation.

### Phase 3: Email Detection (1-2 days)

**Goal**: Full email detection with filter pipeline.

**Deliverables:**

1. `emails.rs`:
   - `emails_regex()` — compiled email regex
   - `find_emails(location: &Path, config: &DetectionConfig) -> Vec<EmailDetection>`
   - Filter pipeline: `junk_email_domains_filter`, `uninteresting_emails_filter`, `unique_filter`

**Key Fixes:**

- Extend TLD regex from `{2,4}` to `{2,63}`

**Testing**: Test with various email formats, junk filtering, deduplication.

### Phase 4: URL Detection (2-3 days)

**Goal**: Full URL detection with filter pipeline (more complex than email).

**Deliverables:**

1. `urls.rs`:
   - `urls_regex()` — compiled URL regex (three alternatives)
   - `find_urls(location: &Path, config: &DetectionConfig) -> Vec<UrlDetection>`
   - Full 10-step filter pipeline

2. `filters.rs`: Shared filter infrastructure

**Filter Implementation (in order):**

1. `verbatim_crlf_url_cleaner` — strip `\n`/`\r` from non-trailing-slash URLs
2. `end_of_url_cleaner` — strip trailing HTML entities, punctuation, brackets
3. `empty_urls_filter` — remove empty/stub URLs
4. `scheme_adder` — add `http://` to bare-domain URLs
5. `user_pass_cleaning_filter` — strip credentials from URL host
6. `invalid_urls_filter` — remove `scheme://[$%*/_]+` patterns
7. `canonical_url_cleaner` — normalize URL via `url` crate
8. `junk_url_hosts_filter` — remove junk host/domain URLs
9. `junk_urls_filter` — remove exact junk URLs and prefixed URLs
10. `unique_filter` — deduplicate

**Key Fixes:**

- Use `url` crate instead of `urlpy` for canonicalization (proper error handling, no silent swallowing)
- Git-style URL support (`git@github.com:user/repo.git`)
- Non-standard VCS schemes (`http+git://`, `http+svn://`, `http+hg://`)

**Testing**: Test each filter individually, then full pipeline with real-world URLs.

### Phase 5: Scanner Integration (1-2 days)

**Goal**: Wire email/URL detection into the scanner pipeline.

**Deliverables:**

1. `mod.rs`: Public API combining email + URL detection
2. Scanner integration: Call detection for each file during scanning
3. Output format: Populate `emails` and `urls` arrays in `FileInfo`
4. CLI flags: `--email`, `--url`, `--max-email INT`, `--max-url INT`

**Integration Points:**

- `src/scanner/process/pipeline.rs`: Call email/URL detection during per-file scanning
- `src/models/file_info.rs`: Use updated `OutputURL` and new `OutputEmail`
- Output JSON: Match Python ScanCode's output format exactly

**Testing**: Integration tests with real files, golden tests against Python output.

### Phase 6: Golden Tests and Polish (1 day)

**Goal**: Validate against Python reference, fix discrepancies.

**Deliverables:**

1. Golden test infrastructure for email/URL detection
2. Run against Python ScanCode's test corpus
3. Document intentional behavioral differences
4. Performance benchmarks

---

## Beyond-Parity Improvements

### 1. Fixed IPv6 Private IP Detection (Bug Fix)

**Python**: `private(` instead of `private = (` — IPv6 private detection silently returns `None`.
**Rust**: Correct implementation using `std::net::Ipv6Addr` methods.

### 2. Extended TLD Support (Bug Fix)

**Python**: Email regex `[A-Z]{2,4}` rejects TLDs > 4 chars (`.museum`, `.technology`, etc.).
**Rust**: `[A-Z]{2,63}` per RFC 1035 maximum label length.

### 3. Proper Error Handling in URL Canonicalization (Bug Fix)

**Python**: `except Exception: pass` silently swallows all errors.
**Rust**: Use `Result<T, E>` — log warnings for parse failures, don't silently drop.

### 4. Standard URL Parser (Enhancement)

**Python**: Uses `urlpy` (less maintained).
**Rust**: Uses `url` crate (WHATWG URL Standard, actively maintained, widely used in the Rust ecosystem).

### 5. Thread-Safe Design (Enhancement)

**Python**: Filter functions use global compiled regex objects (implicitly thread-safe in CPython due to GIL, but not designed for it).
**Rust**: All regex compiled once via `OnceLock`, immutable — truly `Send + Sync`.

### 6. Substring Match Fix in Junk Classification (Enhancement)

**Python**: `any(d in s for d in data_set)` uses substring matching — `"a.b.c"` in `JUNK_HOSTS_AND_DOMAINS` could match URLs containing `"a.b.c"` as a substring anywhere.
**Rust**: Consider using exact host matching for `JUNK_HOSTS_AND_DOMAINS` to prevent false positives on hosts that happen to contain a junk host as a substring.

---

## Testing Strategy

### Unit Tests

Test each component in isolation:

1. **Email regex**: Valid emails, invalid emails, edge cases (dots, hyphens, underscores)
2. **URL regex**: Scheme URLs, bare domain URLs, git-style URLs, VCS schemes
3. **Email filters**: Junk domain filtering, uninteresting email filtering, dedup
4. **URL filters**: Each of the 10 filters individually
5. **Host validation**: IPv4/IPv6 private/public, good/bad hosts, domain validation
6. **Junk classification**: Exact matches, prefix matches, suffix matches
7. **Threshold**: Verify `max_email`/`max_url` limiting
8. **IP address handling**: IPv4 private ranges, IPv6 private/reserved, edge cases

### Golden Tests

Compare output against Python ScanCode reference:

1. Run Python ScanCode on test corpus, capture JSON output
2. Run Rust implementation on same corpus
3. Compare `emails` and `urls` arrays
4. Document any intentional differences (e.g., TLD length fix may find more emails)

### Test Data

- Use existing test files from `reference/scancode-toolkit/tests/cluecode/`
- Add new test files for:
  - Long TLD emails (`.museum`, `.technology`) — beyond-parity
  - IPv6 URLs and hosts
  - Git-style URLs
  - VCS compound schemes
  - Edge cases in URL trailing character stripping
  - Unicode domain names (punycode)

### Performance Tests

- Benchmark regex matching speed
- Compare with Python ScanCode on large codebases
- Profile filter pipeline overhead

---

## Success Criteria

- [ ] Extracts valid email addresses (RFC-compliant regex)
- [ ] Extracts URLs with standard schemes (http, https, ftp, ssh, git, svn, etc.)
- [ ] Extracts bare domain URLs (`www.example.com`, `ftp.example.com`)
- [ ] Extracts git-style URLs (`git@github.com:user/repo.git`)
- [ ] Filters junk emails (example.com, test.com, localhost)
- [ ] Filters junk URLs (W3C schemas, DTDs, XML namespaces, CDN/PKI URLs)
- [ ] Filters private/reserved IP addresses
- [ ] Canonicalizes URLs (default port removal, normalization)
- [ ] Strips user/password from URLs
- [ ] Supports `--max-email` and `--max-url` thresholds
- [ ] Deduplicates results
- [ ] Output format matches Python ScanCode exactly (`email`/`url` + `start_line` + `end_line`)
- [ ] Golden tests pass against Python reference (with documented intentional differences)
- [ ] Thread-safe (no global mutable state)
- [ ] All known Python bugs are fixed
- [ ] `cargo clippy` clean, `cargo fmt` clean
- [ ] Comprehensive test coverage

---

## Related Documents

- **Architecture**: [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) — Scanner pipeline, email/URL detection section
- **Copyright Detection**: [`COPYRIGHT_DETECTION_PLAN.md`](COPYRIGHT_DETECTION_PLAN.md) — Related text detection feature
- **License Detection Architecture**: [`docs/LICENSE_DETECTION_ARCHITECTURE.md`](../../LICENSE_DETECTION_ARCHITECTURE.md) — Implemented license-detection engine and related text-detection context
- **Testing Strategy**: [`docs/TESTING_STRATEGY.md`](../../TESTING_STRATEGY.md) — Testing approach
- **Python Reference**: `reference/scancode-toolkit/src/cluecode/finder.py` — Original implementation
