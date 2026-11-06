//! Original CompactTabStrip hosted inside the existing winit content view.
//!
//! AppKit owns interaction/animation; the browser remains authoritative for tabs.
//! Callbacks only enqueue `NativeTabEvent` through winit, never call BrowserApp.
//! Every native operation (including destruction) is confined to the main thread.

use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NativeTabItem {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub secure: bool,
    pub pinned: bool,
    /// False while moving the tab would break a supervised session's owner.
    pub transferable: bool,
    /// Whether this tab's original native address field may accept user edits.
    /// The host must revalidate this policy when consuming a queued Search event.
    pub editable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeTabAction {
    Select,
    Close,
    New,
    Move,
    Pin,
    Detach,
    Transfer,
    Reload,
    Search,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NativeTabEvent {
    pub window_token: u64,
    pub action: NativeTabAction,
    pub tab_id: u64,
    pub target_index: usize,
    pub pinned: bool,
    pub dest_window_token: u64,
    /// AppKit global screen points: bottom-left origin, not winit physical pixels.
    pub screen_x: f64,
    pub screen_y: f64,
    pub text: Option<String>,
}

/// Pixels from the actual AppKit strip, not a Rust reconstruction.
#[derive(Debug)]
pub struct NativeTabCapture {
    /// Top-to-bottom, tightly packed, straight-alpha sRGB RGBA8.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Logical content-view coordinates, measured from its top-left.
    pub x: f64,
    pub y: f64,
    pub logical_width: f64,
    pub logical_height: f64,
}

impl NativeTabCapture {
    /// Composite only the native row into a real renderer screenshot. The target
    /// must also be straight-alpha RGBA8. Backing-scale differences are resampled.
    pub fn composite_into(
        &self,
        rgba: &mut [u8],
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<(), String> {
        let required = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("screenshot dimensions overflow")?;
        let source_len = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("native capture dimensions overflow")?;
        if rgba.len() != required || self.rgba.len() != source_len {
            return Err("RGBA buffer length does not match dimensions".into());
        }
        if !scale.is_finite()
            || scale <= 0.0
            || !self.x.is_finite()
            || !self.y.is_finite()
            || !self.logical_width.is_finite()
            || !self.logical_height.is_finite()
            || self.logical_width <= 0.0
            || self.logical_height <= 0.0
            || self.width == 0
            || self.height == 0
        {
            return Err("invalid native capture frame or scale".into());
        }
        let left = self.x * scale;
        let top = self.y * scale;
        let span_x = self.logical_width * scale;
        let span_y = self.logical_height * scale;
        if ![left, top, span_x, span_y, left + span_x, top + span_y]
            .iter()
            .all(|n| n.is_finite())
        {
            return Err("native capture scaled frame overflows".into());
        }
        let start_x = left.floor().clamp(0.0, f64::from(width)) as u32;
        let start_y = top.floor().clamp(0.0, f64::from(height)) as u32;
        let end_x = (left + span_x).ceil().clamp(0.0, f64::from(width)) as u32;
        let end_y = (top + span_y).ceil().clamp(0.0, f64::from(height)) as u32;
        for dy in start_y..end_y {
            let sy = (((f64::from(dy) + 0.5 - top) / span_y) * f64::from(self.height))
                .floor()
                .clamp(0.0, f64::from(self.height - 1)) as usize;
            for dx in start_x..end_x {
                let sx = (((f64::from(dx) + 0.5 - left) / span_x) * f64::from(self.width))
                    .floor()
                    .clamp(0.0, f64::from(self.width - 1)) as usize;
                let src = &self.rgba[(sy * self.width as usize + sx) * 4..][..4];
                let dst = &mut rgba[(dy as usize * width as usize + dx as usize) * 4..][..4];
                let sa = u32::from(src[3]);
                let da = u32::from(dst[3]);
                let alpha_numerator = sa * 255 + da * (255 - sa);
                if alpha_numerator == 0 {
                    dst.fill(0);
                    continue;
                }
                for channel in 0..3 {
                    let color = u32::from(src[channel]) * sa * 255
                        + u32::from(dst[channel]) * da * (255 - sa);
                    dst[channel] = ((color + alpha_numerator / 2) / alpha_numerator) as u8;
                }
                dst[3] = ((alpha_numerator + 127) / 255) as u8;
            }
        }
        Ok(())
    }
}

/// `Rc` phantom deliberately makes this wrapper !Send and !Sync. Its Window
/// stays alive until after native tracking/hosting views and callbacks detach.
pub struct NativeTabStrip {
    #[cfg(target_os = "macos")]
    handle: std::ptr::NonNull<std::ffi::c_void>,
    #[cfg(target_os = "macos")]
    _callback: Box<mac::CallbackContext>,
    _window: Arc<Window>,
    items: Vec<NativeTabItem>,
    selection: Option<u64>,
    frame: Cell<(f64, f64, f64, f64)>,
    _main_thread_only: PhantomData<Rc<()>>,
}

impl NativeTabStrip {
    pub fn new<T: Send + 'static>(
        window: Arc<Window>,
        window_token: u64,
        proxy: EventLoopProxy<T>,
        wrap: fn(NativeTabEvent) -> T,
    ) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            mac::require_main_thread()?;
            let view = mac::content_view(&window)?;
            let mut callback = Box::new(mac::CallbackContext {
                enqueue: Box::new(move |event| proxy.send_event(wrap(event)).is_ok()),
            });
            // SAFETY: content view belongs to the retained Window; callback is
            // boxed at a stable address until synchronous destroy returns.
            let handle = unsafe {
                mac::walkaway_tabs_create(
                    view,
                    window_token,
                    mac::callback,
                    (&mut *callback as *mut mac::CallbackContext).cast(),
                )
            };
            let handle = std::ptr::NonNull::new(handle).ok_or_else(mac::last_error)?;
            Ok(Self {
                handle,
                _callback: callback,
                _window: window,
                items: Vec::new(),
                selection: None,
                frame: Cell::new((0.0, 74.0, 0.0, 38.0)),
                _main_thread_only: PhantomData,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (window, window_token, proxy, wrap);
            Err("original native CompactTabStrip is available only on macOS".into())
        }
    }

    pub fn update(&mut self, items: &[NativeTabItem], selected: u64) -> Result<(), String> {
        if self.items == items && self.selection == Some(selected) {
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        {
            mac::require_main_thread()?;
            let json = serde_json::to_vec(&serde_json::json!({
                "items": items,
                "selected": selected,
            }))
            .map_err(|e| e.to_string())?;
            // SAFETY: Swift copies/parses the bytes synchronously, on main.
            if unsafe { mac::walkaway_tabs_update(self.handle.as_ptr(), json.as_ptr(), json.len()) }
                == 0
            {
                return Err(mac::last_error());
            }
            self.items.clone_from(&items.to_vec());
            self.selection = Some(selected);
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("original native CompactTabStrip is available only on macOS".into())
        }
    }

    /// Frame in AppKit/content logical points, with y measured from the top.
    /// The normal browser row is (0, 74, logical_window_width, 38).
    pub fn set_frame(&self, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            mac::require_main_thread()?;
            // Always let native derive origin from content bounds: its height
            // can change even if this logical row did not.
            if unsafe { mac::walkaway_tabs_frame(self.handle.as_ptr(), x, y, width, height) } == 0 {
                return Err(mac::last_error());
            }
            self.frame.set((x, y, width, height));
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (x, y, width, height);
            Err("original native CompactTabStrip is available only on macOS".into())
        }
    }

    /// Optional explicit appearance follows the browser's light/dark setting.
    /// `None` restores normal AppKit appearance inheritance.
    pub fn set_dark_mode(&self, dark: Option<bool>) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            mac::require_main_thread()?;
            let value = dark.map_or(-1, i32::from);
            if unsafe { mac::walkaway_tabs_appearance(self.handle.as_ptr(), value) } == 0 {
                return Err(mac::last_error());
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = dark;
            Err("original native CompactTabStrip is available only on macOS".into())
        }
    }

    /// Temporarily yield the native row/address to the renderer's dropdown.
    /// Original views/cells remain alive; no native state is rebuilt on show.
    pub fn set_overlays_visible(&self, visible: bool) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            mac::require_main_thread()?;
            if unsafe {
                mac::walkaway_tabs_overlays_visible(self.handle.as_ptr(), i32::from(visible))
            } == 0
            {
                return Err(mac::last_error());
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = visible;
            Err("native overlays are available only on macOS".into())
        }
    }

    /// Cmd+L focuses the selected original tab cell's native editor and selects all.
    /// URL/loading/security/editability come only from that tab's snapshot item.
    pub fn focus_address(&self) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            mac::require_main_thread()?;
            if unsafe { mac::walkaway_tabs_focus_address(self.handle.as_ptr()) } == 0 {
                return Err(mac::last_error());
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("native address field is available only on macOS".into())
        }
    }

    /// Capture the original tab row, including its actual address field/editor.
    pub fn capture_rgba(&self) -> Result<NativeTabCapture, String> {
        #[cfg(target_os = "macos")]
        {
            mac::require_main_thread()?;
            let mut bytes = std::ptr::null_mut();
            let (mut width, mut height) = (0, 0);
            let len = unsafe {
                mac::walkaway_tabs_capture_rgba(
                    self.handle.as_ptr(),
                    &mut bytes,
                    &mut width,
                    &mut height,
                )
            };
            if bytes.is_null() || len == 0 {
                return Err(mac::last_error());
            }
            // Swift owns this allocation, so only its matching free may release it.
            let allocation = mac::CaptureAllocation(bytes);
            let expected = (width as usize)
                .checked_mul(height as usize)
                .and_then(|n| n.checked_mul(4));
            if expected != Some(len) || len > isize::MAX as usize {
                return Err("invalid RGBA dimensions from native capture".into());
            }
            let rgba = unsafe { std::slice::from_raw_parts(allocation.0, len) }.to_vec();
            let (x, y, logical_width, logical_height) = self.frame.get();
            Ok(NativeTabCapture {
                rgba,
                width,
                height,
                x,
                y,
                logical_width,
                logical_height,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("original native CompactTabStrip is available only on macOS".into())
        }
    }
}

impl Drop for NativeTabStrip {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            // Constructed only on main and !Send/!Sync. Destroy disconnects all
            // Swift callbacks before Rust frees their context or the winit window.
            assert!(
                mac::require_main_thread().is_ok(),
                "native tab drop off main thread"
            );
            unsafe { mac::walkaway_tabs_destroy(self.handle.as_ptr()) };
        }
    }
}

