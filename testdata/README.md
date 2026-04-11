# Testdata self-scan hygiene notes

Most `testdata/` fixtures are intended to be scanable by Provenant during repo self-scans.

Some fixtures are intentionally malformed negatives or lightweight placeholder parser probes. When
the goal is repository self-scan hygiene rather than negative-fixture coverage, exclude those
inputs explicitly in the scan command instead of treating their failures as scanner regressions.

Keep the policy here principled:

- valid parser and golden fixtures should stay visible to repo self-scans
- intentionally malformed negatives should document their behavior in the owning tests
- placeholder fixtures should either be upgraded to representative inputs or excluded explicitly in
  hygiene-oriented scan commands

Avoid maintaining a long fixture-by-fixture allowlist in this README. Exact exclusions belong in the
command or workflow that performs the hygiene scan.
