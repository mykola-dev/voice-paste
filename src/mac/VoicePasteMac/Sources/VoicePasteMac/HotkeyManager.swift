import AppKit
import Carbon

final class HotkeyManager {
    var onPressed: (() -> Void)?

    private var hotKeyRef: EventHotKeyRef?
    private var eventHandlerRef: EventHandlerRef?
    private var eventTap: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var isRightOptionDown = false
    private let hotKeyID = EventHotKeyID(signature: OSType(fourCC: "VPST"), id: 1)

    func register(_ hotkey: MacHotkey) throws {
        unregister()

        if hotkey == .rightOption {
            try registerRightOptionTap()
            Logger.shared.info("Registered hotkey: Right Option")
            return
        }

        installHandlerIfNeeded()

        let mapping = hotkey.carbonMapping
        let status = RegisterEventHotKey(
            mapping.keyCode,
            mapping.modifiers,
            hotKeyID,
            GetApplicationEventTarget(),
            0,
            &hotKeyRef
        )

        guard status == noErr else {
            throw NSError(domain: "VoicePasteMac.Hotkey", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "Failed to register hotkey \(hotkey.rawValue). It may be used by another app."])
        }

        Logger.shared.info("Registered hotkey: \(hotkey.rawValue)")
    }

    func unregister() {
        if let hotKeyRef {
            UnregisterEventHotKey(hotKeyRef)
            self.hotKeyRef = nil
        }
        if let runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), runLoopSource, .commonModes)
            self.runLoopSource = nil
        }
        if let eventTap {
            CGEvent.tapEnable(tap: eventTap, enable: false)
            self.eventTap = nil
        }
        isRightOptionDown = false
    }

    private func registerRightOptionTap() throws {
        guard Permissions.accessibilityStatus == .granted else {
            Permissions.requestAccessibilityPrompt()
            throw NSError(domain: "VoicePasteMac.Hotkey", code: 2, userInfo: [NSLocalizedDescriptionKey: "Right Option hotkey requires Accessibility permission."])
        }

        let mask = CGEventMask(1 << CGEventType.flagsChanged.rawValue)
        let selfPointer = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
        guard let tap = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: { _, type, event, userData in
                guard type == .flagsChanged, let userData else { return Unmanaged.passUnretained(event) }
                let manager = Unmanaged<HotkeyManager>.fromOpaque(userData).takeUnretainedValue()
                manager.handleFlagsChanged(event)
                return Unmanaged.passUnretained(event)
            },
            userInfo: selfPointer
        ) else {
            throw NSError(domain: "VoicePasteMac.Hotkey", code: 3, userInfo: [NSLocalizedDescriptionKey: "Failed to create Right Option event tap. Grant Accessibility permission and restart VoicePaste."])
        }

        let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        eventTap = tap
        runLoopSource = source
    }

    private func handleFlagsChanged(_ event: CGEvent) {
        let keyCode = event.getIntegerValueField(.keyboardEventKeycode)
        guard keyCode == Int64(kVK_RightOption) else { return }

        let isDown = event.flags.contains(.maskAlternate)
        if isDown && !isRightOptionDown {
            DispatchQueue.main.async { [weak self] in self?.onPressed?() }
        }
        isRightOptionDown = isDown
    }

    private func installHandlerIfNeeded() {
        guard eventHandlerRef == nil else { return }

        var eventType = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let selfPointer = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())

        InstallEventHandler(
            GetApplicationEventTarget(),
            { _, event, userData in
                guard let userData else { return noErr }
                var eventID = EventHotKeyID()
                let status = GetEventParameter(
                    event,
                    EventParamName(kEventParamDirectObject),
                    EventParamType(typeEventHotKeyID),
                    nil,
                    MemoryLayout<EventHotKeyID>.size,
                    nil,
                    &eventID
                )
                guard status == noErr, eventID.id == 1 else { return noErr }
                let manager = Unmanaged<HotkeyManager>.fromOpaque(userData).takeUnretainedValue()
                DispatchQueue.main.async { manager.onPressed?() }
                return noErr
            },
            1,
            &eventType,
            selfPointer,
            &eventHandlerRef
        )
    }

    deinit {
        unregister()
        if let eventHandlerRef {
            RemoveEventHandler(eventHandlerRef)
        }
    }
}

private extension MacHotkey {
    var carbonMapping: (keyCode: UInt32, modifiers: UInt32) {
        switch self {
        case .rightOption:
            return (UInt32(kVK_RightOption), 0)
        case .f13:
            return (UInt32(kVK_F13), 0)
        case .f14:
            return (UInt32(kVK_F14), 0)
        case .f15:
            return (UInt32(kVK_F15), 0)
        case .controlSpace:
            return (UInt32(kVK_Space), UInt32(controlKey))
        case .optionSpace:
            return (UInt32(kVK_Space), UInt32(optionKey))
        }
    }
}

private extension OSType {
    init(fourCC: String) {
        var result: UInt32 = 0
        for scalar in fourCC.unicodeScalars.prefix(4) {
            result = (result << 8) + UInt32(scalar.value)
        }
        self = result
    }
}
