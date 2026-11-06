import AppKit

// These are component-test fixture windows, not a demo/browser success state.
// The production bridge never starts an application or pumps its own event loop.
private final class NativeTabTestEvents {
    var values: [[String: Any]] = []
    var accepts = true
}

private let nativeTabTestCallback: WalkawayTabCallback = { context, bytes, count in
    guard let context, let bytes,
          let event = try? JSONSerialization.jsonObject(with: Data(bytes: bytes, count: count)) as? [String: Any] else { return 0 }
    let recorder = Unmanaged<NativeTabTestEvents>.fromOpaque(context).takeUnretainedValue()
    recorder.values.append(event)
    return recorder.accepts ? 1 : 0
}

private final class NativeTabTestContent: NSView {
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        NSColor.windowBackgroundColor.setFill(); bounds.fill()
        NSColor.systemBlue.setFill(); NSRect(x: 25, y: 180, width: 220, height: 140).fill()
        ("Real NSWindow native component test" as NSString).draw(at: NSPoint(x: 28, y: 345),
            withAttributes: [.font: NSFont.systemFont(ofSize: 18), .foregroundColor: NSColor.labelColor])
    }
}

@MainActor
private func nativeTabPump(_ seconds: TimeInterval) {
    let until = Date(timeIntervalSinceNow: seconds)
    repeat {
        while let event = NSApp.nextEvent(matching: .any, until: .distantPast, inMode: .default, dequeue: true) {
            NSApp.sendEvent(event)
        }
        NSApp.updateWindows()
        _ = RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.005))
    } while Date() < until
}

