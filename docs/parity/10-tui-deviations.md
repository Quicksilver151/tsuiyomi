# Mihon Parity: Documented TUI Deviations

Area detail for Mihon parity. Aggregate scores live in the [roadmap](../roadmap.md).

| Android feature | TUI equivalent | Status | Notes |
|---|---|---|---|
| WebView source page | Copy URL (`C`) / open absolute URL (`o`) | 🟡 | Relative source URLs cannot be opened externally without a base URL. |
| Android share sheet | OSC 52 clipboard copy | ✅ | `C` on story. |
| File picker | Path-entry dialog | ✅ | Custom covers. |
| Swipe actions | Keys and download menu | ➖ | Intentional terminal deviation. |
| Tracker OAuth dialogs | None | ❌ | Separate integration. |
| Android notifications | TUI status bar/queue markers | ➖ | No background daemon notifications. |
