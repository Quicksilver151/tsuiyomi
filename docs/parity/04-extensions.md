# Mihon Parity: Extensions and Host Lifecycle

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

| Area | Feature | Mihon | tsuiyomi | Status | Notes |
|---|---|---|---|---|---|
| Sources | Repo + installed merge | Yes | ✅ | ✅ | Installed version preserved separately. |
| Updates | Update detection | Yes | ✅ | ✅ | Version comparison. |
| Updates | Update badge / update all | Yes | ✅ | ✅ | `U`. |
| Grouping | Updates / not loaded / installed / available | Yes | ✅ | ✅ | Available grouped by language. |
| Install | Async progress | Yes | ✅ | ✅ | `pending/downloading/installing/done/error/cancelled`. |
| Install | Cancel active install | Yes | ✅ | ✅ | Host job cancellation; `c` in Extensions. |
| Install | Retry failed install | Yes | ✅ | ✅ | `i` clears the error and retries. |
| Install | Old-host blocking fallback | Yes | ✅ | ✅ | Falls back when job API is missing. |
| Install | Desktop JVM jar preference | Yes | ✅ | ✅ | Avoids dex2jar corruption. |
| Not loaded | Section and reason | Yes | ✅ | ✅ | Package, file, reason, message. |
| Not loaded | Inspect / delete failed file | Yes | ✅ | ✅ | `o`, `x`/Delete with confirmation. |
| Trust | Signature verification / trust action | Yes | ❌ | ❌ | Host cannot verify signatures; intentionally not faked. |
| Uninstall | Confirmation and batch uninstall | Yes | ✅ | ✅ | |
| Details | Package, version, store, sources | Yes | ✅ | ✅ | |
| Details | Enable/disable all sources | Yes | ✅ | ✅ | |
| Details | Per-source toggle | Yes | ✅ | ✅ | |
| Details | Per-source preferences | Yes | ✅ | ✅ | |
| Details | Clear cookies | Yes | ✅ | ✅ | Fixed against the 1.6 source API. |
| Details | Per-extension Incognito | Yes | ✅ | ✅ | |
| Warnings | NSFW/content warning indicator | Yes | ✅ | ✅ | `18+` in rows and details. |
| Warnings | Hide NSFW extensions | Yes | ✅ | ✅ | Settings → Browse. |
| Warnings | Private/shared/obsolete indicators | Yes | 🟡 | 🟡 | Private (installed outside a repo) and obsolete flags shown; shared/signature states remain. |
| Languages | Enabled-language filtering | Yes | ✅ | ✅ | Installed entries remain manageable. |
| Search | Comma-separated extension terms | Yes | ✅ | ✅ | Matches names, packages, languages, source names. |
| WebView | WebView-dependent sources | Yes | ❌ | ❌ | Unsupported by design. |