@MainActor
private func runNativeTabComponentTests() throws -> Int32 {
    var checks: Int32 = 0
    func check(_ predicate: @autoclosure () -> Bool, _ message: String) throws {
        guard predicate() else { throw WalkawayTabError("native tab selftest: \(message)") }
        checks += 1
    }
    func descendants(_ view: NSView) -> [NSView] { [view] + view.subviews.flatMap(descendants) }
    func window(_ x: CGFloat) -> NSWindow {
        let window = NSWindow(contentRect: NSRect(x: x, y: 180, width: 900, height: 540),
                              styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.title = "Walkaway native component test"
        window.contentView = NativeTabTestContent(frame: NSRect(x: 0, y: 0, width: 900, height: 540))
        return window
    }
    func install(_ bridge: WalkawayTabBridge, _ snapshot: WalkawayTabSnapshot) throws {
        let json = try JSONEncoder().encode(snapshot)
        let result = json.withUnsafeBytes { buffer in
            walkawayTabsUpdate(bridge.opaque, buffer.bindMemory(to: UInt8.self).baseAddress, buffer.count)
        }
        try check(result == 1, "JSON update accepted")
        nativeTabPump(0.025)
    }

    _ = NSApplication.shared
    NSApp.setActivationPolicy(.accessory)
    NSApp.finishLaunching()
    let sourceWindow = window(120), destinationWindow = window(1040)
    sourceWindow.makeKeyAndOrderFront(nil)
    destinationWindow.orderFront(nil)
    let recorder = NativeTabTestEvents()
    let context = Unmanaged.passUnretained(recorder).toOpaque()
    guard let sourceContent = sourceWindow.contentView, let destinationContent = destinationWindow.contentView,
          let sourceHandle = walkawayTabsCreate(Unmanaged.passUnretained(sourceContent).toOpaque(), 900001, nativeTabTestCallback, context),
          let targetHandle = walkawayTabsCreate(Unmanaged.passUnretained(destinationContent).toOpaque(), 900002, nativeTabTestCallback, context) else {
        throw WalkawayTabError("native tab selftest: create real NSWindow bridges")
    }
    let source = try WalkawayTabBridge.lookup(sourceHandle), target = try WalkawayTabBridge.lookup(targetHandle)
    defer {
        source.destroy(); target.destroy()
        sourceWindow.close(); destinationWindow.close()
    }
    let initialItems = (1...5).map { number in
        WalkawayTabSnapshot.Item(id: UInt64(number), title: "Real native tab \(number)",
            url: "https://example.test/\(number)", loading: number == 3, secure: true, pinned: false, transferable: true)
    }
    var state = WalkawayTabSnapshot(items: initialItems, selected: 1)
    try install(source, state)
    try install(target, .init(items: [.init(id: UInt64.max, title: "Destination", url: "https://example.test/destination",
        loading: false, secure: true, pinned: false, transferable: true)], selected: UInt64.max))
    guard let strip = source.stripView, let destination = target.stripView else {
        throw WalkawayTabError("native tab selftest: NSHostingView failed to create original CompactTabStripView")
    }
    try check(strip.window === sourceWindow && destination.window === destinationWindow, "original strips inhabit their existing windows")
    try check(strip.previewSourceView === sourceContent, "preview captures actual contentView")
    try check(strip.labelMode == .fixed && strip.hoverPreview != nil, "fixed labels and original hover controller enabled")
    try check(strip.items.count == 5 && strip.slotFrames.count == 5, "original layout has real cells")
    try check(walkawayTabUUID(UInt64.max) == destination.items[0].id && walkawayTabUUID(1) != walkawayTabUUID(UInt64.max), "stable full-width UInt64 identity")
    let originalHost = source.hostingView
    try install(source, state)
    try check(source.stripView === strip && source.hostingView === originalHost, "equal snapshot preserves native view identity")
    try check(source.hostingView?.frame.minY == 106 && source.hostingView?.frame.height == 38, "only original 106..144 row is overlaid")
    try check(sourceContent.subviews.count == 1, "renderer contentView remains in place")

    // Native omnibox uses the actual AppKit field editor; no hand-built key
    // append/backspace simulation, clipboard replacement or navigation fixture.
    let canonicalAddress = "https://example.test/original"
    try source.setAddressFrame(x: 64, y: 78, width: 826, height: 24)
    try source.setAddress(text: canonicalAddress, loading: true, secure: true, editable: true)
    sourceWindow.makeKeyAndOrderFront(nil)
    try source.focusAddress()
    guard let field = source.addressField, let editor = field.currentEditor() as? NSTextView else {
        throw WalkawayTabError("native tab selftest: NSTextField did not create a real NSTextView editor")
    }
    try check(editor.selectedRange().length == canonicalAddress.utf16.count, "native focus selects complete URL")
    editor.insertText("https://example.test/edited", replacementRange: editor.selectedRange())
    try check(editor.string == "https://example.test/edited", "native field editor inserts and replaces selection")
    let selectAll = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command,
        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: sourceWindow.windowNumber,
        context: nil, characters: "a", charactersIgnoringModifiers: "a", isARepeat: false, keyCode: 0)!
    try check(field.performKeyEquivalent(with: selectAll) && editor.selectedRange().length == editor.string.utf16.count,
              "Cmd+A routes to actual AppKit field editor without winit Edit menu")
    try source.setAddress(text: canonicalAddress, loading: false, secure: true, editable: true)
    try check(editor.string == "https://example.test/edited", "authoritative refresh does not overwrite native draft")
    recorder.values.removeAll()
    try check(source.control(field, textView: editor, doCommandBy: #selector(NSResponder.insertNewline(_:))), "native Enter handled")
    try check(recorder.values.last?["action"] as? String == "search"
        && recorder.values.last?["text"] as? String == "https://example.test/edited"
        && recorder.values.last?["tab_id"] as? UInt64 == 1, "native Enter queues selected-tab navigation without claiming success")
    try source.focusAddress()
    guard let escapeEditor = field.currentEditor() as? NSTextView else { throw WalkawayTabError("missing second native editor") }
    escapeEditor.insertText("discard this draft", replacementRange: escapeEditor.selectedRange())
    recorder.values.removeAll()
    try check(source.control(field, textView: escapeEditor, doCommandBy: #selector(NSResponder.cancelOperation(_:))), "native Escape handled")
    try check(field.stringValue == canonicalAddress && recorder.values.isEmpty, "Escape restores canonical URL without navigation")
    try source.setAddress(text: canonicalAddress, loading: false, secure: true, editable: false)
    try check(!field.isEditable && field.currentEditor() == nil, "supervised address editing disabled")
    try source.setAddress(text: canonicalAddress, loading: false, secure: true, editable: true)
    let (addressPixels, addressBytes, addressWidth, addressHeight) = try source.capture(address: true)
    try check(addressBytes == Int(addressWidth) * Int(addressHeight) * 4 && addressWidth > 0,
              "native address capture returns real pixels")
    walkawayTabsFreeRGBA(addressPixels)
    source.setOverlaysVisible(false)
    try check(field.isHidden && source.hostingView?.isHidden == true, "renderer dropdown is not occluded by native overlays")
    source.setOverlaysVisible(true)
    try check(!field.isHidden && source.hostingView?.isHidden == false && source.stripView === strip,
              "dismissed dropdown restores same native views")

    // CPU/model-call and RunLoop observations, deliberately NOT display-frame
    // or physical input-latency claims. The same original retained view is used.
    func percentile(_ values: [Double], _ fraction: Double) -> Double {
        let sorted = values.sorted()
        return sorted.isEmpty ? 0 : sorted[min(sorted.count - 1, Int(Double(sorted.count - 1) * fraction))]
    }
    var noopMilliseconds: [Double] = []
    for _ in 0..<1000 {
        let began = ProcessInfo.processInfo.systemUptime
        try source.update(state)
        noopMilliseconds.append((ProcessInfo.processInfo.systemUptime - began) * 1000)
    }
    try check(source.stripView === strip && source.hostingView === originalHost, "1000 no-op updates retain original view")
    var selectionMilliseconds: [Double] = []
    for index in 0..<100 {
        let next = WalkawayTabSnapshot(items: state.items, selected: UInt64(index % state.items.count + 1))
        let began = ProcessInfo.processInfo.systemUptime
        try source.update(next)
        selectionMilliseconds.append((ProcessInfo.processInfo.systemUptime - began) * 1000)
    }
    try source.update(state)
    nativeTabPump(0.025)
    try check(source.stripView === strip, "100 selection updates retain original view")
    var timerIntervals: [Double] = []
    var previousTick: TimeInterval?
    var tick = 0
    let sampleTimer = Timer(timeInterval: 1.0 / 120.0, repeats: true) { _ in
        MainActor.assumeIsolated {
            let now = ProcessInfo.processInfo.systemUptime
            if let previousTick { timerIntervals.append((now - previousTick) * 1000) }
            previousTick = now
            tick += 1
            if tick % 4 == 0 {
                var next = state
                next.selected = tick % 8 == 0 ? 1 : 2
                try? source.update(next)
            }
        }
    }
    RunLoop.main.add(sampleTimer, forMode: .common)
    nativeTabPump(0.3)
    sampleTimer.invalidate()
    try source.update(state)
    try check(timerIntervals.count >= 2, "real native RunLoop timer serviced during selection updates")
    print(String(format: "NATIVE OBSERVED: no-op update p95 %.4fms/1000; selection-update CPU p95 %.4fms/100; 120Hz RunLoop interval p95 %.3fms max %.3fms (%d intervals). Not display/input latency.",
                 percentile(noopMilliseconds, 0.95), percentile(selectionMilliseconds, 0.95),
                 percentile(timerIntervals, 0.95), timerIntervals.max() ?? 0, timerIntervals.count))

    // Native accessibility action routes through the original cell callback;
    // the queued event does not mutate the browser-owned model synchronously.
    guard let cell = descendants(strip).compactMap({ $0 as? CompactTabCell }).first(where: { $0.item.id == walkawayTabUUID(2) }) else {
        throw WalkawayTabError("native tab selftest: missing native tab cell")
    }
    recorder.values.removeAll()
    try check(cell.accessibilityPerformPress(), "original cell accessibility press")
    try check(recorder.values.last?["action"] as? String == "select" && recorder.values.last?["tab_id"] as? UInt64 == 2, "selection callback queues real tab ID")
    try check(source.snapshot?.selected == 1, "browser remains authoritative after select callback")
    strip.onReload()
    try check(recorder.values.last?["action"] as? String == "reload" && recorder.values.last?["tab_id"] as? UInt64 == 2,
              "inactive-tab context refresh targets queued selection rather than old browser selection")
    strip.requestPin(walkawayTabUUID(2))
    try check(recorder.values.last?["action"] as? String == "pin" && recorder.values.last?["pinned"] as? Bool == true, "original pin callback")
    state.items[1].pinned = true
    let pinned = state.items.remove(at: 1); state.items.insert(pinned, at: 0)
    try install(source, state)
    try check(strip.items.first?.id == pinned.id.nativeUUID && strip.items.first?.isPinned == true, "authoritative pin update reaches original layout")

    recorder.values.removeAll()
    strip.onMove(walkawayTabUUID(4), 1)
    try check(recorder.values.last?["action"] as? String == "move" && recorder.values.last?["target_index"] as? Int == 1,
              "original reorder callback preserves insertion index")
    let moved = state.items.remove(at: state.items.firstIndex(where: { $0.id == 4 })!)
    state.items.insert(moved, at: 1)
    try install(source, state)
    try check(strip.items[0].isPinned && strip.items[1].id == walkawayTabUUID(4), "authoritative reorder keeps pinned group")
    recorder.values.removeAll()
    strip.onDetach(walkawayTabUUID(4), NSPoint(x: -32.5, y: 420))
    try check(recorder.values.last?["action"] as? String == "detach"
        && recorder.values.last?["screen_x"] as? Double == -32.5
        && recorder.values.last?["screen_y"] as? Double == 420, "detach callback preserves AppKit screen points")
    try check(source.snapshot?.items.contains(where: { $0.id == 4 }) == true, "detach request does not fabricate committed window ownership")

    // Exercise the original close-width freeze against real cell geometry.
    // Main owns the authoritative removal; the original retains width targets
    // until its tracking-area exit. No replacement geometry is calculated here.
    let trackingEvent = NSEvent.enterExitEvent(with: .mouseEntered, location: NSPoint(x: 90, y: 120),
        modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: sourceWindow.windowNumber,
        context: nil, eventNumber: 1, trackingNumber: 0, userData: nil)!
    strip.mouseEntered(with: trackingEvent)
    let beforeWidths = Dictionary(uniqueKeysWithValues: zip(strip.items.map(\.id), strip.slotFrames.map(\.width)))
    recorder.values.removeAll()
    strip.requestClose(walkawayTabUUID(5))
    try check(recorder.values.last?["action"] as? String == "close" && recorder.values.last?["tab_id"] as? UInt64 == 5,
              "original close action queues browser request")
    state.items.removeAll(where: { $0.id == 5 })
    try install(source, state)
    try check(zip(strip.items, strip.slotFrames).allSatisfy { item, frame in
        item.isPinned || abs((beforeWidths[item.id] ?? -1) - frame.width) < 0.01
    }, "close retains original native widths while pointer remains inside")
    strip.mouseExited(with: trackingEvent)
    nativeTabPump(0.025)
    try check(zip(strip.items, strip.slotFrames).contains { item, frame in
        !item.isPinned && abs((beforeWidths[item.id] ?? frame.width) - frame.width) > 0.5
    }, "leaving row releases original close width freeze")

    recorder.values.removeAll()
    try check(strip.onTransfer(walkawayTabUUID(1), destination, 1), "registered destination accepts queued transfer")
    try check(recorder.values.last?["dest_window_token"] as? UInt64 == 900002 && recorder.values.last?["tab_id"] as? UInt64 == 1, "destination NSView resolves actual window owner")
    try check(source.snapshot?.items.contains(where: { $0.id == 1 }) == true && target.snapshot?.items.count == 1, "accepted transfer not falsely committed")
    recorder.accepts = false
    try check(!strip.onTransfer(walkawayTabUUID(1), destination, 1), "closed event queue rejects transfer")
    recorder.accepts = true
    state.items[state.items.firstIndex(where: { $0.id == 1 })!].transferable = false
    try install(source, state)
    recorder.values.removeAll()
    try check(!strip.onTransfer(walkawayTabUUID(1), destination, 1), "supervised source cannot transfer")
    strip.onDetach(walkawayTabUUID(1), NSPoint(x: 300, y: 400))
    try check(recorder.values.isEmpty, "supervised source cannot detach")

    var output: UnsafeMutablePointer<UInt8>?
    var width: UInt32 = 0, height: UInt32 = 0
    let captured = walkawayTabsCaptureRGBA(sourceHandle, &output, &width, &height)
    guard let output else { throw WalkawayTabError("native tab selftest: real AppKit capture failed") }
    defer { walkawayTabsFreeRGBA(output) }
    try check(captured == Int(width) * Int(height) * 4 && width > 0 && height > 0, "real native RGBA capture dimensions")
    let pixels = UnsafeBufferPointer(start: output, count: captured)
    try check(stride(from: 3, to: captured, by: 4).contains(where: { pixels[$0] > 0 }), "capture contains native drawn pixels")
    try check(Set(pixels).count > 8, "capture is not a fabricated uniform success image")

    // Retain an old SwiftUI callback over destruction to verify Rust context
    // cannot be called after explicit native teardown.
    let retiredCallback = strip.onSelect
    target.destroy()
    try check(!strip.onTransfer(walkawayTabUUID(2), destination, 0), "destroyed destination rejects stale transfer")
    recorder.values.removeAll()
    source.destroy()
    retiredCallback(walkawayTabUUID(2))
    nativeTabPump(0.04)
    try check(recorder.values.isEmpty, "no callback after destruction")
    try check(sourceContent.subviews.isEmpty && sourceWindow.isMovable, "hosting views detach and original window movability restored")
    try check(WalkawayTabBridge.handles[UInt(bitPattern: sourceHandle)] == nil && WalkawayTabBridge.handles[UInt(bitPattern: targetHandle)] == nil, "both native handles unregistered")
    print("NATIVE TAB COMPONENT PASS: \(checks) real AppKit/bridge assertions; no browser E2E or physical gesture claim")
    return checks
}

private extension UInt64 {
    var nativeUUID: UUID { walkawayTabUUID(self) }
}

@_cdecl("walkaway_tabs_selftest")
func walkawayTabsSelftest() -> Int32 {
    guard Thread.isMainThread else { walkawaySetError(WalkawayTabError("native component test requires main thread")); return -1 }
    do { return try MainActor.assumeIsolated { try runNativeTabComponentTests() } }
    catch { walkawaySetError(error); return -1 }
}
