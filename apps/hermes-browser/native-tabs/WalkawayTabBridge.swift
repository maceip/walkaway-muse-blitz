import AppKit
import SwiftUI

// The original sources compile in this SAME module. In particular, every strip
// shares CompactTabStripView's original global cross-window drag registry.
// No separate application, executable, event loop, or substitute tab UI.

typealias WalkawayTabCallback = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Int32

struct WalkawayTabSnapshot: Codable, Equatable {
    struct Item: Codable, Equatable {
        var id: UInt64
        var title: String
        var url: String
        var loading: Bool
        var secure: Bool
        var pinned: Bool
        var transferable: Bool
        // Required in host snapshots: missing policy must not enable editing.
        var editable: Bool
    }
    var items: [Item]
    var selected: UInt64
}

private struct WalkawayTabEvent: Encodable {
    var window_token: UInt64
    var action: String
    var tab_id: UInt64 = 0
    var target_index: Int = 0
    var pinned: Bool = false
    var dest_window_token: UInt64 = 0
    var screen_x: Double = 0
    var screen_y: Double = 0
    var text: String? = nil
}

struct WalkawayTabError: Error, CustomStringConvertible {
    let description: String
    init(_ message: String) { description = message }
}

func walkawaySetError(_ error: Error) {
    Thread.current.threadDictionary["WalkawayNativeTabsError"] = String(describing: error)
}

/// Stable, injective identity, including UInt64.max. Neither random UUIDs nor a
/// JSON/Double conversion can change a tab's identity as it crosses windows.
func walkawayTabUUID(_ id: UInt64) -> UUID {
    UUID(uuid: (0x57, 0x41, 0x4c, 0x4b, 0x54, 0x41, 0x42, 0x53,
                UInt8(truncatingIfNeeded: id >> 56), UInt8(truncatingIfNeeded: id >> 48),
                UInt8(truncatingIfNeeded: id >> 40), UInt8(truncatingIfNeeded: id >> 32),
                UInt8(truncatingIfNeeded: id >> 24), UInt8(truncatingIfNeeded: id >> 16),
                UInt8(truncatingIfNeeded: id >> 8), UInt8(truncatingIfNeeded: id)))
}

@MainActor
final class WalkawayTabBridge: NSObject {
    // Registry is main-thread-only. It owns live bridges, and is the authority
    // used to reject stale opaque handles and foreign/destroyed drop windows.
    static var handles: [UInt: WalkawayTabBridge] = [:]
    static var windows: [ObjectIdentifier: UInt] = [:]

    let owner: UInt64
    weak var contentView: NSView?
    weak var window: NSWindow?
    let transferIdentity = NSObject()
    private var callback: WalkawayTabCallback?
    private var callbackContext: UnsafeMutableRawPointer?
    private(set) var hostingView: NSHostingView<CompactTabStrip>?
    private(set) var snapshot: WalkawayTabSnapshot?
    private var tabIDs: [UUID: UInt64] = [:]
    private var addressDrafts: [UUID: String] = [:]
    private var cellBindings: [UUID: (cell: CompactTabCell, focus: (Bool) -> Void, observations: [NSKeyValueObservation])] = [:]
    private var textObservers: [NSObjectProtocol] = []
    private var keyMonitor: Any?
    private var enforcingPolicy = false
    // Future completion providers receive only this strip's real edits and an
    // AppKit screen-space anchor. The original cell remains the field delegate.
    var onAddressTextChange: ((UInt64, String, NSRect) -> Void)?
    // Original context-menu Refresh calls onSelect(id) then onReload() before
    // Rust consumes either event. This is callback routing, not browser state.
    private var callbackSelection: UUID?
    private(set) var logicalFrame = NSRect(x: 0, y: 74, width: 0, height: 38)
    private(set) var destroyed = false
    private var appearanceMode: Int32 = -1
    private(set) var overlaysVisible = true

