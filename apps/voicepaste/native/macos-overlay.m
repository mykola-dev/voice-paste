#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#include <stdbool.h>

@interface VPBackdropPanel : NSPanel
@end
@implementation VPBackdropPanel
- (BOOL)canBecomeKeyWindow { return NO; }
- (BOOL)canBecomeMainWindow { return NO; }
@end
@interface VPOverlay : NSObject
@property(nonatomic, strong) NSWindow *foreground;
@property(nonatomic, strong) VPBackdropPanel *backdrop;
@property(nonatomic, strong) NSVisualEffectView *effect;
@property(nonatomic, strong) NSView *tint;
@end
@implementation VPOverlay
@end

void *vp_overlay_create(void *view) {
    NSView *foregroundView = (__bridge NSView *)view;
    NSWindow *foreground = foregroundView.window;
    if (!foreground || ![foreground isKindOfClass:NSPanel.class]) return NULL;
    foreground.styleMask = NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel;
    foreground.opaque = NO; foreground.backgroundColor = NSColor.clearColor;
    foreground.hasShadow = NO; foreground.ignoresMouseEvents = YES;
    foreground.level = NSStatusWindowLevel;
    foreground.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorFullScreenAuxiliary | NSWindowCollectionBehaviorIgnoresCycle;
    ((NSPanel *)foreground).hidesOnDeactivate = NO;
    ((NSPanel *)foreground).becomesKeyOnlyIfNeeded = YES;
    VPOverlay *owner = [VPOverlay new]; owner.foreground = foreground;
    owner.backdrop = [[VPBackdropPanel alloc] initWithContentRect:NSMakeRect(0,0,244,100) styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel backing:NSBackingStoreBuffered defer:NO];
    owner.backdrop.opaque = NO; owner.backdrop.backgroundColor = NSColor.clearColor;
    owner.backdrop.hasShadow = NO; owner.backdrop.ignoresMouseEvents = YES;
    owner.backdrop.level = NSStatusWindowLevel; owner.backdrop.hidesOnDeactivate = NO;
    owner.backdrop.collectionBehavior = foreground.collectionBehavior;
    owner.backdrop.contentView.wantsLayer = YES; owner.backdrop.contentView.layer.masksToBounds = YES;
    owner.effect = [[NSVisualEffectView alloc] initWithFrame:NSMakeRect(0,0,224,100)];
    owner.effect.blendingMode = NSVisualEffectBlendingModeBehindWindow;
    owner.effect.material = NSVisualEffectMaterialPopover;
    owner.effect.state = NSVisualEffectStateActive;
    owner.effect.wantsLayer = YES; owner.effect.layer.cornerRadius = 16; owner.effect.layer.masksToBounds = YES;
    owner.tint = [[NSView alloc] initWithFrame:owner.effect.bounds]; owner.tint.wantsLayer = YES;
    [owner.effect addSubview:owner.tint]; [owner.backdrop.contentView addSubview:owner.effect];
    return (__bridge_retained void *)owner;
}

bool vp_overlay_prepare(void *raw, bool blur, double alpha) {
    VPOverlay *owner = (__bridge VPOverlay *)raw;
    NSPoint mouse = NSEvent.mouseLocation; NSScreen *screen = NSScreen.mainScreen;
    for (NSScreen *candidate in NSScreen.screens) if (NSPointInRect(mouse,candidate.frame)) {screen=candidate;break;}
    if (!screen) return false;
    NSRect area = screen.visibleFrame;
    NSRect frame = NSMakeRect(NSMaxX(area)-244,NSMinY(area)+20,244,100);
    [owner.foreground setFrame:frame display:NO]; [owner.backdrop setFrame:frame display:NO];
    bool material = blur && !NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceTransparency;
    owner.effect.hidden = !material;
    owner.tint.layer.backgroundColor = [NSColor colorWithWhite:1 alpha:alpha].CGColor;
    return material;
}

void vp_overlay_present(void *raw, bool active, double opacity) {
    VPOverlay *owner = (__bridge VPOverlay *)raw;
    if (!active) { [owner.foreground orderOut:nil]; [owner.backdrop orderOut:nil]; return; }
    owner.effect.frame = NSMakeRect(244*(1-opacity),0,224,100);
    owner.effect.alphaValue = opacity;
    if (!owner.foreground.visible) {
        [owner.backdrop orderFrontRegardless];
        [owner.foreground orderWindow:NSWindowAbove relativeTo:owner.backdrop.windowNumber];
    }
}
void vp_overlay_destroy(void *raw) {
    VPOverlay *owner = (__bridge_transfer VPOverlay *)raw;
    [owner.foreground orderOut:nil]; [owner.backdrop orderOut:nil];
    [owner.backdrop close];
}
