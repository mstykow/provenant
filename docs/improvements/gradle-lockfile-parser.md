# Gradle Lockfile Parser: Improvements Over Python

## Summary

**✨ New Feature**: Rust parses `gradle.lockfile`, a dependency surface that the Python reference does not cover.

## Reference limitation

The Python reference can parse Gradle manifests, but it does not understand `gradle.lockfile`, the text format Gradle uses to pin resolved dependency versions.

Without lockfile parsing, a scan can see declared Gradle intent but miss the exact dependency versions that were actually locked.

## Rust improvement

Rust extracts resolved dependency information from `gradle.lockfile` entries such as:

```text
com.google.guava:guava:30.1-jre=compileClasspath,runtimeClasspath
```

For each dependency, Rust can preserve:

- the Maven-style package identity
- the exact locked version
- the fact that the dependency is pinned
- the Gradle configurations that include the locked coordinate

Comments and empty lines are ignored cleanly.

## Why this matters

- **Better SBOM accuracy**: lockfiles describe the resolved versions a build actually pinned
- **Reproducibility context**: the locked dependency set is visible to downstream tooling
- **Build-context visibility**: configuration membership stays visible alongside the locked coordinates

## Reference

- [Gradle dependency locking](https://docs.gradle.org/current/userguide/dependency_locking.html)