    init(content: NSView, owner: UInt64, callback: WalkawayTabCallback?, context: UnsafeMutableRawPointer?) throws {
        guard let window = content.window else { throw WalkawayTabError("winit content view has no NSWindow") }
        guard window.contentView === content else {
            throw WalkawayTabError("expected the actual winit NSWindow.contentView")
        }
        guard Self.windows[ObjectIdentifier(window)] == nil,
              !Self.handles.values.contains(where: { $0.owner == owner }) else {
            throw WalkawayTabError("native tab window or owner is already registered")
        }
        self.owner = owner
        self.contentView = content
        self.window = window
        self.callback = callback
        self.callbackContext = context
        super.init()
        let host = NSHostingView(rootView: root(items: [], selected: walkawayTabUUID(0)))
        host.sizingOptions = []
        host.translatesAutoresizingMaskIntoConstraints = true
        host.autoresizingMask = [.width, content.isFlipped ? .maxYMargin : .minYMargin]
        host.setAccessibilityLabel("Browser tabs")
        hostingView = host
        let key = UInt(bitPattern: Unmanaged.passUnretained(self).toOpaque())
        Self.handles[key] = self
        Self.windows[ObjectIdentifier(window)] = key
        // Address editing is INSIDE the original tabs in this one 74..112 row.
        content.addSubview(host, positioned: .above, relativeTo: nil)
        try setFrame(x: 0, y: 74, width: max(1, content.bounds.width), height: 38)
        textObservers.append(NotificationCenter.default.addObserver(
            forName: NSControl.textDidChangeNotification, object: nil, queue: .main
        ) { [weak self] notification in
            MainActor.assumeIsolated { self?.addressTextChanged(notification) }
        })
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            let handled = MainActor.assumeIsolated { self?.handleAddressKey(event) == true }
            return handled ? nil : event
        }
    }

    var opaque: UnsafeMutableRawPointer { Unmanaged.passUnretained(self).toOpaque() }

    static func lookup(_ handle: UnsafeMutableRawPointer?) throws -> WalkawayTabBridge {
        guard let handle, let bridge = handles[UInt(bitPattern: handle)], !bridge.destroyed else {
            throw WalkawayTabError("native tab handle is absent or destroyed")
        }
        return bridge
    }

    var stripView: CompactTabStripView? {
        func find(_ view: NSView) -> CompactTabStripView? {
            if let strip = view as? CompactTabStripView { return strip }
            for child in view.subviews { if let strip = find(child) { return strip } }
            return nil
        }
        return hostingView.flatMap(find)
    }

    var tabCells: [CompactTabCell] {
        guard let strip = stripView else { return [] }
        func find(_ view: NSView) -> [CompactTabCell] {
            if let cell = view as? CompactTabCell, cell.owner === strip { return [cell] }
            return view.subviews.flatMap(find)
        }
        return find(strip)
    }

    private var editingCell: CompactTabCell? {
        guard let window, let editor = window.firstResponder as? NSTextView, editor.isFieldEditor else { return nil }
        return tabCells.first { $0.address.currentEditor() === editor }
    }

    private func isEditable(_ id: UUID) -> Bool {
        guard !destroyed, let tabID = tabIDs[id] else { return false }
        return snapshot?.items.first(where: { $0.id == tabID })?.editable == true
    }

    private func root(items: [CompactTabItem], selected: UUID) -> CompactTabStrip {
        CompactTabStrip(
            items: items, selection: selected,
            onSelect: { [weak self] id in
                guard let self else { return }
                if self.emit("select", id: id) { self.callbackSelection = id }
            },
            onClose: { [weak self] id in _ = self?.emit("close", id: id) },
            onInsert: { [weak self] in _ = self?.emit("new") },
            onMove: { [weak self] id, index in _ = self?.emit("move", id: id, index: index) },
            onDetach: { [weak self] id, point in
                guard let self, self.canTransfer(id) else { return }
                _ = self.emit("detach", id: id, point: point)
            },
            onSearch: { [weak self] text in
                guard let self,
                      let id = self.editingCell?.item.id ?? self.callbackSelection,
                      self.isEditable(id), !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
                // The original invokes onSearch BEFORE resigning its editor.
                // A queued selection may differ from the cell that was edited.
                _ = self.emit("search", id: id, text: text)
            },
            onReload: { [weak self] in
                guard let self else { return }
                _ = self.emit("reload", id: self.callbackSelection ?? self.snapshot.map { walkawayTabUUID($0.selected) })
            },
            onSetPinned: { [weak self] id, pinned in _ = self?.emit("pin", id: id, pinned: pinned) },
            labelMode: .address,
            hoverPreview: overlaysVisible ? .init() : nil,
            // Original captures the actual GPU-backed winit view once, before
            // creating its floating drag panel, not once per animation frame.
            previewSourceView: contentView,
            transferOwner: transferIdentity,
            onTransfer: { [weak self] id, destinationView, index in
                self?.transfer(id, to: destinationView, at: index) ?? false
            }
        )
    }

    func update(_ next: WalkawayTabSnapshot) throws {
        guard !destroyed, let host = hostingView else { throw WalkawayTabError("native tab host was destroyed") }
        guard next != snapshot else { synchronizeCells(); return }
        guard Set(next.items.map(\.id)).count == next.items.count else {
            throw WalkawayTabError("duplicate browser tab ids")
        }
        guard next.items.isEmpty || next.items.contains(where: { $0.id == next.selected }) else {
            throw WalkawayTabError("selected browser tab is not present")
        }
        let previous = snapshot
        let items = next.items.map { item in
            CompactTabItem(id: walkawayTabUUID(item.id), title: item.title, address: item.url,
                          searchPrompt: "Search or enter address", symbol: item.url.isEmpty ? "doc.text" : "globe",
                          isBusy: item.loading, isTrusted: item.secure, isPinned: item.pinned)
        }
        tabIDs = Dictionary(uniqueKeysWithValues: next.items.map { (walkawayTabUUID($0.id), $0.id) })
        snapshot = next
        callbackSelection = walkawayTabUUID(next.selected)
        addressDrafts = addressDrafts.filter { id, _ in
            guard let item = next.items.first(where: { walkawayTabUUID($0.id) == id }), item.editable else { return false }
            return previous?.items.first(where: { $0.id == item.id })?.url == item.url
        }
        // Revoke a live editor before original configure can change its item.
        synchronizeCells()
        // Stable root type retains original cells, tracking, timers and springs.
        host.rootView = root(items: items, selected: walkawayTabUUID(next.selected))
        host.layoutSubtreeIfNeeded()
        synchronizeCells()
    }

    /// Bind only the original fields. configure()/appearance can reset the
    /// original public editable flags; per-field observations reapply policy
    /// synchronously, including between an original mouse-down and focus call.
    private func synchronizeCells() {
        guard !destroyed else { return }
        let cells = tabCells
        for id in Array(cellBindings.keys) where !cells.contains(where: { $0.item.id == id }) {
            if let binding = cellBindings.removeValue(forKey: id) {
                binding.cell.address.onFocusChange = binding.focus
                binding.observations.forEach { $0.invalidate() }
            }
        }
        for cell in cells {
            if cellBindings[cell.item.id] == nil {
                let focus = cell.address.onFocusChange
                cell.address.onFocusChange = { [weak self, weak cell] focused in
                    focus(focused) // Preserve original halo, alignment and draft lifecycle.
                    guard let self, let cell else { return }
                    if focused, !self.isEditable(cell.item.id) { self.applyPolicy(to: cell); return }
                    if focused, let draft = self.addressDrafts[cell.item.id] {
                        cell.address.stringValue = draft
                        cell.address.currentEditor()?.string = draft
                    }
                }
                let observations = [
                    cell.address.observe(\.isEditable, options: [.new]) { [weak self, weak cell] _, _ in
                        MainActor.assumeIsolated { if let cell { self?.applyPolicy(to: cell) } }
                    },
                    cell.address.observe(\.isSelectable, options: [.new]) { [weak self, weak cell] _, _ in
                        MainActor.assumeIsolated { if let cell { self?.applyPolicy(to: cell) } }
                    }
                ]
                cellBindings[cell.item.id] = (cell, focus, observations)
            }
            applyPolicy(to: cell)
        }
    }

    private func applyPolicy(to cell: CompactTabCell) {
        guard !destroyed, !enforcingPolicy else { return }
        enforcingPolicy = true
        defer { enforcingPolicy = false }
        let editable = isEditable(cell.item.id)
        if !editable {
            addressDrafts.removeValue(forKey: cell.item.id)
            if let editor = cell.address.currentEditor() as? NSTextView {
                // Use the original cancellation path, not a substitute delegate.
                _ = cell.control(cell.address, textView: editor, doCommandBy: #selector(NSResponder.cancelOperation(_:)))
            }
        }
        if cell.address.isEditable != editable { cell.address.isEditable = editable }
        if cell.address.isSelectable != editable { cell.address.isSelectable = editable }
        cell.address.setAccessibilityHelp(editable ? "Address or search query" : "Address editing disabled by supervision")
    }

    private func addressTextChanged(_ notification: Notification) {
        guard !destroyed, let field = notification.object as? CompactAddressField,
              let cell = field.superview as? CompactTabCell, cell.owner === stripView,
              let tabID = tabIDs[cell.item.id], isEditable(cell.item.id), let window = field.window else { return }
        addressDrafts[cell.item.id] = field.stringValue
        onAddressTextChange?(tabID, field.stringValue, window.convertToScreen(field.convert(field.bounds, to: nil)))
    }

    /// winit has no Edit menu. Route only this strip's live field editor to
    /// standard AppKit selectors; other fields/windows keep their normal input.
    @discardableResult
    func handleAddressKey(_ event: NSEvent) -> Bool {
        guard !destroyed, overlaysVisible, event.window === window,
              let cell = editingCell, let editor = cell.address.currentEditor() as? NSTextView else { return false }
        guard isEditable(cell.item.id) else { applyPolicy(to: cell); return true }
        if event.keyCode == 53, event.modifierFlags.intersection([.command, .control, .option]).isEmpty {
            addressDrafts.removeValue(forKey: cell.item.id)
            // Let the original delegate handle Escape and restore item.address.
            return false
        }
        guard event.modifierFlags.intersection([.command, .control, .option]) == .command,
              let key = event.charactersIgnoringModifiers?.lowercased() else { return false }
        switch key {
        case "a", "l": editor.selectAll(nil)
        case "c": editor.copy(nil)
        case "x": editor.cut(nil)
        case "v": editor.paste(nil)
        case "z":
            if event.modifierFlags.contains(.shift) { editor.undoManager?.redo() }
            else { editor.undoManager?.undo() }
        default: return false
        }
        return true
    }

    func setFrame(x: Double, y: Double, width: Double, height: Double) throws {
        guard [x, y, width, height].allSatisfy(\.isFinite), width > 0, height > 0,
              width <= 32768, height <= 32768, let content = contentView, let host = hostingView else {
            throw WalkawayTabError("invalid native tab frame or detached content view")
        }
        logicalFrame = NSRect(x: x, y: y, width: width, height: height)
        let frame = NSRect(x: content.bounds.minX + x,
                           y: content.isFlipped ? content.bounds.minY + y : content.bounds.maxY - y - height,
                           width: width, height: height)
        if host.frame != frame { host.frame = frame }
        host.layoutSubtreeIfNeeded()
        synchronizeCells()
    }

    func setAppearance(_ mode: Int32) throws {
        guard (-1...1).contains(mode), let host = hostingView else { throw WalkawayTabError("invalid native appearance") }
        guard appearanceMode != mode else { return }
        appearanceMode = mode
        host.appearance = mode < 0 ? nil : NSAppearance(named: mode == 1 ? .darkAqua : .aqua)
        host.layoutSubtreeIfNeeded()
        synchronizeCells()
    }

    func setOverlaysVisible(_ visible: Bool) {
        guard !destroyed, overlaysVisible != visible else { return }
        overlaysVisible = visible
        if !visible {
            stripView?.cancelOperation(nil)
            stripView?.hoverPreview = nil
            if editingCell != nil { window?.makeFirstResponder(nil) }
        } else { stripView?.hoverPreview = .init() }
        hostingView?.isHidden = !visible
        synchronizeCells()
    }

    func focusAddress() throws {
        hostingView?.layoutSubtreeIfNeeded()
        synchronizeCells()
        guard !destroyed, overlaysVisible, let selected = snapshot?.selected,
              let cell = tabCells.first(where: { $0.item.id == walkawayTabUUID(selected) && $0.selected }),
              isEditable(cell.item.id), !cell.address.isHidden else {
            throw WalkawayTabError("native tab address is unavailable or not editable")
        }
        cell.focusAddress()
        guard let editor = cell.address.currentEditor() as? NSTextView else {
            throw WalkawayTabError("original tab address did not acquire its field editor")
        }
        editor.selectAll(nil)
    }

    private func canTransfer(_ id: UUID) -> Bool {
        guard !destroyed, let tabID = tabIDs[id] else { return false }
        return snapshot?.items.first(where: { $0.id == tabID })?.transferable == true
    }

    private func transfer(_ id: UUID, to object: AnyObject, at index: Int) -> Bool {
        // onTransfer's AnyObject IS the destination CompactTabStripView, not
        // transferOwner. Resolve its NSWindow through our registration table.
        guard canTransfer(id), let view = object as? CompactTabStripView,
              let targetWindow = view.window,
              let key = Self.windows[ObjectIdentifier(targetWindow)],
              let target = Self.handles[key], target !== self, !target.destroyed,
              target.snapshot?.items.allSatisfy(\.transferable) == true,
              view.transferOwner === target.transferIdentity,
              target.stripView === view,
              target.contentView?.window === targetWindow else { return false }
        // true means validated and queued, NOT committed. Rust revalidates owners
        // and supervision on delivery, then publishes authoritative snapshots.
        return emit("transfer", id: id, index: index, destination: target.owner, point: NSEvent.mouseLocation)
    }

    @discardableResult
    fileprivate func emit(_ action: String, id: UUID? = nil, index: Int = 0,
                      pinned: Bool = false, destination: UInt64 = 0,
                      point: NSPoint = .zero, text: String? = nil) -> Bool {
        guard !destroyed, let callback else { return false }
        let tabID: UInt64
        if let id {
            guard let value = tabIDs[id], snapshot?.items.contains(where: { $0.id == value }) == true else { return false }
            tabID = value
        } else { tabID = 0 }
        let event = WalkawayTabEvent(window_token: owner, action: action, tab_id: tabID,
                                    target_index: max(0, index), pinned: pinned,
                                    dest_window_token: destination, screen_x: point.x, screen_y: point.y, text: text)
        guard let json = try? JSONEncoder().encode(event) else { return false }
        return json.withUnsafeBytes { bytes in
            callback(callbackContext, bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count) != 0
        }
    }

    func capture() throws -> (UnsafeMutablePointer<UInt8>, Int, UInt32, UInt32) {
        guard let host = hostingView, host.window != nil, host.bounds.width > 0, host.bounds.height > 0 else {
            throw WalkawayTabError("native tab strip is not attached or sized")
        }
        host.layoutSubtreeIfNeeded()
        synchronizeCells()
        host.displayIfNeeded()
        CATransaction.flush()
        guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else {
            throw WalkawayTabError("AppKit cannot allocate native tab capture")
        }
        host.cacheDisplay(in: host.bounds, to: rep)
        guard let image = rep.cgImage, image.width > 0, image.height > 0,
              image.width <= 65536, image.height <= 65536 else {
            throw WalkawayTabError("AppKit native tab capture has no pixels")
        }
        let width = image.width, height = image.height
        let length = width * height * 4
        guard length <= 512 * 1024 * 1024 else { throw WalkawayTabError("native tab capture is too large") }
        let bytes = UnsafeMutablePointer<UInt8>.allocate(capacity: length)
        bytes.initialize(repeating: 0, count: length)
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(data: bytes, width: width, height: height, bitsPerComponent: 8,
                                      bytesPerRow: width * 4, space: space,
                                      bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue) else {
            bytes.deallocate()
            throw WalkawayTabError("CoreGraphics cannot allocate RGBA context")
        }
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        // CoreGraphics is premultiplied; the FFI contract is straight alpha.
        for offset in stride(from: 0, to: length, by: 4) {
            let alpha = Int(bytes[offset + 3])
            if alpha == 0 { bytes[offset] = 0; bytes[offset + 1] = 0; bytes[offset + 2] = 0 }
            else if alpha < 255 {
                for channel in 0..<3 {
                    bytes[offset + channel] = UInt8(min(255, (Int(bytes[offset + channel]) * 255 + alpha / 2) / alpha))
                }
            }
        }
        return (bytes, length, UInt32(width), UInt32(height))
    }

    func destroy() {
        guard !destroyed else { return }
        destroyed = true
        // Disconnect Rust FIRST; retained original callbacks cannot enqueue work.
        callback = nil
        callbackContext = nil
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
        keyMonitor = nil
        textObservers.forEach(NotificationCenter.default.removeObserver)
        textObservers.removeAll()
        onAddressTextChange = nil
        if editingCell != nil { window?.makeFirstResponder(nil) }
        for binding in cellBindings.values {
            binding.cell.address.onFocusChange = binding.focus
            binding.observations.forEach { $0.invalidate() }
        }
        cellBindings.removeAll()
        if let window { Self.windows.removeValue(forKey: ObjectIdentifier(window)) }
        Self.handles.removeValue(forKey: UInt(bitPattern: opaque))
        if let strip = stripView {
            strip.cancelOperation(nil)
            strip.hoverPreview = nil
            strip.previewSourceView = nil
            strip.transferOwner = nil
            strip.onSelect = { _ in }; strip.onClose = { _ in }; strip.onInsert = {}
            strip.onMove = { _, _ in }; strip.onDetach = { _, _ in }
            strip.onSearch = { _ in }; strip.onReload = {}; strip.onSetPinned = { _, _ in }
            strip.onTransfer = { _, _, _ in false }
        }
        func untrack(_ view: NSView) {
            view.trackingAreas.forEach(view.removeTrackingArea)
            (view as? NSProgressIndicator)?.stopAnimation(nil)
            view.subviews.forEach(untrack)
        }
        if let host = hostingView {
            untrack(host)
            // Original viewDidMoveToWindow unregisters its global drag target.
            host.removeFromSuperview()
            host.rootView = CompactTabStrip(items: [], selection: walkawayTabUUID(0),
                onSelect: { _ in }, onClose: { _ in }, onInsert: {}, onMove: { _, _ in }, onDetach: { _, _ in },
                hoverPreview: nil)
        }
        addressDrafts.removeAll()
        callbackSelection = nil
        hostingView = nil
        snapshot = nil
        tabIDs.removeAll()
    }
}