/// Position a just-detached window around AppKit's global screen point without
/// confusing it with winit's physical, top-left screen coordinates. The native
/// helper constrains the window to the containing screen's visible frame.
pub fn position_window_at_screen_point(window: &Window, x: f64, y: f64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        mac::require_main_thread()?;
        if unsafe { mac::walkaway_tabs_position_window(mac::content_view(window)?, x, y) } == 0 {
            return Err(mac::last_error());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (window, x, y);
        Err("native AppKit screen coordinates are available only on macOS".into())
    }
}

/// Real NSWindow/AppKit component test, not a browser or end-to-end fixture.
/// Run via --nativetabtest on the process's main thread before starting winit.
pub fn run_native_tab_selftest() -> Result<usize, String> {
    #[cfg(target_os = "macos")]
    {
        mac::require_main_thread()?;
        let count = unsafe { mac::walkaway_tabs_selftest() };
        if count < 0 {
            Err(mac::last_error())
        } else {
            Ok(count as usize)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("native tab component tests require macOS/AppKit".into())
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::NativeTabEvent;
    use std::ffi::c_void;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    pub(super) struct CallbackContext {
        pub enqueue: Box<dyn Fn(NativeTabEvent) -> bool>,
    }

    pub(super) extern "C" fn callback(context: *mut c_void, json: *const u8, len: usize) -> i32 {
        if context.is_null() || json.is_null() || len > isize::MAX as usize {
            return 0;
        }
        // Never unwind into Swift. The callback does no AppKit work, borrows no
        // BrowserApp, and can only enqueue a typed winit user event.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let context = unsafe { &*context.cast::<CallbackContext>() };
            let bytes = unsafe { std::slice::from_raw_parts(json, len) };
            serde_json::from_slice(bytes)
                .ok()
                .is_some_and(|event| (context.enqueue)(event)) as i32
        }))
        .unwrap_or(0)
    }

    pub(super) fn content_view(window: &Window) -> Result<*mut c_void, String> {
        match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
            RawWindowHandle::AppKit(handle) => Ok(handle.ns_view.as_ptr()),
            _ => Err("winit window does not have an AppKit content view".into()),
        }
    }

    pub(super) fn require_main_thread() -> Result<(), String> {
        if unsafe { walkaway_tabs_is_main_thread() } == 1 {
            Ok(())
        } else {
            Err("native tabs require the AppKit main thread".into())
        }
    }

    pub(super) fn last_error() -> String {
        let mut buffer = vec![0_u8; 4096];
        let len = unsafe { walkaway_tabs_last_error(buffer.as_mut_ptr(), buffer.len()) };
        buffer.truncate(len.min(buffer.len()));
        String::from_utf8_lossy(&buffer).into_owned()
    }

    pub(super) struct CaptureAllocation(pub *mut u8);
    impl Drop for CaptureAllocation {
        fn drop(&mut self) {
            unsafe { walkaway_tabs_free_rgba(self.0) };
        }
    }

    unsafe extern "C" {
        pub fn walkaway_tabs_is_main_thread() -> i32;
        pub fn walkaway_tabs_last_error(buffer: *mut u8, capacity: usize) -> usize;
        pub fn walkaway_tabs_create(
            content_view: *mut c_void,
            owner: u64,
            callback: extern "C" fn(*mut c_void, *const u8, usize) -> i32,
            context: *mut c_void,
        ) -> *mut c_void;
        pub fn walkaway_tabs_update(handle: *mut c_void, json: *const u8, len: usize) -> i32;
        pub fn walkaway_tabs_frame(handle: *mut c_void, x: f64, y: f64, w: f64, h: f64) -> i32;
        pub fn walkaway_tabs_appearance(handle: *mut c_void, dark: i32) -> i32;
        pub fn walkaway_tabs_overlays_visible(handle: *mut c_void, visible: i32) -> i32;
        pub fn walkaway_tabs_focus_address(handle: *mut c_void) -> i32;
        pub fn walkaway_tabs_destroy(handle: *mut c_void);
        pub fn walkaway_tabs_capture_rgba(
            handle: *mut c_void,
            bytes: *mut *mut u8,
            width: *mut u32,
            height: *mut u32,
        ) -> usize;
        pub fn walkaway_tabs_free_rgba(bytes: *mut u8);
        pub fn walkaway_tabs_position_window(content_view: *mut c_void, x: f64, y: f64) -> i32;
        pub fn walkaway_tabs_selftest() -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(rgba: Vec<u8>, x: f64, y: f64) -> NativeTabCapture {
        NativeTabCapture {
            rgba,
            width: 1,
            height: 1,
            x,
            y,
            logical_width: 1.0,
            logical_height: 1.0,
        }
    }

    #[test]
    fn composite_uses_straight_alpha_and_leaves_page_untouched() {
        let mut pixels = vec![20, 40, 60, 255, 20, 40, 60, 255];
        capture(vec![220, 140, 60, 128], 1.0, 0.0)
            .composite_into(&mut pixels, 2, 1, 1.0)
            .unwrap();
        assert_eq!(&pixels[..4], &[20, 40, 60, 255]);
        assert_eq!(&pixels[4..], &[120, 90, 60, 255]);
    }

    #[test]
    fn composite_handles_transparent_destination_and_clipping() {
        let mut pixels = vec![0; 4];
        capture(vec![220, 140, 60, 128], 0.0, 0.0)
            .composite_into(&mut pixels, 1, 1, 1.0)
            .unwrap();
        assert_eq!(pixels, [220, 140, 60, 128]);
        capture(vec![0, 0, 0, 255], -2.0, -2.0)
            .composite_into(&mut pixels, 1, 1, 1.0)
            .unwrap();
        assert_eq!(pixels, [220, 140, 60, 128]);
    }

    #[test]
    fn composite_rejects_bad_dimensions() {
        assert!(
            capture(vec![0; 4], 0.0, 0.0)
                .composite_into(&mut [0; 3], 1, 1, 1.0)
                .is_err()
        );
        assert!(
            capture(vec![0; 4], 0.0, 0.0)
                .composite_into(&mut [0; 4], 1, 1, f64::NAN)
                .is_err()
        );
    }

    #[test]
    fn transfer_event_roundtrips_full_u64_ids() {
        let event: NativeTabEvent = serde_json::from_str(
            r#"{"window_token":3,"action":"transfer","tab_id":18446744073709551615,"target_index":2,"pinned":false,"dest_window_token":4,"screen_x":-25.5,"screen_y":54,"text":null}"#,
        )
        .unwrap();
        assert_eq!(event.tab_id, u64::MAX);
        assert_eq!(event.action, NativeTabAction::Transfer);
        assert_eq!(event.dest_window_token, 4);
    }
}
