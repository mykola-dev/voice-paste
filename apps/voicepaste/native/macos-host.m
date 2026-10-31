// Narrow AppKit adapter. All entry points and callbacks run on the main thread;
// callbacks only coalesce intents and never call Rust or perform IO.
#import <AppKit/AppKit.h>
#import <mach/mach.h>
#import <AVFoundation/AVFoundation.h>
#include <libproc.h>
#include <stdint.h>
#include <stdbool.h>

@interface VPMenuHost : NSObject
@property(nonatomic, strong) NSStatusItem *item;
@property(nonatomic, strong) NSMenu *menu;
@property(nonatomic) uint32_t pending;
@property(atomic) bool microphoneRequestPending;
@property(nonatomic, strong) NSMenuItem *captureItem;
@property(nonatomic, strong) NSMenuItem *cancelItem;
@property(nonatomic, strong) NSMenuItem *captureStatus;
@property(nonatomic, strong) NSMenuItem *dictationItem;
@end

@implementation VPMenuHost
- (void)showSettings:(id)sender { (void)sender; self.pending |= 1; }
- (void)quit:(id)sender { (void)sender; self.pending |= 2; }
- (void)toggle:(id)sender { (void)sender; self.pending |= 4; }
- (void)cancel:(id)sender { (void)sender; self.pending |= 8; }
- (void)download:(id)sender { (void)sender; self.pending |= 16; }
- (void)enableKeys:(id)sender { (void)sender; self.pending |= 32; }
- (void)clicked:(id)sender {
    (void)sender;
    if (NSApp.currentEvent.type == NSEventTypeRightMouseUp) {
        [self.menu popUpMenuPositioningItem:nil
                               atLocation:NSMakePoint(0, self.item.button.bounds.size.height)
                                   inView:self.item.button];
    } else {
        self.pending |= 1;
    }
}
@end

void *vp_menu_create(const uint8_t *png, size_t length) {
    NSCAssert([NSThread isMainThread], @"menu creation must be on the main thread");
    VPMenuHost *host = [VPMenuHost new];
    host.item = [NSStatusBar.systemStatusBar statusItemWithLength:NSSquareStatusItemLength];
    if (!host.item.button) return NULL;
    NSImage *image = [[NSImage alloc] initWithData:[NSData dataWithBytes:png length:length]];
    if (!image) { [NSStatusBar.systemStatusBar removeStatusItem:host.item]; return NULL; }
    image.size = NSMakeSize(18, 18);
    image.template = NO;
    host.item.button.image = image;
    host.item.button.toolTip = @"VoicePaste — Settings preview (dictation not connected)";
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
    [host.menu addItem:item];
}

uint32_t vp_menu_drain(void *raw) {
    NSCAssert([NSThread isMainThread], @"menu drain must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    uint32_t pending = host.pending;
    host.pending = 0;
    return pending;
}

void vp_menu_destroy(void *raw) {
    NSCAssert([NSThread isMainThread], @"menu destruction must be on the main thread");
    VPMenuHost *host = (__bridge_transfer VPMenuHost *)raw;
    host.item.button.target = nil;
    host.item.button.action = NULL;
    [NSStatusBar.systemStatusBar removeStatusItem:host.item];
}

// Explicit capture-only mode: normal launches never request microphone access.
void vp_capture_menu_enable(void *raw) {
    NSCAssert([NSThread isMainThread], @"capture menu must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
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

void vp_dictation_menu_enable(void *raw) {
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    vp_capture_menu_enable(raw);
    host.captureItem.title = @"Start recording";
    host.captureStatus.title = @"Preparing dictation";
    NSMenuItem *download = [[NSMenuItem alloc] initWithTitle:@"Download selected model…" action:@selector(download:) keyEquivalent:@""];
    download.target = host;
    [host.menu addItem:download];
    NSMenuItem *keys = [[NSMenuItem alloc] initWithTitle:@"Enable recording keys…" action:@selector(enableKeys:) keyEquivalent:@""];
    keys.target = host;
    [host.menu addItem:keys];
    host.dictationItem = host.captureItem;
}

bool vp_confirm_download(const char *model) {
    NSAlert *alert = [NSAlert new];
    alert.messageText = @"Download selected Whisper model?";
    alert.informativeText = [NSString stringWithFormat:@"Download %@ and Silero VAD from Hugging Face? Full-size weights need approximately 1.6–3.1 GB plus temporary disk space. No smaller or quantized model will be substituted.", [NSString stringWithUTF8String:model]];
    [alert addButtonWithTitle:@"Download"]; [alert addButtonWithTitle:@"Cancel"];
    return [alert runModal] == NSAlertFirstButtonReturn;
}

void vp_capture_menu_update(void *raw, bool recording, bool busy, const char *status) {
    NSCAssert([NSThread isMainThread], @"capture menu must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    host.captureItem.title = host.dictationItem ? (recording ? @"Stop recording" : @"Start recording") : (recording ? @"Stop and discard capture" : @"Start microphone capture (no transcription)");
    host.captureItem.enabled = !busy;
    host.cancelItem.enabled = recording && !busy;
    host.captureStatus.title = [NSString stringWithUTF8String:status];
    host.item.button.toolTip = host.captureStatus.title;
}

// 0 undecided, 1 granted, 2 denied/restricted, 3 request in flight.
uint32_t vp_microphone_permission(void *raw, bool request) {
    NSCAssert([NSThread isMainThread], @"permission request must be on the main thread");
    VPMenuHost *host = (__bridge VPMenuHost *)raw;
    AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
    if (status == AVAuthorizationStatusAuthorized) return 1;
    if (status == AVAuthorizationStatusDenied || status == AVAuthorizationStatusRestricted) return 2;
    if (!request) return host.microphoneRequestPending ? 3 : 0;
    if (!host.microphoneRequestPending) {
        host.microphoneRequestPending = YES;
        // The block owns host until completion. No borrowed Rust pointer, no
        // GPUI reentry and no menu mutation from the arbitrary completion thread.
        [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL granted) {
            (void)granted;
            host.microphoneRequestPending = NO;
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
    return count == sizeof(info) ? info.pti_resident_size : UINT64_MAX;
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