private func walkawayMain<T>(_ fallback: T, _ body: @MainActor () throws -> T) -> T {
    guard Thread.isMainThread else {
        walkawaySetError(WalkawayTabError("all native tab operations must run on the main thread"))
        return fallback
    }
    do { return try MainActor.assumeIsolated { try body() } }
    catch { walkawaySetError(error); return fallback }
}

@_cdecl("walkaway_tabs_is_main_thread")
func walkawayTabsIsMainThread() -> Int32 { Thread.isMainThread ? 1 : 0 }

@_cdecl("walkaway_tabs_last_error")
func walkawayTabsLastError(_ buffer: UnsafeMutablePointer<UInt8>?, _ capacity: Int) -> Int {
    let text = (Thread.current.threadDictionary["WalkawayNativeTabsError"] as? String) ?? "native tab operation failed"
    let bytes = Array(text.utf8)
    guard let buffer, capacity > 0 else { return bytes.count }
    let count = min(capacity, bytes.count)
    bytes.withUnsafeBufferPointer { source in
        if let start = source.baseAddress { buffer.update(from: start, count: count) }
    }
    return count
}

@_cdecl("walkaway_tabs_create")
func walkawayTabsCreate(_ content: UnsafeMutableRawPointer?, _ owner: UInt64,
                        _ callback: WalkawayTabCallback?, _ context: UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer? {
    walkawayMain(nil) {
        guard let content else { throw WalkawayTabError("null winit content view") }
        let view = Unmanaged<NSView>.fromOpaque(content).takeUnretainedValue()
        return try WalkawayTabBridge(content: view, owner: owner, callback: callback, context: context).opaque
    }
}

@_cdecl("walkaway_tabs_update")
func walkawayTabsUpdate(_ handle: UnsafeMutableRawPointer?, _ json: UnsafePointer<UInt8>?, _ length: Int) -> Int32 {
    walkawayMain(0) {
        guard let json, length > 0, length <= 32 * 1024 * 1024 else { throw WalkawayTabError("invalid native tab JSON buffer") }
        let snapshot = try JSONDecoder().decode(WalkawayTabSnapshot.self, from: Data(bytes: json, count: length))
        try WalkawayTabBridge.lookup(handle).update(snapshot)
        return 1
    }
}

@_cdecl("walkaway_tabs_frame")
func walkawayTabsFrame(_ handle: UnsafeMutableRawPointer?, _ x: Double, _ y: Double, _ width: Double, _ height: Double) -> Int32 {
    walkawayMain(0) {
        try WalkawayTabBridge.lookup(handle).setFrame(x: x, y: y, width: width, height: height)
        return 1
    }
}

@_cdecl("walkaway_tabs_appearance")
func walkawayTabsAppearance(_ handle: UnsafeMutableRawPointer?, _ mode: Int32) -> Int32 {
    walkawayMain(0) {
        try WalkawayTabBridge.lookup(handle).setAppearance(mode)
        return 1
    }
}

@_cdecl("walkaway_tabs_overlays_visible")
func walkawayTabsOverlaysVisible(_ handle: UnsafeMutableRawPointer?, _ visible: Int32) -> Int32 {
    walkawayMain(0) { try WalkawayTabBridge.lookup(handle).setOverlaysVisible(visible != 0); return 1 }
}

@_cdecl("walkaway_tabs_focus_address")
func walkawayTabsFocusAddress(_ handle: UnsafeMutableRawPointer?) -> Int32 {
    walkawayMain(0) { try WalkawayTabBridge.lookup(handle).focusAddress(); return 1 }
}

@_cdecl("walkaway_tabs_destroy")
func walkawayTabsDestroy(_ handle: UnsafeMutableRawPointer?) {
    walkawayMain(()) { try WalkawayTabBridge.lookup(handle).destroy() }
}

@_cdecl("walkaway_tabs_capture_rgba")
func walkawayTabsCaptureRGBA(_ handle: UnsafeMutableRawPointer?, _ output: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
                            _ width: UnsafeMutablePointer<UInt32>?, _ height: UnsafeMutablePointer<UInt32>?) -> Int {
    output?.pointee = nil; width?.pointee = 0; height?.pointee = 0
    return walkawayMain(0) {
        guard let output, let width, let height else { throw WalkawayTabError("missing native capture output pointers") }
        let (bytes, count, w, h) = try WalkawayTabBridge.lookup(handle).capture()
        output.pointee = bytes; width.pointee = w; height.pointee = h
        return count
    }
}

@_cdecl("walkaway_tabs_free_rgba")
func walkawayTabsFreeRGBA(_ bytes: UnsafeMutablePointer<UInt8>?) { bytes?.deallocate() }

@_cdecl("walkaway_tabs_position_window")
func walkawayTabsPositionWindow(_ content: UnsafeMutableRawPointer?, _ x: Double, _ y: Double) -> Int32 {
    walkawayMain(0) {
        guard let content, x.isFinite, y.isFinite else { throw WalkawayTabError("invalid native window position") }
        let view = Unmanaged<NSView>.fromOpaque(content).takeUnretainedValue()
        guard let window = view.window else { throw WalkawayTabError("detached winit content view") }
        let point = NSPoint(x: x, y: y)
        let screen = NSScreen.screens.first(where: { $0.frame.contains(point) }) ?? window.screen ?? NSScreen.main
        var frame = window.frame
        let contentHeight = view.bounds.height
        let titlebarHeight = max(0, frame.height - contentHeight)
        // Put the original tab row near release in AppKit global point space.
        frame.origin = NSPoint(x: x - min(140, frame.width / 2),
                               y: y - frame.height + titlebarHeight + 74 + 19)
        if let screen { frame = window.constrainFrameRect(frame, to: screen) }
        window.setFrame(frame, display: true)
        return 1
    }
}
