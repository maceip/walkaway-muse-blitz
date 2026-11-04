use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    window: Option<Window>,
    start: Instant,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        eprintln!("[minwin] resumed");
        self.window = Some(
            el.create_window(Window::default_attributes().with_title("minwin"))
                .unwrap(),
        );
        eprintln!("[minwin] window created");
    }
    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, e: WindowEvent) {
        if matches!(e, WindowEvent::CloseRequested) {
            el.exit();
        }
        if self.start.elapsed() > Duration::from_secs(3) {
            eprintln!("[minwin] 3s elapsed, exiting");
            el.exit();
        }
    }
}
fn main() {
    eprintln!("[minwin] start");
    let el = EventLoop::new().unwrap();
    let mut app = App {
        window: None,
        start: Instant::now(),
    };
    el.run_app(&mut app).unwrap();
    eprintln!("[minwin] done");
}
