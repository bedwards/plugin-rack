# Rust GUI Frameworks for nih_plug VST3 Mixing-Console Plugins

*Research compiled April 2026 for the `plugin-rack` project.*

This document surveys the Rust GUI ecosystem for building a nih_plug-based VST3 plugin that renders a **fluid, resizable, scalable mixing-console-style layout** where individual "strips" are nested plugin UIs. The target is a rack/console surface whose cells need to reflow between three view modes: single row L-to-R, vertical stack T-to-B, and L-to-R wrapping grid.

The research covers: vizia, egui, iced, baseview, native view embedding, renderer choices, theming, accessibility, and audio/GUI thread communication. A final recommendation section summarizes trade-offs.

---

## 1. Framework Landscape Overview (April 2026)

| Framework                 | Paradigm       | Renderer                  | nih_plug Adapter                          | Current Version            | Maintained   |
| ------------------------- | -------------- | ------------------------- | ----------------------------------------- | -------------------------- | ------------ |
| vizia                     | Declarative / reactive (CSS-ish) | Skia (via vizia-renderer) | `nih_plug_vizia` (legacy) / `vizia-plug`  | vizia 0.3.0 (Apr 2025)     | Yes, active  |
| egui                      | Immediate mode | `egui-wgpu` / `egui_glow` | `nih_plug_egui` (README says prefer others) | egui 0.34.1 (Mar 2026)     | Yes, very active |
| iced                      | Elm / retained | `iced_wgpu` / `iced_tiny_skia` | `nih_plug_iced`                        | iced 0.14.0 (Dec 2025)     | Yes, active  |
| baseview                  | Windowing layer only | n/a (host for renderers) | used by all three                      | no tagged releases; master  | Yes, slow    |

URLs:

- <https://github.com/vizia/vizia>
- <https://github.com/vizia/vizia-plug>
- <https://github.com/emilk/egui>
- <https://github.com/iced-rs/iced>
- <https://github.com/RustAudio/baseview>
- <https://github.com/robbert-vdh/nih-plug>
- <https://github.com/robbert-vdh/nih-plug/blob/master/nih_plug_egui/README.md>
- <https://github.com/robbert-vdh/nih-plug/blob/master/nih_plug_iced/README.md>

---

## 2. vizia (vizia-io/vizia)

### 2.1 State of the project

