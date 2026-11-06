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
    private(set) var addressField: WalkawayAddressField?
    fileprivate var addressText = ""
    fileprivate var addressDraft: String?
    private var addressEditable = true
    private var addressStatus: (loading: Bool, secure: Bool) = (false, false)
    private(set) var addressFrame = NSRect(x: 64, y: 78, width: 400, height: 24)
    private var hasAddressFrame = false
    private(set) var snapshot: WalkawayTabSnapshot?
    private var tabIDs: [UUID: UInt64] = [:]
    // Original context-menu Refresh calls onSelect(id) then onReload() before
    // Rust consumes either event. This carries only callback routing, never a
    // speculative browser/SwiftUI selection or ownership mutation.
    private var callbackSelection: UUID?
    private(set) var logicalFrame = NSRect(x: 0, y: 106, width: 0, height: 38)
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
        // Overlay ONLY the original 106..144pt tab row. The renderer's view and
        // layer, titlebar, navigation, debugger and page are left untouched.
        content.addSubview(host, positioned: .above, relativeTo: nil)
        try setFrame(x: 0, y: 106, width: max(1, content.bounds.width), height: 38)
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
                guard let self else { return }
                _ = self.emit("search", id: self.snapshot.map { walkawayTabUUID($0.selected) }, text: text)
            },
            onReload: { [weak self] in
                guard let self else { return }
                _ = self.emit("reload", id: self.callbackSelection ?? self.snapshot.map { walkawayTabUUID($0.selected) })
            },
            onSetPinned: { [weak self] id, pinned in _ = self?.emit("pin", id: id, pinned: pinned) },
            labelMode: .fixed,
            hoverPreview: overlaysVisible ? .init() : nil,
            // This is the actual GPU-backed winit contentView. The original
            // snapshots it once BEFORE creating the floating drag panel, caches
            // that page thumbnail, then animates without per-frame page capture.
            previewSourceView: contentView,
            transferOwner: transferIdentity,
            onTransfer: { [weak self] id, destinationView, index in
                self?.transfer(id, to: destinationView, at: index) ?? false
            }
        )
    }

    func update(_ next: WalkawayTabSnapshot) throws {
        guard !destroyed, let host = hostingView else { throw WalkawayTabError("native tab host was destroyed") }
        guard next != snapshot else { return }
        guard Set(next.items.map(\.id)).count == next.items.count else {
            throw WalkawayTabError("duplicate browser tab ids")
        }
        guard next.items.isEmpty || next.items.contains(where: { $0.id == next.selected }) else {
            throw WalkawayTabError("selected browser tab is not present")
        }
        let items = next.items.map { item in
            CompactTabItem(id: walkawayTabUUID(item.id), title: item.title, address: item.url,
                          symbol: item.url.isEmpty ? "doc.text" : "globe",
                          isBusy: item.loading, isTrusted: item.secure, isPinned: item.pinned)
        }
        if snapshot?.selected != next.selected, addressField?.currentEditor() != nil {
            addressField?.window?.makeFirstResponder(nil)
            addressDraft = nil
        }
        tabIDs = Dictionary(uniqueKeysWithValues: next.items.map { (walkawayTabUUID($0.id), $0.id) })
        snapshot = next
        callbackSelection = walkawayTabUUID(next.selected)
        // A stable root type keeps the original NSView and its tracking areas,
        // close-width freeze, timers, cells and spring presentation layers alive.
        host.rootView = root(items: items, selected: walkawayTabUUID(next.selected))
        host.layoutSubtreeIfNeeded()
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
        guard host.frame != frame else { return }
        host.frame = frame
        host.layoutSubtreeIfNeeded()
    }

    func setAppearance(_ mode: Int32) throws {
        guard (-1...1).contains(mode), let host = hostingView else { throw WalkawayTabError("invalid native appearance") }
        guard appearanceMode != mode else { return }
        appearanceMode = mode
        let appearance = mode < 0 ? nil : NSAppearance(named: mode == 1 ? .darkAqua : .aqua)
        host.appearance = appearance
        addressField?.appearance = appearance
    }

    func setOverlaysVisible(_ visible: Bool) {
        guard !destroyed, overlaysVisible != visible else { return }
        overlaysVisible = visible
        if !visible {
            stripView?.cancelOperation(nil)
            stripView?.hoverPreview = nil
            if addressField?.currentEditor() != nil { addressField?.window?.makeFirstResponder(nil) }
        } else { stripView?.hoverPreview = .init() }
        hostingView?.isHidden = !visible
        addressField?.isHidden = !visible
    }

    func setAddress(text: String, loading: Bool, secure: Bool, editable: Bool) throws {
        guard !destroyed, let content = contentView else { throw WalkawayTabError("native address host was destroyed") }
        let isNew = addressField == nil
        let field: WalkawayAddressField
        if let existing = addressField { field = existing }
        else {
            field = WalkawayAddressField(frame: .zero)
            field.cell = WalkawayAddressCell(textCell: "")
            field.installAccessories()
            field.setStatus(loading: loading, secure: secure)
            field.delegate = self
            field.font = .systemFont(ofSize: 13)
            field.isBezeled = true; field.bezelStyle = .roundedBezel
            field.drawsBackground = true
            field.backgroundColor = .textBackgroundColor
            field.textColor = .textColor
            field.focusRingType = .default
            field.lineBreakMode = .byTruncatingMiddle
            field.usesSingleLineMode = true
            field.cell?.isScrollable = true
            field.placeholderString = "Search or enter address"
            field.setAccessibilityLabel("Address and search")
            field.autoresizingMask = [.width, content.isFlipped ? .maxYMargin : .minYMargin]
            field.appearance = hostingView?.appearance
            field.isHidden = !overlaysVisible
            // Suggestions are deliberately not invented. NSTextField's editor
            // and textDidChange delegate provide a stable native anchor/query
            // boundary for a future real autocomplete provider.
            content.addSubview(field, positioned: .above, relativeTo: hostingView)
            addressField = field
            let frame = hasAddressFrame ? addressFrame
                : NSRect(x: 64, y: 78, width: max(40, content.bounds.width - 74), height: 24)
            try setAddressFrame(x: frame.minX, y: frame.minY, width: frame.width, height: frame.height)
        }
        let changed = addressText != text || addressEditable != editable
            || addressStatus.loading != loading || addressStatus.secure != secure
        guard isNew || changed || field.stringValue != text && field.currentEditor() == nil else { return }
        addressText = text
        addressEditable = editable
        addressStatus = (loading, secure)
        if !editable, field.currentEditor() != nil { field.window?.makeFirstResponder(nil) }
        field.isEditable = editable
        field.isSelectable = editable
        if field.currentEditor() == nil {
            field.stringValue = text
            addressDraft = nil
        }
        field.setStatus(loading: loading, secure: secure)
        field.setAccessibilityHelp(loading ? "Loading page" : (secure ? "Secure page" : "Address or search query"))
    }

    func setAddressFrame(x: Double, y: Double, width: Double, height: Double) throws {
        guard [x, y, width, height].allSatisfy(\.isFinite), width > 0, height > 0,
              width <= 32768, height <= 32768, let content = contentView, !destroyed else {
            throw WalkawayTabError("invalid native address frame or detached content view")
        }
        addressFrame = NSRect(x: x, y: y, width: width, height: height)
        hasAddressFrame = true
        guard let field = addressField else { return }
        let frame = NSRect(x: content.bounds.minX + x,
                          y: content.isFlipped ? content.bounds.minY + y : content.bounds.maxY - y - height,
                          width: width, height: height)
        if field.frame != frame { field.frame = frame }
    }

    func focusAddress() throws {
        guard !destroyed, let field = addressField, field.isEditable,
              let window = field.window, window.makeFirstResponder(field) else {
            throw WalkawayTabError("native address field is unavailable or not editable")
        }
        field.selectText(nil)
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
        // true means "validated and queued", NOT committed. The browser must
        // revalidate both owners/tab/session on EventLoopProxy delivery, then
        // publish its authoritative snapshots. Original settle waits for them.
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

    func capture(address: Bool = false) throws -> (UnsafeMutablePointer<UInt8>, Int, UInt32, UInt32) {
        let view: NSView? = address ? addressField : hostingView
        guard let host = view, host.window != nil, host.bounds.width > 0, host.bounds.height > 0 else {
            throw WalkawayTabError("native tab strip is not attached or sized")
        }
        host.layoutSubtreeIfNeeded()
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
        // CoreGraphics emits premultiplied RGBA. The FFI contract is straight
        // alpha so Rust can composite without dark fringes over renderer pixels.
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
        // Disconnect the Rust context FIRST. Timers and SwiftUI can keep old
        // closures alive briefly, but every closure captures this bridge weakly
        // and destroyed bridges cannot enqueue callbacks or accept transfers.
        callback = nil
        callbackContext = nil
        if let window { Self.windows.removeValue(forKey: ObjectIdentifier(window)) }
        Self.handles.removeValue(forKey: UInt(bitPattern: opaque))
        if let strip = stripView {
            strip.cancelOperation(nil) // also cancels settling, floating panels and event monitors
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
            // Removal calls the original viewDidMoveToWindow, synchronously
            // unregistering the global drag target and restoring movability.
            host.removeFromSuperview()
            host.rootView = CompactTabStrip(items: [], selection: walkawayTabUUID(0),
                onSelect: { _ in }, onClose: { _ in }, onInsert: {}, onMove: { _, _ in }, onDetach: { _, _ in },
                hoverPreview: nil)
        }
        if let field = addressField {
            if field.currentEditor() != nil { field.window?.makeFirstResponder(nil) }
            field.delegate = nil
            untrack(field)
            field.removeFromSuperview()
        }
        addressField = nil
        addressDraft = nil
        callbackSelection = nil
        hostingView = nil
        snapshot = nil
        tabIDs.removeAll()
    }
}

