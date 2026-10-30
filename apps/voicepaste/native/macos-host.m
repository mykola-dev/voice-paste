// Narrow AppKit adapter. All entry points and callbacks run on the main thread;
// callbacks only coalesce intents and never call Rust or perform IO.
#import <AppKit/AppKit.h>
#import <mach/mach.h>
#include <stdint.h>
#include <stdbool.h>

@interface VPMenuHost : NSObject
@property(nonatomic, strong) NSStatusItem *item;
@property(nonatomic, strong) NSMenu *menu;
@property(nonatomic) uint32_t pending;
@end

@implementation VPMenuHost
- (void)showSettings:(id)sender { (void)sender; self.pending |= 1; }
- (void)quit:(id)sender { (void)sender; self.pending |= 2; }
- (void)toggle:(id)sender { (void)sender; self.pending |= 4; }
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

uint64_t vp_host_resident_bytes(void) {
    struct mach_task_basic_info info;
    mach_msg_type_number_t count = MACH_TASK_BASIC_INFO_COUNT;
    kern_return_t status = task_info(mach_task_self(), MACH_TASK_BASIC_INFO, (task_info_t)&info, &count);
    return status == KERN_SUCCESS ? info.resident_size : UINT64_MAX;
}
