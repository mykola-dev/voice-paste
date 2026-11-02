// Exercise the real tap callback without posting events or requesting TCC.
#import "../apps/voicepaste/native/macos-input.m"
#include <assert.h>

static CGEventRef key(uint16_t code, bool down, CGEventFlags flags) {
    CGEventRef event = CGEventCreateKeyboardEvent(NULL, code, down);
    assert(event);
    CGEventSetFlags(event, flags);
    return event;
}

static CGEventRef send(VPInput *input, CGEventType type, CGEventRef event) {
    return vp_key_event(NULL, type, event, (__bridge void *)input);
}

int main(void) {
    @autoreleasepool {
        VPInput *input = [VPInput new];
        VPToggle queue[8] = {0};
        uint8_t pressed[128] = {0};
        input.queue = queue;
        input.pressed = pressed;
        input.key = 111; // F12
        CGEventRef down = key(111, true, 0), up = key(111, false, 0);
        assert(send(input, kCGEventKeyDown, down) == NULL);
        assert(send(input, kCGEventKeyUp, up) == NULL);
        assert(input.count == 1);
        CFRelease(down); CFRelease(up);

        input.armedSession = 42;
        down = key(53, true, 0); up = key(53, false, 0);
        assert(send(input, kCGEventKeyDown, down) == NULL);
        assert(input.cancelSession == 42);
        assert(send(input, kCGEventKeyUp, up) == NULL);
        CFRelease(down); CFRelease(up);

        input.key = 61; // right Option uses flags-changed, not key-down
        down = key(61, true, kCGEventFlagMaskAlternate | 0x40);
        up = key(61, false, 0);
        assert(send(input, kCGEventFlagsChanged, down) == down);
        assert(send(input, kCGEventFlagsChanged, up) == up);
        assert(input.count == 2);
        CFRelease(down); CFRelease(up);

        input.key = 111;
        input.armedSession = 0;
        // A stale key-down whose key-up was consumed must not block hotkeys.
        input.pressed[0] = 1; input.otherKeys = 1;
        down = key(111, true, 0); up = key(111, false, 0);
        assert(send(input, kCGEventKeyDown, down) == NULL);
        assert(send(input, kCGEventKeyUp, up) == NULL);
        assert(input.count == 3);
        CFRelease(down); CFRelease(up);

        down = key(53, true, 0); up = key(53, false, 0);
        assert(send(input, kCGEventKeyDown, down) == down);
        assert(send(input, kCGEventKeyUp, up) == up);
        CFRelease(down); CFRelease(up);
    }
    return 0;
}