@MainActor
extension WalkawayTabBridge: NSTextFieldDelegate {
    func controlTextDidChange(_ notification: Notification) {
        guard !destroyed, let field = notification.object as? WalkawayAddressField, field === addressField else { return }
        addressDraft = field.stringValue
        // A real autocomplete provider can consume the draft and anchor to
        // addressField.bounds here. No guessed entries or popover are emitted.
    }

    func controlTextDidEndEditing(_ notification: Notification) {
        guard let field = notification.object as? WalkawayAddressField, field === addressField else { return }
        addressDraft = nil
        field.stringValue = addressText
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard !destroyed, control === addressField, let field = addressField else { return false }
        if selector == #selector(NSResponder.insertNewline(_:)) {
            let query = textView.string.trimmingCharacters(in: .whitespacesAndNewlines)
            if !query.isEmpty {
                _ = emit("search", id: snapshot.map { walkawayTabUUID($0.selected) }, text: query)
            }
            field.window?.makeFirstResponder(nil)
            return true
        }
        if selector == #selector(NSResponder.cancelOperation(_:)) {
            addressDraft = nil
            textView.string = addressText
            field.stringValue = addressText
            field.window?.makeFirstResponder(nil)
            return true
        }
        return false
    }
}

/// Normal AppKit field editor, including clipboard selectors, selection,
/// horizontal scrolling, accessibility and IME. Accessories never steal input.
final class WalkawayAddressField: NSTextField {
    private let statusIcon = NSImageView()
    private let spinner = NSProgressIndicator()
    override var mouseDownCanMoveWindow: Bool { false }

