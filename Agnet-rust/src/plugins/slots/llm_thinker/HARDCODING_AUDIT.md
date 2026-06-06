# Hardcoding Audit — `src/plugins/slots/llm_thinker/`

Audit date: 2026-05-27  
Scope: All `.rs` files under `src/plugins/slots/llm_thinker/`  
Method: Automated regex scan + manual cross-reference with design doc §3.1, §3.5, §3.7.  
Rating: **PASS** = documented/justified constant; **FAIL** = should be refactored; **TEST-OK** = test-only fixture.

---

## 1. Hardcoded URLs

| # | File | Line | Value | Context | Rating | Rationale |
|---|------|------|-------|---------|--------|-----------|
| 1 | `config_provider.rs` | 178, 212 | `"https://api.openai.com/v1"` | Default base_url test fixture | TEST-OK | Mirrors `ProviderKind::default_base_url()`; OK in test |
| 2 | `openai.rs` | 453 | `"https://api.openai.com/v1"` | Default base_url test fixture | TEST-OK | Same as above |
| 3 | `anthropic.rs` | 474 | `"https://api.anthropic.com"` | Default base_url test fixture | TEST-OK | Mirrors `ProviderKind::default_base_url()` |
| 4 | `error_classifier.rs` | 279, 385 | `"http://127.0.0.1:1"` | Intentionally invalid address for error testing | TEST-OK | Harmless test-only literal |
| 5 | `error_classifier.rs` | 300 | `"http://127.0.0.1:0"` | Intentionally invalid port for error testing | TEST-OK | Harmless test-only literal |

**Count: 0 FAIL — all PASS or TEST-OK.**

---

## 2. Hardcoded Version Strings

| # | File | Line | Value | Context | Rating | Rationale |
|---|------|------|-------|---------|--------|-----------|
| 1 | `config_provider.rs` | 23 | `"0.2.0"` | Test User-Agent header | TEST-OK | Could be `env!("CARGO_PKG_VERSION")` but test-only |
| 2 | `error_classifier.rs` | 88 | `"0.2.0"` | Test User-Agent header | TEST-OK | Same |
| 3 | `multimodal_formatter.rs` | 131 | `"0.2.0"` | Test User-Agent header | TEST-OK | Same |
| 4 | `retry_manager.rs` | 92 | `"0.2.0"` | Test User-Agent header | TEST-OK | Same |
| 5 | `stream_processor.rs` | 382 | `"0.2.0"` | Test User-Agent header | TEST-OK | Same |
| 6 | `chat_invoker.rs` | 63 | `"0.1.0"` | Test User-Agent header | TEST-OK | Same; minor drift vs 0.2.0 above but test-only |

**Count: 0 FAIL — all TEST-OK. Note: 6 occurrences, could unify with `env!("CARGO_PKG_VERSION")` as low-priority tech debt.**

---

## 3. Naked Duration / Timeout Literals

| # | File | Line | Value | Context | Rating | Rationale |
|---|------|------|-------|---------|--------|-----------|
| 1 | `config_provider.rs` | 131–132, 152–153 | `Duration::from_secs(1)` | Minimum timeout clamp | PASS | §3.1 "min 1s floor"; intentional guard, not configuration |
| 2 | `config_provider.rs` | 188 | `Duration::from_secs(30)` | Test fixture default | TEST-OK | Mirrors production default |
| 3 | `error_classifier.rs` | 276 | `Duration::from_millis(1)` | Test: force timeout error | TEST-OK | Value chosen to guarantee timeout |
| 4 | `error_classifier.rs` | 284, 288, 308, 335, 369 | `Duration::from_secs(30)` | Test expected timeout value | TEST-OK | Mirrors production default |
| 5 | `error_classifier.rs` | 382 | `Duration::from_secs(2)` | Test: request timeout for error | TEST-OK | Arbitrary small value for test |
| 6 | `retry_manager.rs` | 153 | `Duration::from_secs(30)` | Test fixture | TEST-OK | Mirrors production default |
| 7 | `retry_manager.rs` | 170, 183 | `Duration::from_millis(10)` | Test: retry backoff fixture | TEST-OK | Test-specific |
| 8 | `retry_manager.rs` | 202, 228, 255 | `Duration::from_millis(5)` | Test: retry backoff fixture | TEST-OK | Test-specific |
| 9 | `retry_manager.rs` | 279–280 | `Duration::from_millis(1)` / `Duration::from_millis(10)` | Test: exponential backoff params | TEST-OK | Test-specific |
| 10 | `anthropic.rs` | 483 | `std::time::Duration::from_secs(30)` | Test fixture | TEST-OK | Mirrors production default |

**Count: 0 FAIL — all PASS or TEST-OK.**

---

## 4. Naked Numeric Constants

| # | File | Line | Value | Context | Rating | Rationale |
|---|------|------|-------|---------|--------|-----------|
| 1 | `config_provider.rs` | 193 | `max_retries: 3` | Test fixture | TEST-OK | §3.5 default is 3; documented value |
| 2 | `anthropic.rs` | 488 | `max_retries: 3` | Test fixture | TEST-OK | Same |
| 3 | `config_provider.rs` | 195 | `context_window: 128000` | Test fixture | TEST-OK | Large enough for testing; no production impact |

**Count: 0 FAIL — all TEST-OK.**

---

## Summary

| Category | Total | PASS | TEST-OK | FAIL |
|----------|-------|------|---------|------|
| URLs     | 5     | 0    | 5       | 0    |
| Versions | 6     | 0    | 6       | 0    |
| Durations| 10    | 1    | 9       | 0    |
| Numerics | 3     | 0    | 3       | 0    |
| **Total**| **24**| **1**| **23**  | **0**|

**Verdict: No hardcoding violations found.**  
All literal values are either:
- Documented design constants (PASS),
- Test-only fixtures (TEST-OK).

Minor recommendation: unify version-string literals via `env!("CARGO_PKG_VERSION")`, but this is cosmetic (test-only).
