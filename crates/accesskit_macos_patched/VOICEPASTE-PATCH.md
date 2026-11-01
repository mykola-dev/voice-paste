# AccessKit macOS patch

Vendored `accesskit_macos` 0.26.3 from crates.io. `src/subclass.rs` contains
the upstream AccessKit fix from commit `64e9a47ddaac64c9082214f472fa6fcafbc86430`
(https://github.com/AccessKit/accesskit/pull/835), backported without other
source changes. The upstream regression tests are included with a minimal
main-thread harness in place of `libtest-mimic`. It preserves KVO subclasses
when the adapter is dropped, addressing
an AppKit Touch Bar observer crash. Remove this patch when the pinned GPUI
pipeline adopts a compatible AccessKit release containing the fix.
