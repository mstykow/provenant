# ADR 0004: Security-First Parsing

**Status**: Accepted  
**Authors**: Provenant team
**Supersedes**: None

> **Current contract owner**: [`../ARCHITECTURE.md`](../ARCHITECTURE.md) and [`../HOW_TO_ADD_A_PARSER.md`](../HOW_TO_ADD_A_PARSER.md) describe the live parser safety rules. This ADR records the security-first decision and threat model.

## Context

Package parsers must handle untrusted input from arbitrary sources:

- Downloaded package manifests from public repositories
- User-provided codebases during scanning
- Potentially malicious or malformed files

The Python ScanCode Toolkit has several security issues:

1. **Code Execution**: Some parsers execute user code (setup.py with `eval()`, APKBUILD shell scripts, Gemfile/Podfile Ruby execution, Gradle with Groovy engine)
2. **DoS Vulnerabilities**: No limits on file size, recursion depth, or iteration count
3. **Archive Bombs**: Zip bomb protection is incomplete or missing
4. **Memory Exhaustion**: Large manifests can exhaust memory

**Critical Question**: How do we extract package metadata safely without introducing security vulnerabilities?

## Decision

**All parsers MUST follow security-first principles: No code execution, explicit resource limits, robust input validation.**

### Security Principles

#### 1. **No Code Execution** (MANDATORY)

Parsers **NEVER** execute user-provided code, regardless of ecosystem conventions.

| ❌ FORBIDDEN              | ✅ REQUIRED                       | Example                  |
| ------------------------- | --------------------------------- | ------------------------ |
| `eval()`, `exec()`        | AST parsing                       | Python setup.py          |
| Subprocess calls          | Static analysis                   | Shell scripts (APKBUILD) |
| Ruby `instance_eval`      | Regex/AST parsing                 | Ruby Gemfile/Podfile     |
| Groovy engine             | Token-based lexer                 | Gradle build files       |
| Jinja2 template rendering | String interpolation preservation | Conan conanfile.py       |

**Rationale**:

- User code can be malicious (arbitrary code execution)
- Even benign code may have side effects (network calls, file writes)
- AST parsing provides same metadata without execution risk

#### 2. **DoS Protection** (MANDATORY)

All parsers enforce explicit limits:

| Resource            | Limit           | Enforcement              | Example                   |
| ------------------- | --------------- | ------------------------ | ------------------------- |
| **File Size**       | 100 MB default  | Check before reading     | Prevent memory exhaustion |
| **Recursion Depth** | 50 levels       | Track in parser state    | Prevent stack overflow    |
| **Iteration Count** | 100,000 items   | Break early with warning | Prevent infinite loops    |
| **String Length**   | 10 MB per field | Truncate with warning    | Prevent memory attacks    |

In practice this means parsers perform size checks before loading or recursing, bound collection/iteration work, and degrade safely with warnings or partial results instead of panicking.

#### 3. **Archive Safety** (Archives Only)

For parsers that extract archives (.deb, .rpm, .apk, .gem, .whl):

| Protection               | Implementation                                 | Threshold            |
| ------------------------ | ---------------------------------------------- | -------------------- |
| **Size Limits**          | Check uncompressed size before extraction      | 1 GB uncompressed    |
| **Compression Ratio**    | Reject excessive compression (zip bombs)       | 100:1 ratio max      |
| **Path Traversal**       | Validate extracted paths don't escape temp dir | Block `../` patterns |
| **Decompression Limits** | Stop decompression after size threshold        | 1 GB limit           |

Archive-aware parsers validate uncompressed size, compression ratio, and extracted paths before any unpacking work, and they reject or skip suspicious entries rather than trusting archive contents.

#### 4. **Input Validation** (MANDATORY)

All parsers validate input before processing:

| Validation             | Check                             | Action on Failure                      |
| ---------------------- | --------------------------------- | -------------------------------------- |
| **File Exists**        | `fs::metadata()`                  | Return error, don't panic              |
| **UTF-8 Encoding**     | `String::from_utf8()`             | Log warning, try lossy conversion      |
| **JSON/YAML Validity** | `serde_json::from_str()`          | Return default PackageData             |
| **Required Fields**    | Check `name`, `version` presence  | Populate with `None`, continue         |
| **URL Format**         | Basic validation (not exhaustive) | Accept as-is, don't parse aggressively |

Input validation follows the same pattern across parsers: fail closed on unreadable or malformed inputs, avoid panics, and preserve as much safe metadata as possible when partial parsing is still meaningful.

#### 5. **Circular Dependency Detection** (Dependency Resolution Only)