    func installAccessories() {
        statusIcon.imageScaling = .scaleProportionallyDown
        statusIcon.setAccessibilityElement(false)
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isIndeterminate = true
        spinner.isDisplayedWhenStopped = false
        spinner.setAccessibilityLabel("Loading page")
        addSubview(statusIcon); addSubview(spinner)
    }

    func setStatus(loading: Bool, secure: Bool) {
        statusIcon.image = NSImage(systemSymbolName: secure ? "lock" : "globe", accessibilityDescription: nil)
        statusIcon.contentTintColor = secure ? .secondaryLabelColor : .tertiaryLabelColor
        if loading { spinner.startAnimation(nil) } else { spinner.stopAnimation(nil) }
        spinner.isHidden = !loading
        needsLayout = true
    }

    override func layout() {
        super.layout()
        statusIcon.frame = NSRect(x: 7, y: (bounds.height - 14) / 2, width: 14, height: 14)
        spinner.frame = NSRect(x: bounds.width - 22, y: (bounds.height - 16) / 2, width: 16, height: 16)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === statusIcon || hit === spinner ? self : hit
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        // winit's default application menu has no Edit menu. Route standard
        // equivalents to AppKit's field editor instead of reimplementing text
        // editing in Rust or letting a browser shortcut swallow the keystroke.
        guard let editor = currentEditor() as? NSTextView,
              event.modifierFlags.intersection([.command, .control, .option]) == .command,
              let key = event.charactersIgnoringModifiers?.lowercased() else {
            return super.performKeyEquivalent(with: event)
        }
        switch key {
        case "a": editor.selectAll(nil)
        case "c": editor.copy(nil)
        case "x": editor.cut(nil)
        case "v": editor.paste(nil)
        case "z":
            if event.modifierFlags.contains(.shift) { editor.undoManager?.redo() }
            else { editor.undoManager?.undo() }
        default: return super.performKeyEquivalent(with: event)
        }
        return true
    }
}