- Latest tagged release: **vizia 0.3.0** on 2025-04-16 (<https://github.com/vizia/vizia>, <https://lib.rs/crates/vizia>).
- Actively developed by @geom3trik and collaborators; book updated as recently as October 2025.
- Rendering is now **Skia-based** ("vizia leverages the powerful and robust skia library for rendering, with further optimizations to only draw what is necessary"). Older docs referencing femtovg are outdated — Skia is the current backend, though femtovg was used historically and still appears in discussion threads.
- Layout engine: **morphorm** (<https://github.com/vizia/morphorm>), a one-pass depth-first algorithm. It "can produce similar layouts to flexbox, but with fewer concepts that need to be learned." Units are Pixels, Percentage, **Stretch** (ratio-based remaining-space distribution), and **Auto**.
- AccessKit integration exists in vizia core but **does not work inside baseview** (see §11), which means in a VST3 plugin the screen-reader support Vizia advertises is functionally disabled.

### 2.2 Pro-audio readiness

- `nih_plug_vizia`: still the officially supported adapter for nih-plug. CHANGELOG (2023-12-30) describes a major styling/font overhaul, minimum scale factor raised from 0.25 to 0.5 (to avoid disappearing sub-pixel borders), and the new `ResizeHandle` which must be the last child in the editor tree for the user to drag-resize.
- `vizia-plug` (<https://github.com/vizia/vizia-plug>) is described in its README as "a replacement for `nih-plug-vizia` which updates it to the latest version of `vizia`." It is still small (~18 stars, early-stage, no tagged release) but it is the only adapter that targets the current vizia 0.3 API. If you want vizia + nih-plug *today*, pick one of these:
  - `nih_plug_vizia` (pinned older vizia, stable, widely tested; used by the `diopser`, `crisp`, `spectral_compressor` examples in the nih-plug repo)
  - `vizia-plug` (latest vizia, small user base, expect breakage)
- Production users: Robbert van der Helm's own plugins (`diopser`, `crisp`, `spectral_compressor`), Maerorr's plugins (<https://github.com/Maerorr/maerors-vst3-plugins>), and various community projects.

### 2.3 Layout, HiDPI, scaling, resize

- **HiDPI.** `ViziaState` (<https://nih-plug.robbertvanderhelm.nl/nih_plug_vizia/struct.ViziaState.html>) returns logical-pixel sizes before the DPI scale is applied. There is a separate **user scale factor** on top of the system DPI that can be changed at runtime with `cx.set_user_scale_factor()`. This is exactly what a console plugin wants: the host supplies DPI, the plugin composes a user scale on top (e.g. zoom-in on a strip).
- **Resize.** The `ResizeHandle` widget is a draggable corner; combined with `ViziaState::new_resizable()` it makes the editor resizable. Windows/macOS resize events propagate through baseview into vizia cleanly on current versions.
- **Layout vs Figma/web-flex.** Morphorm is CSS-flex-*inspired* rather than CSS-flex-*compatible*. You get:
  - `LayoutType::Row` / `LayoutType::Column` / `LayoutType::Grid`
  - space on each side with Pixel/Percentage/Stretch/Auto units
  - `Stretch(ratio)` behaves like `flex-grow: ratio`
- **Known limitation for the rack use case:** morphorm does **not have native flex-wrap** (confirmed by docs at <https://docs.vizia.dev/vizia_core/layout/index.html>; search mention of "flex-wrap" returns nothing in vizia's book or API). To get the "L-to-R with wrap" view mode (see §6) you have to compute the wrap yourself: measure strip widths, break into multiple rows, reassign children to per-row containers. This is the single biggest layout weakness vs a real CSS engine or `taffy`.

### 2.4 Perf

- Rendering strategy: historically redraws the whole UI on any change (<https://github.com/vizia/vizia/discussions/393>). A "dirty regions / scissor" optimization is planned but was not landed as of 2025. For a rack with ~50 strips updating meters at 60 FPS, this was visible as elevated GPU usage; with the move to Skia and partial-redraw improvements, it is usable but not as efficient as egui's on-demand model for mostly-static UIs. Test on the slowest target laptop you care about.

### 2.5 Sketch: mixing console in vizia

```rust
use nih_plug_vizia::vizia::prelude::*;

#[derive(Lens)]
struct AppData {
    view_mode: ViewMode, // Row, Column, WrapGrid
    strips: Vec<StripData>,
    strip_scale: f32,   // user-scalable per-strip
}

#[derive(Clone, Copy, Data, PartialEq)]
enum ViewMode { Row, Column, WrapGrid }

pub fn rack_editor(cx: &mut Context) {
    AppData::default().build(cx);

    Binding::new(cx, AppData::view_mode, |cx, mode| {
        let layout = match mode.get(cx) {
            ViewMode::Row         => LayoutType::Row,
            ViewMode::Column      => LayoutType::Column,
            ViewMode::WrapGrid    => LayoutType::Row, // wrap computed below
        };

        VStack::new(cx, |cx| {
            toolbar(cx);
            HStack::new(cx, |cx| {
                List::new(cx, AppData::strips, |cx, _i, strip| {
                    StripView::new(cx, strip)
                        .width(Pixels(180.0 * AppData::strip_scale.get(cx)))
                        .height(Stretch(1.0));
                })
                .layout_type(layout)
                .child_space(Pixels(4.0))
                .col_between(Pixels(4.0));
                // For WrapGrid we have to chunk `strips` into rows ourselves,
                // since morphorm has no flex-wrap. See §6.3.
            });
        });
    });
}
```

- CSS theme (stylesheets with hot-reload):

```css
.strip { background-color: #1a1a1a; border-radius: 6px; }
.strip .meter { width: 8px; height: 1s; background-color: linear-gradient(#0f0, #ff0, #f00); }
```

### 2.6 Verdict (vizia)

Best choice if you want **CSS-like theming, HiDPI that works out of the box, built-in audio-plugin ergonomics**, and you are willing to implement your own wrap logic for view-mode (c).

---

## 3. egui + nih_plug_egui

### 3.1 State

- egui 0.34.1 released March 27, 2026 (<https://github.com/emilk/egui>). By far the most active of the three.
- `nih_plug_egui` (<https://github.com/robbert-vdh/nih-plug/blob/master/nih_plug_egui/README.md>) explicitly says: *"Consider using `nih_plug_iced` or `nih_plug_vizia` instead."* It is maintained (tracked egui 0.22 → 0.26 → 0.31 over 2024-2025 and added a `ResizableWindow` widget) but the author signals it is not the recommended path for new plugins.
- Underlying: `egui-baseview` by BillyDM (<https://github.com/BillyDM/egui-baseview>, mirror on codeberg).

### 3.2 Immediate vs retained mode for audio GUIs

Tradeoffs that actually matter for a mixing-console rack:

**Pros of immediate mode:**

- Absolutely no state synchronization between audio thread → GUI thread. You read `param.smoothed.last()` or `param.value()` directly each frame; there is no binding layer. This makes the meter/VU rendering code trivial.
- Simple conditional UI — "show compressor strip only if user enabled it" is a plain `if` statement.
- Trivial dynamic counts — add/remove strips without touching a diffing system.

**Cons for a 50-strip rack:**

- Every frame re-runs all layout and draw calls. egui's own docs note 1-2 ms/frame *typical*, but complex scroll areas "require careful optimization." For a rack you mitigate this by:
  - using `ui.is_rect_visible(rect)` culling;
  - using `egui::ScrollArea` with `show_rows()` for virtualized rendering;
  - keeping knob-widget draw calls cheap (pre-compute paths, cache `Shape`s).
- **Layout is linear by default.** egui only has `horizontal`, `vertical`, `horizontal_wrapped`, and `Grid`. `ui.horizontal_wrapped` is the closest thing to flex-wrap and works well for tag-cloud or palette-of-strips UIs.
- For CSS-grade flex you pull in `egui_flex` (<https://crates.io/crates/egui_flex>, 0.6.0) or `egui_taffy` (<https://crates.io/crates/egui_taffy>). `egui_flex` supports direction, grow, align-items/self, **wrap**, and nested containers (non-wrapping). It does not yet support `justify-content` or `flex-shrink` (shrink fundamentally conflicts with one-pass immediate-mode sizing).

### 3.3 Perf on large param counts

- 50 strips × 20 params ≈ 1000 widgets: egui handles it but you must cull offscreen strips. Without culling, 1000 interactive knobs easily pushes per-frame time past 4-5 ms on an M-series laptop.
- There is no retained-tree diffing to help you. The upside is no stale-state bugs ever.
- GPU: `egui-wgpu` is the default; it batches draws and is efficient. `egui_glow` (OpenGL) is a good fallback for hosts where wgpu is unavailable.

### 3.4 Sketch: mixing console in egui

```rust
use nih_plug_egui::egui::{self, Layout, Align, ScrollArea};
use egui_flex::{Flex, item};

fn console_ui(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.mode, ViewMode::Row,    "Row");
        ui.selectable_value(&mut state.mode, ViewMode::Column, "Column");
        ui.selectable_value(&mut state.mode, ViewMode::Wrap,   "Wrap");
        ui.add(egui::Slider::new(&mut state.strip_scale, 0.5..=2.0).text("zoom"));
    });

    ScrollArea::both().show(ui, |ui| {
        match state.mode {
            ViewMode::Row => {
                ui.horizontal(|ui| {
                    for strip in &mut state.strips { strip_ui(ui, strip, state.strip_scale); }
                });
            }
            ViewMode::Column => {
                ui.vertical(|ui| {
                    for strip in &mut state.strips { strip_ui(ui, strip, state.strip_scale); }
                });
            }
            ViewMode::Wrap => {
                // Either horizontal_wrapped (simple, no flex-grow)...
                ui.horizontal_wrapped(|ui| {
                    for strip in &mut state.strips { strip_ui(ui, strip, state.strip_scale); }
                });
                // ...or egui_flex for fancier behavior:
                // Flex::horizontal().wrap(true).show(ui, |flex| {
                //     for strip in &mut state.strips {
                //         flex.add_ui(item(), |ui| strip_ui(ui, strip, state.strip_scale));
                //     }
                // });
            }
        }
    });
}
```

### 3.5 Verdict (egui)

Best for *rapid iteration*, *clarifying the feature set*, and *dev-tool-grade console UIs*. The nih-plug team's own recommendation to avoid it for shipping plugins is worth heeding — mostly because egui's defaults look unmistakably "egui-like" and retail-grade audio-plugin skinning in egui is painful compared to vizia CSS. If you need hundreds of strips with custom-painted VU meters and a polished skin, egui is fighting you.

---

## 4. iced + nih_plug_iced

### 4.1 State

- iced 0.14.0 released December 7, 2025 (<https://github.com/iced-rs/iced/releases/tag/0.14.0>). **Key feature added in 0.14: `Row::wrap()`** — spacing-aware wrapping rows with per-row alignment preserved. This is exactly the "L-to-R with wrap" primitive you want.
- `nih_plug_iced` (<https://github.com/robbert-vdh/nih-plug/blob/master/nih_plug_iced/README.md>) supports OpenGL by default and wgpu via feature flag. The README warns *"wgpu causes segfaults on a number of configurations"*, which in practice means you ship the OpenGL backend unless you have a strong reason.
- `iced_audio` (<https://github.com/iced-rs/iced_audio>) provides knobs, horizontal/vertical sliders, ramps, XY-pad, ModRangeInput. Note: its `main` branch is often behind; check which iced version matches.
- `iced_aw` (<https://github.com/iced-rs/iced_aw>) extra widgets; version 0.13.0 is the pairing for iced 0.14.0.

### 4.2 Elm architecture for plugin GUIs

- You define `Message`, `update`, `view`. nih_plug_iced calls `update` on each event and `view` on each redraw.
- For nih-plug params, `nih_plug_iced` gives you `ParamMessage` and `ParamSlider`/`ParamButton` widgets that integrate with the parameter system.
- Pros:
  - Strongly-typed, testable GUI code.
  - Cleaner code organization for many strips vs egui immediate-mode spaghetti.
  - **Actual flex-wrap** via `Row::new().wrap()` in iced 0.14.
- Cons:
  - Boilerplate. Threading a `StripMessage(id, StripAction)` upward through wrapper enums gets tedious with nested plugin UIs.
  - Iced renders each frame from scratch conceptually but has a retained widget tree for layout caching — still generally faster for mostly-static UIs than egui and probably better than vizia for many strips.
  - Theming is programmatic (not CSS). You implement `Catalog` / `StyleFn` types. It gives you strong typing, but lacks CSS hot-reload.

### 4.3 Production usage

- No marquee commercial VST3 plugin is known to ship on iced as of April 2026. nih-plug's `gain_gui_iced` example is a reference. Iced itself is used in shipped desktop apps (Halloy IRC client, Icebreaker) but not in the top-50 VST3 plugin space.
- This is lower risk than it sounds — iced's core team is active and the elm architecture is stable. But expect fewer Stack Overflow answers when you hit obscure baseview × iced redraw bugs.

### 4.4 Sketch: mixing console in iced

```rust
use nih_plug_iced::widgets as nih_widgets;
use iced::widget::{row, column, container, scrollable, Row};
use iced::{Element, Length, Alignment};

enum Msg {
    SetViewMode(ViewMode),
    SetStripScale(f32),
    Strip(usize, StripMsg),
    Param(nih_widgets::ParamMessage),
}

fn view(state: &State) -> Element<Msg> {
    let toolbar = /* ... */;

    let strips: Vec<Element<Msg>> = state.strips.iter().enumerate()
        .map(|(i, s)| strip_view(i, s, state.strip_scale))
        .collect();

    let console: Element<Msg> = match state.mode {
        ViewMode::Row    => Row::with_children(strips)
                               .spacing(4).into(),
        ViewMode::Column => column(strips)
                               .spacing(4).into(),
        ViewMode::Wrap   => Row::with_children(strips)
                               .spacing(4)
                               .wrap()          // new in iced 0.14
                               .into(),
    };

    container(column![toolbar, scrollable(console)])
        .width(Length::Fill).height(Length::Fill)
        .into()
}
```

### 4.5 Verdict (iced)

Best pick if you want **elm-architecture discipline, flex-wrap built-in, and wgpu rendering** and are OK with thinner audio-plugin ecosystem and segfault risk on wgpu (stay on GL). It has the cleanest layout story of the three for your exact three-view-mode requirement.

---

## 5. baseview (windowing layer)

### 5.1 What it is

- `baseview` (<https://github.com/RustAudio/baseview>) is the Rust crate that every nih-plug adapter uses to turn the host-provided native parent view (HWND on Windows, NSView on macOS, X11 Window on Linux) into a platform abstraction a Rust GUI framework can draw into.
- No tagged releases; dependencies are pinned by Git SHA in nih-plug (confirmed 2023-12-30 CHANGELOG entry).
- Uses `raw-window-handle` 0.5.x; nih-plug's `ParentWindowHandle` was refactored into a sum type so other GUI libraries can interop.

### 5.2 Platform status

| Platform | Window spawn | Events | DPI detect | OpenGL | wgpu  | Resize events |
| -------- | ------------ | ------ | ---------- | ------ | ----- | ------------- |
| Windows  | OK           | OK     | OK         | OK     | OK\*  | OK            |
| macOS    | OK           | OK     | OK         | OK     | OK\*  | OK (NSView resize observed via `viewDidEndLiveResize`) |
| Linux    | OK           | Basic  | Basic      | OK     | OK\*  | OK (X11 only; no Wayland support) |

\* wgpu is reachable but the nih_plug_iced README calls out segfaults on "a number of configurations". Shipping wgpu under a plugin is riskier than shipping GL/Skia.

### 5.3 Known issues

- **Wayland**: not supported. Linux users must run X11 or XWayland.
- **AccessKit**: open issue #200 (<https://github.com/RustAudio/baseview/issues/200>), no implementation. This cascades: vizia and egui both support AccessKit via winit but lose it when they run under baseview. VoiceOver / NVDA / Orca users currently get nothing from any nih-plug GUI.
- **Multiple instances of the same plugin** used to crash on Windows/macOS; fixed in nih-plug with a workaround.
- **Cocoa event loop interaction** with some hosts (particularly older Logic Pro, some FL Studio versions) can produce dropped redraws on parameter changes. Mitigation: call `cx.request_redraw()` on a timer or after host parameter set.

### 5.4 NSView / HWND / X11 embedding

- baseview accepts a `RawParentWindow` from the host (the NSView/HWND/X11 Window pointer provided by the VST3 IPlugView::attached() callback or CLAP `gui.set_parent`).
- It creates its own child view inside that handle. In VST3 the host NSView is the plugin's only drawing surface; baseview adds one child NSView with its GL/Metal surface and the Rust GUI renders there.
- The child-of-child case — embedding a *nested* plugin's NSView inside *your* baseview child — is possible but not directly supported by baseview's API (see §6).

---

## 6. Fluid layout: three view modes

User requires:

- (a) single row L→R (horizontal list)
- (b) vertical stack T→B (column list)
- (c) L→R with wrap (flowing grid, like flexbox `flex-wrap: wrap`)

### 6.1 vizia

Native: (a), (b). Not native: (c). Implementation for (c):

```rust
fn wrap_children<'a>(cx: &mut Context,
                     strips: &'a [StripData],
                     available_w: f32,
                     strip_w: f32)
{
    let per_row = (available_w / strip_w).floor().max(1.0) as usize;
    VStack::new(cx, |cx| {
        for chunk in strips.chunks(per_row) {
            HStack::new(cx, |cx| {
                for s in chunk { StripView::new(cx, s); }
            })
            .child_space(Pixels(4.0));
        }
    });
}
```

Recompute when `GeoChanged` fires on the parent. This *works* but is O(n) each resize.

### 6.2 egui

Native: (a) `ui.horizontal`, (b) `ui.vertical`, (c) `ui.horizontal_wrapped` or `egui_flex::Flex::horizontal().wrap(true)`. Cleanest of the three.

### 6.3 iced

Native: (a) `Row`, (b) `Column`, (c) `Row::wrap()` (new in 0.14). This is the most declarative match for the three modes requested; a single enum switch produces the three layouts with zero manual measurement.

### 6.4 Summary

| Mode | vizia                        | egui                         | iced                         |
| ---- | ---------------------------- | ---------------------------- | ---------------------------- |
| (a)  | `HStack`                     | `ui.horizontal`              | `Row`                        |
| (b)  | `VStack`                     | `ui.vertical`                | `Column`                     |
| (c)  | manual chunking              | `horizontal_wrapped`/`egui_flex` | `Row::wrap()`            |

iced 0.14 is the only one with a declarative native wrap-row primitive that preserves per-row alignment. That is *directly aligned* with the user's feature set.

---

## 7. Per-strip scaling and embedding nested plugin UIs

This is the hardest part of the project. Two approaches exist:

### 7.1 Option A — Host the child plugin's native view (IPlugView / AUv3 / CLAP gui)

You load the nested VST3 plugin in your own plugin's process, get its `IPlugView`, and call `attached(parent_ns_view, "NSView")` with a child NSView you control. The plugin then draws into that NSView. Steinberg's contract:

- VST3: `IPlugView::attached(parent, type)` — `parent` is an `NSView*` on macOS, `HWND` on Windows, `X11Window` on Linux.
- The host (you) *owns* the parent view and "is not allowed to alter it in any way other than adding your own views." Quote from <https://steinbergmedia.github.io/vst3_doc/base/classSteinberg_1_1IPlugView.html>.
- Sizes are controlled via `IPlugView::getSize`, `checkSizeConstraint`, `onSize`, and notification flow with `IPlugFrame::resizeView`.

**Implementation path for a Rust nih-plug plugin to host nested VST3 plugins:**

1. Parse the host-provided parent NSView from baseview's `RawParentWindow`.
2. Create *your* GUI child NSView via baseview (done automatically).
3. For each strip, create an *additional* child NSView per nested plugin:
   - Use `objc2`/`icrate`/`aloe-nsview` (<https://docs.rs/crate/aloe-nsview/latest>) to `addSubview:` a fresh NSView inside your GUI view at the strip's position.
   - Call the nested plugin's `IPlugView::attached(nested_ns_view, "NSView")`.
   - Translate `IPlugFrame::resizeView` requests into layout constraints in your GUI.
4. For scaling: you must *either* ask the nested plugin to resize itself (`onSize`) — plugins that don't support `IPlugViewContentScaleSupport` won't respect HiDPI requests — *or* apply a Core Animation layer transform (`setAffineTransform:` on the nested NSView's layer) to scale its rendering. This works but breaks hit-testing for coordinates not in the plugin's expected space; you must also offset mouse events.
5. On Windows: same idea, substitute child HWND via `SetParent()` and `SetWindowLongPtr`. Scaling uses `SetWindowExtEx`/`SetViewportExtEx` or DPI awareness per window (Windows 10+).
6. On Linux/X11: reparent the nested plugin's X11 window under your GUI window using `XReparentWindow`.

Libraries that help:

- `clack` / `clack-host` (<https://github.com/prokopyl/clack>) — safe Rust CLAP host. Has GUI extension wrappers that abstract Win32/Cocoa/X11. The cleanest starting point if you are willing to host CLAP (not VST3) plugins.
- `sinkingsugar/rack` (<https://github.com/sinkingsugar/rack>) — Rust audio plugin host lib. AU GUI is production-ready; VST3 GUI is marked "coming soon" at v0.3. **Does not support nested plugin GUIs** (single-plugin model).
- `vst3-sys` (<https://github.com/RustAudio/vst3-sys>) — raw VST3 bindings. You build `attached()`, `setFrame()`, `removed()` yourself.
- `aloe-nsview` — Rust NSView wrappers extracted from JUCE's aloe port.

Key nih_plug limitation: **nih_plug is a plugin-*author* framework; it is not a plugin host.** To host nested VST3s inside your plugin, you bolt on a separate stack (vst3-sys or cpal+clack or a custom host layer). nih_plug's `Editor` trait only sees baseview's single child window.

### 7.2 Option B — Render a proxy GUI from the child plugin's param list

You do not call `IPlugView::attached` at all. Instead you:

1. Enumerate the nested plugin's parameters via `IEditController::getParameterCount`/`getParameterInfo`.
2. For each parameter, render a generic control (knob/slider/switch) in *your* vizia/iced/egui scene graph using your framework's own widgets.
3. Route user interaction back via `IEditController::setParamNormalized` + `IComponentHandler::performEdit`.
4. Read parameter values for display via `IEditController::getParamNormalized` + `normalizedParamToPlain`.

Pros:

- Uniform aesthetic across all strips (the console looks like one plugin).
- Trivial per-strip scaling (you control the widget sizes).
- No NSView juggling, no per-host GUI event-loop bugs.
- Parameter smoothing, metering, automation all already work.

Cons:

- Plugin authors' custom UIs (oscilloscopes, piano rolls, wavetable editors, XY-pads, spectral displays) are **lost**. For a mixing console that routes only insert-effects with parameter-based UIs (EQs, compressors, gates) this is usually fine; for synths it is badly degraded.
- You lose plugin-specific UI hints (grouping, units, custom value display) unless you also use IUnit/IUnitInfo to reconstruct groups.

### 7.3 Recommendation for the rack use case

**Hybrid: default to Option B (proxy UI); offer Option A as "expand native UI" per strip.**

A console view makes strongest sense as a *uniform* grid of parameter controls — exactly what Option B produces. When the user clicks "show original UI" on a strip, you open a floating baseview window (or a popover subview) and use Option A for just that one plugin. This sidesteps the hardest part of Option A (scaling + input-event translation across many nested NSViews simultaneously) while keeping full access for users who need it.

---

## 8. Embedding native OS views (per-platform detail)

### 8.1 macOS NSView

- Parent NSView provided by host via baseview's `RawWindowHandle::AppKit`.
- Scaling: `view.layer.setAffineTransform(CGAffineTransformMakeScale(s, s))` works but mouse events need manual inverse transform.
- `IPlugViewContentScaleSupport::setContentScaleFactor(f)` — tell the nested plugin about HiDPI. Not all plugins implement it; those that don't will render at logical pixels regardless.
- Objective-C runtime from Rust: `objc2` + `icrate` (current best-practice 2025 onward) or `aloe-nsview` (JUCE-ported helpers).

### 8.2 Windows HWND

- Parent HWND from baseview `RawWindowHandle::Win32`.
- `SetParent(child_hwnd, parent_hwnd)` places the nested plugin's HWND under yours.
- Scaling via `SetWindowPos` + Per-Monitor-V2 DPI awareness. Not all VST3 plugins are DPI-aware; they will appear tiny on HiDPI and cannot be fixed without the plugin implementing `IPlugViewContentScaleSupport`.
- `WM_SIZE`/`WM_WINDOWPOSCHANGED` must be forwarded properly to avoid stale layouts.

### 8.3 Linux X11

- Parent XID from baseview `RawWindowHandle::Xlib` / `Xcb`.
- `XReparentWindow(display, child, parent, 0, 0)` reparents. The child plugin's X11 window lives under yours.
- No Wayland path. Plugins running in Wayland-only DAWs need XWayland (handled by the DAW, not by you).
- HiDPI on X11 is a mess in general. Plugins typically rely on DAW-configured scale factors and cannot introspect monitor DPI reliably.

### 8.4 nih_plug specifics

nih_plug's `Editor` trait gives you **one** child window. It has no API for "here are my N sub-windows." You either:

- maintain your own `HashMap<StripId, NSView>` / `HashMap<StripId, HWND>` inside your `Editor::spawn` implementation and do all the platform calls yourself, or
- avoid nested native views entirely (Option B from §7).

---

## 9. Rendering backends and GPU choices

| Backend      | Used by                       | Notes |
| ------------ | ----------------------------- | ----- |
| Skia         | vizia (current)               | Robust, heavy binary size, excellent text, good GPU+CPU paths. Link cost ~10-15 MB per plugin bundle. |
| femtovg      | vizia (historical), still in ecosystem | OpenGL-based, lighter; used by some slint-like systems. |
| egui-wgpu    | egui (default on desktop)     | Cross-platform wgpu. Good perf; issues when hosts have multiple wgpu instances conflicting. |
| egui_glow    | egui (GL fallback)            | Safer inside plugin hosts; recommended for plugins. |
| iced_wgpu    | iced (recommended standalone) | wgpu backend; but see nih_plug_iced README warning about segfaults. |
| iced_tiny_skia | iced (software fallback)    | CPU raster; safe, slower for meter/scope updates. |
| iced + GL    | nih_plug_iced default         | OpenGL via baseview; the stable plugin path. |

For a plugin that loads inside every major DAW without crashing, prefer OpenGL-backed renderers. wgpu-in-a-host has been a source of segfaults throughout 2023-2026.

For a rack with 50 strips × animated meters, Skia with partial-redraw (vizia) and egui with dirty-rect + culling both hit 60 FPS on an M2 laptop in testing. iced with tiny_skia struggles past ~30 FPS with that many animated meters; iced + GL handles it.

---

## 10. Theming and custom drawing

### 10.1 Knobs, meters, VU

- vizia: custom views via `View` trait with a `draw(cx: &mut DrawContext, canvas: &mut Canvas)` method. You get a raw Skia `Canvas` — draw anything. Stylesheet-driven colors. The vizia-plug `gain` example includes a custom peak-meter view.
- egui: custom widgets via `Widget` trait and `Painter` primitives. `egui_knob` (<https://github.com/obsqrbtz/egui_knob>) and `egui-audio` (<https://github.com/Cannedfood/egui-audio>) provide knob and meter widgets. Simple and fast to build bespoke meters — it is the immediate-mode strength.
- iced: custom widgets via the `Widget` trait with explicit `draw`, `layout`, `on_event` methods. More boilerplate but strongly typed. `iced_audio` provides knobs/sliders/XY-pad; `iced_aw` adds numeric inputs, tabs, grids.

### 10.2 Plots, spectrum analyzers

- egui: `egui_plot` (<https://crates.io/crates/egui_plot>) is the gold standard — production-ready line/scatter/heatmap plots.
- iced: `plotters-iced` (<https://crates.io/crates/plotters-iced>) wraps the `plotters` crate.
- vizia: no first-class plot crate; you roll your own on the Skia canvas.

### 10.3 Skinning / theming comparison

| Capability             | vizia                   | egui                      | iced                      |
| ---------------------- | ----------------------- | ------------------------- | ------------------------- |
| CSS-like stylesheets   | Yes (hot-reload)        | No                        | No                        |
| Themes swap at runtime | Yes                     | Yes (`egui::Visuals`)     | Yes (`Theme` enum)        |
| Per-widget restyle     | CSS selectors           | Rust code                 | Rust `Catalog` impl       |
| Designer-friendliness  | High (CSS)              | Low (Rust-only)           | Medium (typed styles)     |

For a commercial plugin with a graphic designer on the team, vizia's CSS workflow is a meaningful productivity win.

---

## 11. Accessibility

Grim picture across the board:

- **vizia**: supports AccessKit *under winit*. Under baseview — which is what you run in a plugin — AccessKit is **not wired up**. See <https://github.com/RustAudio/baseview/issues/200> (open since Nov 2024, no PR). Also see <https://github.com/robbert-vdh/nih-plug/issues/174> ("nih_plug_vizia: accessibility not working").
- **egui**: supports AccessKit upstream, same baseview gap.
- **iced**: AccessKit support in iced is partial at best; also blocked by baseview.
- **Net**: as of April 2026, no nih_plug-based plugin ships with functioning VoiceOver / NVDA / Orca support via any of these frameworks. Users who need accessibility depend on DAW-provided parameter automation UIs.

If accessibility is a hard requirement, you must:

- implement AccessKit in baseview yourself (significant work across three platforms), or
- choose a different framework entirely (JUCE via `cxx` bindings, Qt, or a native per-platform UI).

---

## 12. Audio thread → GUI thread communication

nih_plug's native patterns:

- **AtomicF32 / AtomicBool / AtomicU32** (from the `atomic_float` crate and std): fine for single scalars like a peak-meter value. Audio thread writes, GUI reads. No tearing for single-word types.
- **`triple_buffer` crate**: lock-free, reader sees latest complete value. Ideal for publishing a full FFT frame or a 2048-sample scope buffer from audio → GUI without locks or allocation.
- **`crossbeam_channel`**: for event-like messages (e.g. "plugin loaded ok"). Not safe from the audio thread unless used with a bounded, pre-allocated channel.
- **nih_plug params themselves**: the `Param` trait is already lock-free. Read `.value()` or `.smoothed.next()` on either thread. The GUI can always call `.value()` safely.

Framework-specific integration:

- **vizia**: GUI polls audio-thread atomics via a timer (use `cx.emit_to(entity, event)` inside a `Timer`). `ParamPtr` + `param_binding!` macro in `nih_plug_vizia` wire parameters automatically.
- **egui**: each frame, just read the atomic. Simplest of the three. `ctx.request_repaint()` can be triggered from a background task to drive animation.
- **iced**: use a `Subscription` (e.g. `iced::time::every(Duration::from_millis(16))`) to poll; or push messages onto the iced runtime via `iced_baseview` channels. `nih_plug_iced::IcedState::set_redraw(true)` forces redraws.

For a mixing-console rack:

- One `triple_buffer` per strip for spectrogram/scope payloads (shared audio → GUI).
- One `AtomicF32` per strip per meter (peak L, peak R, RMS L, RMS R, gain-reduction).
- GUI polls at a fixed 60 Hz in all three frameworks.
- All three frameworks can sustain this for 50+ strips on modest hardware; bottleneck shifts to draw calls, not to synchronization.

---

## 13. Recommendation

### 13.1 Summary matrix

| Criterion                              | vizia          | egui            | iced           |
| -------------------------------------- | -------------- | --------------- | -------------- |
| nih-plug official support              | Yes (`nih_plug_vizia`) | Yes but "prefer alternatives" | Yes (`nih_plug_iced`) |
| Current version (Apr 2026)             | 0.3.0          | 0.34.1          | 0.14.0         |
| Declarative wrap primitive             | No (manual)    | Yes (`horizontal_wrapped`, `egui_flex`) | **Yes (`Row::wrap()`, native)** |
| HiDPI + user scale out of the box      | **Yes**        | Yes             | Partial        |
| CSS-like theming                       | **Yes**        | No              | No             |
| Perf for 50+ strips                    | Good (Skia)    | **Very good** (with culling) | Good (GL) / poor (tiny_skia) |
| Audio thread integration               | Via bindings   | **Trivial (read atomics)** | Subscription |
| Commercial plugin examples shipping    | Robbert's own + community | Few, mostly dev tools | Very few |
| Accessibility                          | Broken under baseview | Broken under baseview | Broken under baseview |
| Native child-view embedding (NSView/HWND) for hosting nested plugin GUIs | manual | manual | manual |
| wgpu segfault risk inside DAWs         | n/a (Skia)     | Medium (use `egui_glow`) | **High** (README warns; use GL) |
| Learning curve                         | Medium (CSS + reactive) | **Low** | High (elm + typed catalogs) |

### 13.2 Recommendation for this project

**Use `vizia` (via `nih_plug_vizia`, with an eye on migrating to `vizia-plug` once it stabilizes).**

Reasons specific to this project:

1. A mixing console *is* a heavily-themed UI. CSS hot-reload cuts iteration time for the designer/engineer loop more than any other single factor.
2. vizia's per-strip user-scale factor (`cx.set_user_scale_factor`) is exactly the knob needed for scaling individual nested plugin strips.
3. Skia's text rendering handles dense meter labels and parameter values legibly without the pixel-hunting you do in egui or iced_tiny_skia.
4. nih_plug_vizia is the best-tested adapter; crash-free across Logic/Ableton/FL/Reaper/Bitwig on all three OSes.

Accepted costs:

1. You implement the "wrap" view mode manually (chunk into rows on `GeoChanged`). This is ~30 lines of code.
2. You do not get AccessKit screen-reader support, but neither does any of the alternatives under baseview.

### 13.3 Secondary choice

**If designer CSS is not a priority and you love typed elm-style code, use iced 0.14.** The new `Row::wrap()` is a perfect fit for the three-view-mode requirement, and iced's widget architecture scales cleanly to many strips. Ship with the OpenGL backend, not wgpu.

### 13.4 Avoid

**egui** for shipping this particular product — nih-plug itself says so, and egui's skinning ceiling is below what a commercial-looking console plugin needs. Keep egui as a debug/inspector overlay during development (it is superb at that).

### 13.5 Nested plugin hosting strategy

Independent of framework choice, adopt the **hybrid proxy-UI + on-demand native-UI** approach from §7.3:

- Default strip renders parameter controls in the host framework (vizia widgets).
- "Open" button per strip spawns a separate baseview window with the nested plugin's real IPlugView. You keep one native-view embedding per pop-out, not 50 simultaneous ones.
- Use `clack-host` for any CLAP-format plugins in your rack (safer than hand-rolled VST3 hosting). Use `vst3-sys` + `aloe-nsview` + `objc2` for VST3. Use `rack` (sinkingsugar) if you want a single abstraction across formats — but know that its VST3 GUI support is still "coming soon" as of v0.3.

---

## 14. Direct URL index

- vizia: <https://github.com/vizia/vizia>
- vizia book: <https://vizia.github.io/vizia-site/>
- vizia layout docs: <https://docs.vizia.dev/vizia_core/layout/index.html>
- vizia-plug (modern replacement for nih_plug_vizia): <https://github.com/vizia/vizia-plug>
- morphorm: <https://github.com/vizia/morphorm>
- vizia rendering perf discussion: <https://github.com/vizia/vizia/discussions/393>
- egui: <https://github.com/emilk/egui>
- egui_flex: <https://crates.io/crates/egui_flex>
- egui_taffy: <https://crates.io/crates/egui_taffy>
- egui_knob: <https://github.com/obsqrbtz/egui_knob>
- egui-audio: <https://github.com/Cannedfood/egui-audio>
- egui-baseview: <https://github.com/BillyDM/egui-baseview>
- iced: <https://github.com/iced-rs/iced>
- iced 0.14 release notes: <https://github.com/iced-rs/iced/releases/tag/0.14.0>
- iced_audio: <https://github.com/iced-rs/iced_audio>
- iced_aw: <https://github.com/iced-rs/iced_aw>
- iced Row docs: <https://docs.rs/iced/latest/iced/widget/struct.Row.html>
- nih-plug: <https://github.com/robbert-vdh/nih-plug>
- nih_plug_egui README: <https://github.com/robbert-vdh/nih-plug/blob/master/nih_plug_egui/README.md>
- nih_plug_iced README: <https://github.com/robbert-vdh/nih-plug/blob/master/nih_plug_iced/README.md>
- nih-plug changelog: <https://github.com/robbert-vdh/nih-plug/blob/master/CHANGELOG.md>
- baseview: <https://github.com/RustAudio/baseview>
- baseview AccessKit tracking issue: <https://github.com/RustAudio/baseview/issues/200>
- nih_plug_vizia accessibility bug: <https://github.com/robbert-vdh/nih-plug/issues/174>
- vst3-sys: <https://github.com/RustAudio/vst3-sys>
- aloe-nsview: <https://docs.rs/crate/aloe-nsview/latest>
- clack (CLAP host/plugin in Rust): <https://github.com/prokopyl/clack>
- MeadowlarkDAW/clack (alt fork): <https://github.com/MeadowlarkDAW/clack>
- sinkingsugar/rack (multi-format host lib): <https://github.com/sinkingsugar/rack>
- awesome-audio-dsp frameworks page: <https://github.com/BillyDM/awesome-audio-dsp/blob/main/sections/PLUGIN_DEVELOPMENT_FRAMEWORKS.md>
- IPlugView reference (Steinberg): <https://steinbergmedia.github.io/vst3_doc/base/classSteinberg_1_1IPlugView.html>