For parsers that resolve transitive dependencies:

Any parser that resolves nested or transitive relationships must track visited state and break cycles explicitly rather than assuming the input graph is acyclic.

## Consequences

### Benefits

1. **Safe by Default**
   - No arbitrary code execution risk
   - Resistant to DoS attacks
   - Protected against zip bombs

2. **Predictable Resource Usage**
   - Known upper bounds on memory/CPU
   - Won't exhaust system resources
   - Safe to run in parallel

3. **Robust Error Handling**
   - Graceful degradation on malformed input
   - Warnings instead of panics
   - Continues scanning even if one file fails

4. **Auditability**
   - Clear security boundaries
   - Easy to review for vulnerabilities
   - Documented threat model

5. **Better than Python Reference**
   - Python executes setup.py, APKBUILD, Gemfiles (UNSAFE)
   - Python has no DoS limits (VULNERABLE)
   - Python zip bomb protection incomplete (VULNERABLE)

### Trade-offs

1. **Less Dynamic**
   - Can't evaluate dynamic expressions (e.g., Python `version = get_version()`)
   - Must extract static values only
   - **Acceptable**: Metadata should be static for reproducibility

2. **Incomplete Extraction in Edge Cases**
   - Some packages use dynamic version calculation
   - Template-based manifests (Jinja2 in conanfile.py)
   - **Acceptable**: Extract what's safe, document limitations

3. **Performance Overhead**
   - File size checks add syscalls
   - Iteration counting adds overhead
   - **Acceptable**: Safety > raw speed, overhead is minimal

## Alternatives Considered

### 1. Sandboxed Execution

**Approach**: execute user code inside an isolated sandbox (Docker, seccomp, namespaces, or similar).

**Rejected because**:

- Complex infrastructure requirement (Docker daemon)
- Slower (container startup overhead)
- Still vulnerable to malicious code (resource exhaustion inside container)
- Not portable (requires Docker/system-level sandboxing)
- Doesn't solve fundamental problem (untrusted code execution)

### 2. Static Analysis Only (No Parsing)

**Approach**: use regex/heuristics instead of structured parsing.

**Rejected because**:

- Too fragile for complex formats (JSON, TOML, YAML)
- Misses edge cases (multiline strings, escaping)
- Hard to maintain (regex soup)
- Less accurate than proper parsing

### 3. Trust User Input (No Limits)

**Approach**: parse without validation or resource limits.

**Rejected because**:

- Vulnerable to DoS (large files, deeply nested structures)
- Vulnerable to zip bombs
- Vulnerable to malicious input
- Not production-ready for security-sensitive contexts

### 4. Per-Ecosystem Security Policies

**Approach**: apply different security levels to different ecosystems.

**Rejected because**:

- Inconsistent security posture
- Creates "secure" vs "insecure" parser classes
- Hard to document and reason about
- All parsers should be equally safe

## Python Reference Comparison

**Python Security Issues in Reference Implementation**:

| Issue                          | Risk                     | Our Solution     |
| ------------------------------ | ------------------------ | ---------------- |
| `exec()` in setup.py parsing   | Arbitrary code execution | AST parsing only |
| Ruby `instance_eval`           | Code execution           | Regex parsing    |
| Shell execution (APKBUILD)     | Command injection        | Static parser    |
| Groovy engine for Gradle       | Code execution           | Custom lexer     |
| No DoS limits                  | Memory exhaustion        | Explicit limits  |
| Incomplete zip bomb protection | DoS via decompression    | Full protection  |

**We significantly improve security compared to the Python reference.**

## Quality Gates

Before marking a parser complete:

- ✅ No code execution (verified by code review)
- ✅ DoS limits enforced (file size, iterations, recursion)
- ✅ Archive safety if applicable (size, compression ratio)
- ✅ Input validation with graceful degradation
- ✅ No `.unwrap()` in library code
- ✅ Security review documented (this ADR)

## Related ADRs

- [ADR 0001: Trait-Based Parser Architecture](0001-trait-based-parsers.md) - Parser structure enables security boundaries
- [ADR 0002: Extraction vs Detection Separation](0002-extraction-vs-detection.md) - Separating concerns simplifies security
- [ADR 0003: Golden Test Strategy](0003-golden-test-strategy.md) - Property-based tests for security (fuzzing, malicious input)

## References

- OWASP: [Deserialization Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Deserialization_Cheat_Sheet.html)
- Wikipedia: [Zip Bomb](https://en.wikipedia.org/wiki/Zip_bomb)
- Rust security best practices: [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)
