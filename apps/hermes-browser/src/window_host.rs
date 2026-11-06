use super::*;

pub(super) struct WindowHost {
    windows: Vec<BrowserApp>,
    next_window: u64,
    next_tab_base: u64,
}

impl WindowHost {
    pub(super) fn new(app: BrowserApp) -> Self {
        Self {
            windows: vec![app],
            next_window: 2,
            next_tab_base: 1 << 32,
        }
    }

    fn owner_index(&self, owner: u64) -> Option<usize> {
        self.windows
            .iter()
            .position(|app| app.window_token == owner)
    }

    fn remove_window(&mut self, index: usize) {
        let mut app = self.windows.remove(index);
        if app.control.is_some() {
            app.control_stop("user", "window-close");
            app.flush_control_out();
        }
        for tab in &mut app.tabs {
            if let Some(cancel) = tab.canonical.navigation_cancel.take() {
                cancel.store(true, Ordering::Release);
            }
        }
        app.native_tabs.take();
        app.renderer.suspend();
    }

    fn finish_transfer(&mut self, source: usize) {
        if !self.windows[source].tabs.is_empty() {
            return;
        }
        if self.windows[source].control_hub.is_some() {
            self.windows[source].new_tab();
        } else {
            self.remove_window(source);
        }
    }

    fn transfer(&mut self, source: usize, destination: usize, id: TabId, index: usize) -> bool {
        if source == destination
            || self.windows[source]
                .control
                .as_ref()
                .is_some_and(|s| s.tab_id == id)
            || self.windows[destination]
                .control
                .as_ref()
                .is_some_and(|s| !s.state.is_terminal())
        {
            return false;
        }
        let Some(tab_index) = self.windows[source]
            .tabs
            .iter()
            .position(|tab| tab.canonical.id == id)
        else {
            return false;
        };
        let selected = self.windows[source].tab().canonical.id;
        let tab = self.windows[source].tabs.remove(tab_index);
        if !self.windows[source].tabs.is_empty() {
            let app = &mut self.windows[source];
            app.active = app
                .tabs
                .iter()
                .position(|tab| tab.canonical.id == selected)
                .unwrap_or_else(|| tab_index.min(app.tabs.len() - 1));
            app.tab_animation.snap(app.active);
            app.sync_chrome();
            app.request_redraw();
        }
        let dest = &mut self.windows[destination];
        let pins = dest
            .tabs
            .iter()
            .take_while(|tab| tab.canonical.pinned)
            .count();
        let target = if tab.canonical.pinned {
            index.min(pins)
        } else {
            index.clamp(pins, dest.tabs.len())
        };
        dest.tabs.insert(target, tab);
        dest.active = target;
        dest.tab_animation.snap(target);
        let viewport = dest.page_viewport();
        dest.tab_mut()
            .instance
            .page
            .doc_mut()
            .set_viewport(viewport);
        dest.tab_mut().instance.page.resolve();
        dest.sync_chrome();
        dest.request_redraw();
        true
    }

    fn native_event(&mut self, event_loop: &ActiveEventLoop, event: native_tabs::NativeTabEvent) {
        use native_tabs::NativeTabAction;
        let Some(source) = self.owner_index(event.window_token) else {
            return;
        };
        let id = TabId(event.tab_id);
        match event.action {
            NativeTabAction::Detach => {
                if self.windows[source]
                    .control
                    .as_ref()
                    .is_some_and(|s| s.tab_id == id)
                    || !self.windows[source]
                        .tabs
                        .iter()
                        .any(|tab| tab.canonical.id == id)
                {
                    return;
                }
                if self.windows[source].tabs.len() == 1 {
                    if let Some(window) = &self.windows[source].window {
                        if let Err(error) = native_tabs::position_window_at_screen_point(
                            window,
                            event.screen_x,
                            event.screen_y,
                        ) {
                            eprintln!("[tabs] detach position failed: {error}");
                        }
                    }
                    return;
                }
                let mut app = BrowserApp::for_window(
                    self.windows[source].proxy.clone(),
                    self.windows[source].net_dirty.clone(),
                    self.windows[source].provider.clone(),
                );
                app.window_token = self.next_window;
                self.next_window += 1;
                app.next_tab_id = self.next_tab_base;
                self.next_tab_base += 1 << 32;
                app.tabs[0].canonical.id = TabId(app.next_tab_id);
                app.next_tab_id += 1;
                app.exit_after = None;
                app.resumed(event_loop);
                if let Some(window) = &app.window {
                    if let Err(error) = native_tabs::position_window_at_screen_point(
                        window,
                        event.screen_x,
                        event.screen_y,
                    ) {
                        eprintln!("[tabs] detach position failed: {error}");
                    }
                }
                let destination = self.windows.len();
                self.windows.push(app);
                if self.transfer(source, destination, id, 0) {
                    let destination = &mut self.windows[destination];
                    destination.tabs.retain(|tab| tab.canonical.id == id);
                    destination.active = 0;
                    destination.sync_chrome();
                    self.finish_transfer(source);
                } else {
                    self.remove_window(destination);
                }
            }
            NativeTabAction::Transfer => {
                let Some(destination) = self.owner_index(event.dest_window_token) else {
                    return;
                };
                if self.transfer(source, destination, id, event.target_index) {
                    self.finish_transfer(source);
                }
            }
            _ => self.windows[source].handle_native_tab(event),
        }
    }
}

