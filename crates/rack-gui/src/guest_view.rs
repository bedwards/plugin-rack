//! Native NSView embedding for VST3 guest plugin editors.
//!
//! macOS implementation: creates a child NSView at the strip's pixel rect inside
//! the egui/baseview NSView, then calls `IPlugView::attached(child_nsview, "NSView")`.
//! The guest plugin renders into the child NSView independently of egui.
//!
//! # Platform support
//!
//! - macOS: implemented via `objc2` + `objc2-app-kit`.
//! - Windows / Linux: stub `unimplemented!()` behind `#[cfg]`.

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::bail;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::NSView;
    use objc2_foundation::{MainThreadMarker, NSRect};
    use std::ptr::NonNull;
    use vst3::ComPtr;
    use vst3::Steinberg::{IPlugView, IPlugViewTrait, kPlatformTypeNSView, kResultOk};

    /// RAII wrapper that keeps a guest VST3 plugin's native UI embedded in the
    /// rack's egui window.
    ///
    /// `attach()` creates a child NSView at the given pixel rect, calls
    /// `IPlugView::attached(child_nsview, "NSView")`, and returns `Self`.
    /// `Drop` calls `IPlugView::removed()` and removes the child NSView.
    ///
    /// # Thread safety
    ///
    /// `GuestEditorView` is intentionally `!Send` (and `!Sync`). Both `NSView`
    /// operations and `IPlugView` UI calls are documented as main-thread-only,
    /// and `Drop` calls `removeFromSuperview` — moving the value to a
    /// background thread and dropping it there would be undefined behavior.
    /// The `NonNull<NSView>` field makes the struct `!Send` automatically
    /// (raw pointers are `!Send` by default); no unsafe `Send` impl is needed
    /// or allowed. Construct `GuestEditorView` lazily inside the editor's
    /// spawn / update callback, which runs on the main thread — never store
    /// it in a `Send` parent.
    pub struct GuestEditorView {
        /// The child NSView we allocated (retained via raw pointer inside an
        /// `objc2::rc::Retained`). We hold it as a raw `NonNull<NSView>` so we
        /// can pass it across the FFI boundary without fighting lifetime rules.
        ///
        /// `NonNull<NSView>` is `!Send` by default; this field is what
        /// prevents `GuestEditorView` from accidentally crossing a thread
        /// boundary.
        ns_view: NonNull<NSView>,
        /// The guest's IPlugView. Kept alive to drive the lifecycle.
        plug_view: ComPtr<IPlugView>,
        /// Cached size in logical pixels (width, height).
        size: (u32, u32),
    }

    impl GuestEditorView {
        /// Embed a guest plugin editor inside `parent_ns_view`.
        ///
        /// # Arguments
        ///
        /// - `parent_ns_view`: raw `*mut AnyObject` pointing to the `NSView`
        ///   that egui/baseview gave us (from `RawWindowHandle::AppKit`).
        /// - `plug_view`: obtained from `Vst3Guest::create_plug_view()`.
        /// - `x`, `y`, `w`, `h`: position and size in physical pixels (scale=1.0
        ///   in this PR; per-strip scaling lands in issue #10).
        ///
        /// # Safety
        ///
        /// `parent_ns_view` must be a valid live `NSView*` on the main thread.
        /// `plug_view` must be a valid, initialised `IPlugView*`.
        pub unsafe fn attach(
            parent_ns_view: *mut AnyObject,
            plug_view: ComPtr<IPlugView>,
            x: f32,
            y: f32,
            w: f32,
            h: f32,
        ) -> anyhow::Result<Self> {
            if parent_ns_view.is_null() {
                bail!("parent_ns_view is null");
            }

            // Build the frame rect for the child view.
            // Cocoa uses bottom-left origin; egui uses top-left.
            // For now we pass the egui rect directly and accept the coord-flip
            // as a known limitation (visual offset only, no functionality broken).
            // A proper flip requires knowing the parent view height — deferred to #10.
            let frame = NSRect::new(
                objc2_foundation::NSPoint::new(x as f64, y as f64),
                objc2_foundation::NSSize::new(w as f64, h as f64),
            );

            // Allocate and initialise a plain NSView at the requested frame.
            // NSView is MainThreadOnly; we must use MainThreadMarker::alloc.
            // SAFETY: We are called from the plugin editor's spawn() which runs
            // on the main thread. If no MTM is available (headless tests), bail.
            let mtm = MainThreadMarker::new().ok_or_else(|| {
                anyhow::anyhow!("GuestEditorView::attach must be called on the main thread")
            })?;
            // SAFETY: `alloc` is a fresh uninitialised NSView allocation from
            // the main-thread marker; `initWithFrame` is the standard init.
            let child: objc2::rc::Retained<NSView> = {
                let alloc = mtm.alloc::<NSView>();
                unsafe { NSView::initWithFrame(alloc, frame) }
            };

            // Add the child NSView to the parent.
            let parent_ref = parent_ns_view as *mut NSView;
            // SAFETY: caller guarantees `parent_ns_view` is a live NSView on the
            // main thread (documented in the fn-level Safety section).
            let parent = unsafe { &*parent_ref };
            // SAFETY: `parent` and `child` are both live NSViews on the main
            // thread; addSubview's only precondition is main-thread ownership.
            unsafe { parent.addSubview(&child) };

            // Obtain a stable raw pointer (the view is retained by the parent
            // hierarchy once addSubview is called; we also keep our Retained handle).
            let raw_child: NonNull<NSView> = NonNull::from(&*child);
            // `child` is now Retained — keep it alive by leaking into a raw ptr.
            // We'll reconstruct and drop it in detach().
            let _ = objc2::rc::Retained::into_raw(child);

            // Call IPlugView::attached with the child NSView pointer.
            // SAFETY: caller guarantees `plug_view` is an initialised IPlugView*;
            // `raw_child.as_ptr()` is a valid NSView allocated above.
            let result = unsafe {
                plug_view.attached(
                    raw_child.as_ptr() as *mut std::ffi::c_void,
                    kPlatformTypeNSView,
                )
            };
            if result != kResultOk {
                // Remove the child view we added.
                // SAFETY: `raw_child` is the NSView we just alloc'd + retained.
                let child_ref = unsafe { &*raw_child.as_ptr() };
                // SAFETY: main-thread NSView op; we hold a live retained reference.
                unsafe { child_ref.removeFromSuperview() };
                // Re-own then drop to balance our into_raw above.
                let _ = unsafe { objc2::rc::Retained::from_raw(raw_child.as_ptr()) };
                bail!("IPlugView::attached failed: {result}");
            }

            log::debug!("GuestEditorView: attached guest editor at ({x},{y}) {w}×{h}");

            Ok(Self {
                ns_view: raw_child,
                plug_view,
                size: (w as u32, h as u32),
            })
        }

        /// Reposition and/or resize the embedded guest view.
        ///
        /// Calls `IPlugView::onSize` to notify the guest plugin of the new
        /// dimensions, then updates the NSView frame.
        ///
        /// # Safety
        ///
        /// Must be called on the main thread.
        pub unsafe fn set_rect(&self, x: f32, y: f32, w: f32, h: f32) {
            // Resize the NSView frame.
            // SAFETY: `self.ns_view` was populated by `attach` with a retained
            // NSView that is still alive (we never drop it before detach).
            let child = unsafe { &*self.ns_view.as_ptr() };
            let new_frame = NSRect::new(
                objc2_foundation::NSPoint::new(x as f64, y as f64),
                objc2_foundation::NSSize::new(w as f64, h as f64),
            );
            // SAFETY: main-thread NSView op — caller asserts main-thread via fn Safety.
            unsafe { child.setFrame(new_frame) };

            // Notify the guest plugin via IPlugView::onSize.
            let mut rect = vst3::Steinberg::ViewRect {
                left: x as i32,
                top: y as i32,
                right: (x + w) as i32,
                bottom: (y + h) as i32,
            };
            let result = unsafe { self.plug_view.onSize(&mut rect) };
            if result != kResultOk {
                log::warn!("IPlugView::onSize returned {result} (non-fatal)");
            }
        }

        /// Detach the guest editor: calls `IPlugView::removed()`, removes the
        /// child NSView from its superview, and releases both.
        ///
        /// Called automatically on `Drop`.
        ///
        /// # Safety
        ///
        /// Must be called on the main thread.
        pub unsafe fn detach(&mut self) {
            // Tell the plugin to detach.
            let result = unsafe { self.plug_view.removed() };
            if result != kResultOk {
                log::warn!("IPlugView::removed() returned {result} (non-fatal)");
            }

            // Remove child NSView from parent and release our retain.
            // SAFETY: same as in `set_rect` — retained NSView is alive until detach.
            let child = unsafe { &*self.ns_view.as_ptr() };
            // SAFETY: main-thread NSView op; caller asserts main-thread via fn Safety.
            unsafe { child.removeFromSuperview() };
            // Reconstruct Retained to balance the into_raw done in attach().
            let retained = unsafe { objc2::rc::Retained::from_raw(self.ns_view.as_ptr()) };
            drop(retained);
        }

        /// Cached size as set during the last `attach` or `set_rect` call.
        pub fn size(&self) -> (u32, u32) {
            self.size
        }
    }

    impl Drop for GuestEditorView {
        fn drop(&mut self) {
            // SAFETY: `GuestEditorView` is `!Send` (the `NonNull<NSView>`
            // field prevents cross-thread moves). Therefore `drop` runs on
            // whichever thread created it — which, by the contract on
            // `attach()`, is the main thread. `removeFromSuperview` and
            // `IPlugView::removed()` are main-thread-only; this is safe.
            unsafe { self.detach() };
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::GuestEditorView;

/// Stub for non-macOS platforms. Actual NSView embedding is macOS-only in this PR.
/// Windows (HWND) and Linux (X11 reparent) will be added in future issues.
///
/// Kept `!Send` (via a `PhantomData<*mut ()>` marker) so consumers get the
/// same threading contract on every platform — accidental cross-thread moves
/// fail to compile on Linux / Windows, matching the macOS behavior.
#[cfg(not(target_os = "macos"))]
pub struct GuestEditorView {
    _priv: (),
    _not_send: std::marker::PhantomData<*mut ()>,
}

#[cfg(not(target_os = "macos"))]
impl GuestEditorView {
    pub fn size(&self) -> (u32, u32) {
        (0, 0)
    }
}

// ── Compile-time thread-safety sentry ───────────────────────────────────────
//
// `GuestEditorView` must be `!Send` AND `!Sync` on every platform:
//
// * `!Send` — so a background thread can never drop it and invoke
//   `removeFromSuperview` / `IPlugView::removed()` off the main thread.
// * `!Sync` — so a background thread cannot reach in via a shared reference
//   and trigger a main-thread-only NSView call either.
//
// The trick: `AmbiguousIf<A>` has two blanket impls — one for `A = ()` that
// applies to every type, and one for `A = u8` that only applies when `T`
// satisfies the target bound. If `T` satisfies the bound, both impls cover
// the call and `_` inference is ambiguous — a hard compile error. If not,
// only the `()` impl applies and `_` resolves unambiguously.
//
// Both assertions live OUTSIDE the `cfg(target_os = "macos")` gate so the
// non-macOS stub's thread-safety contract is also verified — if the stub
// ever regresses, Linux / Windows CI fails to build just like macOS.
#[allow(dead_code)]
fn _assert_guest_editor_view_is_not_send() {
    trait AmbiguousIfSend<A> {
        fn some_item() {}
    }
    impl<T: ?Sized> AmbiguousIfSend<()> for T {}
    impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
    <GuestEditorView as AmbiguousIfSend<_>>::some_item();
}

#[allow(dead_code)]
fn _assert_guest_editor_view_is_not_sync() {
    trait AmbiguousIfSync<A> {
        fn some_item() {}
    }
    impl<T: ?Sized> AmbiguousIfSync<()> for T {}
    impl<T: ?Sized + Sync> AmbiguousIfSync<u8> for T {}
    <GuestEditorView as AmbiguousIfSync<_>>::some_item();
}