final class WalkawayAddressCell: NSTextFieldCell {
    private func textFrame(_ rect: NSRect) -> NSRect {
        NSRect(x: rect.minX + 26, y: rect.minY, width: max(1, rect.width - 52), height: rect.height)
    }
    override func drawInterior(withFrame rect: NSRect, in view: NSView) {
        super.drawInterior(withFrame: textFrame(rect), in: view)
    }
    override func edit(withFrame rect: NSRect, in view: NSView, editor: NSText, delegate: Any?, event: NSEvent?) {
        super.edit(withFrame: textFrame(rect), in: view, editor: editor, delegate: delegate, event: event)
    }
    override func select(withFrame rect: NSRect, in view: NSView, editor: NSText, delegate: Any?, start: Int, length: Int) {
        super.select(withFrame: textFrame(rect), in: view, editor: editor, delegate: delegate, start: start, length: length)
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

@_cdecl("walkaway_tabs_set_address")
func walkawayTabsSetAddress(_ handle: UnsafeMutableRawPointer?, _ utf8: UnsafePointer<UInt8>?, _ length: Int,
                           _ loading: Int32, _ secure: Int32, _ editable: Int32) -> Int32 {
    walkawayMain(0) {
        guard length >= 0, length <= 1024 * 1024, let utf8 else { throw WalkawayTabError("invalid address text buffer") }
        guard let text = String(bytes: UnsafeBufferPointer(start: utf8, count: length), encoding: .utf8) else {
            throw WalkawayTabError("address text is not UTF-8")
        }
        try WalkawayTabBridge.lookup(handle).setAddress(text: text, loading: loading != 0, secure: secure != 0, editable: editable != 0)
        return 1
    }
}

@_cdecl("walkaway_tabs_address_frame")
func walkawayTabsAddressFrame(_ handle: UnsafeMutableRawPointer?, _ x: Double, _ y: Double, _ width: Double, _ height: Double) -> Int32 {
    walkawayMain(0) {
        try WalkawayTabBridge.lookup(handle).setAddressFrame(x: x, y: y, width: width, height: height)
        return 1
    }
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

@_cdecl("walkaway_tabs_capture_address_rgba")
func walkawayTabsCaptureAddressRGBA(_ handle: UnsafeMutableRawPointer?, _ output: UnsafeMutablePointer<UnsafeMutablePointer<UInt8>?>?,
                                   _ width: UnsafeMutablePointer<UInt32>?, _ height: UnsafeMutablePointer<UInt32>?) -> Int {
    output?.pointee = nil; width?.pointee = 0; height?.pointee = 0
    return walkawayMain(0) {
        guard let output, let width, let height else { throw WalkawayTabError("missing native address capture output pointers") }
        let (bytes, count, w, h) = try WalkawayTabBridge.lookup(handle).capture(address: true)
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
        // Put the new native tab row near the release point; constrain using
        // AppKit's global point space (including negative secondary displays).
        frame.origin = NSPoint(x: x - min(140, frame.width / 2),
                               y: y - frame.height + titlebarHeight + 106 + 19)
        if let screen { frame = window.constrainFrameRect(frame, to: screen) }
        window.setFrame(frame, display: true)
        return 1
    }
}
