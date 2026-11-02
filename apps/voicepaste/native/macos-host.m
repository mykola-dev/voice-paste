// Narrow AppKit adapter. All entry points and callbacks run on the main thread;
// callbacks only coalesce intents and never call Rust or perform IO.
#import <AppKit/AppKit.h>
#import <mach/mach.h>
#import <AVFoundation/AVFoundation.h>
#include <libproc.h>
#include <stdint.h>
#include <stdbool.h>

bool vp_settings_activate(void *rawView) {
    NSCAssert([NSThread isMainThread], @"Settings activation must be on the main thread");
    NSView *view = (__bridge NSView *)rawView;
    NSWindow *window = view.window;
    if (!window) return false;
    if (window.miniaturized) [window deminiaturize:nil];
    [window makeKeyAndOrderFront:nil];
    [NSApp activate];
    return true;
}

@interface VPMenuHost : NSObject
@property(nonatomic, strong) NSStatusItem *item;
@property(nonatomic, strong) NSMenu *menu;
@property(nonatomic) uint32_t pending;
@property(nonatomic) bool microphoneRequestPending;
@property(nonatomic) bool alive;
@property(nonatomic, strong) NSMenuItem *captureItem;
@property(nonatomic, strong) NSMenuItem *cancelItem;
@property(nonatomic, strong) NSMenuItem *captureStatus;
@property(nonatomic, strong) NSMenuItem *dictationItem;
@property(nonatomic, strong) NSImage *idleImage;
@property(nonatomic, strong) NSImage *recordingImage;
@property(nonatomic) bool recording;
@property(nonatomic) uint64_t recordingSession;
@property(nonatomic) uint64_t pendingCancelSession;
@end

@implementation VPMenuHost
- (void)showSettings:(id)sender { (void)sender; self.pending |= 1; }
- (void)quit:(id)sender { (void)sender; self.pending |= 2; }
- (void)toggle:(id)sender { (void)sender; self.pending |= 4; }
- (void)cancel:(id)sender { (void)sender; self.pendingCancelSession = self.recordingSession; }
- (void)clicked:(id)sender {
    (void)sender;
    if (NSApp.currentEvent.type == NSEventTypeRightMouseUp) {
        // A status item can inherit the menu bar's appearance instead of the
        // application's system appearance; refresh on every open for theme changes.
        self.menu.appearance = NSApp.effectiveAppearance;
        [self.menu popUpMenuPositioningItem:nil
                               atLocation:NSMakePoint(0, self.item.button.bounds.size.height)
                                   inView:self.item.button];
    } else {
        self.pending |= 1;
    }
}
@end

void *vp_menu_create(const uint8_t *png, size_t length, const uint8_t *openPng, size_t openLength) {
    NSCAssert([NSThread isMainThread], @"menu creation must be on the main thread");
    VPMenuHost *host = [VPMenuHost new];
    host.alive = YES;
    host.item = [NSStatusBar.systemStatusBar statusItemWithLength:NSSquareStatusItemLength];
    if (!host.item.button) return NULL;
    NSImage *image = [[NSImage alloc] initWithData:[NSData dataWithBytes:png length:length]];
    if (!image) { [NSStatusBar.systemStatusBar removeStatusItem:host.item]; return NULL; }
    NSSize idleSize = image.size;
    if (idleSize.width <= 0 || idleSize.height <= 0) { [NSStatusBar.systemStatusBar removeStatusItem:host.item]; return NULL; }
    CGFloat idleScale = 18.0 / MAX(idleSize.width, idleSize.height);
    image.size = NSMakeSize(idleSize.width * idleScale, idleSize.height * idleScale);
    image.template = NO;
    NSImage *recordingImage = [[NSImage alloc] initWithData:[NSData dataWithBytes:openPng length:openLength]];
    if (!recordingImage) { [NSStatusBar.systemStatusBar removeStatusItem:host.item]; return NULL; }
    NSSize recordingSize = recordingImage.size;
    if (recordingSize.width <= 0 || recordingSize.height <= 0) { [NSStatusBar.systemStatusBar removeStatusItem:host.item]; return NULL; }
    CGFloat recordingScale = 18.0 / MAX(recordingSize.width, recordingSize.height);
    recordingImage.size = NSMakeSize(recordingSize.width * recordingScale, recordingSize.height * recordingScale);
    recordingImage.template = NO;
    host.idleImage = image; host.recordingImage = recordingImage;
    host.item.button.image = image;
    host.item.button.toolTip = @"VoicePaste — local dictation";
    host.item.button.target = host;
    host.item.button.action = @selector(clicked:);
    [host.item.button sendActionOn:(NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp)];
    host.menu = [NSMenu new];
    host.menu.autoenablesItems = NO;
    return (__bridge_retained void *)host;
}