pub(super) fn selftest() -> i32 {
    let mut failures = 0;
    let mut check = |condition: bool, label: &str| {
        if condition {
            println!("ok {label}");
        } else {
            eprintln!("FAIL {label}");
            failures += 1;
        }
    };
    let first = test_app();
    let proxy = first.proxy.clone();
    let provider = first.provider.clone();
    let dirty = first.net_dirty.clone();
    let mut host = WindowHost::new(first);
    host.windows[0].new_tab();
    let moving = host.windows[0].tab().canonical.id;
    host.windows[0].pin_tab(moving, true);
    check(
        host.windows[0].tabs[0].canonical.id == moving,
        "pin moves actual tab into pinned group",
    );
    let page_id = host.windows[0].tabs[0].instance.page_id;
    let runtime = host.windows[0].tabs[0].instance.page.hermes;
    let history = host.windows[0].tabs[0].canonical.browser.history.len();
    let mut second = BrowserApp::new(proxy, dirty, provider);
    second.window_token = 2;
    second.tabs[0].canonical.id = TabId(1 << 32);
    second.next_tab_id = (1 << 32) + 1;
    host.windows.push(second);
    check(
        host.transfer(0, 1, moving, 0),
        "cross-window transfer succeeds",
    );
    check(
        host.windows[1].tab().canonical.id == moving && host.windows[1].tab().canonical.pinned,
        "stable identity and pin state survive transfer",
    );
    check(
        host.windows[1].tab().instance.page.hermes == runtime
            && host.windows[1].tab().instance.page_id == page_id,
        "transfer moves same runtime and document without recreation",
    );
    check(
        host.windows[1].tab().canonical.browser.history.len() == history,
        "history survives transfer",
    );
    check(
        !host.windows[0]
            .tabs
            .iter()
            .any(|tab| tab.canonical.id == moving),
        "source does not duplicate transferred tab",
    );
    host.windows[1].new_tab();
    let other = host.windows[1].tab().canonical.id;
    host.windows[1].move_tab(other, 0);
    check(
        host.windows[1].tabs[0].canonical.id == moving,
        "unpinned reorder cannot cross pinned group",
    );
    host.windows[1].pin_tab(moving, false);
    check(
        !host.windows[1].tabs.iter().any(|tab| tab.canonical.pinned),
        "unpin preserves real model",
    );
    check(
        !host.transfer(0, 1, TabId(u64::MAX), 0),
        "stale transfer cannot consume an unrelated tab",
    );
    if failures == 0 {
        println!("TAB HOST TEST PASS");
        0
    } else {
        1
    }
}

impl ApplicationHandler<UserEvent> for WindowHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        for app in &mut self.windows {
            app.resumed(event_loop);
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        for app in &mut self.windows {
            app.suspended(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(index) = self
            .windows
            .iter()
            .position(|app| app.window.as_ref().is_some_and(|w| w.id() == id))
        else {
            return;
        };
        if matches!(event, WindowEvent::CloseRequested) {
            self.remove_window(index);
            if self.windows.is_empty() {
                event_loop.exit();
            }
        } else {
            self.windows[index].window_event(event_loop, id, event);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::NativeTabs(event) => self.native_event(event_loop, event),
            UserEvent::Navigated {
                tab,
                generation,
                url,
                result,
            } => {
                if let Some(app) = self
                    .windows
                    .iter_mut()
                    .find(|app| app.tabs.iter().any(|t| t.canonical.id == tab))
                {
                    app.handle_navigated(tab, generation, url, result);
                }
            }
            UserEvent::NetWake => {
                for app in &mut self.windows {
                    app.net_dirty.store(false, Ordering::SeqCst);
                    for tab in &mut app.tabs {
                        tab.instance.page.resolve();
                    }
                    app.request_redraw();
                }
            }
            other => {
                if let Some(app) = self
                    .windows
                    .iter_mut()
                    .find(|app| app.control_hub.is_some())
                {
                    app.user_event(event_loop, other);
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let mut next = None;
        for app in &mut self.windows {
            app.about_to_wait(event_loop);
            if app.debugger_ui.busy()
                || (app.native_tabs.is_none() && app.tab_animation.is_animating())
            {
                let due = now + Duration::from_millis(16);
                next = Some(next.map_or(due, |old: Instant| old.min(due)));
            }
            if let Some(limit) = app.exit_after {
                let due = app.started + limit;
                next = Some(next.map_or(due, |old: Instant| old.min(due)));
            }
        }
        event_loop.set_control_flow(next.map_or(
            winit::event_loop::ControlFlow::Wait,
            winit::event_loop::ControlFlow::WaitUntil,
        ));
    }
}
