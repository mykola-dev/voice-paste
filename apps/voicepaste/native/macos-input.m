// UI-thread event-tap owner. No Rust callbacks, disk IO or inference in hooks.
#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#include <stdbool.h>
#include <stdint.h>

typedef struct { int32_t pid; } VPToggle;
@interface VPInput : NSObject
@property(nonatomic) CFMachPortRef tap;
@property(nonatomic) CFRunLoopSourceRef source;
@property(nonatomic) uint16_t key;
@property(nonatomic) bool held;
@property(nonatomic) bool contaminated;
@property(nonatomic) bool functionConsumed;
@property(nonatomic) uint64_t armedSession;
@property(nonatomic) bool escapeHeld;
@property(nonatomic) uint64_t cancelSession;
@property(nonatomic) bool overflow;
@property(nonatomic) bool disabled;
@property(nonatomic) bool active;
@property(nonatomic) uint32_t count;
@property(nonatomic) VPToggle *queue;
@property(nonatomic) uint8_t *pressed;
@property(nonatomic) uint32_t otherKeys;
@end
@implementation VPInput
@end

static void vp_refresh_keys(VPInput *input) {
    input.otherKeys = 0;
    memset(input.pressed, 0, 128);
    for (uint16_t key = 0; key < 128; key++) {
        if (key != input.key && !(key >= 54 && key <= 63) && CGEventSourceKeyState(kCGEventSourceStateCombinedSessionState, key)) {input.pressed[key]=1;input.otherKeys++;}
    }
}

static CGEventRef vp_key_event(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *raw) {
    (void)proxy;
    VPInput *input = (__bridge VPInput *)raw;
    if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
        if (input.active) input.disabled = YES;
        input.held = NO;
        input.escapeHeld = NO;
        input.functionConsumed = NO;
        // Repair and resynchronize outside the event callback. An intentionally
        // disabled staged replacement must never re-enable itself here.
        return event;
    }
    uint16_t key = (uint16_t)CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
    bool down = type == kCGEventKeyDown;
    if (key < 128 && key != input.key && (type == kCGEventKeyDown || type == kCGEventKeyUp)) {
        if (down && !input.pressed[key]) { input.pressed[key] = 1; input.otherKeys++; }
        else if (!down && input.pressed[key]) { input.pressed[key] = 0; if (input.otherKeys) input.otherKeys--; }
    }
    if (key == 53) {
        if (down && input.escapeHeld) return NULL;
        if (down && input.armedSession) { input.escapeHeld = YES; input.cancelSession = input.armedSession; return NULL; }
        if (type == kCGEventKeyUp && input.escapeHeld) { input.escapeHeld = NO; return NULL; }
    }
    if (type == kCGEventFlagsChanged && key == input.key) {
        CGEventFlags side = key == 62 ? (CGEventFlags)0x2000 : key == 61 ? (CGEventFlags)0x40 : (CGEventFlags)0x10;
        down = (CGEventGetFlags(event) & side) != 0;
    } else if (type != kCGEventKeyDown && type != kCGEventKeyUp) {
        if (input.held) input.contaminated = YES;
        return event;
    }
    if (key != input.key) { if (input.held) input.contaminated = YES; return event; }
    if (CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat)) return input.functionConsumed ? NULL : event;
    CGEventFlags modifiers = CGEventGetFlags(event) & (kCGEventFlagMaskCommand | kCGEventFlagMaskShift | kCGEventFlagMaskControl | kCGEventFlagMaskAlternate);
    CGEventFlags own = input.key == 62 ? kCGEventFlagMaskControl : input.key == 61 ? kCGEventFlagMaskAlternate : input.key == 54 ? kCGEventFlagMaskCommand : 0;
    if (down) {
        if (!input.held) {
            // Another tap can consume a key-up. Discard stale tracked downs so
            // an unrelated old key cannot block every subsequent hotkey.
            for (uint16_t k = 0; k < 128 && input.otherKeys; k++) {
                if (input.pressed[k] && !CGEventSourceKeyState(kCGEventSourceStateCombinedSessionState, k)) {
                    input.pressed[k] = 0;
                    input.otherKeys--;
                }
            }
            CGEventFlags all = CGEventGetFlags(event);
            input.held = YES;
            input.contaminated = input.otherKeys != 0 || (modifiers & ~own) != 0 || (input.key == 62 && (all & 0x1)) || (input.key == 61 && (all & 0x20)) || (input.key == 54 && (all & 0x8));
            input.functionConsumed = (input.key == 100 || input.key == 111) && !input.contaminated;
        }
        if (input.functionConsumed) return NULL;
    } else if (input.held) {
        input.held = NO;
        if (!input.contaminated) {
            if (input.count < 8) {
                input.queue[input.count++] = (VPToggle){ .pid = NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier };
            } else { input.overflow = YES; }
        }
        if (input.functionConsumed) { input.functionConsumed = NO; return NULL; }
    }
    return event;
}

