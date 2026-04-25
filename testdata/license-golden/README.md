# License Detection Golden Tests

This directory contains golden tests for the license detection engine, copied from Python ScanCode Toolkit.

## Test Data Source

Tests are copied from `reference/scancode-toolkit/tests/licensedcode/data/datadriven/`:

| Directory | Description | Test Count |
|-----------|-------------|------------|
| `lic1/` | Mixed license tests | ~291 |
| `lic2/` | Mixed license tests | ~340 |
| `lic3/` | Mixed license tests | ~292 |
| `lic4/` | Mixed license tests | ~345 |
| `external/` | External tool test results (recursive) | varies |
| `unknown/` | Unknown license detection | ~10 |

**Total: ~1,268 test cases**

## Test Format

Each test consists of two files:

1. **Source file**: The file to scan (e.g., `mit.c`, `apache.txt`)
2. **YAML expectation file**: Same name with `.yml` extension

Example YAML format:
```yaml
license_expressions:
  - mit
```

For multiple licenses:
```yaml
license_expressions:
  - apache-2.0
  - gpl-2.0
```

## Running Tests

```bash
# Run all license golden tests
cargo test --test license_detection_golden --features golden-tests

# Run specific suite
cargo test test_golden_lic1

# Run with output
cargo test test_golden_summary -- --nocapture
```

## Test Implementation

Tests are defined in `tests/license_detection_golden.rs`:

- `test_golden_lic1()` - Run lic1 test suite
- `test_golden_lic2()` - Run lic2 test suite  
- `test_golden_lic3()` - Run lic3 test suite
- `test_golden_lic4()` - Run lic4 test suite
- `test_golden_external()` - Run external test suite
- `test_golden_unknown()` - Run unknown test suite
- `test_golden_summary()` - Run all and print summary

## Adding New Tests

1. Add the source file to the appropriate directory
2. Create a `.yml` file with the same base name
3. Specify expected `license_expressions`
4. Run tests to verify

## Known Differences from Python

See `COMPARISON.md` for documented differences between Python and Rust detection results.
