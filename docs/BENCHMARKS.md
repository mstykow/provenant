# Package Detection Benchmarks

This document records explicit [`compare-outputs`](../xtask/README.md#compare-outputs) runs with high-level timing metrics and notable end-state Provenant-vs-ScanCode outcomes on recorded targets.

These rows are not ad hoc performance snapshots. They are the public record of an iterative compare-review-fix-rerun loop on one concrete target at a time.

Provenant and ScanCode are run on the same repository or artifact with the maintained shared profile, the resulting deltas are reviewed to find where ScanCode is actually better, Provenant is improved with generic fixes and focused regression coverage, and the comparison is rerun until Provenant reaches parity or a justified better result on that target. Each row is therefore a maintained verification checkpoint and a snapshot of one recorded `compare-outputs` run, not a blanket claim about every scan mode, target, or future revision.

## Scan duration vs. file count

The chart below uses a log-log scatter plot: file count on the x-axis, wall-clock duration in seconds on the y-axis, and both scanners on the same numeric axes. That keeps tiny artifact snapshots and very large repository scans readable in one view without flattening the smaller runs.

![Scan duration vs. file count for Provenant and ScanCode](benchmarks/scan-duration-vs-files.svg)

> Provenant is faster on 175 of 175 recorded runs, with a **11.5× median speedup** and **10.8× geometric-mean speedup** overall; the median gap grows from **7.1×** on sub-100-file targets to **17.4×** on 10k+ file targets.
> Generated from the benchmark timing rows in this document via `cargo run --manifest-path xtask/Cargo.toml --bin generate-benchmark-chart`.

## Current benchmark examples

The quick index below links to benchmark sections. Each benchmark entry then records the snapshot size, benchmark date, machine context, raw timing comparison, and notable end-state Provenant-vs-ScanCode outcome for that target.

<!-- benchmark-quick-index:start -->

### Quick index

- **Repository-backed targets**
  - [Android / AOSP](#android--aosp)
  - [Chef](#chef)
  - [Python / Conda / Pixi](#python--conda--pixi)
  - [R / CRAN](#r--cran)
  - [Hex / Elixir / Erlang / OTP](#hex--elixir--erlang--otp)
  - [JavaScript / TypeScript / web stacks](#javascript--typescript--web-stacks)
  - [JVM / Java / Scala / Clojure](#jvm--java--scala--clojure)
  - [Rust / Go / native / infrastructure](#rust--go--native--infrastructure)
  - [Apple / Swift / Flutter / mobile](#apple--swift--flutter--mobile)
  - [.NET / NuGet / Windows / vcpkg](#net--nuget--windows--vcpkg)
  - [Ruby / PHP / Perl](#ruby--php--perl)
  - [Julia / Nix / Haskell / other ecosystems](#julia--nix--haskell--other-ecosystems)
- **Artifact/rootfs-backed targets**
  - [Linux rootfs images](#linux-rootfs-images)
  - [Installed package database snapshots](#installed-package-database-snapshots)
  - [Package archives](#package-archives)
  - [Mobile app artifacts](#mobile-app-artifacts)
  - [Release binaries and extracted app snapshots](#release-binaries-and-extracted-app-snapshots)

<!-- benchmark-quick-index:end -->

### Repository-backed targets

#### Android / AOSP

##### [aosp-mirror/platform_build @ 045a3d6](https://github.com/aosp-mirror/platform_build/tree/045a3d6a3e359633a14853a5a5e1e4f2a11cbdae) — **9.52× faster**

- Files: 1,515
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `25.23s`; ScanCode `240.24s`
- Broader Android package visibility (`14` vs `0` file-level package records) across committed Soong `METADATA`, `AndroidManifest.xml`, and `TestApp.apk` surfaces, plus extra `go.work` and Docker metadata detection, with cleaner clue-only handling of bare-word GPL/LGPL, placeholder-author, and URL-shape noise

##### [KhronosGroup/Vulkan-ValidationLayers @ d72c5f5](https://github.com/KhronosGroup/Vulkan-ValidationLayers/tree/d72c5f52886913598d4064fe8d03bf8ac471e215) — **17.97× faster**

- Files: 979
- Run context: 2026-04-21 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `38.32s`; ScanCode `688.72s`
- Direct AndroidManifest package visibility (`1` vs `0` on `tests/android/AndroidManifest.xml`), clue-only weak GPL handling across Graphics Pipeline Library acronym sites instead of ScanCode's hard `GPL-1.0-or-later` detections, and cleaner Khronos documentation copyright or holder recovery without appended `- ! Khronos Vulkan` noise

#### Chef

##### [chef/chef @ 0e353ff](https://github.com/chef/chef/tree/0e353ffcc8c03ac5b57025081787913121c785d5) — **12.07× faster**

- Files: 2,274
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `30.70s`; ScanCode `370.51s`
- Richer mixed-surface package identity with fewer placeholder-only Debian rows and far broader dependency extraction (`351` vs `278`) across `Gemfile`, `Gemfile.lock`, `chef-*/Gemfile`, gemspec, Dockerfile, and fixture archive/control surfaces, plus email-preserving author normalization and cleaner placeholder-holder filtering

##### [sous-chefs/apache2 @ 420d824](https://github.com/sous-chefs/apache2/tree/420d82402811a131729a6bcc80aaac08d307ac87) — **7.27× faster**

- Files: 246
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `12.01s`; ScanCode `87.31s`
- Matched Chef package and dependency coverage on committed `metadata.rb` surfaces, with fuller Debian-style script-header author capture and cleaner rejection of weak README maintainer prose as an author

##### [sous-chefs/mysql @ 6b7110b](https://github.com/sous-chefs/mysql/tree/6b7110bee2bc64c9149f24d524cbb740387e527a) — **6.45× faster**

- Files: 92
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `11.72s`; ScanCode `75.55s`
- Matched Chef package and dependency coverage on committed `metadata.rb` surfaces, with cleaner rejection of config-word author noise such as `chef-client` and fuller `Author:: Name (<email>)` identity capture

#### Python / Conda / Pixi

##### [aboutcode-org/dejacode @ 4938cd4](https://github.com/aboutcode-org/dejacode/tree/4938cd4f28aec23afe6b88c4443e573c2db930ea) — **11.14× faster**

- Files: 1,278
- Run context: 2026-04-24 · dejacode-80604 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `24.48s`; ScanCode `272.67s`
- Broader ABOUT, Python, wheel, and Docker package visibility (`126` vs `1` packages, `117` vs `104` dependencies) across committed `.ABOUT` sidecars, bundled `thirdparty/dist/*.whl` artifacts, and product manifests, with real ecosystem PURLs derived from `download_url` metadata instead of fallback `pkg:about/...` identities

##### [aboutcode-org/scancode.io @ 904373a](https://github.com/aboutcode-org/scancode.io/tree/904373abf472e0567a99a3b1b5213e084040b5c1) — **9.16× faster**

- Files: 764
- Run context: 2026-04-24 · scancode.io-63382 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `42.96s`; ScanCode `393.40s`
- Broader ABOUT and Python package visibility (`25` vs `1` packages, `284` vs `56` dependencies) across committed `.ABOUT` files, root and suffixed `pyproject.toml` manifests, and `uv.lock`, plus zero scan-file errors where ScanCode times out on large generated scan-result JSON fixtures

##### [aboutcode-org/scancode-toolkit @ 6570c13](https://github.com/aboutcode-org/scancode-toolkit/tree/6570c131e2821388286f661368a70e0120aaf2c6) — **13.48× faster**

- Files: 64,369
- Run context: 2026-04-25 · scancode-toolkit-41061 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `535.40s`; ScanCode `7214.43s`
- Far broader ABOUT-adjacent package and dependency visibility (`1281` vs `6` packages, `10943` vs `377` dependencies) across committed `.ABOUT` sidecars, Python/Swift/Dart/CocoaPods fixture manifests, and bounded RPM header metadata recovery, with real ecosystem PURLs derived from ABOUT `download_url` metadata instead of `pkg:about/...` fallbacks and zero scan-file errors where ScanCode times out on heavy fixture snapshots; the remaining ScanCode edge is concentrated in a small set of license-detection corpus and legal-text cases where it still preserves extra detections beyond Provenant’s current policy or refinement choices

##### [apache/airflow @ 47ce5f3](https://github.com/apache/airflow/tree/47ce5f32b4fae95f5865ba256d409c778d53a3d5) — **14.33× faster**

- Files: 11,854
- Run context: 2026-04-11 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `65.32s`; ScanCode `936.34s`
- Far broader Python/provider package coverage (`142` vs `1`) and dependency extraction (`7579` vs `450`) from `uv.lock`, provider `pyproject.toml`, and committed `pnpm-lock.yaml` inputs, plus extra Docker and Helm package visibility, safer URL credential stripping, and cleaner copyright/author normalization across large documentation and kernel-style metadata blocks

##### [astral-sh/uv @ 9581f2b](https://github.com/astral-sh/uv/tree/9581f2b0ea65550a3efe28bd7aabde19d98b39ba) — **17.90× faster**

- Files: 1,259
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `20.79s`; ScanCode `372.18s`
- Far broader Python-family package and dependency extraction (`112` vs `1` packages, `5277` vs `759` dependencies) from the large `test/requirements/**` tree, many fixture/workspace `pyproject.toml` files, and multiple `uv.lock` inputs that ScanCode leaves at zero, with safer URL credential stripping, Unicode-preserving party normalization, and METADATA-backed wheel identity instead of double-counting a misleading filename

##### [astropy/astropy @ 40280e3](https://github.com/astropy/astropy/tree/40280e3bd715a4968eda816c73bf88f05aa6cdc0) — **22.04× faster**

- Files: 1,970
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `24.26s`; ScanCode `534.66s`
- Direct `CITATION.cff` package visibility on the root citation metadata (`1` vs `0` on that file), plus far broader Python dependency extraction (`79` vs `1`) from `pyproject.toml` and `docs/rtd_environment.yaml`, with cleaner vendored holder recovery and Unicode-preserving copyright normalization

##### [conda/conda @ 37549c4](https://github.com/conda/conda/tree/37549c41a1925b0625e346e2823a5e15af03b862) — **11.70× faster**

- Files: 285
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.05s`; ScanCode `117.64s`
- Broader Conda and Python package coverage (`5` vs `2` packages, `73` vs `26` dependencies) from `conda.recipe/meta.yaml`, multiple `environment.yml` fixtures, and the root `setup.py`, with safer URL credential stripping across authentication test fixtures

##### [conda/conda-build @ 5da509d](https://github.com/conda/conda-build/tree/5da509d13764d96c02c80f24b54ab87d652b2538) — **5.73× faster**

- Files: 835
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `17.96s`; ScanCode `102.91s`
- Far broader Conda recipe and dependency extraction (`257` vs `1` packages, `164` vs `13` dependencies) across committed `meta.yaml` recipe fixtures, split-package test recipes, and sidecar Python manifests, with explicit malformed-recipe scan errors on duplicate-key negative fixtures instead of silently treating them as ordinary package metadata

##### [conda-forge/pandas-feedstock @ 4063b72](https://github.com/conda-forge/pandas-feedstock/tree/4063b725cd252c02b0cebe935a8859a6b540fe00) — **7.59× faster**

- Files: 51
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `8.66s`; ScanCode `65.66s`
- Direct schema-versioned conda-forge feedstock package visibility (`1` vs `0` packages, `51` vs `0` dependencies) from `recipe/recipe.yaml`, plus assembled top-level Conda package identity and preserved source/about metadata

##### [DefectDojo/django-DefectDojo @ 2f25c45](https://github.com/DefectDojo/django-DefectDojo/tree/2f25c4510361e2f27f63fbbcff3901cbd2ef4a07) — **18.83× faster**

- Files: 4,301
- Run context: 2026-04-16 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `78.26s`; ScanCode `1473.92s`
- Broader full-repo package and dependency extraction (`3` vs `2` packages, `616` vs `535` dependencies) from `.gitmodules`, `helm/defectdojo/Chart.yaml`, `helm/defectdojo/Chart.lock`, and the root `requirements*.txt` manifests, with direct Helm chart package visibility, pinned PostgreSQL or Valkey chart dependencies, Git-submodule package metadata, and zero scan errors where ScanCode reports 3 scan-file failures on large vulnerability fixtures

##### [django/django @ 09f27cc](https://github.com/django/django/tree/09f27cc373eb1e6e5e8b286204809a79b61d55c3) — **12.03× faster**

- Files: 6,994
- Run context: 2026-04-09 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `29.74s`; ScanCode `357.65s`
- Far broader Python-family package and dependency extraction (`2` vs `1` packages, `16` vs `6` dependencies) because `pyproject.toml` contributes both a real PyPI root package and 5 Python dependencies while `docs/requirements.txt` adds 5 more documentation dependencies that ScanCode leaves at zero, with clearer `BSD-3-Clause` declared-license capture and visibility into the vendored CVS marker that ScanCode skips

##### [OpenMDAO/OpenMDAO @ bf1fcb6](https://github.com/OpenMDAO/OpenMDAO/tree/bf1fcb6f09a07a49cdba27c2fd765153ec54694c) — **16.66× faster**

- Files: 1,199
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `17.94s`; ScanCode `298.91s`
- Broader Pixi, Julia, and Docker package visibility (`3` vs `1` packages, `1489` vs `76` dependencies) from the root `pixi.toml`, resolved `pixi.lock`, and the experimental Julia `Project.toml`, with no `pixi.lock` scan errors where ScanCode times out and much richer lockfile license visibility

##### [pandas-dev/pandas @ c385d01](https://github.com/pandas-dev/pandas/tree/c385d0188cbfb2294fb6362ec24b514b211c7fb1) — **7.59× faster**

- Files: 2,608
- Run context: 2026-04-09 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `35.66s`; ScanCode `270.61s`
- Far broader Python/Conda/Pixi package and dependency extraction (`4` vs `1` packages, `3242` vs `251` dependencies) because `environment.yml` contributes a large resolved Conda environment, `pixi.toml` and current YAML `pixi.lock` surface an additional Pixi package graph, and `ci/meta.yaml` adds Conda recipe dependencies and package metadata beyond the root `pyproject.toml` package, while avoiding ScanCode's `pixi.lock` timeout and preserving clearer `BSD-3-Clause` declared-license capture on the Conda recipe metadata

##### [prefix-dev/pixi @ 6458b15](https://github.com/prefix-dev/pixi/tree/6458b15a855cf6beeaad1853ef007d9d20a5bccc) — **8.09× faster**

- Files: 2,372
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `62.21s`; ScanCode `503.12s`
- Broader Pixi package and dependency extraction (`223` vs `128` packages, `18016` vs `3116` dependencies) from the root and example `pixi.toml` or `pixi.lock` surfaces plus feature-scoped `pypi-dependencies`, with no example-lock scan errors where ScanCode times out and safer credential stripping or git URL normalization across Pixi source fixtures

##### [pydata/xarray @ f7e47a1](https://github.com/pydata/xarray/tree/f7e47a19726321e56d74bca896eb55c6f330506b) — **14.48× faster**

- Files: 429
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `12.89s`; ScanCode `186.62s`
- Broader Pixi and Conda environment coverage (`3` vs `1` packages, `509` vs `84` dependencies) from the repo-root `pixi.toml` plus committed Binder and CI environment manifests, with direct Pixi package identity and cleaner URL normalization across docs and SVG metadata

##### [python-poetry/poetry @ bfce511](https://github.com/python-poetry/poetry/tree/bfce5118814fa95445e823cb07a59bd77ffe1474) — **4.20× faster**

- Files: 987
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `20.09s`; ScanCode `84.36s`
- Far broader Python package and dependency extraction (`124` vs `16` packages, `531` vs `91` dependencies) from the root PEP 621 `pyproject.toml`, Poetry dependency groups, committed `poetry.lock` fixtures, and bundled wheel/sdist metadata, plus safer URL credential stripping and Unicode-preserving party normalization across repository docs and test fixtures

##### [scipy/scipy @ 8a4633f](https://github.com/scipy/scipy/tree/8a4633fa0e01d62e9ccdd06ebe5bb30551cfa056) — **14.10× faster**

- Files: 2,998
- Run context: 2026-04-09 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `23.57s`; ScanCode `332.23s`
- Far broader Python/Conda/Pixi package and dependency extraction (`4` vs `1` packages, `1469` vs `78` dependencies) from `pixi.lock`'s large resolved Conda graph, `environment.yml`, `pixi.toml`, and the aggregated `requirements/*.txt` tree that ScanCode leaves at zero, with cleaner `pyproject.toml` requirement shaping for exact pins and environment markers

#### R / CRAN

##### [r-lib/devtools @ a3447b9](https://github.com/r-lib/devtools/tree/a3447b9f3d59abb6cc8b63a54db3435819324c1e) — **8.71× faster**

- Files: 266
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.28s`; ScanCode `80.85s`
- Far broader CRAN package and dependency extraction (`14` vs `1` packages, `45` vs `1` dependencies) from the root `DESCRIPTION` plus committed test-package fixtures, with correct filtering of fake `pkg:cran/R` dependency noise and cleaner maintainer or URL normalization

##### [tidyverse/dplyr @ 2f9f49e](https://github.com/tidyverse/dplyr/tree/2f9f49ef0d361dc612abc55982d68db3fb3854d0) — **12.32× faster**

- Files: 462
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `13.86s`; ScanCode `170.71s`
- Direct CRAN package visibility on the root `DESCRIPTION` plus declared dependency extraction (`29` vs `0`) across `Depends`, `Imports`, `Suggests`, `Enhances`, and `LinkingTo`, with cleaner Rd or markdown URL normalization and preserved shipped license-holder metadata

##### [tidyverse/ggplot2 @ 7d79c95](https://github.com/tidyverse/ggplot2/tree/7d79c956b5707cb7c762d834caf842dc6496b032) — **12.33× faster**

- Files: 1,154
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `14.46s`; ScanCode `178.35s`
- Direct CRAN package visibility on the root `DESCRIPTION` plus declared dependency extraction (`41` vs `0`) across `Imports`, `Suggests`, and `Enhances`, with correct hyphenated CRAN version constraints such as `sf (>= 0.7-3)` and cleaner Rd or roxygen URL recovery

#### Hex / Elixir / Erlang / OTP

##### [elixir-ecto/ecto @ 28d9282](https://github.com/elixir-ecto/ecto/tree/28d928267388018d5b0bb1f83e04368b7e8cae50) — **9.66× faster**

- Files: 156
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `14.03s`; ScanCode `135.56s`
- Broader Hex dependency extraction (`16` vs `0`) from the repo-root `mix.lock` plus `examples/friends/mix.lock`, with direct locked package identities for entries such as `ecto_sql`, `postgrex`, and `telemetry` that ScanCode leaves dependency-blind

##### [elixir-plug/plug @ 47649aa](https://github.com/elixir-plug/plug/tree/47649aa7bb910f481b66cc3e98c14b2c3b761c3c) — **8.55× faster**

- Files: 104
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `10.77s`; ScanCode `92.08s`
- Direct Hex package visibility on `mix.lock` (`1` vs `0`) plus locked dependency extraction (`9` vs `0`) for `plug_crypto`, `telemetry`, `ex_doc`, and sibling Hex pins that ScanCode leaves at zero, with Unicode-preserving `Loïc Hoguin` holder normalization

##### [erlang/otp @ 264def5](https://github.com/erlang/otp/tree/264def545b8214ea7100bfede1a4629c676ff1c0) — **23.52× faster**

- Files: 11,749
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `135.93s`; ScanCode `3197.26s`
- Direct OTP application package visibility (`11` vs `0`) across committed `lib/*/src/*.app.src` templates, with bounded `%PLACEHOLDER%` handling that keeps canonical manifests such as `diameter.app.src` scannable and preserves the same non-stdlib runtime dependency inventory ScanCode finds

##### [phoenixframework/phoenix @ e7b8081](https://github.com/phoenixframework/phoenix/tree/e7b8081792fa51c9fede6d0fb9ddb610bac3f26f) — **11.66× faster**

- Files: 476
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `12.80s`; ScanCode `149.17s`
- Direct Hex package visibility on the repo-root, `installer/mix.lock`, and `integration_test/mix.lock` surfaces (`3` vs `0` file-level package records), while preserving top-level package and dependency parity elsewhere and preserving structured npm party metadata

##### [processone/ejabberd @ 87475d8](https://github.com/processone/ejabberd/tree/87475d813b974492f338720eab5c9c3d4646a4ce) — **12.80× faster**

- Files: 623
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `16.74s`; ScanCode `214.30s`
- Broader Erlang/Rebar package and dependency extraction (`2` vs `1` packages, `43` vs `3` dependencies) from the root `rebar.config`, `rebar.lock`, nested `_checkouts/configure_deps` manifests, and committed Dockerfiles, with the bundled `priv/mod_invites/copyright` notice kept as clue-level license evidence instead of being overstated as Debian package metadata

##### [vernemq/vernemq @ 4681e54](https://github.com/vernemq/vernemq/tree/4681e5490cc42e6cc26a504bb4b3c5413315c21f) — **10.74× faster**

- Files: 441
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `13.90s`; ScanCode `149.29s`
- Broader Erlang/Rebar dependency extraction (`119` vs `0`) from the repo-root and per-app `rebar.config` / `.app.src` manifests, plus direct `.gitmodules` package visibility and mixed Hex or git package identity across the VerneMQ app tree where ScanCode stays manifest-blind

#### JavaScript / TypeScript / web stacks

##### [appsmithorg/appsmith @ 6ca79d1](https://github.com/appsmithorg/appsmith/tree/6ca79d1de1fa63ead9bcaed2d7509b309aa6825b) — **14.79× faster**

- Files: 13,366
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `59.00s`; ScanCode `872.68s`
- Direct Helm chart package visibility on `deploy/helm/Chart.yaml` (`1` vs `0`) with declared dependency extraction (`4` vs `0`) for the pinned MongoDB, PostgreSQL, Prometheus, and Redis chart inputs that ScanCode leaves unmodeled

##### [baserow/baserow @ 18a5fc1](https://github.com/baserow/baserow/tree/18a5fc1fbf60666dc2509872efee5e8fa6ff750f) — **32.06× faster**

- Files: 8,755
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `23.76s`; ScanCode `761.82s`
- Direct Helm package visibility on `deploy/helm/baserow/Chart.yaml` and `Chart.lock` (`2` file-level Helm surfaces vs `0`), with declared plus locked dependency extraction (`12` vs `0` on each chart file) covering sibling `baserow-common` aliases and the pinned Bitnami/Caddy chart inputs that ScanCode leaves at zero

##### [denoland/fresh @ 49c4be1](https://github.com/denoland/fresh/tree/49c4be1ac60603174bad1c6e3c13bd88602c51bb) — **11.69× faster**

- Files: 567
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `13.32s`; ScanCode `155.71s`
- Broader Deno package and dependency extraction (`8` vs `0` packages, `966` vs `0` dependencies) from the root `deno.json`, `deno.lock`, and nested `packages/*/deno.json` manifests, with direct JSR and npm import-map or lockfile package identity where ScanCode stays manifest-blind

##### [denoland/std @ a864f62](https://github.com/denoland/std/tree/a864f62bcc8a5f20716d2becab3cfe224a2ad810) — **24.22× faster**

- Files: 2,812
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `16.30s`; ScanCode `394.76s`
- Broader Deno package visibility (`45` vs `3` packages) from the root and leaf `*/deno.json` manifests across the standard-library tree, plus concrete Cargo lock package identities on embedded Rust fixtures instead of anonymous `cargo_lock` rows, with zero top-level license-expression deltas under the shared profile

##### [getsentry/self-hosted @ 8728919](https://github.com/getsentry/self-hosted/tree/8728919e080836c53724f277d4d36cc310fc5011) — **6.50× faster**

- Files: 129
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `12.14s`; ScanCode `78.89s`
- Broader mixed Docker/npm/Python package extraction (`2` vs `1` packages, `111` vs `0` dependencies) from the integration-test `package-lock.json`, `uv.lock`, and committed service Dockerfiles, plus the more specific `Apache-2.0 AND FSL-1.1-ALv2` license classification on `LICENSE.md` where ScanCode reports only `FSL-1.1-ALv2`

##### [iTowns/itowns @ 08e08f5](https://github.com/iTowns/itowns/tree/08e08f512983b6f3d60d04d431b67b3c5e2e1584) — **13.58× faster**

- Files: 616
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `12.53s`; ScanCode `170.19s`
- Direct `publiccode.yml` package visibility on the root metadata file (`1` vs `0` on that file), with matched top-level package and dependency counts elsewhere plus Unicode-preserving Potree copyright normalization and cleaner URL shaping across README and docs material

##### [jashkenas/backbone @ da75718](https://github.com/jashkenas/backbone/tree/da75718e896e52e84aa1f0411ba67fafcdcf6af3) — **9.28× faster**

- Files: 122
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `11.27s`; ScanCode `104.56s`
- Matched Bower package and dependency coverage on the repo-root `bower.json`, with datasource-tagged Bower package identity instead of a bare purl-only row and package-level party metadata from `package.json`

##### [jquery/jquery-ui @ eda7aa3](https://github.com/jquery/jquery-ui/tree/eda7aa34fa59d8f764b2164be3e3b7f14639b0db) — **19.49× faster**

- Files: 1,083
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `15.56s`; ScanCode `303.29s`
- Matched Bower package and dependency coverage on the repo-root `bower.json`, with datasource-tagged Bower package identity instead of a bare purl-only row and cleaner Unicode-preserving author normalization across locale files and vendored docs

##### [metabase/metabase @ 10997b1](https://github.com/metabase/metabase/tree/10997b10908414ab05773b085a56a37fcdebcd1a) — **25.67× faster**

- Files: 18,030
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `51.84s`; ScanCode `1330.92s`
- Broader package and dependency extraction (`8` vs `1` packages, `1436` vs `423` dependencies) from the root and driver `deps.edn` manifests plus committed `bun.lock` and `uv.lock`, with cleaner OFL font URL normalization where ScanCode preserves broken concatenated links

##### [microsoft/vscode @ 0c1e100](https://github.com/microsoft/vscode/tree/0c1e100626c19724d1222c2bc4b63ba3556858a7) — **23.92× faster**

- Files: 14,398
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `58.96s`; ScanCode `1410.57s`
- Broader monorepo package and dependency extraction (`138` vs `1` packages, `7718` vs `1815` dependencies) from the root `package-lock.json`, many extension fixture manifests and lockfiles, and embedded Cargo/Docker metadata, plus richer named package identities where ScanCode emits generic lockfile and archive rows

##### [npm/cli @ 05dbba5](https://github.com/npm/cli/tree/05dbba5b8d727ddb2c098ce0553714eae791c5f2) — **11.44× faster**

- Files: 6,698
- Run context: 2026-04-09 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `295.10s`; ScanCode `3376.85s`
- Clean root npm workspace manifest coverage without ScanCode's workspace-assembly scan errors, fewer large registry-fixture JSON timeouts, and cleaner handling of duplicated private-workspace dependency exports and repeated MIT-style registry-fixture metadata noise

##### [oakserver/oak @ 185baef](https://github.com/oakserver/oak/tree/185baef02551a84798000f25d3bd01c2fdfcb1ce) — **8.94× faster**

- Files: 103
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `12.95s`; ScanCode `115.73s`
- Direct Deno package visibility on the root `deno.json` (`1` vs `0` packages), plus Dockerfile package visibility on `.devcontainer/Dockerfile`, with cleaner trailing-slash URL normalization across README and docs material

##### [oven-sh/bun @ 700fc11](https://github.com/oven-sh/bun/tree/700fc117a2fd01ac0201deaa6fa69c5557acb04f) — **19.72× faster**

- Files: 12,551
- Run context: 2026-04-09 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `43.05s`; ScanCode `849.10s`
- Far broader Bun/npm-family package extraction (`382` vs `29` packages, `5773` vs `323` dependencies) from the repo's 52 committed `bun.lock` / `bun.lockb` inputs that ScanCode leaves at zero, plus legacy `bun.lockb` coverage on `bench/bundle` and plainer `BSD-2-Clause` rebucketing where ScanCode uses the over-specific `BSD-2-Clause-Views` label

##### [renovatebot/renovate @ 91a7213](https://github.com/renovatebot/renovate/tree/91a72131e8aefcda8f0dab7499f378f7eb41300f) — **18.82× faster**

- Files: 3,663
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `23.74s`; ScanCode `446.79s`
- Broader fixture-heavy package and dependency extraction (`52` vs `1` packages, `1778` vs `1485` dependencies) from committed `project.clj`, `deps.edn`, and cross-ecosystem manager fixtures, plus Leiningen package identity on `lib/modules/manager/leiningen/__fixtures__/project.clj` where ScanCode stays manifest-blind

##### [select2/select2 @ 595494a](https://github.com/select2/select2/tree/595494a72fee67b0a61c64701cbb72e3121f97b9) — **11.63× faster**

- Files: 704
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `12.57s`; ScanCode `146.24s`
- Matched Bower package and dependency coverage on the repo-root `bower.json`, with datasource-tagged Bower package identity instead of a bare purl-only row and cleaner package-author normalization in `package.json`

##### [vercel/next.js @ 8e5a36f](https://github.com/vercel/next.js/tree/8e5a36f6347528d8968da97262f372f908897bac) — **20.68× faster**

- Files: 28,044
- Run context: 2026-04-11 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `41.11s`; ScanCode `850.20s`
- Broader monorepo package and dependency extraction (`464` vs `249` packages, `13787` vs `12017` dependencies) from the root `pnpm-lock.yaml`, many workspace fixture subtrees, and embedded Cargo/npm metadata, plus zero scan errors where ScanCode crashes on workspace `package.json` and `pnpm-lock.yaml` inputs

##### [yarnpkg/berry @ c0274d6](https://github.com/yarnpkg/berry/tree/c0274d6d7ba5939f447e78aaf16e456a00cf0bd1) — **8.20× faster**

- Files: 3,552
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `23.75s`; ScanCode `194.82s`
- Broader dependency extraction (`2835` vs `1301`) from Berry `yarn.lock`, workspace manifests, and `.pnp.cjs`, plus cleaner workspace package assembly that avoids ScanCode's duplicated npm package rows (`204` vs `395`) and `package.json` / `yarn.lock` assembly crashes while still surfacing extra Docker and Windows package inputs committed in the tree

#### JVM / Java / Scala / Clojure

##### [akka/akka @ 5ace141](https://github.com/akka/akka/tree/5ace141e1c80a9f832430ee3ab7ff4fb3b581c40) — **25.26× faster**

- Files: 4,623
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `26.97s`; ScanCode `681.19s`
- Matched top-level SBT package coverage (`7` vs `7`) with broader dependency extraction (`49` vs `40`) from the root `build.sbt`, sample applications, and native-image test manifests, plus cleaner rejection of weak actor-name author noise such as `the ActorSystem` and `the ReceiveBuilder`

##### [apache/felix-dev @ 20aee77](https://github.com/apache/felix-dev/tree/20aee77cce8cad21493368403701d9c44c168f62) — **9.09× faster**

- Files: 5,354
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `52.75s`; ScanCode `479.56s`
- Matched Maven/OSGi package coverage (`196` vs `196`) with richer dependency extraction (`995` vs `962`) from classifier/type-aware Maven coordinates, OSGi integration-test POMs, and committed JAR or `MANIFEST.MF` metadata

##### [apache/camel @ c9c34a1](https://github.com/apache/camel/tree/c9c34a1c2fbc5d093241565c0272ca466407a8e1) — **11.38× faster**

- Files: 36,792
- Run context: 2026-04-26 · camel-80585 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `469.31s`; ScanCode `5338.67s`
- Broader Maven dependency extraction (`14818` vs `7645`) from the large multi-module reactor, archetype template POMs, and mixed package-adjacent Helm, Docker, and Cargo surfaces, plus restored UTF-16 template license detection and broader notice-author recovery across Apache/Spring/OpenShift acknowledgements, with zero scan-file errors where ScanCode times out on the committed `camel-sbom.json` and `camel-sbom.xml`

##### [apache/kafka @ 0d9fe51](https://github.com/apache/kafka/tree/0d9fe518b616725fecd96162297fee89a7b7a6a5) — **14.02× faster**

- Files: 7,179
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `53.61s`; ScanCode `751.77s`
- Far broader Gradle and sidecar Python package extraction (`6` vs `4` packages, `662` vs `15` dependencies) from the root multi-project `build.gradle`, Kafka module wiring, and the committed `tests/setup.py`, plus extra Docker package visibility on the bundled image fixtures

##### [apache/maven @ 459de76](https://github.com/apache/maven/tree/459de765537854376dd499e931ab87e1d53f9c23) — **10.94× faster**

- Files: 9,688
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `49.40s`; ScanCode `540.33s`
- Almost identical Maven package coverage (`2516` vs `2518`) with much richer dependency extraction (`5032` vs `2267`) from parent/module inheritance, `dependencyManagement`, and committed `.pom` fixtures, plus more specific classifier-bearing Maven identities where ScanCode flattens coordinates and quieter unresolved-placeholder handling that preserves Maven semantics without flooding the scan with property/cycle noise

##### [elastic/elasticsearch @ a414f3d](https://github.com/elastic/elasticsearch/tree/a414f3d06c7ab59a5a0b350e80e5674bf9864688) — **32.25× faster**

- Files: 40,293
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `146.56s`; ScanCode `4726.52s`
- Matched top-level package coverage (`1` vs `1`) with richer dependency extraction (`2378` vs `2067`) from the large multi-project Gradle build graph, plus extra Docker package visibility on committed fixture and distribution Dockerfiles

##### [gradle/gradle @ 92068b4](https://github.com/gradle/gradle/tree/92068b4fd4e6f3689b5164d9bf7f3b7c97bc4f4e) — **12.96× faster**

- Files: 27,912
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `145.05s`; ScanCode `1879.67s`
- Broader Gradle package and dependency extraction (`73` vs `68` packages, `1675` vs `1541` dependencies) from committed `build.gradle`, `build.gradle.kts`, `gradle.lockfile`, and `.module` metadata across docs and test fixtures

##### [playframework/playframework @ c2c114f](https://github.com/playframework/playframework/tree/c2c114ff31eff1557bef65cc3f586fbc53c974a6) — **18.23× faster**

- Files: 2,579
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `14.94s`; ScanCode `272.30s`
- Broader SBT dependency extraction (`7` vs `3`) and file-level SBT package visibility across the root build and committed `play-sbt-plugin` fixture projects, plus correct no-year copyright and holder recovery on vendored jQuery banners that ScanCode-only parity previously exposed

##### [scalatest/scalatest @ f6ba8f2](https://github.com/scalatest/scalatest/tree/f6ba8f25999f240831362cd7498ba5beee7dc375) — **17.92× faster**

- Files: 1,935
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `32.53s`; ScanCode `582.97s`
- Broader file-level SBT package visibility on `build.sbt` and `project/build.sbt`, with declared dependency extraction from `project/build.sbt` and correct copyright recovery from XML-attribute notices in the legacy `build.xml` ant workflow

##### [spring-projects/spring-boot @ 53827d4](https://github.com/spring-projects/spring-boot/tree/53827d47d0802670fd53b665643aef8af4fe7bc8) — **11.49× faster**

- Files: 11,610
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `67.58s`; ScanCode `776.24s`
- Broader JVM monorepo package and dependency extraction (`173` vs `165` packages, `4434` vs `4233` dependencies) from nested Maven example POMs, the committed Antora `package-lock.json`, and Docker/WAR metadata, plus more specific SBOM license expressions where ScanCode flattens `EPL-2.0 AND Classpath-exception-2.0` or `BSD-2-Clause-Views AND BSD-3-Clause`

##### [technomancy/leiningen @ 4022732](https://github.com/technomancy/leiningen/tree/40227328d4a9c8945362d6d626d19c2449175df6) — **8.90× faster**

- Files: 302
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.34s`; ScanCode `91.99s`
- Broader Clojure manifest and dependency extraction (`82` vs `10` dependencies) from the root, nested checkout, and test-project `project.clj` surfaces that ScanCode leaves at manifest-only visibility, plus OFL font-license recovery and cleaner URL normalization where ScanCode preserves regex suffixes, trailing-slash drift, or percent-encoded placeholder text

#### Rust / Go / native / infrastructure

##### [alpinelinux/aports @ d6ebad7](https://github.com/alpinelinux/aports/tree/d6ebad7b4d949b16634e6c5be202ccafbb1b9658) — **18.56× faster**

- Files: 23,293
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `96.91s`; ScanCode `1798.33s`
- Broader Alpine package visibility (`12609` vs `12502`) and dependency extraction (`102257` vs `1438`) from committed `APKBUILD` metadata plus nested Cargo and Docker surfaces, with static shell-style manifest handling that preserves concrete package identities instead of malformed placeholder expansions

##### [archlinux/packaging/packages/grep @ 29d2e10](https://gitlab.archlinux.org/archlinux/packaging/packages/grep/-/tree/29d2e1085e3c69ded524b8fae3b436f10f801ed0) — **7.13× faster**

- Files: 6
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.27s`; ScanCode `73.20s`
- Direct Arch source-package visibility on committed `.SRCINFO` (`1` vs `0` file-level package records) with broader dependency extraction (`9` vs `0`) across runtime, make, and check edges, plus Unicode-preserving maintainer recovery and exact trailing-slash URL normalization on `PKGBUILD` while avoiding ScanCode's low-coverage `LGPL-2.0-or-later` false positive

##### [archlinux/packaging/packages/pacman @ 4ee8983](https://gitlab.archlinux.org/archlinux/packaging/packages/pacman/-/tree/4ee8983653633d6fad7b2b9d6c35027c9705de5d) — **6.64× faster**

- Files: 12
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `11.49s`; ScanCode `76.28s`
- Direct Arch source-package visibility on committed `.SRCINFO` (`1` vs `0` file-level package records) with broader dependency extraction (`26` vs `0`) across runtime, make, check, and optional package metadata, plus copyright and holder recovery on the repo-owned `LICENSE` and `REUSE.toml` surfaces that ScanCode leaves empty

##### [bazelbuild/bazel @ eb5aeaa](https://github.com/bazelbuild/bazel/tree/eb5aeaaa23d52601a2aca11ff6fd1a74ea97f0d6) — **9.83× faster**

- Files: 11,496
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `200.80s`; ScanCode `1974.56s`
- Broader Bazel package and dependency extraction (`1729` vs `1711` packages, `79` vs `14` dependencies) from root and nested `BUILD` files plus direct `MODULE.bazel` dependency visibility, with richer Debian and RPM sidecar package metadata

##### [boostorg/boost @ 4f1cbeb](https://github.com/boostorg/boost/tree/4f1cbeb724d9f3c08a826fbcee5a3db2f5480441) — **4.98× faster**

- Files: 236
- Run context: 2026-04-10 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `14.29s`; ScanCode `71.17s`
- Cleaner XML author extraction without ScanCode's prose-tainted suffixes such as `A.Meredith Compiler`, while still recovering real names like `Jeremy Siek` and `David Goodger` that ScanCode misses

##### [boostorg/json @ 70efd4b](https://github.com/boostorg/json/tree/70efd4b032b7f3e718bb4ca4ae144c3171b21568) — **8.46× faster**

- Files: 705
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `28.27s`; ScanCode `239.21s`
- Cleaner GSoC participant-name extraction in `bench/data/gsoc-2018.json`, preserving real names like `Adrián Bazaga` instead of ScanCode's `type' Person name' ...` noise, plus more complete placeholder URL closure on templated GitHub API routes

##### [catchorg/Catch2 @ 10f6248](https://github.com/catchorg/Catch2/tree/10f62484bff73e3a58a411e2e10b4e1c13cfba9f) — **15.10× faster**

- Files: 576
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `14.57s`; ScanCode `219.94s`
- Broader Conan, Meson, and Bazel package visibility (`2` vs `1` packages, `3` vs `0` dependencies) from the root `conanfile.py`, `MODULE.bazel`, and committed `meson.build` manifests, with the local `LICENSE` notice in `.conan/test_package/conanfile.py` collapsed to plain `BSL-1.0` instead of ScanCode's extra unknown-reference placeholder

##### [chromium/chromium @ 2befda7](https://github.com/chromium/chromium/tree/2befda78fcc7fa5649540420eedcdd87a2583fe0) — **23.90× faster**

- Files: 491,354
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `957.91s`; ScanCode `22892.20s`
- Broader dependency extraction (`16620` vs `12378`) from three tracked `.gitmodules` manifests plus vendored package surfaces, richer package coverage (`1310` vs `1279`), matched `README.chromium` package visibility across 940 vendored README files (`927` package records each), direct Git-submodule visibility where ScanCode reports zero package data on those `.gitmodules`, and fewer scan errors (`1` vs `4`) under the shared profile

##### [conan-io/conan-center-index @ bc78dfb](https://github.com/conan-io/conan-center-index/tree/bc78dfb366e6596d21a7a5c51b97970656f73254) — **24.17× faster**

- Files: 14,527
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `33.41s`; ScanCode `807.63s`
- Broader Conan dependency extraction (`4346` vs `3289`) from versioned `conandata.yml`, `conanfile.py`, and committed test-package manifests, with zero scan errors where ScanCode still crashes on two recipe files, multi-source `conandata.yml` coverage across the recipe corpus, cleaner one-package-per-recipe assembly instead of ScanCode's duplicate unversioned-plus-versioned Conan rows, repo-root `LICENSE` following on docs and recipe reference notices such as `docs/faqs.md` and `recipes/cpp-sort/all/conanfile.py`, and cleaner recipe-corpus license classification by suppressing filename-token false positives such as `lgpl.txt`

##### [containerd/containerd @ 83044a43](https://github.com/containerd/containerd/tree/83044a43a1032ea53ceca6d2d11018d7c103f9de) — **17.05× faster**

- Files: 6,332
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `31.31s`; ScanCode `533.84s`
- Matched Go package coverage (`2` vs `2`) with slightly richer dependency extraction (`652` vs `651`) from vendored `mkdocs-reqs.txt` and committed Python sidecar requirements, while preserving Go module inventory parity on the root `go.mod` and `go.sum` surfaces

##### [curl/curl @ 40d57c9](https://github.com/curl/curl/tree/40d57c9f588c42ed3f75fe0ba9b12aa18170a404) — **10.57× faster**

- Files: 4,195
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `23.00s`; ScanCode `243.12s`
- Matched ScanCode's file-level Autotools `configure.ac` coverage while promoting one top-level Autotools package (`1` vs `0`), with the real `pkg:autotools/curl` identity instead of a generic input placeholder, plus extra Docker package and dependency visibility from the committed `Dockerfile`

##### [Debian/apt @ 6b12812](https://github.com/Debian/apt/tree/6b128124271e94bdb0f4e7850d9286170d712b04) — **15.11× faster**

- Files: 889
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `17.56s`; ScanCode `265.28s`
- Matched Debian source-package coverage (`7` vs `7`) with broader dependency extraction (`32` vs `0`) from the root multi-binary `debian/control` Build-Depends plus runtime relation fields such as `Depends`, `Recommends`, `Suggests`, `Breaks`, `Conflicts`, and `Provides`

##### [docker-library/official-images @ 71567fb](https://github.com/docker-library/official-images/tree/71567fbcfa7945774c08c32c04f67ef34c9bce82) — **3.66× faster**

- Files: 365
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `22.49s`; ScanCode `82.24s`
- Matched top-level package coverage (`1` vs `1`) with broader dependency extraction (`9` vs `2`) from the repo-root `Dockerfile` and committed Ruby test `Gemfile`s, plus Docker-library `Maintainers` author recovery across `library/*` definitions with cleaner Unicode-preserving normalization and `GitRepo` trailers left out of author values

##### [docker-library/python @ ced4ac7](https://github.com/docker-library/python/tree/ced4ac7ca9f8f8bdbb113f06fe02c42895875aa4) — **6.42× faster**

- Files: 53
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `11.96s`; ScanCode `76.81s`
- Broader Docker package visibility across 42 generated image Dockerfiles where ScanCode reports none, plus maintainer-line author recovery on `generate-stackbrew-library.sh`, with exact top-level package, dependency, and license parity elsewhere

##### [e-ale/meta-pocketbeagle @ 7cb4956](https://github.com/e-ale/meta-pocketbeagle/tree/7cb4956d206728af96833e513594693dec98e163) — **6.69× faster**

- Files: 31
- Run context: 2026-04-21 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `11.06s`; ScanCode `73.99s`
- Broader BitBake package visibility (`4` vs `0` packages) from committed `.bb` and `.bbappend` metadata, with `linuxconsoletools_1.6.0.bb` carrying source URL/checksum plus local file-reference evidence and wildcard append manifests such as `u-boot%.bbappend` and `linux-yocto_%.bbappend` retained as package records instead of scanner-silent inputs

##### [facebook/buck2 @ 3359f75](https://github.com/facebook/buck2/tree/3359f75abe3c7b6f543fdb2c7a775d47347b8897) — **15.27× faster**

- Files: 9,600
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `35.72s`; ScanCode `545.33s`
- Slightly richer mixed-repository dependency extraction (`7079` vs `7034`) from committed `yarn.lock`, `flake.nix` / `flake.lock`, and Conan fixtures, plus zero scan errors where ScanCode still trips on `prelude/third-party/hmaptool/METADATA.bzl` and richer Buck target visibility on multi-rule `BUCK` files

##### [facebook/watchman @ 426a7b7](https://github.com/facebook/watchman/tree/426a7b7dbd8600e1f3f9a33fd6715bb08295ca1a) — **5.63× faster**

- Files: 896
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `19.03s`; ScanCode `107.21s`
- Richer Buck target visibility on `watchman/BUCK` and `watchman/fs/BUCK` (`43` and `4` file-level Buck package records where ScanCode reports none), plus extra Docker and Gemfile package visibility, with matched zero-scan-error output

##### [ffmpeg/ffmpeg @ 056562a](https://github.com/ffmpeg/ffmpeg/tree/056562a5ff64e79ad40b141ded3f644811e812f6) — **13.41× faster**

- Files: 10,200
- Run context: 2026-04-09 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `60.60s`; ScanCode `812.80s`
- Matched ScanCode's file-level Autotools `configure` package identity while also promoting one top-level Autotools package (`1` vs `0`), plus cleaner clue-only handling of weak `configure` variable-name and bare-word GPL noise such as `EXTERNAL_LIBRARY_GPL_LIST` and `LICENSE_LIST="gpl"`

##### [fmtlib/fmt @ 2cb3983](https://github.com/fmtlib/fmt/tree/2cb39832132a5c56a802bc817179e85d5f32fb9c) — **13.15× faster**

- Files: 133
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `11.99s`; ScanCode `157.63s`
- Matched package and dependency parity (`0` packages, `1` dependency) while collapsing the local `LICENSE-MIT` notice in `support/docopt.py` to plain `MIT`, with cleaner copyright normalization on mkdocstrings support code and consistent URL normalization across README and docs

##### [git/git @ 9f223ef](https://github.com/git/git/tree/9f223ef1c026d91c7ac68cc0211bde255dda6199) — **18.30× faster**

- Files: 4,734
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `24.70s`; ScanCode `452.09s`
- Broader package-adjacent Git metadata visibility on the tracked `.gitmodules` manifest (`1` vs `0` dependencies on that file), plus one extra top-level package row (`4` vs `3`) from treating the manifest as package metadata instead of leaving it scanner-silent

##### [go-gitea/gitea @ 47fdf3e2](https://github.com/go-gitea/gitea/tree/47fdf3e284308c6b648936b5c15e136b08f5e1da) — **10.15× faster**

- Files: 5,201
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `26.21s`; ScanCode `266.07s`
- Broader package and dependency extraction (`3` vs `2` packages, `1943` vs `1917` dependencies) from `flake.nix`, `flake.lock`, `Dockerfile`, and `uv.lock`, plus a correct root Go module identity on `go.mod` where ScanCode emits the malformed `pkg:golang/%28` package row

##### [grpc/grpc @ f87c29f](https://github.com/grpc/grpc/tree/f87c29f069971d1356e5784005af499db52e7f31) — **14.43× faster**

- Files: 10,361
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `48.11s`; ScanCode `694.17s`
- Far broader dependency extraction (`418` vs `92`) from the root `.gitmodules`, `MODULE.bazel`, and vendored package surfaces, richer package coverage (`782` vs `761`), and direct Git-submodule visibility on 17 tracked third-party submodules where ScanCode reports zero package data on the same manifest

##### [guillemj/dpkg @ 0061122](https://github.com/guillemj/dpkg/tree/006112209ac937b373d4497c81998a415cbef0f5) — **20.22× faster**

- Files: 1,766
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `27.87s`; ScanCode `563.43s`
- Broader Debian source-package and dependency extraction (`23` vs `19` packages, `18` vs `0` dependencies) from the root multi-binary `debian/control` file plus committed `.dsc` fixtures, with explicit package visibility for `dpkg-dev`, `libdpkg-dev`, and `libdpkg-perl` and one extra top-level Autotools package on `configure.ac`

##### [kubernetes/kubernetes @ d3b9c54](https://github.com/kubernetes/kubernetes/tree/d3b9c54bd952117924fb0790f6989c0d30715b19) — **16.19× faster**

- Files: 29,080
- Run context: 2026-04-08 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `141.58s`; ScanCode `2291.67s`
- Broader Dockerfile and `go.work` package coverage, richer staging-workspace dependency extraction (`7187` vs `6950`), and richer `BSD-3-Clause AND Apache-2.0` compound license classification where ScanCode collapses many of the same files to plain `Apache-2.0`

##### [libevent/libevent @ 4829651](https://github.com/libevent/libevent/tree/48296514d8fd9c0b3812b11d45ad80b0c002c14e) — **4.14× faster**

- Files: 260
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `11.67s`; ScanCode `48.32s`
- Matched ScanCode's file-level Autotools `configure.ac` coverage while promoting one top-level Autotools package (`1` vs `0`), with the real `pkg:autotools/libevent` identity instead of a generic input placeholder

##### [libgit2/libgit2 @ 1f34e2a](https://github.com/libgit2/libgit2/tree/1f34e2a57a3d03f174771203b64aed2b17e8522c) — **7.75× faster**

- Files: 8,406
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `22.61s`; ScanCode `175.31s`
- Broader mixed-repository dependency extraction (`12` vs `0`) from committed `script/api-docs/package.json` and `script/api-docs/package-lock.json`, while preserving top-level Autotools package parity (`1` vs `1`)

##### [LinuxCNC/linuxcnc @ cd534c9](https://github.com/LinuxCNC/linuxcnc/tree/cd534c951aefa3c57ced93d84d1eec5aa5596672) — **6.21× faster**

- Files: 9,078
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `419.63s`; ScanCode `2606.91s`
- Direct Meson package visibility on the root `meson.build` plus declared dependency extraction (`2` vs `0` packages, `2` vs `0` dependencies) for `boost` and `python2`, with Debian copyright metadata carrying a Debian namespace instead of an unqualified source-package row

##### [moby/moby @ 21bd660](https://github.com/moby/moby/tree/21bd660cd595929275d8f1361d224f663a2cfc44) — **24.79× faster**

- Files: 12,375
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `56.00s`; ScanCode `1388.49s`
- Matched top-level package coverage (`5` vs `5`) with slightly richer dependency extraction (`1093` vs `1088`) from relative Go module edges, vendored `.gitmodules`, and committed `requirements.txt`, plus extra Docker package visibility on committed Dockerfiles and cleaner rejection of weak prose-only author or holder matches such as `the Prometheus`

##### [mongodb/mongo @ d6877a3](https://github.com/mongodb/mongo/tree/d6877a33a90e253f4e7a9641a3eb237518a5a495) — **13.91× faster**

- Files: 52,443
- Run context: 2026-04-11 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `313.61s`; ScanCode `4363.53s`
- Broader package/dependency extraction (`40` vs `1` packages, `618` vs `7` dependencies) from vendored gRPC Bazel BUILD files plus `poetry.lock`, `pnpm-lock.yaml`, and RPM spec metadata, richer Debian namespace/PURL identity on package metadata, and cleaner SBOM author recovery with score-fusion code examples left as code data instead of people

##### [nmap/nmap @ d9199d7](https://github.com/nmap/nmap/tree/d9199d7cd5e99f54fc4b67d592a30fa597a94c40) — **8.46× faster**

- Files: 2,587
- Run context: 2026-04-08 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `52.87s`; ScanCode `447.07s`
- Broader package/dependency extraction (`18` vs `2` packages, `13` vs `2` dependencies), preserved NPSL/source-available handling across core Nmap and Zenmap reference-notice files, and cleaner rejection of weak translated-manpage GPL bare-word and placeholder noise

##### [nginx/nginx @ 6e14e95](https://github.com/nginx/nginx/tree/6e14e954aaacce9a433d9b07b4653809c7594ab8) — **17.78× faster**

- Files: 521
- Run context: 2026-04-25 · nginx-35550 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `11.92s`; ScanCode `211.97s`
- Direct CPAN package visibility (`1` vs `0` packages) from the embedded Perl `src/http/modules/perl/Makefile.PL`, with concrete `pkg:cpan/nginx@%%VERSION%%` identity and author metadata instead of ScanCode's generic CPAN placeholder, plus safer rejection of nginx's custom `auto/configure` shell script as Autotools package metadata and cleaner author, email, and URL normalization across manpage markup and README badge links

##### [openembedded/meta-openembedded @ 7bf89d0](https://github.com/openembedded/meta-openembedded/tree/7bf89d06a41405b48fa3af260da36bc686973afc) — **14.04× faster**

- Files: 6,983
- Run context: 2026-04-21 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `35.69s`; ScanCode `501.03s`
- Broader BitBake package and dependency visibility (`1437` vs `0` packages, `10149` vs `0` dependencies) from committed `.bb`, `.bbappend`, and `.inc` metadata, plus recipe-side declared-license and source-reference recovery on manifests such as `nilfs-utils_v2.2.11.bb`, with patch-header and comment-style author recovery kept separate from ScanCode's bare-word GPL/LGPL and patch-prose overcalls

##### [openssl/openssl @ 7fb28b9](https://github.com/openssl/openssl/tree/7fb28b9cd05ba89cbbe038dfa85804fe22bc146a) — **20.36× faster**

- Files: 6,074
- Run context: 2026-04-25 · openssl-2710 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `58.93s`; ScanCode `1199.73s`
- Broader package and dependency visibility (`1` vs `0` packages, `41` vs `0` dependencies) from bundled `external/perl/Text-Template-1.56` CPAN metadata plus committed `.gitmodules` and `test/quic-openssl-docker/Dockerfile` surfaces, with stronger `Written by ...` author recovery on OpenSSL-style comment headers and cleaner rejection of weak CPAL or MIT overcalls on standard OpenSSL license footers

##### [protocolbuffers/protobuf @ e3370c2](https://github.com/protocolbuffers/protobuf/tree/e3370c2e26bbfaa63bc9f8e4ac0f8dc066ba3eeb) — **28.62× faster**

- Files: 3,463
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `29.73s`; ScanCode `851.02s`
- Broader Bazel and cross-language dependency extraction (`551` vs `537` packages, `144` vs `64` dependencies) from root and example `MODULE.bazel`, many `BUILD` files, committed `*.csproj`, and Maven BOM imports, with direct Git-submodule package visibility on `.gitmodules`

##### [qemu/qemu @ da6c4fe](https://github.com/qemu/qemu/tree/da6c4fe60fee30dd77267764d55b38af9cb89d4b) — **31.76× faster**

- Files: 10,989
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `78.51s`; ScanCode `2493.21s`
- Broader Meson and package-adjacent dependency extraction (`22` vs `21` packages, `260` vs `176` dependencies) from the root `.gitmodules`, `python/tests/minreqs.txt`, and many committed `subprojects/**/meson.build` manifests, with the real `pkg:autotools/qemu` root identity instead of ScanCode's generic input placeholder

##### [rpm-software-management/dnf @ e47634f](https://github.com/rpm-software-management/dnf/tree/e47634fbe3565d0580e89ec21adb7c1b308642ce) — **14.16× faster**

- Files: 655
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `14.37s`; ScanCode `203.47s`
- Broader RPM package and dependency extraction (`163` vs `138` packages, `579` vs `1` dependencies) from committed `.rpm` fixtures and sibling `.spec` metadata, with normalized RPM header license expressions and one-package-per-spec ownership across the shipped module fixture trees

##### [rpm-software-management/libdnf @ d395731](https://github.com/rpm-software-management/libdnf/tree/d39573195e24b43687587a8d83b9f6ac274e2412) — **12.33× faster**

- Files: 1,162
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `13.65s`; ScanCode `168.27s`
- Broader RPM package and dependency extraction (`352` vs `327` packages, `1441` vs `0` dependencies) from committed `.rpm` fixture trees and sibling `.spec` metadata, with normalized RPM header license expressions and cleaner rejection of config or doc false positives such as `baseurl` and `doxygen. Using` as holder or author data

##### [rust-lang/cargo @ b54fe55](https://github.com/rust-lang/cargo/tree/b54fe551a982d75d299e0d54daeac70cb854eef0) — **8.35× faster**

- Files: 2,883
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `15.25s`; ScanCode `127.38s`
- Matched Cargo package coverage (`552` vs `552`) with workspace-root package retention, legacy `dev_dependencies` / `build_dependencies` manifest coverage, and zero scan errors on malformed fixture manifests, plus extra Docker package visibility on committed test containers

##### [rust-lang/rust @ dab8d9d](https://github.com/rust-lang/rust/tree/dab8d9d1066c4c95008163c7babf275106ce3f32) — **30.57× faster**

- Files: 58,818
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `61.49s`; ScanCode `1879.48s`
- Largely matched native-tree package and dependency extraction (`341` vs `344` packages, `5771` vs `5921` dependencies) with better nested Cargo lock dependency visibility across mixed workspaces, additional Nix package visibility, and more specific versioned Cargo package identities where ScanCode emits generic lockfile rows or versionless crate names

##### [systemd/systemd @ 89d705a](https://github.com/systemd/systemd/tree/89d705a892b3476de14e548f3f9b0af96207d4b0) — **23.26× faster**

- Files: 6,994
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `51.67s`; ScanCode `1201.90s`
- Broader Meson dependency extraction (`40` vs `2`) from the root and nested `meson.build` files, with literal `\x2d` filenames preserved on committed unit and fuzz fixtures instead of being path-shaped into different resources

##### [tensorflow/tensorflow @ 2cd48d2](https://github.com/tensorflow/tensorflow/tree/2cd48d27d98b3fefd565f246f41bf93724f3f23c) — **20.41× faster**

- Files: 36,237
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `290.44s`; ScanCode `5927.08s`
- Broader Bazel and mixed-tree dependency extraction (`8202` vs `8056` packages, `1465` vs `700` dependencies) from root and vendored `MODULE.bazel`, many committed `BUILD` files, Python lockfiles, Dockerfiles, and Debian control metadata, plus direct `CITATION.cff` package visibility

##### [tokio-rs/tokio @ 5db10f5](https://github.com/tokio-rs/tokio/tree/5db10f538b683fe88d699dfd11be31d193db011c) — **3.31× faster**

- Files: 833
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `18.81s`; ScanCode `62.23s`
- Matched Cargo workspace package and dependency coverage (`12` vs `12` packages, `83` vs `83` dependencies) while preserving collective manifest-author names like `Tokio Contributors <team@tokio.rs>`, plus cleaner rejection of ScanCode's weak `(c)`-plus-URL copyright and holder noise and normalized docs.rs URL variants

##### [torvalds/linux @ b42ed3b](https://github.com/torvalds/linux/tree/b42ed3bb884e6b399b46d19df3f5cf015a79c804) — **27.47× faster**

- Files: 92,523
- Run context: 2026-04-10 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `401.15s`; ScanCode `11017.99s`
- Broader sparse-tree package visibility (`4` vs `2` packages, `20` vs `19` dependencies), plus cleaner common-profile author extraction on representative native-source docs such as `sysrq`, `cpusets`, and `hwmon` rosters while rejecting several ScanCode-only prose false positives like `the Coreboot BIOS.` and `the Host`

##### [yoctoproject/poky @ cb2dcb4](https://git.yoctoproject.org/poky/tree/?id=cb2dcb4963e5fbe449f1bcb019eae883ddecc8ec) — **15.58× faster**

- Files: 6,295
- Run context: 2026-04-21 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `47.33s`; ScanCode `737.50s`
- Broader BitBake package and dependency visibility (`546` vs `2` packages, `3061` vs `22` dependencies) from committed `.bb`, `.bbappend`, and `.inc` metadata, plus recipe-local declared-license output on manifests such as `rdma-core_62.0.bb` and `libowfat_0.32.bb`, with cleaner package records for wildcard append files and comment-style author recovery where ScanCode still mixes in low-signal project/community strings

#### Apple / Swift / Flutter / mobile

##### [AFNetworking/AFNetworking @ d9f589c](https://github.com/AFNetworking/AFNetworking/tree/d9f589cc2c1fe9d55eb5eea00558010afea7a41e) — **8.07× faster**

- Files: 211
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.94s`; ScanCode `88.26s`
- Matched top-level CocoaPods package coverage (`1` vs `1`) with broader dependency extraction (`124` vs `115`) from `AFNetworking.podspec` subspec edges and the root `Gemfile`

##### [Alamofire/Alamofire @ ac01666](https://github.com/Alamofire/Alamofire/tree/ac016668a19532686e320edf447f79a5cf5bd057) — **11.91× faster**

- Files: 567
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `14.71s`; ScanCode `175.16s`
- Matched top-level CocoaPods package coverage (`1` vs `1`) and main podspec/license parity, with slightly richer dependency extraction (`56` vs `54`) from the root `Gemfile`

##### [Carthage/Carthage @ e33e133](https://github.com/Carthage/Carthage/tree/e33e133a5427129b38bfb1ae18d8f56b29a93204) — **12.22× faster**

- Files: 183
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.76s`; ScanCode `131.47s`
- Matched top-level package coverage (`9` vs `9`) with direct Carthage manifest visibility and hoisted declared or pinned dependency extraction (`20` vs `0`) from committed `Cartfile`, `Cartfile.private`, and `Cartfile.resolved`, plus safer `Package.resolved` modeling as one resolved-file package record with structured pinned dependencies instead of exploded duplicate pseudo-packages

##### [facebook/react-native @ 179e0cd](https://github.com/facebook/react-native/tree/179e0cdef68d12249a5a13b975a82f72bca7f368) — **15.08× faster**

- Files: 7,765
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `34.99s`; ScanCode `527.81s`
- Far broader CocoaPods and sidecar package extraction (`111` vs `34` packages, `2134` vs `1572` dependencies) from many committed `.podspec` files plus the root `Gemfile` and Kotlin `build.gradle.kts` plugin manifests, with richer package author visibility across React Native podspecs

##### [firebase/flutterfire @ 90d2e1f](https://github.com/firebase/flutterfire/tree/90d2e1f70b23fdad8f2fa4ca0c5e5d744d4e4f69) — **8.77× faster**

- Files: 3,544
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `25.02s`; ScanCode `219.35s`
- Broader Flutter/Firebase package and dependency extraction (`102` vs `100` packages, `964` vs `803` dependencies) from many committed `pubspec.yaml`, CocoaPods `podspec` / `Podfile`, and Android Gradle inputs, plus contributor-roster visibility from `AUTHORS` where ScanCode stays silent

##### [flutter/packages @ 06fee7a](https://github.com/flutter/packages/tree/06fee7af139504f708b5eb10bfb5593c08a24985) — **22.90× faster**

- Files: 8,983
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `37.11s`; ScanCode `849.94s`
- Far broader Dart/Flutter monorepo package and dependency extraction (`293` vs `201` packages, `2087` vs `1167` dependencies) from many package and example `pubspec.yaml` manifests plus committed podspec and Android `build.gradle.kts` inputs, with contributor-roster visibility across `AUTHORS` files that ScanCode leaves empty

##### [Mantle/Mantle @ 2a8e212](https://github.com/Mantle/Mantle/tree/2a8e2123a3931038179ee06105c9e6ec336b12ea) — **11.03× faster**

- Files: 79
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `11.11s`; ScanCode `122.53s`
- Matched top-level package coverage (`1` vs `1`) with broader package-adjacent dependency extraction (`11` vs `0`) from `.gitmodules`, `Cartfile.private`, and `Cartfile.resolved`, plus Unicode-preserving author recovery for `Robert Böhnke` and cleaner normalization of repeated workflow contact addresses and GitHub query URLs

##### [pointfreeco/swift-composable-architecture @ 7517cc3](https://github.com/pointfreeco/swift-composable-architecture/tree/7517cc3) — **12.26× faster**

- Files: 1,098
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.40s`; ScanCode `127.50s`
- Matched Swift package coverage (`67` vs `67`), with safer `Package.resolved` modeling as one resolved-file package record with structured pinned dependencies instead of exploded duplicate file-level pseudo-packages

##### [ReactiveCocoa/ReactiveCocoa @ f2d9bd5](https://github.com/ReactiveCocoa/ReactiveCocoa/tree/f2d9bd56ae9f345821d9cd53fe3479db77e29094) — **11.52× faster**

- Files: 216
- Run context: 2026-04-20 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.48s`; ScanCode `120.75s`
- Matched top-level package coverage (`7` vs `7`) with broader package-adjacent dependency extraction (`14` vs `0`) from `.gitmodules`, `Cartfile`, `Cartfile.private`, `Cartfile.resolved`, and the sibling podspecs, plus safer `Package.resolved` modeling as one resolved-file package record with structured pinned dependencies instead of exploded duplicate pseudo-packages

##### [rrousselGit/riverpod @ cac77b1](https://github.com/rrousselGit/riverpod/tree/cac77b1ec1c4b4c0ca7c6e9b1436f80250b4edc0) — **14.36× faster**

- Files: 1,930
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `12.13s`; ScanCode `174.19s`
- Broader Dart/Flutter workspace package and dependency extraction (`29` vs `26` packages, `1417` vs `1350` dependencies) from package, example, and test `pubspec.yaml` manifests across the monorepo, plus cleaner structured-literal copyright extraction on generated Dart and JSON fixtures

##### [SDWebImage/SDWebImage @ c3ad5e1](https://github.com/SDWebImage/SDWebImage/tree/c3ad5e1a9bf55c9b76d4c362430b5fcded96c502) — **10.20× faster**

- Files: 371
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `12.61s`; ScanCode `128.67s`
- Matched top-level CocoaPods package coverage (`3` vs `3`) with broader dependency extraction (`10` vs `0`) from `Podfile`-declared pod relationships, while preserving separate package identities for the sibling test podspecs

##### [SwiftFiddle/swiftfiddle-web @ df09b80](https://github.com/SwiftFiddle/swiftfiddle-web/tree/df09b80) — **8.30× faster**

- Files: 109
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.21s`; ScanCode `84.73s`
- Much richer dependency extraction (`297` vs `36`) from committed `Resources/Package.swift.json`, `Package.resolved`, and `package-lock.json`, matched Swift package coverage (`32` vs `32`), and extra Docker package visibility

#### .NET / NuGet / Windows / vcpkg

##### [AvaloniaUI/Avalonia @ b7e95c2](https://github.com/AvaloniaUI/Avalonia/tree/b7e95c2b0961c33f06a3a64846c4207fb406eada) — **9.95× faster**

- Files: 5,273
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `38.15s`; ScanCode `379.55s`
- Broader .NET/NuGet package and dependency extraction (`105` vs `3` packages, `145` vs `33` dependencies) from many `*.csproj` files plus `Directory.Packages.props` and `Directory.Build.props` across samples, tooling, and test projects, with zero scan errors where ScanCode trips on `TwitterColorEmoji-SVGinOT.ttf`

##### [microsoft/onnxruntime @ 97e0a00](https://github.com/microsoft/onnxruntime/tree/97e0a001d43f8783db4507c9b2ac3731dc95a1ed) — **23.89× faster**

- Files: 9,802
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `54.99s`; ScanCode `1313.69s`
- Broader mixed-repository package and dependency extraction (`45` vs `1` packages, `3607` vs `80` dependencies) from `cmake/vcpkg.json` plus committed `cmake/vcpkg-ports/*/vcpkg.json` manifests, with the large `package-lock.json` license-count gap reduced with any residual license delta concentrated in ONNX model fixtures that still stay scan-error-free and explicit vcpkg package identities where ScanCode stays manifest-blind

##### [microsoft/terminal @ 84ae7ad](https://github.com/microsoft/terminal/tree/84ae7adec6b3975314d8ca73d8f0bf2128ae59e2) — **14.55× faster**

- Files: 3,625
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `23.10s`; ScanCode `336.14s`
- Broader mixed-package extraction (`15` vs `2` packages, `40` vs `0` dependencies) from the root `vcpkg.json`, overlay-port `dep/vcpkg-overlay-ports/*/vcpkg.json`, and committed `packages.config` files, with explicit vcpkg package identities where ScanCode reports none

##### [microsoft/vcpkg @ b21ff8f](https://github.com/microsoft/vcpkg/tree/b21ff8f3cadbd8e0b175b49be2dd9202f1f208f4) — **13.91× faster**

- Files: 13,670
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `30.13s`; ScanCode `419.05s`
- Far broader vcpkg registry package and dependency extraction (`9` vs `1` packages, `13650` vs `39` dependencies) from many committed `ports/*/vcpkg.json` manifests with host, feature, and platform-qualified dependencies, plus standalone Debian copyright package rows on `ports/*/copyright` and explicit vcpkg package identities where ScanCode stays largely manifest-blind

##### [OrchardCMS/OrchardCore @ 01386f3](https://github.com/OrchardCMS/OrchardCore/tree/01386f38ee3fef620a93934f05ba1363ff05c291) — **17.53× faster**

- Files: 9,118
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `35.79s`; ScanCode `627.46s`
- Broader .NET/NuGet package and dependency extraction (`276` vs `41` packages, `1758` vs `1597` dependencies) from many `*.csproj` files plus `Directory.Packages.props` and `Directory.Build.props` across Orchard modules, abstractions, and templates, with richer package visibility on the solution-style tree where ScanCode stays mostly manifest-local

#### Ruby / PHP / Perl

##### [composer/composer @ a2bf8cb](https://github.com/composer/composer/tree/a2bf8cba45d3b2de8eca6e4c444d58a0c8b283a6) — **4.00× faster**

- Files: 1,030
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `21.23s`; ScanCode `84.94s`
- Matched Composer package coverage (`40` vs `40`) and dependency extraction (`324` vs `324`) across `composer.json` and `composer.lock`, with more specific pinned dependency identities in committed fixtures, safer URL credential stripping, and Unicode-preserving author normalization

##### [laravel/framework @ a3960e8](https://github.com/laravel/framework/tree/a3960e8ff8ae2daa7ff609a245c51d9fe0aca684) — **7.34× faster**

- Files: 3,086
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `22.16s`; ScanCode `162.58s`
- Matched Composer package coverage (`37` vs `37`) with broader dependency extraction (`656` vs `498`) from the committed exception-renderer `package-lock.json`, plus cleaner rejection of Blade-template pseudo-copyrights and author false positives such as `extends Model`

##### [libwww-perl/libwww-perl @ 7420d1b](https://github.com/libwww-perl/libwww-perl/tree/7420d1bfff7cd5369ca24e87c37edf97b2cbb0c1) — **7.40× faster**

- Files: 98
- Run context: 2026-04-18 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.94s`; ScanCode `80.95s`
- Direct CPAN package identity and broader dependency extraction (`1` vs `0` packages, `44` vs `0` dependencies) from `META.json` prereq scopes, with repository and homepage metadata preserved from CPAN resources

##### [PerlDancer/Dancer2 @ a1faa22](https://github.com/PerlDancer/Dancer2/tree/a1faa22a78ff6f3c40ef5b71424dbe3f2c4a13a8) — **10.44× faster**

- Files: 436
- Run context: 2026-04-18 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `9.33s`; ScanCode `97.37s`
- Direct CPAN package identity on the root `dist.ini`, extra dependency visibility from the shipped skeleton `Makefile.PL`, plus Docker package visibility on `share/docker/Dockerfile`, with unresolved template placeholders kept out of CPAN names and PURLs

##### [Plack/Plack @ b3984f1](https://github.com/Plack/Plack/tree/b3984f1c59de36903bb924c9da1273f3e11d4d2b) — **8.67× faster**

- Files: 275
- Run context: 2026-04-18 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.04s`; ScanCode `87.06s`
- Direct CPAN package identity and broader dependency extraction (`1` vs `0` packages, `22` vs `0` dependencies) from `META.json`, `dist.ini`, and `Makefile.PL`, with CPAN resource metadata preserved from the distribution manifest

##### [rails/rails @ 27fb2a9](https://github.com/rails/rails/tree/27fb2a9192b2492791528fc7c3afb53736696bc5) — **13.69× faster**

- Files: 4,869
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `23.27s`; ScanCode `318.46s`
- Broader Ruby/Bundler package and dependency extraction (`20` vs `17` packages, `899` vs `802` dependencies) from the root `Gemfile`, the multi-gemspec Rails component tree, and resolved `RAILS_VERSION`-backed gemspec versions, with real `8.2.0.alpha` gem identities where ScanCode leaves literal `version` placeholders

##### [rubocop/rubocop @ 4e0d642](https://github.com/rubocop/rubocop/tree/4e0d642eca6e9a694b8a359d39e0d4b5b6b26bb8) — **7.55× faster**

- Files: 2,081
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `24.15s`; ScanCode `182.30s`
- Matched top-level package coverage (`1` vs `1`) with much richer Ruby dependency extraction (`28` vs `10`) from the root `Gemfile`, plus resolved `RuboCop::Version::STRING` gem identity on `rubocop.gemspec` and more-correct `CC-BY-NC-4.0` README logo licensing where ScanCode overstates it as `CC-BY-NC-SA-4.0`

##### [symfony/symfony @ 5b8e0c9](https://github.com/symfony/symfony/tree/5b8e0c97bf39a14aeae9cc353b7ed6cf14532e40) — **13.98× faster**

- Files: 13,294
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `46.98s`; ScanCode `656.63s`
- Matched split-package Composer monorepo package and dependency coverage (`188` vs `188` packages, `1460` vs `1460` dependencies), with Unicode-preserving author normalization, cleaner rejection of URL-style pseudo-authors such as `Tobias Schultze http://tobion.de`, and more explicit proprietary-license normalization where ScanCode leaves an unknown-license bucket

#### Julia / Nix / Haskell / other ecosystems

##### [commercialhaskell/stack @ cb6070f](https://github.com/commercialhaskell/stack/tree/cb6070feb211ddb305ee2384c86932ffeef76cbe) — **10.81× faster**

- Files: 1,110
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `15.49s`; ScanCode `167.47s`
- Far broader Hackage package and dependency extraction (`76` vs `1` packages, `524` vs `4` dependencies) from the root `stack.cabal`, `stack.yaml`, `cabal.project`, and committed integration-fixture manifests, with richer maintainer identity on Cabal metadata

##### [HaxeFlixel/flixel @ ec54c5a](https://github.com/HaxeFlixel/flixel/tree/ec54c5a582b252de3aca69283045719d3201778b) — **12.66× faster**

- Files: 446
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.70s`; ScanCode `135.43s`
- Matched Haxe package and dependency coverage on the repo-root `haxelib.json`, with compound `LicenseRef-scancode-public-domain AND OFL-1.1` font licensing on `assets/fonts/monsterrat.ttf` instead of split duplicate detections and cleaner URL normalization across docs and snippets

##### [HeapsIO/heaps @ d2992b0](https://github.com/HeapsIO/heaps/tree/d2992b061db3f51b47cdb87c39d659a5bb96dd83) — **15.91× faster**

- Files: 666
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.63s`; ScanCode `169.15s`
- Matched Haxe package and dependency coverage on the repo-root `haxelib.json`, with cleaner copyright and holder recovery on `hxd/fmt/fbx/Writer.hx` and `samples/text_res/trueTypeFont.ttf` plus safer trailing-slash URL normalization

##### [jgm/pandoc @ d9838eb](https://github.com/jgm/pandoc/tree/d9838eba11ae18216f52e233dbbca735f0f97ccb) — **14.61× faster**

- Files: 2,768
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `22.78s`; ScanCode `332.82s`
- Broader mixed Hackage and Nix package extraction (`5` vs `0` packages, `197` vs `0` dependencies) from sibling `pandoc*.cabal` manifests, `stack.yaml`, and `flake.nix` / `flake.lock`, with explicit package identities across `pandoc`, `pandoc-cli`, `pandoc-lua-engine`, and `pandoc-server`

##### [JuliaLang/julia @ afc71c2](https://github.com/JuliaLang/julia/tree/afc71c255e327d8a64b69061c15994e80740974d) — **21.75× faster**

- Files: 1,948
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `25.28s`; ScanCode `549.75s`
- Direct Julia package visibility and much broader dependency extraction (`115` vs `0` packages, `240` vs `0` dependencies) from stdlib, test, and nested `Project.toml` / `Manifest.toml` pairs across the tree, with richer author recovery on Julia metadata and cleaner rejection of prose-only copyright or holder noise

##### [JuliaLang/Pkg.jl @ c96cfdf](https://github.com/JuliaLang/Pkg.jl/tree/c96cfdf70976e8a5cc21fcef53c0ba137f6b2f64) — **7.29× faster**

- Files: 486
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `13.20s`; ScanCode `96.27s`
- Direct Julia package visibility and much broader dependency extraction (`98` vs `0` packages, `150` vs `0` dependencies) from `Project.toml`, `Manifest.toml`, and sibling project-plus-manifest assembly across root, docs, and test fixture trees, with safer URL credential stripping in Julia metadata examples

##### [JuliaPlots/Plots.jl @ 70f0cd7](https://github.com/JuliaPlots/Plots.jl/tree/70f0cd7a59dc667791503eaf0ab14190069a9be4) — **9.58× faster**

- Files: 327
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `10.67s`; ScanCode `102.27s`
- Direct Julia package visibility and much broader dependency extraction (`7` vs `0` packages, `202` vs `0` dependencies) from sibling `Project.toml` files across `Plots`, `GraphRecipes`, `RecipesBase`, and test environments, with richer author recovery on Julia metadata or README ownership lines and safer URL normalization

##### [nix-community/dream2nix @ 69eb01f](https://github.com/nix-community/dream2nix/tree/69eb01fa0995e1e90add49d8ca5bcba213b0416f) — **1.68× faster**

- Files: 515
- Run context: 2026-04-12 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `19.91s`; ScanCode `33.50s`
- Broader Nix package and dependency extraction (`53` vs `22` packages, `887` vs `843` dependencies) from committed `flake.lock` inputs and flake-compat-backed `default.nix` wrapper surfaces across the tree, with cleaner root-package visibility on repository entrypoints that ScanCode leaves unassembled

##### [NixOS/nix @ 6a659e1](https://github.com/NixOS/nix/tree/6a659e16bd2bcd871aedcb38724a1cff77690a31) — **18.21× faster**

- Files: 2,917
- Run context: 2026-04-27 · nix-25195 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `12.29s`; ScanCode `223.79s`
- Broader Nix package and dependency extraction (`3` vs `0` packages, `69` vs `0` dependencies) from committed `flake.lock`, root `default.nix`, and other Nix manifest surfaces, with richer structured author, email, and URL recovery across repository docs and release metadata

##### [NixOS/nixpkgs @ c407343](https://github.com/NixOS/nixpkgs/tree/c4073437f5ffeaeee270c37a2eddf370658d1332) — **14.84× faster**

- Files: 52,167
- Run context: 2026-04-27 · nixpkgs-12663 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `312.87s`; ScanCode `4641.96s`
- Broader Nix package visibility (`1340` vs `737` packages) across committed Nix manifests, provider metadata, and lockfile-adjacent package surfaces, plus zero scan-file errors where ScanCode times out on huge metadata captures such as `hackage-packages.nix` and `typst-packages-from-universe.toml`

##### [numtide/devshell @ 255a2b1](https://github.com/numtide/devshell/tree/255a2b1725a20d060f566e4755dbf571bbbb5f76) — **7.44× faster**

- Files: 87
- Run context: 2026-04-27 · devshell-25970 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `8.84s`; ScanCode `65.75s`
- Broader Nix package and dependency extraction (`5` vs `0` packages, `17` vs `0` dependencies) from committed `flake.lock`, root `default.nix`, and template flake surfaces, with cleaner structured author, copyright, and URL recovery

##### [ocaml/dune @ b13ab94](https://github.com/ocaml/dune/tree/b13ab949e185a205a39eb6163eea050b7d60a047) — **25.02× faster**

- Files: 7,751
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `20.74s`; ScanCode `519.01s`
- Broader opam and Nix package visibility (`4` vs `2` packages, `130` vs `116` dependencies) from the generated `opam/*.opam` manifests and `flake.lock`, with structured opam description, maintainer, and dependency recovery instead of ScanCode's field-bleeding author text on those manifests

##### [ocaml/merlin @ 30b4f24](https://github.com/ocaml/merlin/tree/30b4f24fdd76fdbf32685aac73de7fd4a6ff7470) — **20.55× faster**

- Files: 2,120
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `31.93s`; ScanCode `656.13s`
- Direct opam package visibility (`1` vs `0` packages) with broader dependency extraction (`27` vs `24`) from the repo-root `merlin*.opam`, `dot-merlin-reader.opam`, `ocaml-index.opam`, and `flake.lock` surfaces, plus Unicode-preserving copyright normalization across the Merlin source tree

##### [ocaml/ocaml-lsp @ 788ff73](https://github.com/ocaml/ocaml-lsp/tree/788ff738991189537141776bfa07652547bff9c4) — **13.40× faster**

- Files: 546
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `13.83s`; ScanCode `185.33s`
- Broader opam package visibility (`3` vs `1` packages) with slightly richer dependency extraction (`380` vs `376`) from the root and submodule `.opam` manifests plus `flake.lock`, with cleaner maintainer and email recovery on opam metadata and Unicode-preserving copyright normalization

##### [openfl/openfl @ 74d8f72](https://github.com/openfl/openfl/tree/74d8f72890b9ae70bba589d034ea35b86588e548) — **16.94× faster**

- Files: 1,196
- Run context: 2026-04-22 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `12.77s`; ScanCode `216.36s`
- Matched Haxe package and dependency coverage on the repo-root `haxelib.json`, with richer bundled Windows executable identity on `assets/templates/bin/openfl.exe`, extra Docker package visibility on `Dockerfile`, and cleaner URL normalization across shipped font metadata

##### [univention/Nubus @ fef2258](https://github.com/univention/Nubus/tree/fef2258483c56cce0e1f14e4c8d8fce24d26b891) — **6.84× faster**

- Files: 16
- Run context: 2026-04-19 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 10 proc
- Timing: Provenant `10.53s`; ScanCode `72.03s`
- Direct `publiccode.yml` package visibility on the root metadata file (`1` vs `0` on that file), with cleaner SPDX copyright placeholder normalization for `Univention GmbH` and the same zero-scan-error behavior under the shared profile

##### [yesodweb/yesod @ 1b033c7](https://github.com/yesodweb/yesod/tree/1b033c741ce81d01070de993b285a17e71178156) — **9.32× faster**

- Files: 324
- Run context: 2026-04-17 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.62s`; ScanCode `99.03s`
- Broader multi-package Hackage extraction (`16` vs `0` packages, `391` vs `0` dependencies) from the repo's many sibling `yesod-*/*.cabal` manifests, with explicit package identities across the Yesod family where ScanCode stays manifest-blind

### Artifact/rootfs-backed targets

#### Linux rootfs images

##### [Alpine 3.23.3 minirootfs @ sha256:42d0e6d](https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/alpine-minirootfs-3.23.3-x86_64.tar.gz) — **1.22× faster**

- Files: 84
- Run context: 2026-04-05 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `19.47s`; ScanCode `23.84s`
- Equal top-level Alpine package count with Alpine-native installed-db dependency requirements and virtual providers preserved, plus cleaner BusyBox/OpenSSL binary-text normalization and richer `os-release` package identity

##### [debian:bookworm-slim @ sha256:f065376](https://hub.docker.com/layers/library/debian/bookworm-slim/images/sha256-f06537653ac770703bc45b4b113475bd402f451e85223f0f2837acbf89ab020a) — **7.47× faster**

- Files: 3,267
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `77.04s`; ScanCode `575.28s`
- More correct Linux-distro identity on `usr/lib/os-release` (`debian` instead of ScanCode's incorrect `distroless`) with homepage, support, and bug-report URLs preserved, plus broader dependency extraction (`536` vs `0`) from the real `dpkg/status` relation fields while preserving top-level package count parity

##### [distroless base-debian12 @ sha256:9dce90e](https://github.com/GoogleContainerTools/distroless/blob/main/PACKAGE_METADATA.md) — **9.17× faster**

- Files: 1,264
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `12.05s`; ScanCode `110.49s`
- Direct Distroless Debian 12 identity on `usr/lib/os-release` with homepage, support, and bug-report URLs preserved despite the sparse image layout, plus broader dependency extraction (`52` vs `0`) from `status.d` and zero scan errors where ScanCode crashes on six `*.md5sums` companions

##### [Fedora Minimal 42 container rootfs @ sha256:c30f069](https://quay.io/repository/fedora/fedora-minimal) — **13.35× faster**

- Files: 1,989
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `34.69s`; ScanCode `463.11s`
- Direct Fedora distro identity on `usr/lib/os-release` with homepage, documentation, and support URLs preserved, plus installed-RPM package and dependency extraction (`102` vs `0` packages, `1427` vs `0` dependencies) from the real rpmdb where ScanCode stays package-blind

#### Installed package database snapshots

##### [Alpine 3.23.3 installed DB snapshot @ sha256:42d0e6d](https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/alpine-minirootfs-3.23.3-x86_64.tar.gz) — **7.52× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.09s`; ScanCode `75.84s`
- Matched standalone Alpine installed-db package and license coverage on the shipped `lib/apk/db/installed` snapshot, with one extra maintainer email recovered from package metadata

##### [debian:bookworm-slim dpkg DB snapshot @ sha256:f065376](https://hub.docker.com/layers/library/debian/bookworm-slim/images/sha256-f06537653ac770703bc45b4b113475bd402f451e85223f0f2837acbf89ab020a) — **8.45× faster**

- Files: 421
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `11.36s`; ScanCode `95.97s`
- Matched installed Debian package coverage (`88` vs `88`) with broader dependency extraction (`536` vs `0`) from the real `status` relation fields, richer Debian-qualified package identities on `.list` and `.md5sums` companions, and maintainer parties preserved in package metadata instead of only generic file-author guesses

##### [distroless base-debian13 status.d @ sha256:c83f022](https://github.com/GoogleContainerTools/distroless/blob/main/PACKAGE_METADATA.md) — **6.75× faster**

- Files: 18
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.31s`; ScanCode `69.61s`
- Matched distroless Debian package coverage (`9` vs `9`) with broader dependency extraction (`84` vs `0`) from `status.d` relation fields, maintainer parties preserved in package metadata, and zero scan errors where ScanCode crashes on all nine `*.md5sums` companions

##### [Fedora Minimal 42 rpmdb SQLite snapshot @ sha256:c30f069](https://quay.io/repository/fedora/fedora-minimal) — **17.88× faster**

- Files: 3
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.09s`; ScanCode `162.53s`
- No installed-RPM package extraction on the narrow SQLite primary-DB snapshot (`0` vs `0` packages, `0` vs `0` dependencies); this lane is mostly a raw database byte scan, and the remaining ScanCode-only detections on `rpmdb.sqlite` are low-value noise/false positives rather than useful package or license coverage

##### [openSUSE Tumbleweed rpmdb NDB snapshot @ sha256:25afd25](https://registry.opensuse.org/) — **16.99× faster**

- Files: 2
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.13s`; ScanCode `172.04s`
- Direct installed-RPM package and dependency extraction (`123` vs `0` packages, `1460` vs `0` dependencies) from the real openSUSE `Packages.db`/`Index.db` NDB snapshot, with zero scan errors

#### Package archives

##### [7zip 25.01-r0 .apk @ sha256:6602ccb](https://dl-cdn.alpinelinux.org/alpine/latest-stable/main/x86_64/7zip-25.01-r0.apk) — **8.03× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.06s`; ScanCode `80.82s`
- Direct Alpine archive package visibility on the shipped `.apk` (`1` vs `1` file-level package records), with a concrete `pkg:alpine/7zip@25.01-r0?arch=x86_64` identity instead of ScanCode's weaker generic package-data row

##### [bash 5.2.15-2+b10 .deb @ sha256:be3ab2f](https://deb.debian.org/debian/pool/main/b/bash/bash_5.2.15-2%2Bb10_amd64.deb) — **3.02× faster**

- Files: 1
- Run context: 2026-04-15 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `22.19s`; ScanCode `66.94s`
- Matched shipped Debian package coverage (`1` vs `1`) with broader dependency extraction (`9` vs `0`) from the archive control metadata, plus the correct `pkg:deb` `arch=amd64` qualifier where ScanCode uses the nonstandard `architecture` key

##### [bash 5.3.9 .pkg +COMPACT_MANIFEST sample @ sha256:37207e8](https://pkg.freebsd.org/FreeBSD:14:amd64/latest/All/Hashed/bash-5.3.9~37207e82d6.pkg) — **7.27× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.39s`; ScanCode `68.27s`
- Matched FreeBSD package-manifest package coverage (`1` vs `1`) on the `+COMPACT_MANIFEST` extracted from the shipped `.pkg`, with normalized `GPL-3.0-or-later` declared-license reporting and a single top-level declared-license detection instead of ScanCode's duplicated GPL row

##### [curl 8.19.0_2 .pkg +COMPACT_MANIFEST sample @ sha256:b78b1ff](https://pkg.freebsd.org/FreeBSD:14:amd64/latest/All/Hashed/curl-8.19.0_2~b78b1ff26d.pkg) — **7.14× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.81s`; ScanCode `70.01s`
- Matched FreeBSD package-manifest package coverage (`1` vs `1`) on the `+COMPACT_MANIFEST` extracted from the shipped `.pkg`, with normalized `MIT` declared-license reporting instead of a raw manifest-license structure

##### [Humanizer.Core 3.0.10 .nupkg @ sha256:99f9521](https://api.nuget.org/v3-flatcontainer/humanizer.core/3.0.10/humanizer.core.3.0.10.nupkg) — **7.24× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.66s`; ScanCode `69.97s`
- Real NuGet package-archive extraction on the shipped `.nupkg` (`1` vs `0` packages, `6` vs `0` dependencies), with a named `pkg:nuget/Humanizer.Core@3.0.10` identity instead of ScanCode's generic unnamed archive row, plus an `MIT` license detection from modern package metadata

##### [pkg 2.7.4 .pkg +COMPACT_MANIFEST sample @ sha256:4128dba](https://pkg.freebsd.org/FreeBSD:14:amd64/latest/Latest/pkg.pkg) — **7.72× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.60s`; ScanCode `74.17s`
- Matched FreeBSD package-manifest package coverage (`1` vs `1`) on the `+COMPACT_MANIFEST` extracted from the shipped `.pkg`, with normalized `BSD-2-Clause` declared-license reporting where ScanCode leaves the package license unknown

##### [python-construct 2.10.70-6 .PKGINFO from Arch package @ sha256:2020ae3](https://archlinux.org/packages/extra/any/python-construct/) — **7.01× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.89s`; ScanCode `69.35s`
- Direct Arch built-package visibility on real `.PKGINFO` metadata (`1` vs `0` file-level package records) with twenty structured dependency edges across `depend`, `makedepend`, `checkdepend`, and `optdepend`, plus an arch-qualified `pkg:alpm/arch/python-construct@2.10.70-6?arch=any` identity instead of a scanner-silent package file

##### [rubocop 1.86.1 .gem @ sha256:44415f3](https://rubygems.org/gems/rubocop/versions/1.86.1) — **3.71× faster**

- Files: 1
- Run context: 2026-04-14 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `19.82s`; ScanCode `73.62s`
- Matched shipped gem package and dependency coverage (`1` vs `1` packages, `10` vs `10` dependencies), with semantically combined author/email party data and an extra parser-declared `MIT` license detection on the archive file itself

##### [sudo 1.9.15-7.p5.fc42 src.rpm @ sha256:96920ba](https://download.fedoraproject.org/pub/fedora/linux/releases/42/Everything/source/tree/Packages/s/sudo-1.9.15-7.p5.fc42.src.rpm) — **7.20× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `9.43s`; ScanCode `67.87s`
- Matched shipped source-RPM package visibility (`1` vs `1`) with broader dependency extraction (`17` vs `0`) from the archive header metadata, plus an RPM namespace-qualified source package identity and an extra `ISC` license detection where ScanCode stays generic

#### Mobile app artifacts

##### [Bitwarden Android v2024.12.0 APK+AAB+manifest](https://github.com/bitwarden/android/releases/tag/v2024.12.0) — **6.98× faster**

- Files: 3
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.07s`; ScanCode `70.31s`
- Direct Android package visibility on the shipped APK and AAB plus the production `AndroidManifest.xml` (`3` file-level package records vs `1` generic APK row), with concrete `com.x8bit.bitwarden` identity and `2024.12.0` version extraction where ScanCode stays unnamed or manifest-blind

#### Release binaries and extracted app snapshots

##### [Apache Tomcat 10.1.52 extracted release snapshot](https://archive.apache.org/dist/tomcat/tomcat-10/v10.1.52/bin/apache-tomcat-10.1.52.tar.gz) — **10.64× faster**

- Files: 643
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `17.30s`; ScanCode `184.01s`
- Broader Apache Tomcat release-tree package visibility on shipped `.war` and `WEB-INF/web.xml` surfaces (`7` file-level package records vs `0`), plus more complete Apache-2.0 coverage across the bundled docs/webapps tree, HTML-entity-faithful `&copy;` normalization on the shipped docs footer notices, and cleaner rejection of ScanCode's weak author fragments such as `the Digester`, `the Cluster`, and `the Connector`

##### [Firefox langpack en-GB 141.0.2 .xpi](https://releases.mozilla.org/pub/mozilla.org/firefox/releases/141.0.2/win64/xpi/en-GB.xpi) — **7.22× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.27s`; ScanCode `74.17s`
- Equivalent Mozilla XPI package visibility on the shipped Firefox language-pack artifact

##### [Firefox Multi-Account Containers 8.3.7 .xpi](https://addons.mozilla.org/en-US/firefox/addon/multi-account-containers/) — **7.22× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `10.11s`; ScanCode `73.02s`
- Equivalent Mozilla XPI package visibility on the shipped Firefox add-on artifact

##### [glzr-io/glazewm v3.10.1 Windows snapshot](https://github.com/glzr-io/glazewm/releases/tag/v3.10.1) — **2.77× faster**

- Files: 3
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `24.89s`; ScanCode `68.91s`
- Richer executable metadata extraction on `glazewm-v3.10.1.exe` (`3` vs `1` copyrights, `3` vs `1` holders), plus matched shipped package identity and declared license (`pkg:winexe/GlazeWM@3.10.1`, `GPL-3.0-only`) and cleaner rejection of ScanCode's bogus installer author fragments such as `uri. Failed` and `elements. Failed`

##### [ILSpy v9.1 binaries x64 snapshot @ sha256:1e925a4](https://github.com/icsharpcode/ILSpy/releases/download/v9.1/ILSpy_binaries_9.1.0.7988-x64.zip) — **1.71× faster**

- Files: 40
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.50s`; ScanCode `17.97s`
- Shipped `.deps.json` coverage on the extracted ILSpy release (`3` vs `0` packages, `86` vs `0` dependencies), with file-level NuGet dependency visibility across `ILSpy.deps.json` and plugin manifests plus cleaner rejection of ScanCode's binary-text holder noise such as `LegalTrademarks OriginalFilename`

##### [itchyny/gojq v0.12.19 darwin arm64 release snapshot @ sha256:40208d4](https://github.com/itchyny/gojq/releases/tag/v0.12.19) — **1.14× faster**

- Files: 2
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `10.45s`; ScanCode `11.95s`
- Embedded Go build-info package visibility on the shipped `gojq` binary (`9` file-level package records vs `0`), plus cleaner rejection of ScanCode's weak binary author false positive `the Go`

##### [lichess-org/fishnet v2.13.2 macOS arm64 snapshot @ sha256:8556a4d](https://github.com/lichess-org/fishnet/releases/tag/v2.13.2) — **3.69× faster**

- Files: 3
- Run context: 2026-04-13 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 9 proc
- Timing: Provenant `24.22s`; ScanCode `89.38s`
- Cargo-auditable dependency visibility on the shipped `fishnet` binary (`406` file-level dependencies vs `0`), plus cleaner normalization of weak binary-text author/email noise around OpenSSL fragments such as `<appro@openssl.org>`

##### [NSIS 3.12 setup.exe](https://prdownloads.sourceforge.net/nsis/nsis-3.12-setup.exe?download) — **3.87× faster**

- Files: 1
- Run context: 2026-04-23 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `20.04s`; ScanCode `77.60s`
- Matched NSIS installer plus Windows PE package visibility (`2` vs `2` file-level package records), with a concrete `pkg:winexe/nsis-3.12-setup@3.12` identity on the executable metadata record and cleaner rejection of ScanCode's spurious `LicenseRef-scancode-unknown` license inferred only from the `LegalCopyright` URL

##### [Windows 10 KB5049993 cumulative update extracted snapshot](https://support.microsoft.com/help/5049993) — **4.32× faster**

- Files: 11
- Run context: 2026-04-24 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `133.69s`; ScanCode `577.11s`
- Broader Windows Update package visibility through assembled `update.mum` metadata (`1` top-level package vs `0`), with correct `Package_for_RollupFix@14393.7699.1.9` wrapper identity, preserved Microsoft owner/support metadata on the CBS manifest, zero scan errors where ScanCode reports one failed CAB scan, and cleaner rejection of random CAB-byte email noise

##### [Windows 10 KB5050109 servicing stack update extracted snapshot](https://support.microsoft.com/help/5050109) — **9.42× faster**

- Files: 597
- Run context: 2026-04-24 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `12.21s`; ScanCode `115.03s`
- Broader Windows Update package visibility through assembled servicing-stack metadata (`1` top-level package vs `0`), plus matching file-level `.mum` coverage across `133` manifests, correct `Package_for_KB5050109@14393.7692.1.1` wrapper identity, richer certificate URL visibility from `update.cat`, and cleaner rejection of a bogus CAB-byte email false positive

##### [WSUS wsusscn2 extracted snapshot](https://support.microsoft.com/en-us/topic/a-new-version-of-the-windows-update-offline-scan-file-wsusscn2-cab-is-available-for-advanced-users-fe433f4d-44f4-28e3-88c5-5b22329c0a08) — **10.04× faster**

- Files: 75
- Run context: 2026-04-24 · macOS 26.3.1 · Apple M1 Max · 32 GB · arm64 · 4 proc
- Timing: Provenant `62.51s`; ScanCode `627.66s`
- Equivalent package visibility on the outer offline-scan snapshot (`0` vs `0` packages), with far cleaner rejection of random CAB-byte email noise (`0` vs `9`) while scanning the signed index-plus-CAB bundle

## Benchmark conventions

### Run identity and comparability

- Treat each row as a **snapshot of one recorded `compare-outputs` run**, not as a rolling claim about the current `main` branch.
- `run-manifest.json` is the source of truth for run identity: target/ref, scan profile/args, command invocations, **Provenant version plus revision/dirty state/diff hash**, and ScanCode runtime/cache metadata.
- Benchmark rows should record the **benchmark date** and the machine context. Keep the full compare-run `run_id` in `run-manifest.json` and the saved artifact path rather than surfacing it in the human-facing benchmark entry.

### Timing methodology

- Use the repository-supported `compare-outputs` workflow with the profile that matches the recorded target: `--profile common` for repository-backed and ordinary artifact/rootfs targets, and `--profile common-with-compiled` for artifact targets where compiled-binary package extraction is part of the comparison.
- Record same-host wall-clock timings for Provenant and ScanCode, plus relative speedup.
- Record machine information per row. If `run-manifest.json` reports `scancode.cache_hit: true`, use the cached ScanCode raw timing for that target/ref/runtime. Otherwise treat both scanner timings as license-cache-cold because the maintained workflow disables persistent license-cache reuse during actual execution.

### Row ordering

- Order rows by **target kind first**, because that matches the maintained `compare-outputs` workflow split:
  1. repository-backed targets (`--repo-url`)
  2. artifact/rootfs-backed targets (`--target-path`)
- Within each target kind, use the example headings below as the canonical placement buckets—dominant ecosystem or repository shape for repository-backed targets, artifact shape for artifact/rootfs-backed targets—and sort rows **alphabetically by target label** within each bucket.
- If a benchmark plausibly fits several ecosystems, place it where a reader is most likely to look first based on the dominant package-detection story in the final notes bullet.
- This keeps the document browsable for readers while still giving maintainers a stable, predictable placement rule for new rows.

### Writing rules for the notes bullet

- Write the final notes bullet as a **present-tense end-state comparison**, not as implementation history.
- Lead with what Provenant does better **today**: broader coverage, richer identity, safer handling, cleaner normalization, more correct classification, or faster runtime.
- Do **not** describe the path taken to get there. Avoid process/history wording such as `fixed`, `restored`, `aligned`, `added support`, `after`, `now that`, `triaged`, `reviewed tail`, or `remaining deltas`.
- If a reviewed non-regression difference matters, either omit it from the final notes bullet or rewrite it as a **user-visible advantage**. Example: write `safer URL credential stripping` instead of `credential deltas were triaged as acceptable`.
- The bullet should still read correctly if the reader has never seen the PR, compare artifact, or debugging history.
- When a row claims **much broader package or dependency counts**, include a **short causal explanation** naming the main surfaces that drive the gap (for example `uv.lock`, `pnpm-lock.yaml`, `go.work`, provider `pyproject.toml`, or Dockerfiles). Keep it to one compact phrase, not a forensic breakdown.
- Preferred sentence shape: **"Broader/richer/safer/more correct X ..., plus Y ..., with Z ..."**.
- Bad: `Fixed nested requirements parsing and triaged the remaining tail.`
- Good: `Broader Python dependency extraction from uv.lock and nested requirements inputs, with safer URL credential stripping.`

## How to extend this document

After adding or editing benchmark rows in this document, rerun `cargo run --manifest-path xtask/Cargo.toml --bin generate-benchmark-chart` so the checked-in headline stats and SVG both reflect the latest timing data.

For each new benchmark example, record:

1. target URL or artifact identity, with the resolved ref/SHA embedded in the target link when applicable
2. the run-context entry: benchmark date plus machine information; keep the full compare-run `run_id` in `.provenant/compare-runs/<run-id>/run-manifest.json` or the saved artifact path, but do not copy that slug/PID suffix into the human-facing benchmark entry
3. a timing bullet that shows Provenant total time and ScanCode total time; keep the relative speedup in the title and quick index label
4. a final notes bullet that records the end-state Provenant advantage over ScanCode, written as the current user-visible outcome rather than the path taken to get there
5. if a reviewed non-regression difference matters, rewrite it as an advantage (`safer credential stripping`, `more correct Unicode preservation`) or leave it out of the final notes bullet and keep the detailed triage in PRs or saved compare artifacts
6. if verification uncovered a regression or required a behavior change, add or update the appropriate automated coverage before treating the benchmark as complete, including focused parser tests, integration tests, and golden tests where appropriate
7. place the entry under the appropriate example heading and keep alphabetical ordering by target label within that heading