void *vp_input_create(uint16_t key, bool prompt, uint32_t *error) {
    NSCAssert(NSThread.isMainThread, @"input creation must be on main thread");
    *error = 0;
    if (key != 62 && key != 61 && key != 54 && key != 100 && key != 111) { *error = 1; return NULL; }
    NSDictionary *options = @{(__bridge NSString *)kAXTrustedCheckOptionPrompt: @(prompt)};
    if (!AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)options)) {
        if (prompt) [NSWorkspace.sharedWorkspace openURL:[NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"]];
        *error = 2; return NULL;
    }
    VPInput *input = [VPInput new];
    input.key = key;
    input.queue = calloc(8, sizeof(VPToggle));
    input.pressed = calloc(128, 1);
    if (!input.queue || !input.pressed) {free(input.queue);free(input.pressed);*error = 3;return NULL;}
    vp_refresh_keys(input);
    CGEventMask mask = CGEventMaskBit(kCGEventKeyDown) | CGEventMaskBit(kCGEventKeyUp) | CGEventMaskBit(kCGEventFlagsChanged);
    input.tap = CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap, kCGEventTapOptionDefault, mask, vp_key_event, (__bridge void *)input);
    if (!input.tap) { free(input.queue); free(input.pressed); *error = 4; return NULL; }
    input.source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, input.tap, 0);
    if (!input.source) { CFRelease(input.tap); free(input.queue); free(input.pressed); *error = 3; return NULL; }
    CFRunLoopAddSource(CFRunLoopGetMain(), input.source, kCFRunLoopCommonModes);
    input.active = YES;
    CGEventTapEnable(input.tap, true);
    return (__bridge_retained void *)input;
}

void vp_input_arm(void *raw, uint64_t session) {
    VPInput *input = (__bridge VPInput *)raw;
    input.armedSession = session;
}
void vp_input_active(void *raw, bool active) {
    VPInput *input = (__bridge VPInput *)raw;
    input.active = active;
    if (active) vp_refresh_keys(input);
    else { input.held = NO; input.escapeHeld = NO; input.functionConsumed = NO; }
    CGEventTapEnable(input.tap, active);
}

uint32_t vp_input_drain(void *raw, int32_t *pids, uint32_t *flags, uint64_t *cancelSession) {
    VPInput *input = (__bridge VPInput *)raw;
    bool invalid = !CFMachPortIsValid(input.tap);
    if (!invalid && input.active && !CGEventTapIsEnabled(input.tap)) {
        input.disabled = YES;
        input.held = NO; input.escapeHeld = NO; input.functionConsumed = NO;
        vp_refresh_keys(input);
        CGEventTapEnable(input.tap, true);
    }
    uint32_t count = input.count;
    for (uint32_t i = 0; i < count; i++) pids[i] = input.queue[i].pid;
    *cancelSession = input.cancelSession;
    *flags = (input.overflow ? 2 : 0) | (input.disabled ? 4 : 0) | (invalid ? 8 : 0);
    input.count = 0; input.cancelSession = 0; input.overflow = NO; input.disabled = NO;
    return count;
}

void vp_input_destroy(void *raw) {
    VPInput *input = (__bridge_transfer VPInput *)raw;
    CGEventTapEnable(input.tap, false);
    CFMachPortInvalidate(input.tap);
    CFRunLoopRemoveSource(CFRunLoopGetMain(), input.source, kCFRunLoopCommonModes);
    CFRelease(input.source); CFRelease(input.tap); free(input.queue); free(input.pressed);
}

int32_t vp_focus_capture(void) { return NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier; }

// Called only on the dictation owner thread, never while holding GPUI borrows.
// Failure to restore deliberately retains current focus, matching Windows policy.
bool vp_paste_prepare(const uint8_t *bytes, size_t length, int32_t pid, uint32_t *error) {
    @autoreleasepool {
        *error = 0;
        NSString *text = [[NSString alloc] initWithBytes:bytes length:length encoding:NSUTF8StringEncoding];
        if (!text) { *error = 1; return false; }
        NSPasteboard *clipboard = NSPasteboard.generalPasteboard;
        [clipboard clearContents];
        if (![clipboard setString:text forType:NSPasteboardTypeString]) { *error = 2; return false; }
        if (!AXIsProcessTrusted() || !CGPreflightPostEventAccess()) { *error = 3; return false; }
        if (pid > 0 && pid != NSRunningApplication.currentApplication.processIdentifier) {
            NSRunningApplication *target = [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
            if (target && !target.terminated) {
                [target activateWithOptions:0];
                // Bounded wait for an asynchronous activation, off the UI thread.
                for (int i = 0; i < 50 && !target.active; i++) [NSThread sleepForTimeInterval:0.01];
            }
        }
        return true;
    }
}

bool vp_paste_post(uint32_t *error) {
    @autoreleasepool {
        *error = 0;
        if (!CGPreflightPostEventAccess()) { *error = 3; return false; }
        CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
        if (!source) { *error = 4; return false; }
        CGEventRef commandDown = CGEventCreateKeyboardEvent(source, 55, true);
        CGEventRef down = CGEventCreateKeyboardEvent(source, 9, true);
        CGEventRef up = CGEventCreateKeyboardEvent(source, 9, false);
        CGEventRef commandUp = CGEventCreateKeyboardEvent(source, 55, false);
        if (!commandDown || !down || !up || !commandUp) {
            if (commandDown) CFRelease(commandDown);
            if (down) CFRelease(down);
            if (up) CFRelease(up);
            if (commandUp) CFRelease(commandUp);
            CFRelease(source); *error = 4; return false;
        }
        CGEventSetFlags(commandDown, kCGEventFlagMaskCommand);
        CGEventSetFlags(down, kCGEventFlagMaskCommand);
        CGEventSetFlags(up, kCGEventFlagMaskCommand);
        CGEventSetFlags(commandUp, 0);
        CGEventPost(kCGHIDEventTap, commandDown);
        CGEventPost(kCGHIDEventTap, down);
        CGEventPost(kCGHIDEventTap, up);
        CGEventPost(kCGHIDEventTap, commandUp);
        CFRelease(commandDown); CFRelease(down); CFRelease(up); CFRelease(commandUp); CFRelease(source);
        return true; // Posted, not a claim that the target accepted the paste.
    }
}