void vp_menu_add(void *raw, const char *title, uint32_t action, bool enabled) {
    NSCAssert([NSThread isMainThread], @"menu mutation must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    if (!title) { [host.menu addItem:NSMenuItem.separatorItem]; return; }
    SEL selector = action == 1 ? @selector(showSettings:) : action == 2 ? @selector(quit:) : @selector(toggle:);
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:[NSString stringWithUTF8String:title] action:selector keyEquivalent:@""];
    item.target = host;
    item.enabled = enabled;
    if (action == 4) host.dictationItem = item;
    [host.menu addItem:item];
}

uint32_t vp_menu_drain(void *raw, uint64_t *cancelSession) {
    NSCAssert([NSThread isMainThread], @"menu drain must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    uint32_t pending = host.pending;
    host.pending = 0;
    *cancelSession = host.pendingCancelSession;host.pendingCancelSession = 0;
    return pending;
}

void vp_menu_destroy(void *raw) {
    NSCAssert([NSThread isMainThread], @"menu destruction must be on the main thread");
    VPMenuHost *host = (__bridge_transfer VPMenuHost *)raw;
    host.alive = NO;
    host.item.button.target = nil;
    host.item.button.action = NULL;
    [NSStatusBar.systemStatusBar removeStatusItem:host.item];
}

// Explicit capture-only mode: normal launches never request microphone access.
void vp_capture_menu_enable(void *raw) {
    NSCAssert([NSThread isMainThread], @"capture menu must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    host.dictationItem = nil;
    host.captureItem = [host.menu itemAtIndex:0];
    host.captureItem.title = @"Start microphone capture (no transcription)";
    host.captureItem.enabled = YES;
    host.cancelItem = [[NSMenuItem alloc] initWithTitle:@"Cancel recording" action:@selector(cancel:) keyEquivalent:@""];
    host.cancelItem.target = host;
    host.cancelItem.enabled = NO;
    [host.menu insertItem:host.cancelItem atIndex:1];
    host.captureStatus = [[NSMenuItem alloc] initWithTitle:@"Capture preview — no transcription or paste" action:NULL keyEquivalent:@""];
    host.captureStatus.enabled = NO;
    [host.menu insertItem:host.captureStatus atIndex:2];
    host.item.button.toolTip = host.captureStatus.title;
}

void vp_menu_update(void *raw, const char *title, bool enabled, bool active, const char *status) {
    NSCAssert([NSThread isMainThread], @"menu update must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    host.dictationItem.title = [NSString stringWithUTF8String:title];
    host.dictationItem.enabled = enabled;
    if (host.recording != active) {host.recording = active;host.item.button.image = active ? host.recordingImage : host.idleImage;}
    host.item.button.toolTip = [NSString stringWithUTF8String:status];
}

bool vp_confirm_download(const char *model) {
    NSCAssert([NSThread isMainThread], @"download alert must be on the main thread");
    NSAlert *alert = [NSAlert new];
    alert.messageText = @"Download model?";
    alert.informativeText = [NSString stringWithUTF8String:model];
    [alert addButtonWithTitle:@"Download"]; [alert addButtonWithTitle:@"Cancel"];
    [NSApp activate];
    return [alert runModal] == NSAlertFirstButtonReturn;
}

bool vp_retry_without_vad(const char *message) {
    NSCAssert([NSThread isMainThread], @"retry alert must be on the main thread");
    NSAlert *alert = [NSAlert new];
    alert.messageText = @"Transcription failed";
    alert.informativeText = [NSString stringWithUTF8String:message];
    [alert addButtonWithTitle:@"Try without VAD"];
    [alert addButtonWithTitle:@"Cancel"];
    [NSApp activate];
    return [alert runModal] == NSAlertFirstButtonReturn;
}

void vp_capture_menu_update(void *raw, bool recording, bool busy, bool active, uint64_t session, const char *status) {
    NSCAssert([NSThread isMainThread], @"capture menu must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    host.captureItem.title = host.dictationItem ? (recording ? @"Stop recording" : @"Start recording") : (recording ? @"Stop and discard capture" : @"Start microphone capture (no transcription)");
    host.recordingSession = session;
    if (host.recording != active) {host.recording = active;host.item.button.image = active ? host.recordingImage : host.idleImage;}
    host.captureItem.enabled = !busy;
    host.cancelItem.enabled = recording && !busy;
    host.captureStatus.title = [NSString stringWithUTF8String:status];
    host.item.button.toolTip = host.captureStatus.title;
}

// 0 undecided, 1 granted, 2 denied/restricted, 3 request in flight.
static void vp_open_microphone_settings(void) {
    [NSWorkspace.sharedWorkspace openURL:[NSURL URLWithString:@"x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"]];
}

uint32_t vp_microphone_permission(void *raw, bool request) {
    NSCAssert([NSThread isMainThread], @"permission request must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
    if (status == AVAuthorizationStatusAuthorized) return 1;
    if (status == AVAuthorizationStatusDenied || status == AVAuthorizationStatusRestricted) {
        if (request) vp_open_microphone_settings();
        return 2;
    }
    if (!request) return host.microphoneRequestPending ? 3 : 0;
    if (!host.microphoneRequestPending) {
        host.microphoneRequestPending = YES;
        // Complete on the owner thread; shutdown suppresses delayed activation.
        // No borrowed Rust pointer or GPUI reentry from either block.
        [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) {
            dispatch_async(dispatch_get_main_queue(), ^{
                host.microphoneRequestPending = NO;
                if (host.alive && !granted) vp_open_microphone_settings();
            });
        }];
    }
    return 3;
}

uint64_t vp_host_resident_bytes(void) {
    struct mach_task_basic_info info;
    mach_msg_type_number_t count = MACH_TASK_BASIC_INFO_COUNT;
    kern_return_t status = task_info(mach_task_self(), MACH_TASK_BASIC_INFO, (task_info_t)&info, &count);
    return status == KERN_SUCCESS ? info.resident_size : UINT64_MAX;
}
uint64_t vp_process_resident_bytes(int32_t pid) {
    struct proc_taskinfo info;
    int count = proc_pidinfo(pid, PROC_PIDTASKINFO, 0, &info, sizeof(info));
    return count == (int)sizeof(info) ? info.pti_resident_size : UINT64_MAX;
}

void vp_startup_error(const char *message) {
    NSCAssert([NSThread isMainThread], @"startup alert must be on the main thread");
    [NSApplication sharedApplication];
    NSAlert *alert = [NSAlert new];
    alert.alertStyle = NSAlertStyleCritical;
    alert.messageText = @"VoicePaste could not start";
    alert.informativeText = [NSString stringWithUTF8String:message];
    [alert addButtonWithTitle:@"OK"];
    [NSApp activate];
    [alert runModal];
}

void vp_host_error(const char *message) {
    NSCAssert([NSThread isMainThread], @"error alert must be on the main thread");
    NSAlert *alert = [NSAlert new];
    alert.alertStyle = NSAlertStyleCritical;
    alert.messageText = @"VoicePaste";
    alert.informativeText = [NSString stringWithUTF8String:message];
    [alert addButtonWithTitle:@"OK"];
    [NSApp activate];
    [alert runModal];
}
