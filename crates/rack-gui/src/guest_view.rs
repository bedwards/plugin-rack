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
    pub struct GuestEditorView {
        /// The child NSView we allocated (retained via raw pointer inside an
        /// `objc2::rc::Retained`). We hold it as a raw `NonNull<NSView>` so we
        /// can pass it across the FFI boundary without fighting lifetime rules.
        ns_view: NonNull<NSView>,
        /// The guest's IPlugView. Kept alive to drive the lifecycle.
        plug_view: ComPtr<IPlugView>,
        /// Cached size in logical pixels (width, height).
        size: (u32, u32),
    }

    // SAFETY: GuestEditorView is used on the main (GUI) thread only.
    // NSView and ComPtr<IPlugView> are both documented as main-thread-only for
    // UI operations; the caller must ensure single-thread access.
    unsafe impl Send for GuestEditorView {}

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
            let child: objc2::rc::Retained<NSView> = {
                let alloc = mtm.alloc::<NSView>();
                NSView::initWithFrame(alloc, frame)
            };

            // Add the child NSView to the parent.
            let parent_ref = parent_ns_view as *mut NSView;
            let parent = unsafe { &*parent_ref };
            parent.addSubview(&child);

            // Obtain a stable raw pointer (the view is retained by the parent
            // hierarchy once addSubview is called; we also keep our Retained handle).
            let raw_child: NonNull<NSView> = NonNull::from(&*child);
            // `child` is now Retained — keep it alive by leaking into a raw ptr.
            // We'll reconstruct and drop it in detach().
            let _ = objc2::rc::Retained::into_raw(child);

            // Call IPlugView::attached with the child NSView pointer.
            let result = plug_view.attached(
                raw_child.as_ptr() as *mut std::ffi::c_void,
                kPlatformTypeNSView,
            );
            if result != kResultOk {
                // Remove the child view we added.
                let child_ref = unsafe { &*raw_child.as_ptr() };
                child_ref.removeFromSuperview();
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
            let child = unsafe { &*self.ns_view.as_ptr() };
            let new_frame = NSRect::new(
                objc2_foundation::NSPoint::new(x as f64, y as f64),
                objc2_foundation::NSSize::new(w as f64, h as f64),
            );
            child.setFrame(new_frame);

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
            let child = unsafe { &*self.ns_view.as_ptr() };
            child.removeFromSuperview();
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
            // SAFETY: Drop is called on whichever thread owns the value.
            // Callers must ensure GuestEditorView is dropped on the main thread.
            unsafe { self.detach() };
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::GuestEditorView;

/// Stub for non-macOS platforms. Actual NSView embedding is macOS-only in this PR.
/// Windows (HWND) and Linux (X11 reparent) will be added in future issues.
#[cfg(not(target_os = "macos"))]
pub struct GuestEditorView {
    _priv: (),
}

#[cfg(not(target_os = "macos"))]
impl GuestEditorView {
    pub fn size(&self) -> (u32, u32) {
        (0, 0)
    }
}
